//! The details pane: what can be done with the selection.

use super::{
    Moved, SELECT_HINT, State,
    attributes::{image_attr, other_attrs, show_other_attrs, visibility_row},
    copy, delete_selection, multi_select,
};
use crate::{
    addon::{
        Live, add_poi, continue_trail, insert_trail_point, move_poi, move_pois_here,
        move_trail_point, move_trail_points_here, toggle_trail,
        ui::{RED, YELLOW, confirm_button, hover_hint, written_as},
    },
    attrs,
    editor::{Editor, Join, Selection, StatusKind},
    format::fmt_float,
    geometry::distance,
    images::IMAGES_DIR,
    namespace, trail,
    view::Visibility,
};
use nexus::imgui::{
    Drag, ListClipper, Selectable, SliderFlags, TableColumnFlags, TableColumnSetup, TableFlags, Ui,
};
use std::collections::BTreeSet;

pub(super) fn details(ui: &Ui, editor: &mut Editor, live: &Live, state: &mut State) {
    // Save dragged or typed positions once the box is let go of.
    if let Some(moved) = state.moved
        && !ui.is_any_item_active()
    {
        state.moved = None;
        match moved {
            Moved::Pois => editor.commit("Moved the marker(s)."),
            Moved::Trail(i) => editor.commit_trail(i, format!("Moved points of trail #{}.", i + 1)),
        }
    }
    if state.edits_for.as_ref() != Some(&editor.selection) {
        editor.selected_points.clear();
        state.point_anchor = None;
        state.continue_section = None;
        fill_edits(editor, state);
    }
    match editor.selection.clone() {
        None => pack_details(ui, editor, state),
        Some(Selection::Category(ns)) => category_details(ui, editor, live, state, &ns),
        Some(Selection::Pois(set)) if set.len() == 1 => {
            let i = *set.first().unwrap_or(&0);
            poi_details(ui, editor, live, state, i);
        }
        Some(Selection::Pois(set)) => group_details(ui, editor, live, state, &set),
        Some(Selection::Trail(i)) => trail_details(ui, editor, live, state, i),
    }
}

/// Fills the text boxes from the selection.
fn fill_edits(editor: &Editor, state: &mut State) {
    state.edits_for = Some(editor.selection.clone());
    state.new_child.clear();
    state.display_name.clear();
    state.category.clear();
    match &editor.selection {
        Some(Selection::Category(ns)) => {
            if let Some(category) = editor.pack.category(ns) {
                state.display_name = attrs::get(&category.attrs, "DisplayName")
                    .unwrap_or_default()
                    .to_owned();
            }
        }
        Some(Selection::Pois(set)) => {
            if let Some(poi) = set.first().and_then(|&i| editor.pack.pois.get(i)) {
                state.category = poi.category.clone();
            }
        }
        Some(Selection::Trail(i)) => {
            if let Some(trail) = editor.pack.trails.get(*i) {
                state.category = trail.category.clone();
            }
        }
        None => {}
    }
}

fn pack_details(ui: &Ui, editor: &mut Editor, state: &mut State) {
    ui.text("Whole pack");
    ui.text_disabled(format!("Saved to {}", editor.file().display()));
    ui.text_disabled(format!(
        "Images: put PNGs in {}",
        editor.dir().join(IMAGES_DIR).display()
    ));
    ui.separator();
    ui.text("New top-level category");
    new_category_input(ui, editor, state, "");
    ui.separator();
    ui.text_wrapped(
        "Select a category to rename it, set its icon or add markers, trails and subcategories \
         to it. Select a marker or trail to move it or change its icon or texture. Ctrl+click and \
         Shift+click select several markers (or trail points) to move, copy or delete together.",
    );
    ui.text_wrapped(format!(
        "To paste trails from another pack, put their .trl files in {} first, then paste their \
         <Trail> lines.",
        editor.dir().join(trail::TRAILS_DIR).display()
    ));
}

fn category_details(ui: &Ui, editor: &mut Editor, live: &Live, state: &mut State, ns: &str) {
    let Some(category) = editor.pack.category(ns) else {
        editor.selection = None;
        return;
    };
    let display = category.display_name().to_owned();
    let other_attrs = other_attrs(
        &category.attrs,
        &[&["DisplayName", "iconFile"][..], &Visibility::ATTRS[..]].concat(),
    );
    let (pois, trails) = editor.pack.count_within(ns);

    ui.text(format!("Category: {display}"));
    ui.text_disabled(format!("type=\"{ns}\""));
    ui.text_disabled(format!(
        "Holds {pois} marker(s) and {trails} trail(s), subcategories included."
    ));
    ui.separator();

    ui.text("Display name");
    ui.same_line();
    ui.set_next_item_width(-1.0);
    ui.input_text("##display_name", &mut state.display_name)
        .hint(&display)
        .build();
    if ui.is_item_deactivated_after_edit()
        && let Some(category) = editor.pack.category_mut(ns)
    {
        let name = state.display_name.trim();
        if name.is_empty() {
            attrs::remove(&mut category.attrs, "DisplayName");
        } else {
            attrs::set(&mut category.attrs, "DisplayName", name);
        }
        editor.commit(format!("Renamed {ns}."));
    }
    hover_hint(
        ui,
        "The name shown in the overlays' menus. Any text. Empty: the overlays show the name.",
    );

    let target = Selection::Category(ns.to_owned());
    image_attr(
        ui,
        editor,
        state,
        &target,
        "iconFile",
        "Icon",
        "Markers of this category and its subcategories use it unless they set their own.",
    );
    visibility_row(ui, editor, &target);
    ui.separator();

    if ui.button("Add marker here") {
        let _ = add_poi(editor, live, ns);
    }
    hover_hint(
        ui,
        "Adds a marker of this category where you stand (raised if that's set in the main window).",
    );
    ui.same_line();
    if editor.recording().is_none() {
        if ui.button("Start a trail here") {
            editor.trail_category_input = ns.to_owned();
            toggle_trail(editor, live);
        }
        hover_hint(
            ui,
            "Starts a trail of this category where you stand, recorded or placed by hand as set in the main window.",
        );
    } else {
        ui.text_disabled("(a trail is being recorded)");
    }
    ui.same_line();
    if ui.button("Select its markers") {
        let map = (state.this_map_only && live.in_game).then_some(live.map_id);
        let set: BTreeSet<usize> = editor
            .pack
            .pois
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                namespace::is_within(&p.category, ns) && map.is_none_or(|m| p.map_id == m)
            })
            .map(|(i, _)| i)
            .collect();
        if set.is_empty() {
            editor.set_status(StatusKind::Warning, "No markers to select here.");
        } else {
            editor.selection = Some(Selection::Pois(set));
        }
    }
    hover_hint(
        ui,
        "Selects every marker of this category and its subcategories (on this map if \"This map only\" is on),\n\
         to move, copy, delete or restyle them together.",
    );
    if ui.small_button("Use for captured markers") {
        editor.category_input = ns.to_owned();
        editor.set_status(
            StatusKind::Info,
            format!("The Capture marker keybind now adds {ns} markers."),
        );
    }
    ui.same_line();
    if ui.small_button("Use for new trails") {
        editor.trail_category_input = ns.to_owned();
        editor.set_status(StatusKind::Info, format!("New trails will be {ns}."));
    }
    ui.separator();

    ui.text("New subcategory");
    new_category_input(ui, editor, state, ns);
    ui.separator();

    if pois + trails == 0 {
        if confirm_button(
            ui,
            &format!("cat:{ns}"),
            "Delete category",
            "Click again to delete",
        ) {
            let _ = editor.remove_category(ns);
        }
        hover_hint(ui, "Deletes this category and its (empty) subcategories.");
    } else {
        ui.text_disabled("To delete this category, move or delete what it holds first.");
    }
    show_other_attrs(ui, &other_attrs);
}

fn new_category_input(ui: &Ui, editor: &mut Editor, state: &mut State, parent: &str) {
    ui.set_next_item_width(200.0);
    let entered = ui
        .input_text("##new_category", &mut state.new_child)
        .hint("e.g. chests")
        .enter_returns_true(true)
        .build();
    ui.same_line();
    if (ui.button("Add##new_category") || entered)
        && let Ok(ns) = editor.add_category(parent, &state.new_child)
    {
        state.new_child.clear();
        editor.selection = Some(Selection::Category(ns));
    }
    written_as(ui, &state.new_child);
}

fn poi_details(ui: &Ui, editor: &mut Editor, live: &Live, state: &mut State, i: usize) {
    let Some(poi) = editor.pack.pois.get(i) else {
        editor.selection = None;
        return;
    };
    let n = i + 1;
    let map_id = poi.map_id;
    let guid = poi.guid.clone();
    let xml = poi.poi_xml();
    let other_attrs = other_attrs(
        &poi.attrs,
        &[&["iconFile"][..], &Visibility::ATTRS[..]].concat(),
    );
    let distance = live.position_on(map_id).map(|pos| poi.distance_to(pos));

    ui.text(format!("Marker #{n}"));
    ui.text_disabled(format!("GUID {guid}"));
    match distance {
        Some(d) => ui.text(format!("Map {map_id}, {} m from you", fmt_float(d, 1))),
        None => ui.text(format!("Map {map_id}")),
    }
    if live.in_game && live.map_id != map_id {
        ui.same_line();
        ui.text_colored(YELLOW, "(not the map you are on)");
    }
    ui.separator();

    ui.text("Position");
    ui.same_line();
    ui.set_next_item_width(-1.0);
    let mut pos = editor.pack.pois[i].pos;
    if Drag::new("##pos")
        .speed(0.01)
        .display_format("%.3f")
        .build_array(ui, &mut pos)
    {
        if state.moved.is_none() {
            editor.checkpoint_pois(format!("move marker #{n}"));
            state.moved = Some(Moved::Pois);
        }
        editor.pack.pois[i].pos = pos;
    }
    hover_hint(
        ui,
        "x (east), y (up), z (north), in metres. Drag to nudge, Ctrl+click to type.",
    );
    if ui.button("Move to my position") {
        move_poi(editor, live, i);
    }
    hover_hint(
        ui,
        "Puts the marker where you stand, on your current map (raised if that's set in the main window).",
    );
    ui.separator();

    category_move(ui, editor, state, |editor, input| {
        editor.set_poi_category(i, input)
    });
    let target = Selection::poi(i);
    image_attr(
        ui,
        editor,
        state,
        &target,
        "iconFile",
        "Icon",
        "Overrides the category's icon. Empty: the category's icon (or the overlay's default).",
    );
    visibility_row(ui, editor, &target);
    ui.separator();

    if ui.button("Copy <POI>") {
        ui.set_clipboard_text(&xml);
        editor.set_status(StatusKind::Info, format!("Copied marker #{n}."));
    }
    ui.same_line();
    if ui.button("Delete marker") {
        state.moved = None;
        editor.remove_poi(i);
    }
    hover_hint(ui, "Undo brings it back.");
    show_other_attrs(ui, &other_attrs);
}

/// Several markers selected.
fn group_details(
    ui: &Ui,
    editor: &mut Editor,
    live: &Live,
    state: &mut State,
    set: &BTreeSet<usize>,
) {
    let maps: BTreeSet<u32> = set
        .iter()
        .filter_map(|&i| editor.pack.pois.get(i).map(|p| p.map_id))
        .collect();
    let n = set.len();
    ui.text(format!("{n} markers selected"));
    match maps.len() {
        1 => ui.text_disabled(format!("on map {}", maps.first().unwrap_or(&0))),
        _ => ui.text_colored(YELLOW, format!("on {} different maps", maps.len())),
    }
    ui.text_disabled(SELECT_HINT);
    ui.separator();

    ui.text("Move them all");
    ui.same_line();
    if let Some(delta) = nudge(ui, "##nudge_pois") {
        if state.moved.is_none() {
            editor.checkpoint_pois(format!("move {n} markers"));
            state.moved = Some(Moved::Pois);
        }
        editor.offset_pois(set, delta);
    }
    hover_hint(
        ui,
        "Drag one of the three boxes (x east, y up, z north) to move them all together.",
    );
    if ui.button("Move them here") {
        move_pois_here(editor, live, set);
    }
    hover_hint(
        ui,
        "Moves them together, keeping their layout, so that the first one is where you stand\n\
         (raised if that's set in the main window), on your current map.",
    );
    ui.separator();

    let set_for_move = set.clone();
    category_move(ui, editor, state, move |editor, input| {
        editor.set_pois_category(&set_for_move, input)
    });
    let target = Selection::Pois(set.clone());
    image_attr(
        ui,
        editor,
        state,
        &target,
        "iconFile",
        "Icon",
        "Sets (or clears) the icon of all of them.",
    );
    visibility_row(ui, editor, &target);
    ui.separator();

    if ui.button("Copy") {
        copy(ui, editor);
    }
    ui.same_line();
    if ui.button(format!("Delete the {n} markers")) {
        state.moved = None;
        editor.remove_pois(set);
    }
    hover_hint(ui, "Undo brings them back.");
    ui.same_line();
    if ui.button("Select none") {
        editor.selection = None;
    }
}

/// Three drag boxes that report how much they were dragged this frame (and always show 0), to
/// move a group. Typing a value is off: each keystroke would move the group again.
fn nudge(ui: &Ui, id: &str) -> Option<[f32; 3]> {
    let mut delta = [0.0f32; 3];
    ui.set_next_item_width(-1.0);
    let changed = Drag::new(id)
        .speed(0.01)
        .display_format("%+.2f")
        .flags(SliderFlags::NO_INPUT)
        .build_array(ui, &mut delta);
    (changed && delta != [0.0; 3]).then_some(delta)
}

fn trail_details(ui: &Ui, editor: &mut Editor, live: &Live, state: &mut State, i: usize) {
    let Some(trail) = editor.pack.trails.get(i) else {
        editor.selection = None;
        return;
    };
    let n = i + 1;
    let guid = trail.guid.clone();
    let xml = trail.trail_xml();
    let map_id = trail.map_id;
    let other_attrs = other_attrs(
        &trail.attrs,
        &[&["texture"][..], &Visibility::ATTRS[..]].concat(),
    );
    let recording = editor.recording().is_some_and(|(r, _)| r == i);
    let here = live.position_on(map_id);
    // Each section's last point, and how far it is from you.
    let ends: Vec<(usize, Option<f32>)> = trail
        .section_ranges()
        .iter()
        .map(|r| {
            let end = trail.points[r.end - 1];
            (r.len(), here.map(|pos| distance(pos, end)))
        })
        .collect();

    ui.text(format!("Trail #{n}"));
    ui.text_disabled(format!("{}   GUID {guid}", trail.file));
    if trail.points.is_empty() && map_id == 0 {
        ui.text_colored(YELLOW, "Its .trl file couldn't be read.");
    } else {
        ui.text(format!(
            "Map {map_id}: {} points, {} section(s), {} m",
            trail.point_count(),
            trail.section_count(),
            trail.length().round()
        ));
    }
    ui.separator();

    category_move(ui, editor, state, |editor, input| {
        editor.set_trail_category(i, input)
    });
    let target = Selection::Trail(i);
    image_attr(
        ui,
        editor,
        state,
        &target,
        "texture",
        "Texture",
        "Repeated along the trail. Empty: the category's texture (or the overlay's default).",
    );
    visibility_row(ui, editor, &target);
    ui.separator();

    if recording {
        ui.text_colored(
            RED,
            "Being recorded: see the Trails tab of the main window.",
        );
    } else if editor.recording().is_none() {
        record_more(ui, editor, live, state, i, &ends);
    }

    if ui.button("Copy <Trail>") {
        ui.set_clipboard_text(&xml);
        editor.set_status(StatusKind::Info, format!("Copied trail #{n}."));
    }
    if !recording {
        ui.same_line();
        if confirm_button(
            ui,
            &format!("trail:{guid}"),
            "Delete trail",
            "Click again to delete",
        ) {
            editor.remove_trail(i);
            return;
        }
        hover_hint(
            ui,
            "Also deletes its .trl file, if the editor created it. This can't be undone.",
        );
    }
    show_other_attrs(ui, &other_attrs);
    ui.separator();
    trail_points(ui, editor, live, state, i);
}

/// "Record more": continue a section's line, or start a new section.
fn record_more(
    ui: &Ui,
    editor: &mut Editor,
    live: &Live,
    state: &mut State,
    i: usize,
    ends: &[(usize, Option<f32>)],
) {
    // By default, the section ending closest to you (else the last one).
    let closest = ends
        .iter()
        .enumerate()
        .filter_map(|(k, &(_, d))| d.map(|d| (k, d)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(k, _)| k)
        .unwrap_or(ends.len().saturating_sub(1));
    let mut section = state
        .continue_section
        .filter(|&s| s < ends.len())
        .unwrap_or(closest);

    ui.text("Record more:");
    ui.same_line();
    if ui.button("Continue the line of") {
        continue_trail(editor, live, i, Join::Section(section));
    }
    hover_hint(
        ui,
        "Records from where you stand, connected to the end of the chosen section\n\
         (a straight line joins them if you're away from it).\n\
         Recorded or placed by hand, as set in the main window. Undo removes it all.",
    );
    ui.same_line();
    let items: Vec<String> = ends
        .iter()
        .enumerate()
        .map(|(k, &(points, d))| match d {
            Some(d) => format!(
                "section {} ({points} points, ends {} m away)",
                k + 1,
                fmt_float(d, 0)
            ),
            None => format!("section {} ({points} points)", k + 1),
        })
        .collect();
    if items.is_empty() {
        ui.text_disabled("(no points)");
    } else {
        ui.set_next_item_width(260.0);
        if ui.combo_simple_string("##continue_section", &mut section, &items) {
            state.continue_section = Some(section);
        }
    }
    ui.same_line();
    if ui.button("New section here") {
        continue_trail(editor, live, i, Join::Never);
    }
    hover_hint(
        ui,
        "Records from where you stand, as a new piece of this trail, not connected to the rest.",
    );
}

/// The trail's points: the controls for the selected ones, then the list.
fn trail_points(ui: &Ui, editor: &mut Editor, live: &Live, state: &mut State, i: usize) {
    let trail = &editor.pack.trails[i];
    let numbered = trail.numbered_points();
    let here = live.position_on(trail.map_id);
    // Forget selected points that are no longer points.
    let valid: BTreeSet<usize> = editor
        .selected_points
        .iter()
        .copied()
        .filter(|&p| trail.points.get(p).is_some_and(|&v| !trail::is_break(v)))
        .collect();
    if valid.len() != editor.selected_points.len() {
        editor.selected_points = valid;
    }

    ui.text(format!("Points ({})", numbered.len()));
    ui.same_line();
    if let Some(pos) = here {
        if ui.small_button("Select the nearest") {
            editor.selected_points = trail.nearest_point(pos).into_iter().collect();
        }
        hover_hint(ui, "Selects the point closest to where you stand.");
        ui.same_line();
    }
    if ui.small_button("Select all") {
        editor.selected_points = numbered.iter().map(|&(p, _)| p).collect();
    }
    if let Some(&first) = editor.selected_points.first() {
        ui.same_line();
        if ui.small_button("Select its section") {
            let ranges = trail.section_ranges();
            if let Some(range) = ranges.iter().find(|r| r.contains(&first)) {
                editor.selected_points = range.clone().collect();
            }
        }
        hover_hint(
            ui,
            "Selects every point of the section of the (first) selected point.",
        );
        ui.same_line();
        if ui.small_button("Select none") {
            editor.selected_points.clear();
        }
    }
    ui.text_disabled(format!(
        "{SELECT_HINT} Selected points show in blue in the world."
    ));

    let selected: Vec<usize> = editor.selected_points.iter().copied().collect();
    match selected[..] {
        [] => ui.text_disabled("Click a point below to edit it."),
        [index] => {
            let row = numbered.iter().position(|&(p, _)| p == index).unwrap_or(0);
            let section = numbered.get(row).map_or(1, |&(_, s)| s);
            ui.text(format!("Point {} (section {section})", row + 1));
            ui.same_line();
            ui.set_next_item_width(-1.0);
            let mut point = editor.pack.trails[i].points[index];
            if Drag::new("##point_pos")
                .speed(0.01)
                .display_format("%.3f")
                .build_array(ui, &mut point)
            {
                if state.moved.is_none() {
                    editor.checkpoint_trail(i, format!("move a point of trail #{}", i + 1));
                    state.moved = Some(Moved::Trail(i));
                }
                editor.pack.trails[i].points[index] = point;
            }
            hover_hint(
                ui,
                "x (east), y (up), z (north), in metres. Drag to nudge, Ctrl+click to type.",
            );
            if ui.small_button("Move to my position") {
                move_trail_point(editor, live, i, index);
            }
            hover_hint(ui, "Puts this point where you stand (your feet).");
            ui.same_line();
            if ui.small_button("Add a point after it, here")
                && let Some(new) = insert_trail_point(editor, live, i, index)
            {
                editor.selected_points = BTreeSet::from([new]);
            }
            hover_hint(
                ui,
                "Inserts a point where you stand, between this point and the next one.",
            );
            ui.same_line();
            if ui.small_button("Delete point") {
                delete_selection(editor, state);
            }
        }
        ref many => {
            let n = many.len();
            ui.text(format!("{n} points selected. Move them all"));
            ui.same_line();
            if let Some(delta) = nudge(ui, "##nudge_points") {
                if state.moved.is_none() {
                    editor.checkpoint_trail(i, format!("move {n} points of trail #{}", i + 1));
                    state.moved = Some(Moved::Trail(i));
                }
                let points = editor.selected_points.clone();
                editor.offset_trail_points(i, &points, delta);
            }
            hover_hint(
                ui,
                "Drag one of the three boxes (x east, y up, z north) to move them all together.",
            );
            if ui.small_button("Move them here") {
                let points = editor.selected_points.clone();
                move_trail_points_here(editor, live, i, &points);
            }
            hover_hint(
                ui,
                "Moves them together, keeping their layout, so that the first one is where you stand.",
            );
            ui.same_line();
            if ui.small_button("Copy them") {
                copy(ui, editor);
            }
            ui.same_line();
            if ui.small_button(format!("Delete the {n} points")) {
                delete_selection(editor, state);
            }
        }
    }

    let trail = &editor.pack.trails[i];
    let numbered = trail.numbered_points();
    let height = ui.content_region_avail()[1].max(200.0);
    let flags = TableFlags::ROW_BG
        | TableFlags::BORDERS_INNER_V
        | TableFlags::SCROLL_Y
        | TableFlags::SIZING_FIXED_FIT;
    let mut clicked = None;
    if let Some(_table) = ui.begin_table_with_sizing("##points", 4, flags, [0.0, height], 0.0) {
        ui.table_setup_scroll_freeze(0, 1);
        for (name, stretch) in [
            ("#", false),
            ("Section", false),
            ("Position (x, y, z)", true),
            ("From you", false),
        ] {
            let mut column = TableColumnSetup::new(name);
            if stretch {
                column.flags = TableColumnFlags::WIDTH_STRETCH;
            }
            ui.table_setup_column_with(column);
        }
        ui.table_headers_row();

        let mut clipper = ListClipper::new(numbered.len() as i32).begin(ui);
        while clipper.step() {
            for row in clipper.display_start()..clipper.display_end() {
                let (index, section) = numbered[row as usize];
                let point = trail.points[index];
                ui.table_next_row();
                ui.table_next_column();
                if Selectable::new(format!("{}##point{index}", row + 1))
                    .selected(editor.selected_points.contains(&index))
                    .span_all_columns(true)
                    .build(ui)
                {
                    clicked = Some(index);
                }
                ui.table_next_column();
                ui.text(section.to_string());
                ui.table_next_column();
                ui.text(format!("{:.2}, {:.2}, {:.2}", point[0], point[1], point[2]));
                ui.table_next_column();
                match here {
                    Some(pos) => ui.text(format!("{} m", fmt_float(distance(pos, point), 1))),
                    None => ui.text_disabled("-"),
                }
            }
        }
    }
    if let Some(index) = clicked {
        let order: Vec<usize> = numbered.iter().map(|&(p, _)| p).collect();
        multi_select(
            ui,
            &mut editor.selected_points,
            &mut state.point_anchor,
            index,
            &order,
        );
    }
}

/// The category box of markers or a trail, with a button to move them there.
fn category_move(
    ui: &Ui,
    editor: &mut Editor,
    state: &mut State,
    apply: impl FnOnce(&mut Editor, &str) -> Result<(), String>,
) {
    ui.text("Category");
    ui.same_line();
    ui.set_next_item_width(-70.0);
    let entered = ui
        .input_text("##item_category", &mut state.category)
        .enter_returns_true(true)
        .build();
    ui.same_line();
    if (ui.button("Move##category") || entered) && apply(editor, &state.category).is_ok() {
        fill_edits(editor, state);
    }
    hover_hint(
        ui,
        "Type a namespace (e.g. mypack.chests): missing categories are created.",
    );
}
