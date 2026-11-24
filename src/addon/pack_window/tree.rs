//! The pack as a tree: categories, with their markers and trails inside.

use super::{SELECT_HINT, State, multi_select};
use crate::{
    addon::{Live, ui::hover_hint},
    attrs,
    editor::{Editor, Selection},
    namespace,
    pack::{Category, Pack},
};
use nexus::imgui::{Condition, Selectable, StyleVar, TreeNode, TreeNodeFlags, TreeNodeToken, Ui};
use std::collections::HashMap;

/// Frame padding of the tree's rows. The expand arrow's click zone is the font size plus twice
/// the horizontal padding, and FRAME_PADDING rows are the text plus twice the vertical one, so
/// this makes the arrow easier to hit than ImGui's default (4, 3).
const TREE_PADDING: [f32; 2] = [10.0, 5.0];

/// The visible markers and trails, grouped by category.
#[derive(Default)]
struct TreeIndex {
    /// Lowercase namespace → indices in `pack.pois` / `pack.trails`.
    pois: HashMap<String, Vec<usize>>,
    trails: HashMap<String, Vec<usize>>,
    /// Lowercase namespace → visible markers and trails in it and below it.
    totals: HashMap<String, (usize, usize)>,
}

impl TreeIndex {
    fn new(pack: &Pack, map: Option<u32>) -> Self {
        let mut index = Self::default();
        let visible = |map_id: u32| map.is_none_or(|m| m == map_id);
        for (i, poi) in pack.pois.iter().enumerate() {
            if visible(poi.map_id) {
                index.add(&poi.category, i, false);
            }
        }
        for (i, trail) in pack.trails.iter().enumerate() {
            if visible(trail.map_id) {
                index.add(&trail.category, i, true);
            }
        }
        index
    }

    fn add(&mut self, category: &str, item: usize, trail: bool) {
        let key = category.to_ascii_lowercase();
        let mut prefix = String::new();
        for segment in namespace::segments(&key) {
            prefix = namespace::join(&prefix, segment);
            let total = self.totals.entry(prefix.clone()).or_default();
            if trail {
                total.1 += 1;
            } else {
                total.0 += 1;
            }
        }
        let list = if trail {
            &mut self.trails
        } else {
            &mut self.pois
        };
        list.entry(key).or_default().push(item);
    }
}

/// A click in the tree.
enum Click {
    /// A category's "show in the preview" box: the namespace and its new state.
    Show(String, bool),
    /// Select this (or nothing: the whole pack).
    Select(Option<Selection>),
    /// A marker: Ctrl and Shift decide what it does.
    Poi(usize),
}

pub(super) fn tree(ui: &Ui, editor: &mut Editor, live: &Live, state: &mut State) {
    let map = (state.this_map_only && live.in_game).then_some(live.map_id);
    let index = TreeIndex::new(&editor.pack, map);
    let filter = state.filter.trim().to_lowercase();
    let mut clicked = None;
    // The markers in the order they're listed, for Shift+click ranges.
    let mut order = Vec::new();
    // Bigger rows and arrows, with children indented by the arrow's width so they line up.
    let padding = ui.push_style_var(StyleVar::FramePadding(TREE_PADDING));
    let indent = ui.push_style_var(StyleVar::IndentSpacing(
        ui.current_font_size() + 2.0 * TREE_PADDING[0],
    ));

    // Always one line, so the tree doesn't move when the first category is hidden.
    let hidden = editor.settings.hidden_categories.len();
    if hidden > 0 {
        ui.text_disabled(format!("{hidden} categor(ies) hidden in the preview."));
        ui.same_line();
        if ui.small_button("Show all") {
            editor.settings.hidden_categories.clear();
        }
        hover_hint(ui, "Ticks every category's box again.");
    } else {
        ui.text_disabled("All categories shown in the preview.");
        ui.same_line();
        if ui.small_button("Hide all") {
            let tops: Vec<String> = editor
                .pack
                .categories
                .iter()
                .map(|c| c.name.clone())
                .collect();
            for ns in tops {
                editor.settings.set_hidden(&ns, true);
            }
        }
        hover_hint(
            ui,
            "Unticks every top-level category, e.g. to then tick back only the ones you're working on.",
        );
    }
    let (pois, trails) = (editor.pack.pois.len(), editor.pack.trails.len());
    if Selectable::new(format!("Whole pack: {pois} marker(s), {trails} trail(s)"))
        .selected(editor.selection.is_none())
        .build(ui)
    {
        clicked = Some(Click::Select(None));
    }

    let tree = Tree {
        editor,
        index: &index,
    };
    for category in &tree.editor.pack.categories {
        tree.category(
            ui,
            category,
            &category.name,
            &filter,
            &mut clicked,
            &mut order,
        );
    }
    // Markers and trails without a category.
    if index.pois.contains_key("") || index.trails.contains_key("") {
        let node = tree_node(
            ui,
            "##uncategorized",
            "(no category)",
            TreeNodeFlags::SPAN_AVAIL_WIDTH | TreeNodeFlags::FRAME_PADDING,
        );
        if let Some(_node) = node {
            tree.items(ui, "", &mut clicked, &mut order);
        }
    }

    indent.pop();
    padding.pop();
    match clicked {
        Some(Click::Select(selection)) => {
            editor.selection = selection;
            state.poi_anchor = None;
        }
        Some(Click::Poi(i)) => click_poi(ui, editor, state, i, &order),
        Some(Click::Show(ns, shown)) => editor.settings.set_hidden(&ns, !shown),
        None => {}
    }
}

fn click_poi(ui: &Ui, editor: &mut Editor, state: &mut State, i: usize, order: &[usize]) {
    let mut set = editor.selected_pois().cloned().unwrap_or_default();
    multi_select(ui, &mut set, &mut state.poi_anchor, i, order);
    editor.selection = (!set.is_empty()).then_some(Selection::Pois(set));
}

struct Tree<'a> {
    editor: &'a Editor,
    index: &'a TreeIndex,
}

impl Tree<'_> {
    fn category(
        &self,
        ui: &Ui,
        category: &Category,
        ns: &str,
        filter: &str,
        clicked: &mut Option<Click>,
        order: &mut Vec<usize>,
    ) {
        let self_matches = filter.is_empty()
            || ns.to_lowercase().contains(filter)
            || category.display_name().to_lowercase().contains(filter);
        if !self_matches && !matches_below(category, ns, filter) {
            return;
        }
        let key = ns.to_ascii_lowercase();
        let (pois, trails) = self.index.totals.get(&key).copied().unwrap_or_default();
        let has_items = self.index.pois.contains_key(&key) || self.index.trails.contains_key(&key);

        let mut flags = TreeNodeFlags::OPEN_ON_ARROW
            | TreeNodeFlags::OPEN_ON_DOUBLE_CLICK
            | TreeNodeFlags::SPAN_AVAIL_WIDTH
            | TreeNodeFlags::FRAME_PADDING;
        if category.children.is_empty() && !has_items {
            flags |= TreeNodeFlags::LEAF;
        }
        let selected = matches!(&self.editor.selection, Some(Selection::Category(s)) if s == ns);
        if selected {
            flags |= TreeNodeFlags::SELECTED;
        }
        let label = format!("{}  {}", category.display_name(), counts(pois, trails));
        // The box: whether the preview draws it. Greyed when a category above is unticked.
        let settings = &self.editor.settings;
        let mut shown = !settings.is_hidden_itself(ns);
        let by_parent = shown && !settings.is_shown(ns);
        let dim = (!shown || by_parent).then(|| ui.push_style_var(StyleVar::Alpha(0.5)));
        if ui.checkbox(format!("##show:{ns}"), &mut shown) {
            *clicked = Some(Click::Show(ns.to_owned(), shown));
        }
        hover_hint(
            ui,
            if by_parent {
                "Hidden in the preview with the category above it."
            } else {
                "Show its markers and trails in the preview (world, minimap, map).
                 Only for you: the pack doesn't change."
            },
        );
        ui.same_line();
        let mut node = TreeNode::new(format!("c:{ns}"))
            .label::<&str, &str>(&label)
            .flags(flags);
        if !self_matches {
            // Open the way to the matches.
            node = node.opened(true, Condition::Always);
        }
        let node = node.push(ui);
        if let Some(dim) = dim {
            dim.pop();
        }
        if ui.is_item_clicked() && !ui.is_item_toggled_open() {
            *clicked = Some(Click::Select(Some(Selection::Category(ns.to_owned()))));
        }
        if category.name != category.display_name() {
            hover_hint(ui, ns);
        }
        if let Some(_node) = node {
            let below = if self_matches { "" } else { filter };
            for child in &category.children {
                self.category(
                    ui,
                    child,
                    &namespace::join(ns, &child.name),
                    below,
                    clicked,
                    order,
                );
            }
            if self_matches {
                self.items(ui, &key, clicked, order);
            }
        }
    }

    /// The markers and trails directly in the category `key` (lowercase namespace).
    fn items(&self, ui: &Ui, key: &str, clicked: &mut Option<Click>, order: &mut Vec<usize>) {
        let pack = &self.editor.pack;
        let recording = self.editor.recording().map(|(i, _)| i);
        let selected_pois = self.editor.selected_pois();
        for &i in self.index.pois.get(key).into_iter().flatten() {
            let poi = &pack.pois[i];
            let label = match attrs::get(&poi.attrs, "tip-name") {
                Some(name) => format!("Marker #{}  {name}", i + 1),
                None => format!(
                    "Marker #{}  ({:.0}, {:.0}, {:.0})",
                    i + 1,
                    poi.pos[0],
                    poi.pos[1],
                    poi.pos[2]
                ),
            };
            order.push(i);
            let selected = selected_pois.is_some_and(|set| set.contains(&i));
            if leaf(ui, &format!("p:{i}"), &label, selected) {
                *clicked = Some(Click::Poi(i));
            }
            hover_hint(ui, SELECT_HINT);
        }
        for &i in self.index.trails.get(key).into_iter().flatten() {
            let trail = &pack.trails[i];
            let mut label = format!(
                "Trail #{}  {} points, {} m",
                i + 1,
                trail.point_count(),
                trail.length().round()
            );
            if recording == Some(i) {
                label.push_str("  (recording)");
            }
            let selected = self.editor.selection == Some(Selection::Trail(i));
            if leaf(ui, &format!("t:{i}"), &label, selected) {
                *clicked = Some(Click::Select(Some(Selection::Trail(i))));
            }
        }
    }
}

fn matches_below(category: &Category, ns: &str, filter: &str) -> bool {
    category.children.iter().any(|child| {
        let child_ns = namespace::join(ns, &child.name);
        child_ns.to_lowercase().contains(filter)
            || child.display_name().to_lowercase().contains(filter)
            || matches_below(child, &child_ns, filter)
    })
}

fn counts(pois: usize, trails: usize) -> String {
    match (pois, trails) {
        (0, 0) => String::new(),
        (p, 0) => format!("({p})"),
        (0, t) => format!("({t} trail{})", if t == 1 { "" } else { "s" }),
        (p, t) => format!("({p}, {t} trail{})", if t == 1 { "" } else { "s" }),
    }
}

/// A tree node with an ID that doesn't change with its label.
fn tree_node<'ui>(
    ui: &'ui Ui,
    id: &str,
    label: &str,
    flags: TreeNodeFlags,
) -> Option<TreeNodeToken<'ui>> {
    TreeNode::new(id)
        .label::<&str, &str>(label)
        .flags(flags)
        .push(ui)
}

/// A selectable leaf of the tree. Returns whether it was clicked.
fn leaf(ui: &Ui, id: &str, label: &str, selected: bool) -> bool {
    let mut flags = TreeNodeFlags::LEAF
        | TreeNodeFlags::NO_TREE_PUSH_ON_OPEN
        | TreeNodeFlags::SPAN_AVAIL_WIDTH
        | TreeNodeFlags::FRAME_PADDING
        | TreeNodeFlags::BULLET;
    if selected {
        flags |= TreeNodeFlags::SELECTED;
    }
    let _ = tree_node(ui, id, label, flags);
    ui.is_item_clicked()
}
