//! The toolbar's Import... and Export... buttons and their popups.

use super::{State, after_history};
use crate::{
    addon::ui::{YELLOW, confirm_button, hover_hint},
    archive::{self, EXPORT_DIR, IMPORT_DIR},
    editor::Editor,
};
use nexus::imgui::{Selectable, Ui};

const POPUP_IMPORT: &str = "##import_pack";
const POPUP_EXPORT: &str = "##export_pack";

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
    ui.text_colored(
        YELLOW,
        "It replaces the pack you're editing: markers, trails, images and scripts.",
    );
    ui.text_disabled(format!(
        "The current pack is saved first to {}/backup-<date>.zip: import that to get it back.",
        editor.dir().join(EXPORT_DIR).display()
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
    if editor.recording().is_some() {
        ui.text_colored(YELLOW, "Stop the trail being recorded first.");
        return;
    }
    let mut picked = None;
    for name in &state.imports {
        if confirm_button(
            ui,
            &format!("import:{name}"),
            &format!("Open##{name}"),
            &format!("Replace the pack##{name}"),
        ) {
            picked = Some(name.clone());
        }
        ui.same_line();
        ui.text(name);
    }
    if let Some(name) = picked {
        editor.import(&name);
        after_history(state);
        state.lua = None;
        state.export_name.clear();
        ui.close_current_popup();
    }
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
