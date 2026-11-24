//! The pack editor: the pack as a tree (categories, markers, trails) and a pane to edit the
//! selection, with an image picker for icons and textures, undo/redo, copy/paste, and groups
//! of markers or trail points (Ctrl+click, Shift+click).
//!
//! - this file: the window, its toolbar and the clipboard;
//! - `archive`: the toolbar's Import... and Export... popups;
//! - `tree`: the left pane;
//! - `details`: the right pane, for the whole pack, a category, markers or a trail;
//! - `attributes`: the image picker and the visibility checkboxes used in the details;
//! - `conditions`: the filter attributes and the `script-*` attributes, checked against the
//!   pack's `.lua` files.

mod archive;
mod attributes;
mod conditions;
mod details;
mod tree;

use self::{attributes::image_picker, details::details, tree::tree};

use super::{
    Live, PACK_WINDOW_TITLE,
    ui::{hover_hint, status_line},
};
use crate::{
    clipboard::{self, Clip},
    editor::{Editor, Selection, StatusKind},
    scripts::LuaScan,
    trail,
};
use nexus::imgui::{ChildWindow, Condition, StyleVar, Ui, Window};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

const SELECT_HINT: &str = "Click to select, Ctrl+click to add or remove, Shift+click for a range.";

/// UI state of the window. Render thread only.
struct State {
    /// Only show the markers and trails of the current map.
    this_map_only: bool,
    /// Shows only the categories whose namespace or display name contains this.
    filter: String,
    /// The selection the text boxes below were filled from.
    edits_for: Option<Option<Selection>>,
    display_name: String,
    /// The category's namespace box, to rename or move it.
    rename: String,
    category: String,
    new_child: String,
    /// What the image picker sets: the item(s) and the attribute.
    picker: Option<(Selection, &'static str)>,
    picker_filter: String,
    /// `images::scan` result, refreshed when the picker opens.
    images: Vec<String>,
    /// Positions edited in the pane and not saved yet.
    moved: Option<Moved>,
    /// Last marker / point clicked without Shift, where Shift+click ranges start.
    poi_anchor: Option<usize>,
    point_anchor: Option<usize>,
    /// The section "Continue its line" continues. `None`: the one ending closest to you.
    continue_section: Option<usize>,
    /// Text boxes of the condition and script attributes, by attribute. Filled from the
    /// selection when first drawn.
    text_edits: BTreeMap<&'static str, String>,
    /// The pack's `.lua` files, read when the scripts are first shown and on "Rescan".
    lua: Option<LuaScan>,
    /// The archives in the import folder, listed when the Import popup opens.
    imports: Vec<String>,
    /// The archive picked in the Import popup, and whether to merge it rather than replace.
    import_pick: Option<String>,
    import_merge: bool,
    /// The Export popup's name, and its extension (index in `archive::EXTENSIONS`).
    export_name: String,
    export_extension: usize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            this_map_only: true,
            filter: String::new(),
            edits_for: None,
            display_name: String::new(),
            rename: String::new(),
            category: String::new(),
            new_child: String::new(),
            picker: None,
            picker_filter: String::new(),
            images: Vec::new(),
            moved: None,
            poi_anchor: None,
            point_anchor: None,
            continue_section: None,
            text_edits: BTreeMap::new(),
            lua: None,
            imports: Vec::new(),
            import_pick: None,
            import_merge: false,
            export_name: String::new(),
            export_extension: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Moved {
    Pois,
    /// Points of a trail, by the trail's index.
    Trail(usize),
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub(super) fn window(ui: &Ui, editor: &mut Editor, live: &Live, open: &mut bool) {
    STATE.with_borrow_mut(|state| {
        Window::new(PACK_WINDOW_TITLE)
            .opened(open)
            .size([900.0, 640.0], Condition::FirstUseEver)
            .build(ui, || {
                toolbar(ui, editor, state, live);
                let footer = ui.text_line_height_with_spacing() * 2.5;
                let [width, _] = ui.content_region_avail();
                ChildWindow::new("##tree")
                    .size([width * 0.42, -footer])
                    .border(true)
                    .build(ui, || tree(ui, editor, live, state));
                ui.same_line();
                ChildWindow::new("##details")
                    .size([0.0, -footer])
                    .border(true)
                    .build(ui, || {
                        details(ui, editor, live, state);
                        // Same ID scope as the "Choose..." buttons that open it.
                        image_picker(ui, editor, state);
                    });
                status_line(ui, editor);
            });
    });
}

// ---- Toolbar and clipboard ----

fn toolbar(ui: &Ui, editor: &mut Editor, state: &mut State, live: &Live) {
    let undo = editor.undo_label().map(str::to_owned);
    if dimmed_button(ui, "Undo", undo.is_none()) {
        editor.undo();
        after_history(state);
    }
    hover_hint(
        ui,
        &undo.map_or("Nothing to undo.".into(), |l| format!("Undo: {l}")),
    );
    ui.same_line();
    let redo = editor.redo_label().map(str::to_owned);
    if dimmed_button(ui, "Redo", redo.is_none()) {
        editor.redo();
        after_history(state);
    }
    hover_hint(
        ui,
        &redo.map_or("Nothing to redo.".into(), |l| format!("Redo: {l}")),
    );
    ui.same_line();
    ui.text_disabled("|");
    ui.same_line();
    if dimmed_button(ui, "Copy", !can_copy(editor)) {
        copy(ui, editor);
    }
    hover_hint(
        ui,
        "Copies the selected markers (as <POI> lines) or trail points (as x, y, z lines).",
    );
    ui.same_line();
    if ui.button("Paste") {
        paste(ui, editor, live, state, false);
    }
    hover_hint(
        ui,
        "Pastes <POI> and <Trail> lines (copied here or from another pack) where they were,\n\
         with new GUIDs, into the selected category if one is. A trail's points are read from\n\
         its .trl file: put it in the trails folder first (or at the path trailData says).\n\
         Or pastes trail points (x, y, z lines) after the selected point of the selected trail.",
    );
    ui.same_line();
    if ui.button("Paste here") {
        paste(ui, editor, live, state, true);
    }
    hover_hint(
        ui,
        "Same, but moved together so the first one is where you stand.",
    );
    ui.same_line();
    ui.text_disabled("|");
    ui.same_line();
    archive::buttons(ui, editor, state);
    ui.same_line();
    ui.text_disabled("|");
    ui.same_line();
    ui.checkbox("Show in world", &mut editor.settings.preview);
    hover_hint(ui, super::ui::PREVIEW_HINT);
    ui.same_line();
    ui.checkbox("This map only", &mut state.this_map_only);
    hover_hint(
        ui,
        &format!(
            "Only list the markers and trails of the map you are on ({}).",
            if live.in_game {
                live.map_id.to_string()
            } else {
                "not in game: all are listed".into()
            }
        ),
    );
    ui.same_line();
    ui.set_next_item_width(-1.0);
    ui.input_text("##filter", &mut state.filter)
        .hint("Filter categories")
        .build();
}

/// After an undo or redo: nothing is being moved any more, and the text boxes show the
/// restored values.
fn after_history(state: &mut State) {
    state.moved = None;
    state.edits_for = None;
}

/// A button drawn faded when it would do nothing (ImGui 1.80 has no disabled state).
fn dimmed_button(ui: &Ui, label: &str, dimmed: bool) -> bool {
    let token = dimmed.then(|| ui.push_style_var(StyleVar::Alpha(0.45)));
    let clicked = ui.button(label);
    if let Some(token) = token {
        token.pop();
    }
    clicked && !dimmed
}

fn can_copy(editor: &Editor) -> bool {
    match &editor.selection {
        Some(Selection::Pois(_)) => true,
        Some(Selection::Trail(_)) => !editor.selected_points.is_empty(),
        _ => false,
    }
}

fn copy(ui: &Ui, editor: &mut Editor) {
    match &editor.selection {
        Some(Selection::Pois(set)) => {
            let text = clipboard::pois_text(set.iter().filter_map(|&i| editor.pack.pois.get(i)));
            ui.set_clipboard_text(&text);
            let n = set.len();
            editor.set_status(StatusKind::Info, format!("Copied {n} marker(s)."));
        }
        Some(Selection::Trail(t)) if !editor.selected_points.is_empty() => {
            let Some(trail) = editor.pack.trails.get(*t) else {
                return;
            };
            let points: Vec<[f32; 3]> = editor
                .selected_points
                .iter()
                .filter_map(|&i| trail.points.get(i).copied())
                .filter(|&p| !trail::is_break(p))
                .collect();
            ui.set_clipboard_text(clipboard::points_text(&points));
            let n = points.len();
            editor.set_status(StatusKind::Info, format!("Copied {n} trail point(s)."));
        }
        _ => editor.set_status(
            StatusKind::Warning,
            "Select markers, or points of a trail, to copy.",
        ),
    }
}

fn paste(ui: &Ui, editor: &mut Editor, live: &Live, state: &mut State, here: bool) {
    let place = if here {
        match live.here() {
            Some(place) => Some(place),
            None => {
                editor.set_status(StatusKind::Warning, "Not in game: can't paste here.");
                return;
            }
        }
    } else {
        None
    };
    let text = ui.clipboard_text().unwrap_or_default();
    match clipboard::parse(&text) {
        Some(Clip::Items(pois, trails)) => {
            let category = match &editor.selection {
                Some(Selection::Category(ns)) => Some(ns.clone()),
                _ => None,
            };
            state.poi_anchor = None;
            // Both set the status: keep both messages.
            let mut messages = Vec::new();
            let mut warning = false;
            let mut take_status = |editor: &mut Editor| {
                if let Some(status) = editor.status.take() {
                    warning |= status.kind != StatusKind::Info;
                    messages.push(status.text);
                }
            };
            let any_pois = !pois.is_empty();
            if any_pois {
                let _ = editor.paste_pois(pois, place, category.as_deref());
                take_status(editor);
            }
            if !trails.is_empty() {
                let pasted = editor.paste_trails(trails, place, category.as_deref());
                take_status(editor);
                if !any_pois && let Ok(added) = pasted {
                    editor.selection = added.first().map(|&t| Selection::Trail(t));
                }
            }
            let kind = if warning {
                StatusKind::Warning
            } else {
                StatusKind::Info
            };
            editor.set_status(kind, messages.join(" "));
        }
        Some(Clip::Points(points)) => {
            let Some(Selection::Trail(t)) = editor.selection else {
                editor.set_status(
                    StatusKind::Warning,
                    "Select the trail to paste the points into (and a point to paste them after).",
                );
                return;
            };
            let after = editor.selected_points.last().copied();
            let _ = editor.paste_trail_points(t, after, points, place);
        }
        None => editor.set_status(
            StatusKind::Warning,
            "Nothing to paste: copy markers or trails (<POI>, <Trail> lines) or trail points (x, y, z lines) first.",
        ),
    }
}

fn delete_selection(editor: &mut Editor, state: &mut State) {
    state.moved = None;
    match editor.selection.clone() {
        Some(Selection::Pois(set)) => editor.remove_pois(&set),
        Some(Selection::Trail(t)) if !editor.selected_points.is_empty() => {
            let points = editor.selected_points.clone();
            let _ = editor.remove_trail_points(t, &points);
        }
        _ => {}
    }
}

/// Applies a click on `clicked` to `set`: Ctrl toggles it, Shift selects the range from the
/// anchor (in `order`, the order things are listed), a plain click selects only it.
fn multi_select(
    ui: &Ui,
    set: &mut BTreeSet<usize>,
    anchor: &mut Option<usize>,
    clicked: usize,
    order: &[usize],
) {
    let (ctrl, shift) = (ui.io().key_ctrl, ui.io().key_shift);
    let position = |p: usize| order.iter().position(|&o| o == p);
    if ctrl {
        if !set.insert(clicked) {
            set.remove(&clicked);
        }
        *anchor = Some(clicked);
    } else if shift
        && let Some(from) = *anchor
        && let (Some(a), Some(b)) = (position(from), position(clicked))
    {
        *set = order[a.min(b)..=a.max(b)].iter().copied().collect();
    } else {
        *set = BTreeSet::from([clicked]);
        *anchor = Some(clicked);
    }
}
