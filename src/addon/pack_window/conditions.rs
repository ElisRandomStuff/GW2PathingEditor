//! The "Conditions" and "Scripts" sections of the details: the declarative filter attributes
//! (`crate::conditions`) and the `script-*` attributes checked against the pack's `.lua` files
//! (`crate::scripts`), plus the whole-pack script report.

use super::{
    State,
    attributes::{attr_values, edit_attrs, inherited_values},
};
use crate::{
    addon::ui::{GREEN, RED, YELLOW, hover_hint},
    attrs::{self, Attrs},
    conditions::{self, CONDITIONS, Condition, Kind},
    editor::{Editor, Selection},
    scripts::{self, HOOKS, Hook, LuaScan, Severity},
};
use nexus::imgui::{Selectable, StyleColor, TreeNodeFlags, Ui};

/// Every attribute these sections edit, so the "Other attributes" list leaves them out.
pub(super) fn edited_attrs() -> Vec<&'static str> {
    CONDITIONS
        .iter()
        .map(|c| c.attr)
        .chain(HOOKS.iter().map(|h| h.attr))
        .collect()
}

pub(super) fn conditions_section(
    ui: &Ui,
    editor: &mut Editor,
    state: &mut State,
    target: &Selection,
) {
    let set = CONDITIONS
        .iter()
        .filter(|c| effective(editor, target, c.attr).is_some())
        .count();
    let title = if set == 0 {
        "Conditions: always shown###conditions".to_owned()
    } else {
        format!("Conditions: {set} set###conditions")
    };
    if !ui.collapsing_header(title, TreeNodeFlags::empty()) {
        return;
    }
    ui.text_disabled("Shown only when every condition set here (or on its categories) holds.");
    for condition in CONDITIONS {
        let _id = ui.push_id(condition.attr);
        match condition.kind {
            Kind::Names(names) => names_row(ui, editor, target, condition, names),
            _ => {
                text_row(
                    ui,
                    editor,
                    state,
                    target,
                    condition.attr,
                    condition.label,
                    &hint(condition),
                );
                if let Some(value) = own_value(editor, target, condition.attr) {
                    problems(ui, &conditions::check(condition, &value));
                }
            }
        }
    }
    let together = conditions::check_together(|attr| effective_ref(editor, target, attr));
    problems(ui, &together);
}

fn hint(condition: &Condition) -> String {
    match conditions::support_note(condition.support) {
        Some(note) => format!("{}\n{note}\n({})", condition.hint, condition.attr),
        None => format!("{}\n({})", condition.hint, condition.attr),
    }
}

/// A list of names as checkboxes, wrapped to the pane's width.
fn names_row(
    ui: &Ui,
    editor: &mut Editor,
    target: &Selection,
    condition: &Condition,
    names: &'static [&'static [&'static str]],
) {
    let attr = condition.attr;
    let own = attr_values(editor, target, attr);
    let inherited = inherited_values(editor, target, attr);
    let effective: Vec<Option<String>> = own
        .iter()
        .zip(&inherited)
        .map(|(o, i)| o.clone().or_else(|| i.clone()))
        .collect();
    let first = effective.first().cloned().flatten().unwrap_or_default();
    let mixed = effective
        .iter()
        .any(|v| v.clone().unwrap_or_default() != first);

    ui.text(condition.label);
    hover_hint(ui, &hint(condition));
    if mixed {
        ui.same_line();
        ui.text_colored(YELLOW, "(mixed)");
    } else if own.first().is_some_and(Option::is_none) && !first.is_empty() {
        ui.same_line();
        ui.text_disabled("(from its category)");
    }
    if let Some(own) = own.first().cloned().flatten().filter(|_| !mixed) {
        let problems_found = conditions::check(condition, &own);
        problems(ui, &problems_found);
    }
    let right = ui.window_pos()[0] + ui.window_content_region_max()[0];
    let spacing = ui.clone_style().item_spacing[0];
    let mut clicked = None;
    for (index, aliases) in names.iter().enumerate() {
        let label = aliases[0];
        if index > 0 {
            let width = ui.frame_height() + spacing + ui.calc_text_size(label)[0];
            if ui.item_rect_max()[0] + spacing + width <= right {
                ui.same_line();
            }
        }
        let mut on = conditions::entries(&first).any(|e| conditions::is_name(names, index, e));
        if ui.checkbox(label, &mut on) {
            clicked = Some((index, on));
        }
    }
    if let Some((index, on)) = clicked {
        edit_each(editor, target, attr, |attrs, inherited| {
            let current = attrs::get(attrs, attr).or(inherited).unwrap_or_default();
            let value = conditions::toggle_name(names, current, index, on);
            if value.is_empty() || Some(value.as_str()) == inherited {
                attrs::remove(attrs, attr);
            } else {
                attrs::set(attrs, attr, value);
            }
        });
        editor.commit(format!("Changed {attr}."));
    }
}

/// A text box for one attribute. Empty removes it, so it's inherited again.
fn text_row(
    ui: &Ui,
    editor: &mut Editor,
    state: &mut State,
    target: &Selection,
    attr: &'static str,
    label: &str,
    hint: &str,
) {
    let own = attr_values(editor, target, attr);
    let mixed = own.iter().any(|v| Some(v) != own.first());
    let inherited = inherited_values(editor, target, attr);
    let inherited = inherited
        .first()
        .cloned()
        .flatten()
        .filter(|v| inherited.iter().all(|i| i.as_deref() == Some(v)));
    let buffer = state.text_edits.entry(attr).or_insert_with(|| {
        if mixed {
            String::new()
        } else {
            own.first().cloned().flatten().unwrap_or_default()
        }
    });
    let placeholder = match (&inherited, mixed) {
        (_, true) => "(mixed: typing sets all)".to_owned(),
        (Some(value), false) => format!("{value} (from its category)"),
        (None, false) => String::new(),
    };

    ui.text(label);
    hover_hint(ui, hint);
    ui.same_line();
    ui.set_next_item_width(-1.0);
    ui.input_text("##text", buffer).hint(&placeholder).build();
    if ui.is_item_deactivated_after_edit() {
        let value = buffer.trim().to_owned();
        edit_attrs(editor, target, &format!("change {attr}"), |attrs| {
            if value.is_empty() {
                attrs::remove(attrs, attr);
            } else {
                attrs::set(attrs, attr, value.as_str());
            }
        });
        if value.is_empty() {
            editor.commit(format!("Removed {attr}."));
        } else {
            editor.commit(format!("Set {attr}=\"{value}\"."));
        }
    }
}

pub(super) fn scripts_section(ui: &Ui, editor: &mut Editor, state: &mut State, target: &Selection) {
    let set = HOOKS
        .iter()
        .filter(|h| effective(editor, target, h.attr).is_some())
        .count();
    let title = if set == 0 {
        "Scripts (Blish HUD only): none###scripts".to_owned()
    } else {
        format!("Scripts (Blish HUD only): {set} set###scripts")
    };
    if !ui.collapsing_header(title, TreeNodeFlags::empty()) {
        return;
    }
    let scan = lua_scan(editor, state).clone();
    ui.text_disabled("Lua functions Pathing calls. TacO and TaimiHUD ignore them.");
    ui.same_line();
    if ui.small_button("Rescan scripts") {
        state.lua = None;
    }
    hover_hint(
        ui,
        &format!(
            "Reads the .lua files in {} again, e.g. after editing them.\n\
             Pathing starts at {} and loads the others with Pack:Require(\"path\").",
            editor.dir().display(),
            scripts::ENTRY
        ),
    );
    for hook in &HOOKS {
        let _id = ui.push_id(hook.attr);
        let hint = format!(
            "{}\nCalled as F({}, then the arguments written here).\nWrite: FunctionName or FunctionName(1,'text',true)\n({})",
            hook.when,
            hook.passes.join(", "),
            hook.attr
        );
        text_row(ui, editor, state, target, hook.attr, hook.label, &hint);
        if let Some(value) = own_or_inherited(editor, target, hook.attr) {
            hook_status(ui, hook, &value, &scan);
        }
    }
}

/// The result of checking one hook: what's wrong, or where the function is.
fn hook_status(ui: &Ui, hook: &Hook, value: &str, scan: &LuaScan) {
    let found = scripts::check(hook, value, scan);
    if !found.iter().any(|p| p.severity == Severity::Error)
        && let (Some(call), _) = scripts::parse_call(value)
        && let Some((file, func)) = scan.function(call.name.trim())
    {
        let args: Vec<String> = hook
            .passes
            .iter()
            .map(|p| (*p).to_owned())
            .chain(call.args.iter().map(scripts::Arg::describe))
            .collect();
        colored_wrapped(
            ui,
            GREEN,
            &format!(
                "Calls {}({}) in {}:{}",
                func.name,
                args.join(", "),
                file.path,
                func.line
            ),
        );
    }
    script_problems(ui, &found);
}

fn script_problems(ui: &Ui, found: &[scripts::Problem]) {
    for problem in found {
        let color = match problem.severity {
            Severity::Error => RED,
            Severity::Warning => YELLOW,
        };
        colored_wrapped(ui, color, &problem.text);
    }
}

/// The whole-pack report: the Lua files, and every `script-*` attribute with its status.
pub(super) fn pack_scripts(ui: &Ui, editor: &mut Editor, state: &mut State) {
    if !ui.collapsing_header("Scripts (Blish HUD only)", TreeNodeFlags::empty()) {
        return;
    }
    let scan = lua_scan(editor, state).clone();
    let report = scripts::pack_report(&editor.pack, &scan);
    if ui.small_button("Rescan scripts") {
        state.lua = None;
    }
    hover_hint(ui, "Reads the .lua files again, e.g. after editing them.");

    if scan.files.is_empty() {
        ui.text_disabled(format!(
            "No .lua files in {}. Pathing starts at {}.",
            editor.dir().display(),
            scripts::ENTRY
        ));
    }
    for file in &scan.files {
        let globals = file.functions.iter().filter(|f| !f.local).count();
        match file.load_order {
            Some(_) => ui.text(format!("{}: {globals} global function(s)", file.path)),
            None => ui.text_colored(
                YELLOW,
                format!("{}: not loaded (nothing requires it)", file.path),
            ),
        }
    }
    script_problems(ui, &scan.problems);
    ui.separator();

    if report.is_empty() {
        ui.text_disabled("No marker, trail or category calls a script.");
        return;
    }
    let errors = report
        .iter()
        .filter(|f| f.problems.iter().any(|p| p.severity == Severity::Error))
        .count();
    if errors == 0 {
        ui.text_colored(
            GREEN,
            format!(
                "{} script attribute(s), all found in the scripts.",
                report.len()
            ),
        );
    } else {
        ui.text_colored(
            RED,
            format!(
                "{errors} of {} script attribute(s) won't call anything.",
                report.len()
            ),
        );
    }
    let mut select = None;
    for (k, finding) in report.iter().enumerate() {
        let worst = finding.problems.iter().map(|p| p.severity).min();
        let mark = match worst {
            Some(Severity::Error) => "x",
            Some(Severity::Warning) => "!",
            None => "ok",
        };
        if Selectable::new(format!(
            "[{mark}] {}: {}=\"{}\"##finding{k}",
            finding.label, finding.hook.attr, finding.value
        ))
        .build(ui)
        {
            select = Some(finding.item.clone());
        }
        hover_hint(ui, "Click to select it.");
        if !finding.problems.is_empty() {
            ui.indent();
            script_problems(ui, &finding.problems);
            ui.unindent();
        }
    }
    if let Some(item) = select {
        editor.selection = Some(item);
    }
}

/// The scan of the pack's `.lua` files, made when first needed.
fn lua_scan<'a>(editor: &Editor, state: &'a mut State) -> &'a LuaScan {
    state.lua.get_or_insert_with(|| LuaScan::scan(editor.dir()))
}

fn problems(ui: &Ui, problems: &[String]) {
    for problem in problems {
        colored_wrapped(ui, YELLOW, problem);
    }
}

fn colored_wrapped(ui: &Ui, color: [f32; 4], text: &str) {
    let token = ui.push_style_color(StyleColor::Text, color);
    ui.text_wrapped(text);
    token.pop();
}

/// The own value, when all the items have the same one.
fn own_value(editor: &Editor, target: &Selection, attr: &str) -> Option<String> {
    let own = attr_values(editor, target, attr);
    let first = own.first().cloned().flatten()?;
    own.iter()
        .all(|v| v.as_deref() == Some(first.as_str()))
        .then_some(first)
}

/// Own or inherited, when all the items have the same one.
fn own_or_inherited(editor: &Editor, target: &Selection, attr: &str) -> Option<String> {
    let own = attr_values(editor, target, attr);
    let inherited = inherited_values(editor, target, attr);
    let values: Vec<Option<String>> = own
        .into_iter()
        .zip(inherited)
        .map(|(o, i)| o.or(i))
        .collect();
    let first = values.first().cloned().flatten()?;
    values
        .iter()
        .all(|v| v.as_deref() == Some(first.as_str()))
        .then_some(first)
}

/// Whether the (first) item has `attr`, its own or inherited.
fn effective(editor: &Editor, target: &Selection, attr: &str) -> Option<String> {
    effective_ref(editor, target, attr).map(str::to_owned)
}

fn effective_ref<'a>(editor: &'a Editor, target: &Selection, attr: &str) -> Option<&'a str> {
    let pack = &editor.pack;
    match target {
        Selection::Category(ns) => pack
            .category(ns)
            .and_then(|c| pack.inherited(super::attributes::parent(ns), &c.attrs, attr)),
        Selection::Pois(set) => set
            .first()
            .and_then(|&i| pack.pois.get(i))
            .and_then(|p| pack.inherited(&p.category, &p.attrs, attr)),
        Selection::Trail(i) => pack
            .trails
            .get(*i)
            .and_then(|t| pack.inherited(&t.category, &t.attrs, attr)),
    }
}

/// Changes each item's attributes, given what it inherits for the attribute being edited.
fn edit_each(
    editor: &mut Editor,
    target: &Selection,
    attr: &str,
    f: impl Fn(&mut Attrs, Option<&str>),
) {
    let inherited = inherited_values(editor, target, attr);
    match target {
        Selection::Category(ns) => {
            if let Some(category) = editor.pack.category_mut(ns) {
                f(
                    &mut category.attrs,
                    inherited.first().cloned().flatten().as_deref(),
                );
            }
        }
        Selection::Pois(set) => {
            editor.checkpoint_pois(format!("change {attr}"));
            for (&i, inherited) in set.iter().zip(&inherited) {
                if let Some(poi) = editor.pack.pois.get_mut(i) {
                    f(&mut poi.attrs, inherited.as_deref());
                }
            }
        }
        Selection::Trail(i) => {
            if let Some(trail) = editor.pack.trails.get_mut(*i) {
                f(
                    &mut trail.attrs,
                    inherited.first().cloned().flatten().as_deref(),
                );
            }
        }
    }
}
