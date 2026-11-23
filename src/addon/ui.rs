//! The editor window (ImGui 1.80 through imgui-rs).

use super::{Live, WINDOW_TITLE, add_trail_point, capture, toggle_pack_window, toggle_trail};
use crate::{
    editor::{DUPLICATE_RANGE_M, Editor, RESUME_JOIN_M, StatusKind},
    format::{fmt_float, fmt_metres},
    geometry::distance,
    namespace,
    pack::{Poi, position_attrs},
    settings::{RAISE_RANGE, SPACING_RANGE},
    trail::TRAILS_DIR,
};
use nexus::imgui::{
    Condition, Drag, ItemFlag, SliderFlags, StyleColor, StyleVar, TableColumnFlags,
    TableColumnSetup, TableFlags, Ui, Window, WindowFlags,
};
use std::{cell::RefCell, ops::RangeInclusive};

pub(super) const GREEN: [f32; 4] = [0.45, 0.85, 0.45, 1.0];
pub(super) const YELLOW: [f32; 4] = [1.0, 0.8, 0.3, 1.0];
pub(super) const RED: [f32; 4] = [1.0, 0.42, 0.42, 1.0];

/// How long a destructive button stays armed, waiting for the confirming second click.
const CONFIRM_SECS: f64 = 3.0;

const RAISE_HINT: &str = "Adds this to the ypos of new markers.\n\
    TacO, Blish HUD and TaimiHUD already draw icons heightOffset (default 1.5 m)\n\
    above ypos, so with this on as well, icons float about 3 m above the ground.\n\
    It also lifts the point that trigger and info ranges are measured from.";

pub(super) const PREVIEW_HINT: &str = "Draws this pack's markers and trails in the world as you edit them.\n\
    Turn it off if another overlay shows the same pack, to avoid seeing everything twice.";

thread_local! {
    /// The destructive button armed by a first click, and `ui.time()` then. Render thread only.
    static ARMED: RefCell<Option<(String, f64)>> = const { RefCell::new(None) };
}

pub(super) fn window(ui: &Ui, editor: &mut Editor, live: &Live, open: &mut bool) {
    Window::new(WINDOW_TITLE)
        .opened(open)
        .size([620.0, 560.0], Condition::FirstUseEver)
        .build(ui, || {
            current_position(ui, editor, live);
            ui.separator();
            if let Some(_tabs) = ui.tab_bar("##modes") {
                if let Some(_tab) = ui.tab_item("Markers") {
                    markers_tab(ui, editor, live);
                }
                if let Some(_tab) = ui.tab_item("Trails") {
                    trails_tab(ui, editor, live);
                }
            }
        });
}

/// A small window showing that a trail is being recorded, so it's visible with the editor
/// closed.
pub(super) fn recording_indicator(ui: &Ui, editor: &Editor) {
    let Some((index, paused)) = editor.recording() else {
        return;
    };
    let trail = &editor.pack.trails[index];
    let [width, _] = ui.io().display_size;
    Window::new("##pathing_editor_recording")
        .position([width * 0.5, 60.0], Condition::FirstUseEver)
        .position_pivot([0.5, 0.0])
        .flags(
            WindowFlags::NO_DECORATION
                | WindowFlags::ALWAYS_AUTO_RESIZE
                | WindowFlags::NO_FOCUS_ON_APPEARING
                | WindowFlags::NO_NAV,
        )
        .bg_alpha(0.65)
        .build(ui, || {
            if editor.settings.trail_manual {
                ui.text_colored(GREEN, "Placing trail points");
            } else if paused {
                ui.text_colored(YELLOW, "Trail paused");
            } else {
                ui.text_colored(RED, "Recording trail");
            }
            ui.same_line();
            ui.text(format!(
                "#{}: {} points, {} m",
                index + 1,
                trail.point_count(),
                trail.length().round()
            ));
        });
}

fn current_position(ui: &Ui, editor: &mut Editor, live: &Live) {
    if live.in_game {
        ui.text(format!(
            "Map {}  ({})",
            live.map_id,
            map_type_name(live.map_type)
        ));
    } else {
        ui.text_colored(
            YELLOW,
            "Not in game (loading screen, character select or cutscene).",
        );
    }
    let [x, y, z] = live.avatar;
    ui.text(format!(
        "x {}     y {}     z {}",
        fmt_metres(x),
        fmt_metres(y),
        fmt_metres(z)
    ));
    let raise = editor.settings.marker_raise();
    if raise > 0.0 {
        ui.text_disabled(format!(
            "Character's feet, in metres (x east, y up, z north). Markers go {} m higher.",
            fmt_float(raise, 2)
        ));
    } else {
        ui.text_disabled("Character's feet, in metres. x = east, y = up, z = north.");
    }

    if ui.button("Copy as <POI>") {
        if live.in_game {
            let pos = editor.marker_pos(live.avatar);
            let poi = Poi::new(live.map_id, pos, editor.category()).poi_xml();
            ui.set_clipboard_text(&poi);
            editor.set_status(
                StatusKind::Info,
                "Copied a <POI> line for the current position.",
            );
        } else {
            editor.set_status(StatusKind::Warning, "Not in game: nothing to copy.");
        }
    }
    hover_hint(
        ui,
        "A full <POI> line (map, position, marker category, new GUID), without adding it to the list.",
    );
    ui.same_line();
    if ui.button("Copy xpos/ypos/zpos") {
        if live.in_game {
            ui.set_clipboard_text(position_attrs(editor.marker_pos(live.avatar)));
            editor.set_status(StatusKind::Info, "Copied the position attributes.");
        } else {
            editor.set_status(StatusKind::Warning, "Not in game: nothing to copy.");
        }
    }
    hover_hint(
        ui,
        "Only the position attributes, to move an existing marker of a pack.",
    );
    ui.same_line();
    if ui.button("Pack editor") {
        toggle_pack_window();
    }
    ui.same_line();
    ui.checkbox("Show in world", &mut editor.settings.preview);
    hover_hint(ui, PREVIEW_HINT);
    hover_hint(
        ui,
        "The whole pack as a tree: categories, markers and trails, to edit them.",
    );
}

fn markers_tab(ui: &Ui, editor: &mut Editor, live: &Live) {
    category_input(
        ui,
        "##category",
        &mut editor.category_input,
        "e.g. mypack.chests",
    );

    ui.checkbox("Raise markers by", &mut editor.settings.raise_markers);
    hover_hint(ui, RAISE_HINT);
    ui.same_line();
    // Dimmed while off (ImGui 1.80 has no BeginDisabled).
    let dim = (!editor.settings.raise_markers).then(|| ui.push_style_var(StyleVar::Alpha(0.5)));
    metres_drag(ui, "##raise_by", RAISE_RANGE, &mut editor.settings.raise_by);
    if let Some(dim) = dim {
        dim.pop();
    }
    hover_hint(ui, RAISE_HINT);

    if ui.button("Capture marker") {
        capture(editor, live);
    }
    ui.same_line();
    ui.checkbox(
        format!("Skip if the same category is within {DUPLICATE_RANGE_M} m"),
        &mut editor.settings.skip_near_duplicates,
    );
    ui.text_disabled(
        "Tip: bind \"Capture marker\" in Nexus > Keybinds to capture with the window closed.",
    );
    status_line(ui, editor);
    ui.separator();
    marker_list(ui, editor, live);
}

fn trails_tab(ui: &Ui, editor: &mut Editor, live: &Live) {
    match editor.recording() {
        None => {
            category_input(
                ui,
                "##trail_category",
                &mut editor.trail_category_input,
                "e.g. mypack.routes",
            );
            point_mode_control(ui, editor);
            let manual = editor.settings.trail_manual;
            if ui.button(if manual {
                "Start trail here"
            } else {
                "Start recording"
            }) {
                toggle_trail(editor, live);
            }
            hover_hint(
                ui,
                if manual {
                    "Starts a trail with its first point where you stand."
                } else {
                    "Records your position as you move. Waypoints and map changes pause it."
                },
            );
            ui.text_disabled(if manual {
                "Tip: bind the \"Trail\" keybind in Nexus > Keybinds: it starts the trail, then places a point each press."
            } else {
                "Tip: bind the \"Trail\" keybind in Nexus > Keybinds: it starts and pauses recording."
            });
        }
        Some((index, paused)) => recorder_controls(ui, editor, live, index, paused),
    }
    status_line(ui, editor);
    ui.separator();
    trail_list(ui, editor);
}

fn recorder_controls(ui: &Ui, editor: &mut Editor, live: &Live, index: usize, paused: bool) {
    let manual = editor.settings.trail_manual;
    let trail = &editor.pack.trails[index];
    let n = index + 1;
    if manual {
        ui.text_colored(GREEN, format!("Placing the points of trail #{n}"));
    } else if paused {
        ui.text_colored(YELLOW, format!("Trail #{n} is paused"));
    } else {
        ui.text_colored(RED, format!("Recording trail #{n}"));
    }
    ui.text(format!(
        "{}, map {}: {} points, {} m, {} section(s)",
        trail.category,
        trail.map_id,
        trail.point_count(),
        trail.length().round(),
        trail.section_count()
    ));
    let on_other_map = live.in_game && live.map_id != trail.map_id;
    let gap = live
        .position_on(trail.map_id)
        .zip(trail.last_point())
        .map(|(pos, last)| distance(last, pos));
    let trail_map = trail.map_id;

    point_mode_control(ui, editor);

    if manual {
        if ui.button("Place point") {
            add_trail_point(editor, live);
        }
        match gap {
            Some(gap) => hover_hint(
                ui,
                &format!(
                    "Adds a point where you stand, {} m from the last one.",
                    fmt_float(gap, 1)
                ),
            ),
            None => hover_hint(ui, "Not in game, or on another map."),
        }
        ui.same_line();
        if ui.button("New section") {
            editor.break_trail_section();
        }
        hover_hint(
            ui,
            "The next point starts a new piece, not connected to the last point.",
        );
    } else {
        if paused {
            if ui.button("Resume") {
                toggle_trail(editor, live);
            }
            match gap {
                Some(gap) if gap > RESUME_JOIN_M => hover_hint(
                    ui,
                    &format!(
                        "Starts a new section here: you are {} m from the last point.\n\
                         Walk back within {RESUME_JOIN_M} m of it to continue the line instead.",
                        gap.round()
                    ),
                ),
                Some(_) => hover_hint(ui, "Continues the line from the last point."),
                None => hover_hint(ui, "Not in game, or on another map."),
            }
        } else if ui.button("Pause") {
            editor.pause_trail();
        }
        ui.same_line();
        if ui.button("Add point here") {
            add_trail_point(editor, live);
        }
        hover_hint(ui, "Adds your position now, whatever the spacing.");
    }
    ui.same_line();
    let repeat = ui.push_item_flag(ItemFlag::ButtonRepeat(true));
    if ui.button("Undo point") {
        editor.undo_trail_point();
    }
    repeat.pop(ui);
    hover_hint(
        ui,
        if manual {
            "Removes the last point (or the pending new section). Hold to remove several."
        } else {
            "Removes the last point. Hold to remove several.\n\
             Pause first: while recording, moving adds points again."
        },
    );
    ui.same_line();
    if ui.button("Stop") {
        editor.stop_trail();
    }
    hover_hint(ui, "Finishes this trail. The next start begins a new one.");

    if on_other_map {
        ui.text_colored(
            YELLOW,
            format!("This trail is on map {trail_map}. Stop it to start one on this map."),
        );
    }
}

/// Automatic points every N metres, or points placed by hand.
fn point_mode_control(ui: &Ui, editor: &mut Editor) {
    ui.text("Points:");
    ui.same_line();
    if ui.radio_button_bool("every", !editor.settings.trail_manual) {
        editor.settings.trail_manual = false;
    }
    hover_hint(
        ui,
        "A point is recorded each time you are this far from the last one.\n\
         Smaller follows curves more closely and makes bigger files. TacO uses 0.76 m.",
    );
    ui.same_line();
    let dim = editor
        .settings
        .trail_manual
        .then(|| ui.push_style_var(StyleVar::Alpha(0.5)));
    metres_drag(
        ui,
        "##spacing",
        SPACING_RANGE,
        &mut editor.settings.trail_spacing,
    );
    if let Some(dim) = dim {
        dim.pop();
    }
    ui.same_line();
    if ui.radio_button_bool("placed by hand", editor.settings.trail_manual) {
        editor.settings.trail_manual = true;
    }
    hover_hint(
        ui,
        "Nothing is recorded as you move: you place each point,\n\
         with \"Place point\" or the Trail keybind.",
    );
}

/// A drag box for a distance in metres. Ctrl+click to type a value.
pub(super) fn metres_drag(ui: &Ui, id: &str, range: RangeInclusive<f32>, value: &mut f32) {
    ui.set_next_item_width(90.0);
    Drag::new(id)
        .range(*range.start(), *range.end())
        .speed(0.05)
        .display_format("%.2f m")
        .flags(SliderFlags::ALWAYS_CLAMP)
        .build(ui, value);
}

/// The category box and the warnings about what will actually be written.
pub(super) fn category_input(ui: &Ui, id: &str, input: &mut String, hint: &str) {
    ui.text("Category (type)");
    ui.same_line();
    ui.set_next_item_width(-1.0);
    ui.input_text(id, input).hint(hint).build();

    if namespace::sanitize(input).is_empty() {
        ui.text_colored(YELLOW, format!("Enter a category namespace, {hint}"));
    } else {
        written_as(ui, input);
    }
}

/// What a category typed as `input` will be written as, when that's not what was typed.
pub(super) fn written_as(ui: &Ui, input: &str) {
    let category = namespace::sanitize(input);
    if !category.is_empty() && category != input.trim() {
        ui.text_colored(
            YELLOW,
            format!("Will be written as \"{category}\" (names use a-z, 0-9 and _ only)."),
        );
    }
}

pub(super) fn status_line(ui: &Ui, editor: &Editor) {
    if let Some(status) = &editor.status {
        let color = match status.kind {
            StatusKind::Info => GREEN,
            StatusKind::Warning => YELLOW,
            StatusKind::Error => RED,
        };
        let token = ui.push_style_color(StyleColor::Text, color);
        ui.text_wrapped(&status.text);
        token.pop();
    }
}

fn marker_list(ui: &Ui, editor: &mut Editor, live: &Live) {
    ui.text(format!("Markers ({})", editor.pack.pois.len()));
    ui.same_line();
    if ui.small_button("Copy all") {
        ui.set_clipboard_text(editor.all_poi_xml());
        let n = editor.pack.pois.len();
        editor.set_status(StatusKind::Info, format!("Copied {n} <POI> line(s)."));
    }
    hover_hint(
        ui,
        "Every marker as <POI> lines, to paste into a pack's <POIs> section.",
    );
    if !editor.pack.pois.is_empty() {
        ui.same_line();
        let confirm = format!("Click again to delete {}", editor.pack.pois.len());
        if confirm_button(ui, "clear-markers", "Clear all", &confirm) {
            editor.clear_pois();
        }
    }
    if let Some(label) = editor.undo_label().map(str::to_owned) {
        ui.same_line();
        if ui.small_button("Undo") {
            editor.undo();
        }
        hover_hint(ui, &format!("Undo: {label}. Redo is in the pack editor."));
    }

    let mut copy = None;
    let mut delete = None;
    if let Some(_table) = list_table(
        ui,
        "markers",
        [
            ("#", false),
            ("Map", false),
            ("Category", true),
            ("Position (x, y, z)", false),
            ("Distance", false),
            ("", false),
        ],
    ) {
        for (i, c) in editor.pack.pois.iter().enumerate() {
            let _id = ui.push_id(i as i32);
            ui.table_next_row();
            ui.table_next_column();
            ui.text((i + 1).to_string());
            ui.table_next_column();
            ui.text(c.map_id.to_string());
            ui.table_next_column();
            ui.text(&c.category);
            ui.table_next_column();
            ui.text(format!("{:.1}, {:.1}, {:.1}", c.pos[0], c.pos[1], c.pos[2]));
            ui.table_next_column();
            if !live.in_game {
                ui.text_disabled("-");
            } else if c.map_id != live.map_id {
                ui.text_disabled("other map");
            } else {
                ui.text(format!("{:.1} m", c.distance_to(live.avatar)));
            }
            ui.table_next_column();
            if ui.small_button("Copy") {
                copy = Some(i);
            }
            ui.same_line();
            if ui.small_button("Delete") {
                delete = Some(i);
            }
        }
    }

    if let Some(i) = copy {
        ui.set_clipboard_text(editor.pack.pois[i].poi_xml());
        editor.set_status(StatusKind::Info, format!("Copied #{}.", i + 1));
    }
    if let Some(i) = delete {
        editor.remove_poi(i);
    }
    ui.text_disabled(format!("Saved to {}", editor.file().display()));
}

fn trail_list(ui: &Ui, editor: &mut Editor) {
    ui.text(format!("Trails ({})", editor.pack.trails.len()));
    ui.same_line();
    if ui.small_button("Copy all") {
        ui.set_clipboard_text(editor.all_trail_xml());
        let n = editor.pack.trails.len();
        editor.set_status(StatusKind::Info, format!("Copied {n} <Trail> line(s)."));
    }
    hover_hint(
        ui,
        "Every trail as <Trail> lines, to paste into a pack's <POIs> section.\n\
         Copy the .trl files they point to into the pack as well.",
    );

    let recording = editor.recording();
    let mut copy = None;
    let mut delete = None;
    if let Some(_table) = list_table(
        ui,
        "trails",
        [
            ("#", false),
            ("Map", false),
            ("Category", true),
            ("Points", false),
            ("Length", false),
            ("", false),
        ],
    ) {
        for (i, t) in editor.pack.trails.iter().enumerate() {
            let _id = ui.push_id(i as i32);
            ui.table_next_row();
            ui.table_next_column();
            ui.text((i + 1).to_string());
            ui.table_next_column();
            if t.map_id == 0 {
                ui.text_disabled("?");
            } else {
                ui.text(t.map_id.to_string());
            }
            ui.table_next_column();
            ui.text(&t.category);
            ui.table_next_column();
            if t.map_id == 0 && t.points.is_empty() {
                ui.text_colored(YELLOW, "no file");
                hover_hint(ui, &format!("{} couldn't be read.", t.file));
            } else {
                match t.section_count() {
                    1 => ui.text(t.point_count().to_string()),
                    s => ui.text(format!("{} ({s} sections)", t.point_count())),
                }
            }
            ui.table_next_column();
            ui.text(format!("{} m", t.length().round()));
            ui.table_next_column();
            if ui.small_button("Copy") {
                copy = Some(i);
            }
            ui.same_line();
            match recording {
                Some((r, paused)) if r == i => {
                    ui.text_disabled(if paused { "paused" } else { "recording" });
                }
                _ => {
                    if confirm_button(ui, &format!("trail:{}", t.guid), "Delete", "Sure?") {
                        delete = Some(i);
                    }
                }
            }
        }
    }

    if let Some(i) = copy {
        ui.set_clipboard_text(editor.pack.trails[i].trail_xml());
        editor.set_status(
            StatusKind::Info,
            format!(
                "Copied trail #{}. Its points are in {}.",
                i + 1,
                editor.pack.trails[i].file
            ),
        );
    }
    if let Some(i) = delete {
        editor.remove_trail(i);
    }
    ui.text_disabled(format!(
        "Points are in {} (trailData paths are relative to {}).",
        editor.dir().join(TRAILS_DIR).display(),
        editor.dir().display()
    ));
}

/// A scrolling list table that fills the space above a one-line footer.
pub(super) fn list_table<'ui>(
    ui: &'ui Ui,
    id: &str,
    columns: [(&str, bool); 6],
) -> Option<nexus::imgui::TableToken<'ui>> {
    let footer = ui.text_line_height_with_spacing();
    let flags = TableFlags::ROW_BG
        | TableFlags::BORDERS_INNER_V
        | TableFlags::RESIZABLE
        | TableFlags::SCROLL_Y
        | TableFlags::SIZING_FIXED_FIT;
    let table = ui.begin_table_with_sizing(id, columns.len(), flags, [0.0, -footer], 0.0)?;
    ui.table_setup_scroll_freeze(0, 1);
    for (name, stretch) in columns {
        let mut column = TableColumnSetup::new(name);
        if stretch {
            column.flags = TableColumnFlags::WIDTH_STRETCH;
        }
        ui.table_setup_column_with(column);
    }
    ui.table_headers_row();
    Some(table)
}

/// A small button that needs a second click within [`CONFIRM_SECS`]. `key` tells buttons
/// apart. Returns true on the confirming click.
pub(super) fn confirm_button(ui: &Ui, key: &str, label: &str, confirm_label: &str) -> bool {
    let now = ui.time();
    let armed = ARMED.with_borrow(|armed| {
        armed
            .as_ref()
            .is_some_and(|(k, at)| k == key && now - at < CONFIRM_SECS)
    });
    if armed {
        let token = ui.push_style_color(StyleColor::Text, RED);
        let clicked = ui.small_button(confirm_label);
        token.pop();
        if clicked {
            ARMED.set(None);
        }
        clicked
    } else {
        if ui.small_button(label) {
            ARMED.set(Some((key.to_owned(), now)));
        }
        false
    }
}

pub(super) fn hover_hint(ui: &Ui, text: &str) {
    if ui.is_item_hovered() {
        ui.tooltip_text(text);
    }
}

/// MumbleLink map type (docs/04 §6.1).
fn map_type_name(map_type: u32) -> &'static str {
    match map_type {
        0 => "redirect",
        1 => "character creation",
        2 => "PvP",
        3 => "GvG",
        4 => "instance",
        5 => "public",
        6 => "tournament",
        7 => "tutorial",
        8 => "user tournament",
        9 => "WvW Eternal Battlegrounds",
        10 => "WvW Blue Borderlands",
        11 => "WvW Green Borderlands",
        12 => "WvW Red Borderlands",
        13 => "WvW Fortune's Vale",
        14 => "WvW Obsidian Sanctum",
        15 => "WvW Edge of the Mists",
        16 => "public mini",
        17 => "big battle",
        18 => "WvW lounge",
        _ => "unknown map type",
    }
}
