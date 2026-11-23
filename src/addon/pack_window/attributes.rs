//! Attribute editors shared by the details: the image picker (`iconFile`, `texture`) and the
//! visibility checkboxes.

use super::State;
use crate::{
    addon::{
        thumbnails,
        ui::{YELLOW, hover_hint},
    },
    attrs::{self, Attrs},
    editor::{Editor, Selection},
    files::resolve,
    images::{self, IMAGES_DIR},
    view::{Visibility, parse_flag},
};
use nexus::imgui::{ChildWindow, Selectable, Ui};

pub(super) const POPUP_IMAGES: &str = "##image_picker";
const THUMB: f32 = 32.0;

/// "Show: in the world / on the map / on the minimap", the three `*Visibility` attributes.
/// Ticking a box back to what the categories give removes the attribute, so it's inherited
/// again.
pub(super) fn visibility_row(ui: &Ui, editor: &mut Editor, target: &Selection) {
    ui.text("Show");
    hover_hint(
        ui,
        "Where the overlays draw it: inGameVisibility, mapVisibility, miniMapVisibility.\n\
         All on by default, and inherited from the categories.",
    );
    for (attr, label) in
        Visibility::ATTRS
            .into_iter()
            .zip(["in the world", "on the map", "on the minimap"])
    {
        ui.same_line();
        let values = flag_values(editor, target, attr);
        let mut value = values.first().copied().unwrap_or(true);
        let mixed = values.iter().any(|&v| v != value);
        let label = if mixed {
            format!("{label} (mixed)##{attr}")
        } else {
            format!("{label}##{attr}")
        };
        if ui.checkbox(label, &mut value) {
            set_flag(editor, target, attr, value);
        }
    }
}

/// The effective value of a boolean attribute for each item of `target`.
fn flag_values(editor: &Editor, target: &Selection, attr: &str) -> Vec<bool> {
    let pack = &editor.pack;
    let item = |category: &str, attrs: &[(String, String)]| {
        pack.inherited(category, attrs, attr).is_none_or(parse_flag)
    };
    match target {
        Selection::Category(ns) => pack
            .category(ns)
            .map(|c| vec![item(parent(ns), &c.attrs)])
            .unwrap_or_default(),
        Selection::Pois(set) => set
            .iter()
            .filter_map(|&i| pack.pois.get(i))
            .map(|p| item(&p.category, &p.attrs))
            .collect(),
        Selection::Trail(i) => pack
            .trails
            .get(*i)
            .map(|t| vec![item(&t.category, &t.attrs)])
            .unwrap_or_default(),
    }
}

fn set_flag(editor: &mut Editor, target: &Selection, attr: &str, value: bool) {
    let pack = &editor.pack;
    // What each item would inherit without its own attribute.
    let inherited = |category: &str| pack.inherited(category, &[], attr).is_none_or(parse_flag);
    let apply = |attrs: &mut Attrs, inherited: bool| {
        if value == inherited {
            attrs::remove(attrs, attr);
        } else {
            attrs::set(attrs, attr, if value { "1" } else { "0" });
        }
    };
    match target {
        Selection::Category(ns) => {
            let inherited = inherited(parent(ns));
            if let Some(category) = editor.pack.category_mut(ns) {
                apply(&mut category.attrs, inherited);
            }
        }
        Selection::Pois(set) => {
            let defaults: Vec<(usize, bool)> = set
                .iter()
                .filter_map(|&i| pack.pois.get(i).map(|p| (i, inherited(&p.category))))
                .collect();
            editor.checkpoint_pois(format!("change {attr}"));
            for (i, inherited) in defaults {
                apply(&mut editor.pack.pois[i].attrs, inherited);
            }
        }
        Selection::Trail(i) => {
            let Some(default) = pack.trails.get(*i).map(|t| inherited(&t.category)) else {
                return;
            };
            apply(&mut editor.pack.trails[*i].attrs, default);
        }
    }
    editor.commit(format!(
        "Set {attr} to {}.",
        if value { "on" } else { "off" }
    ));
}

/// The parent namespace of a category (empty at the top).
pub(super) fn parent(ns: &str) -> &str {
    ns.rsplit_once('.').map_or("", |(parent, _)| parent)
}

/// The image attribute row: preview, path, Choose... and Clear.
pub(super) fn image_attr(
    ui: &Ui,
    editor: &mut Editor,
    state: &mut State,
    target: &Selection,
    attr: &'static str,
    label: &str,
    hint: &str,
) {
    let values = attr_values(editor, target, attr);
    let value = values.first().cloned().flatten();
    let mixed = values.iter().any(|v| *v != value);
    ui.text(label);
    hover_hint(ui, hint);
    ui.same_line();
    match &value {
        _ if mixed => ui.text_disabled("(mixed)"),
        Some(path) => {
            thumbnails::draw(ui, editor.dir(), path, THUMB);
            ui.same_line();
            ui.text(path);
        }
        None => ui.text_disabled("none"),
    }
    ui.same_line();
    if ui.small_button(format!("Choose...##{attr}")) {
        state.picker = Some((target.clone(), attr));
        state.picker_filter.clear();
        state.images = images::scan(editor.dir());
        thumbnails::refresh();
        ui.open_popup(POPUP_IMAGES);
    }
    if value.is_some() || mixed {
        ui.same_line();
        if ui.small_button(format!("Clear##{attr}")) {
            edit_attrs(editor, target, &format!("clear the {attr}"), |attrs| {
                attrs::remove(attrs, attr);
            });
            editor.commit(format!("Removed the {attr}."));
        }
    }
    if let Some(path) = value.filter(|_| !mixed) {
        if !resolve(editor.dir(), &path).is_some_and(|p| p.is_file()) {
            ui.text_colored(
                YELLOW,
                format!("{path} isn't in {}.", editor.dir().display()),
            );
        } else if let Some(warning) = images::path_warning(&path) {
            ui.text_colored(YELLOW, warning);
        }
    }
}

pub(super) fn image_picker(ui: &Ui, editor: &mut Editor, state: &mut State) {
    let Some(_popup) = ui.begin_popup(POPUP_IMAGES) else {
        return;
    };
    let Some((target, attr)) = state.picker.clone() else {
        ui.close_current_popup();
        return;
    };
    let folder = editor.dir().join(IMAGES_DIR);
    ui.text(format!(
        "Pick the {attr} ({} PNGs in {})",
        state.images.len(),
        folder.display()
    ));
    ui.set_next_item_width(240.0);
    ui.input_text("##picker_filter", &mut state.picker_filter)
        .hint("Filter")
        .build();
    ui.same_line();
    if ui.small_button("Rescan") {
        state.images = images::scan(editor.dir());
        thumbnails::refresh();
    }
    hover_hint(
        ui,
        "Lists the folder again, e.g. after adding or changing files.",
    );

    let current = attr_values(editor, &target, attr)
        .first()
        .cloned()
        .flatten();
    let filter = state.picker_filter.trim().to_lowercase();
    let mut picked = None;
    ChildWindow::new("##images")
        .size([520.0, 360.0])
        .border(true)
        .build(ui, || {
            if state.images.is_empty() {
                ui.text_wrapped(format!(
                    "No PNG files in {}. Put your icons and textures there, then Rescan.",
                    folder.display()
                ));
            }
            for path in state
                .images
                .iter()
                .filter(|p| filter.is_empty() || p.to_lowercase().contains(&filter))
            {
                thumbnails::draw(ui, editor.dir(), path, THUMB);
                ui.same_line();
                let name = path.strip_prefix("images/").unwrap_or(path);
                if Selectable::new(name)
                    .selected(current.as_deref() == Some(path))
                    .size([0.0, THUMB])
                    .build(ui)
                {
                    picked = Some(path.clone());
                }
            }
        });
    if let Some(path) = picked {
        edit_attrs(editor, &target, &format!("set the {attr}"), |attrs| {
            attrs::set(attrs, attr, path.as_str());
        });
        editor.commit(format!("Set {attr}=\"{path}\"."));
        ui.close_current_popup();
    }
}

/// The value of `attr` (own, not inherited) for each item of `target`.
pub(super) fn attr_values(editor: &Editor, target: &Selection, attr: &str) -> Vec<Option<String>> {
    let get = |attrs: &Attrs| attrs::get(attrs, attr).map(str::to_owned);
    match target {
        Selection::Category(ns) => editor
            .pack
            .category(ns)
            .map(|c| get(&c.attrs))
            .into_iter()
            .collect(),
        Selection::Pois(set) => set
            .iter()
            .filter_map(|&i| editor.pack.pois.get(i))
            .map(|p| get(&p.attrs))
            .collect(),
        Selection::Trail(i) => editor
            .pack
            .trails
            .get(*i)
            .map(|t| get(&t.attrs))
            .into_iter()
            .collect(),
    }
}

/// Changes the attributes of every item of `target` (undoable for markers). Call
/// [`Editor::commit`] after.
pub(super) fn edit_attrs(
    editor: &mut Editor,
    target: &Selection,
    label: &str,
    f: impl Fn(&mut Attrs),
) {
    match target {
        Selection::Category(ns) => {
            if let Some(category) = editor.pack.category_mut(ns) {
                f(&mut category.attrs);
            }
        }
        Selection::Pois(set) => {
            editor.checkpoint_pois(label);
            for &i in set {
                if let Some(poi) = editor.pack.pois.get_mut(i) {
                    f(&mut poi.attrs);
                }
            }
        }
        Selection::Trail(i) => {
            if let Some(trail) = editor.pack.trails.get_mut(*i) {
                f(&mut trail.attrs);
            }
        }
    }
}

/// The value each item of `target` would inherit for `attr` from its categories, ignoring its
/// own.
pub(super) fn inherited_values(
    editor: &Editor,
    target: &Selection,
    attr: &str,
) -> Vec<Option<String>> {
    let pack = &editor.pack;
    let get = |category: &str| pack.inherited(category, &[], attr).map(str::to_owned);
    match target {
        Selection::Category(ns) => vec![get(parent(ns))],
        Selection::Pois(set) => set
            .iter()
            .filter_map(|&i| pack.pois.get(i))
            .map(|p| get(&p.category))
            .collect(),
        Selection::Trail(i) => pack
            .trails
            .get(*i)
            .map(|t| get(&t.category))
            .into_iter()
            .collect(),
    }
}

/// The attributes the pane has no editor for yet, as `name = value` lines.
pub(super) fn other_attrs(attrs: &[(String, String)], shown: &[&str]) -> Vec<String> {
    attrs
        .iter()
        .filter(|(name, _)| !shown.iter().any(|s| s.eq_ignore_ascii_case(name)))
        .map(|(name, value)| format!("{name} = {value}"))
        .collect()
}

pub(super) fn show_other_attrs(ui: &Ui, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    ui.separator();
    ui.text_disabled("Other attributes (kept as they are):");
    for line in lines {
        ui.text_disabled(line);
    }
}
