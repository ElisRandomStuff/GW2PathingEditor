//! The toolbar's Import... and Export... buttons and their popups.

use super::{State, after_history};
use crate::{
    addon::ui::{YELLOW, confirm_button, hover_hint},
    archive::{self, EXPORT_DIR, IMPORT_DIR, LIBRARY_DIR},
    editor::Editor,
};
use nexus::imgui::{Selectable, Ui};

const POPUP_IMPORT: &str = "##import_pack";
const POPUP_EXPORT: &str = "##export_pack";
const POPUP_BACKUP: &str = "Back up?##import_backup";

/// The two buttons, on the toolbar's line.
pub(super) fn buttons(ui: &Ui, editor: &mut Editor, state: &mut State) {
    if ui.button("Import...") {
        state.imports = archive::list_imports(editor.dir());
        ui.open_popup(POPUP_IMPORT);
    }
    hover_hint(
        ui,
        "Opens a .zip or .taco pack from the import folder in place of this one\n\
         (this one is backed up first).",
    );
    ui.same_line();
    if ui.button("Export...") {
        if state.export_name.is_empty() {
            state.export_name = default_name(editor);
        }
        ui.open_popup(POPUP_EXPORT);
    }
    hover_hint(
        ui,
        "Writes this pack as a .zip or .taco, ready for TacO, Blish HUD or TaimiHUD.",
    );
    import_popup(ui, editor, state);
    export_popup(ui, editor, state);
}

/// The first top-level category's name, a likely pack name.
fn default_name(editor: &Editor) -> String {
    editor
        .pack
        .categories
        .first()
        .map(|c| c.display_name().to_owned())
        .unwrap_or_else(|| "mypack".to_owned())
}

fn import_popup(ui: &Ui, editor: &mut Editor, state: &mut State) {
    let Some(_popup) = ui.begin_popup(POPUP_IMPORT) else {
        return;
    };
    let folder = editor.dir().join(IMPORT_DIR);
    ui.text("Import a pack");
    ui.text_wrapped(format!(
        "Put .zip or .taco packs in {}, then pick one here.",
        folder.display()
    ));
    if ui.small_button("Rescan") {
        state.imports = archive::list_imports(editor.dir());
    }
    hover_hint(
        ui,
        "Lists the import folder again, e.g. after adding a file.",
    );
    ui.separator();
    if state.imports.is_empty() {
        ui.text_disabled("No .zip or .taco files there.");
    }
    if state
        .import_pick
        .as_ref()
        .is_some_and(|p| !state.imports.contains(p))
    {
        state.import_pick = None;
    }
    for name in &state.imports {
        if Selectable::new(name)
            .selected(state.import_pick.as_ref() == Some(name))
            // Picking one isn't the end of the popup: the mode and Import come next.
            .close_popups(false)
            .build(ui)
        {
            state.import_pick = Some(name.clone());
        }
    }
    ui.separator();

    if ui.radio_button_bool("Replace the current pack", !state.import_merge) {
        state.import_merge = false;
    }
    hover_hint(
        ui,
        &format!(
            "Deletes the current pack's markers, trails, images and scripts, and opens this one instead.\n\
             {} and the settings stay.",
            LIBRARY_DIR
        ),
    );
    if ui.radio_button_bool("Add it to the current pack", state.import_merge) {
        state.import_merge = true;
    }
    hover_hint(
        ui,
        "Merges its categories, markers and trails into the pack. Its files go next to yours:\n\
         one that differs from yours at the same path is added under a new name (its items use that),\n\
         except scripts and library files, where yours are kept (merge those by hand).\n\
         Items whose GUID the pack already uses get a new one.",
    );
    if state.import_merge {
        ui.text_disabled("Your markers and trails stay; the pack's are added.");
    } else {
        ui.text_colored(
            YELLOW,
            "Your markers, trails, images and scripts are replaced by the pack's.",
        );
    }
    ui.text_disabled(format!(
        "Your library ({LIBRARY_DIR}/) is never deleted or overwritten."
    ));

    if editor.recording().is_some() {
        ui.text_colored(YELLOW, "Stop the trail being recorded first.");
        return;
    }
    let Some(name) = state.import_pick.clone() else {
        ui.text_disabled("Pick a pack above.");
        return;
    };
    if ui.button(format!("Import {name}...")) {
        ui.open_popup(POPUP_BACKUP);
    }
    let backup_dir = editor.dir().join(EXPORT_DIR);
    let choice = ui
        .popup_modal(POPUP_BACKUP)
        .always_auto_resize(true)
        .build(ui, || backup_dialog(ui, &backup_dir))
        .flatten();
    if let Some(backup) = choice {
        editor.import(&name, state.import_merge, backup);
        after_history(state);
        state.lua = None;
        state.export_name.clear();
        state.import_pick = None;
        ui.close_current_popup();
    }
}

/// "Back up the current pack first?": `Some(true)` back up, `Some(false)` don't (after a second
/// click), `None` not decided yet (or cancelled).
fn backup_dialog(ui: &Ui, backup_dir: &std::path::Path) -> Option<bool> {
    ui.text("Back up the current pack first?");
    ui.text_disabled(format!(
        "Its files go to {}/backup-<date>.zip. Import that to get them back.",
        backup_dir.display()
    ));
    let mut choice = None;
    if ui.button("Yes, back up") {
        choice = Some(true);
    }
    ui.same_line();
    if confirm_button(
        ui,
        "import:no_backup",
        "No backup",
        "Sure? Unsaved work can't be recovered. Click again",
    ) {
        choice = Some(false);
    }
    ui.same_line();
    if ui.button("Cancel") {
        ui.close_current_popup();
    }
    if choice.is_some() {
        ui.close_current_popup();
    }
    choice
}

fn export_popup(ui: &Ui, editor: &mut Editor, state: &mut State) {
    let Some(_popup) = ui.begin_popup(POPUP_EXPORT) else {
        return;
    };
    ui.text("Export the pack");
    ui.text_disabled("The XML, plus the images, .trl files and .lua scripts it uses.");
    ui.set_next_item_width(260.0);
    ui.input_text("Name##export_name", &mut state.export_name)
        .build();
    ui.text("As");
    for (k, extension) in archive::EXTENSIONS.iter().enumerate() {
        ui.same_line();
        if Selectable::new(format!(".{extension}"))
            .selected(state.export_extension == k)
            .size([50.0, 0.0])
            .close_popups(false)
            .build(ui)
        {
            state.export_extension = k;
        }
    }
    hover_hint(
        ui,
        "The same file either way: .taco is the name TacO users expect, .zip opens anywhere.",
    );
    let extension = archive::EXTENSIONS[state.export_extension.min(1)];
    let name = archive::safe_file_name(&state.export_name);
    let out = editor
        .dir()
        .join(EXPORT_DIR)
        .join(format!("{name}.{extension}"));
    ui.text_disabled(format!("To {}", out.display()));
    let export = if out.exists() {
        confirm_button(
            ui,
            &format!("export:{}", out.display()),
            "Export",
            "Overwrite it",
        )
    } else {
        ui.small_button("Export")
    };
    if out.exists() {
        ui.same_line();
        ui.text_colored(YELLOW, "(that file exists)");
    }
    if export {
        editor.export(&name, extension);
        ui.close_current_popup();
    }
}
