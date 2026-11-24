//! Nexus glue: registration, keybinds, live MumbleLink data. Windows only.

mod pack_window;
mod preview;
mod thumbnails;
mod ui;

use crate::{
    editor::{Editor, Join, StatusKind},
    images::{IMAGES_DIR, LIBRARY_DIR},
    view,
};
use nexus::data_link::mumble::UiState;
use nexus::{
    AddonFlags, UpdateProvider,
    alert::send_alert,
    data_link::{get_mumble_link, read_nexus_link},
    gui::{
        RenderType, deregister_close_on_escape, register_close_on_escape, register_render, render,
    },
    imgui::Ui,
    keybind::{keybind_handler, register_keybind_with_string},
    localization::set_translation,
    log::{LogLevel, log},
    paths::get_addon_dir,
    quick_access::add_quick_access,
    texture::load_texture_from_memory,
};
use std::{
    cell::Cell,
    collections::BTreeSet,
    sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

nexus::export! {
    name: "Pathing Editor",
    // Random negative id: we're not hosted on Raidcore (docs/04 §1).
    signature: -0x24dc02d4,
    load,
    unload,
    flags: AddonFlags::None,
    provider: UpdateProvider::None,
}

const ADDON_NAME: &str = "Pathing Editor";
/// Folder in `<GW2>/addons/`.
const ADDON_DIR: &str = "PathingEditor";
/// ImGui window titles. Also the close-on-escape keys, so both must match.
const WINDOW_TITLE: &str = "Pathing Editor";
const PACK_WINDOW_TITLE: &str = "Pathing Editor: Pack";

const KB_TOGGLE: &str = "KB_PATHING_EDITOR_TOGGLE";
const KB_CAPTURE: &str = "KB_PATHING_EDITOR_CAPTURE";
const KB_TRAIL: &str = "KB_PATHING_EDITOR_TRAIL";
const KB_PACK: &str = "KB_PATHING_EDITOR_PACK";
const QA_SHORTCUT: &str = "QA_PATHING_EDITOR";
const TEX_ICON: &str = "TEX_PATHING_EDITOR_ICON";
const TEX_ICON_HOVER: &str = "TEX_PATHING_EDITOR_ICON_HOVER";

/// Editor state. Only touched from the render thread, but load/unload run elsewhere.
static EDITOR: Mutex<Option<Editor>> = Mutex::new(None);
/// Window visibility. Shared with Nexus for close-on-escape, which writes through a raw pointer.
static SHOW_WINDOW: AtomicBool = AtomicBool::new(false);
static SHOW_PACK: AtomicBool = AtomicBool::new(false);
/// Set by the keybinds (input thread), consumed by the next frame (render thread).
static CAPTURE_REQUESTED: AtomicBool = AtomicBool::new(false);
static TRAIL_TOGGLE_REQUESTED: AtomicBool = AtomicBool::new(false);

fn load() {
    let editor = match get_addon_dir(ADDON_DIR) {
        Ok(dir) => {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                log(
                    LogLevel::Critical,
                    ADDON_NAME,
                    format!("couldn't create {}: {e}", dir.display()),
                );
            }
            // Where players put the images they pick icons and textures from.
            let _ = std::fs::create_dir_all(dir.join(IMAGES_DIR));
            let _ = std::fs::create_dir_all(dir.join(LIBRARY_DIR));
            Editor::open(&dir)
        }
        Err(e) => {
            log(
                LogLevel::Critical,
                ADDON_NAME,
                format!("couldn't resolve the addon directory: {e}"),
            );
            return;
        }
    };
    if let Some(status) = &editor.status {
        log(LogLevel::Warning, ADDON_NAME, &status.text);
    }
    *lock_editor() = Some(editor);

    load_texture_from_memory(TEX_ICON, include_bytes!("../../assets/icon.png"), None);
    load_texture_from_memory(
        TEX_ICON_HOVER,
        include_bytes!("../../assets/icon_hover.png"),
        None,
    );

    // Display names in Nexus' keybind list. (Unverified that Nexus shows translations there;
    // harmless otherwise.)
    set_translation(KB_TOGGLE, "en", "Pathing Editor: Show/hide window");
    set_translation(KB_CAPTURE, "en", "Pathing Editor: Capture marker");
    set_translation(
        KB_TRAIL,
        "en",
        "Pathing Editor: Trail (start/pause, or place a point in manual mode)",
    );
    set_translation(KB_PACK, "en", "Pathing Editor: Show/hide pack editor");

    let handler = keybind_handler!(|id, is_release| {
        if !is_release {
            on_keybind(id);
        }
    });
    register_keybind_with_string(KB_TOGGLE, handler, "ALT+SHIFT+P").revert_on_unload();
    // Unbound by default ("(null)"): the user picks spare keys in Nexus' keybind settings.
    register_keybind_with_string(KB_CAPTURE, handler, "(null)").revert_on_unload();
    register_keybind_with_string(KB_TRAIL, handler, "(null)").revert_on_unload();
    register_keybind_with_string(KB_PACK, handler, "(null)").revert_on_unload();

    add_quick_access(QA_SHORTCUT, TEX_ICON, TEX_ICON_HOVER, KB_TOGGLE, ADDON_NAME)
        .revert_on_unload();
    register_render(RenderType::Render, render!(render_frame)).revert_on_unload();

    // SAFETY: the flags are statics, so the pointers stay valid until `unload` deregisters them.
    register_close_on_escape(WINDOW_TITLE, unsafe { &mut *SHOW_WINDOW.as_ptr() });
    register_close_on_escape(PACK_WINDOW_TITLE, unsafe { &mut *SHOW_PACK.as_ptr() });
}

fn unload() {
    deregister_close_on_escape(WINDOW_TITLE);
    deregister_close_on_escape(PACK_WINDOW_TITLE);
    let mut editor = lock_editor();
    // Markers are saved on every change, a recording trail every few points: write the rest.
    if let Some(editor) = editor.as_mut() {
        editor.flush();
    }
    *editor = None;
}

fn lock_editor() -> MutexGuard<'static, Option<Editor>> {
    // A panic while holding the lock must not take the addon down for the rest of the session.
    EDITOR.lock().unwrap_or_else(PoisonError::into_inner)
}

fn on_keybind(id: &str) {
    match id {
        KB_TOGGLE => {
            SHOW_WINDOW.fetch_xor(true, Ordering::Relaxed);
        }
        KB_CAPTURE => CAPTURE_REQUESTED.store(true, Ordering::Relaxed),
        KB_TRAIL => TRAIL_TOGGLE_REQUESTED.store(true, Ordering::Relaxed),
        KB_PACK => toggle_pack_window(),
        _ => {}
    }
}

fn render_frame(ui: &Ui) {
    let mut guard = lock_editor();
    let Some(editor) = guard.as_mut() else { return };
    let live = Live::read();
    // First, so the world preview is drawn under the windows.
    preview::draw(ui, editor, &live, TEX_ICON);

    // The window may be closed, so the recorder's own pauses and the outcome of keybinds
    // (success or not, it's in the status) are also shown as toasts.
    if let Some(text) = editor.trail_tick(live.here()) {
        send_alert(&text);
    }
    if CAPTURE_REQUESTED.swap(false, Ordering::Relaxed) {
        capture(editor, &live);
        alert_status(editor);
    }
    if TRAIL_TOGGLE_REQUESTED.swap(false, Ordering::Relaxed) {
        toggle_trail(editor, &live);
        alert_status(editor);
    }

    if SHOW_WINDOW.load(Ordering::Relaxed) {
        let mut open = true;
        ui::window(ui, editor, &live, &mut open);
        if !open {
            SHOW_WINDOW.store(false, Ordering::Relaxed);
        }
    }
    if SHOW_PACK.load(Ordering::Relaxed) {
        let mut open = true;
        pack_window::window(ui, editor, &live, &mut open);
        if !open {
            SHOW_PACK.store(false, Ordering::Relaxed);
        }
    }
    ui::recording_indicator(ui, editor);
    editor.save_settings_if_changed();
}

fn toggle_pack_window() {
    SHOW_PACK.fetch_xor(true, Ordering::Relaxed);
}

fn alert_status(editor: &Editor) {
    if let Some(status) = &editor.status {
        send_alert(&status.text);
    }
}

/// The character's map and position, or a warning in the status when not in game.
fn here(editor: &mut Editor, live: &Live, action: &str) -> Option<(u32, [f32; 3])> {
    let here = live.here();
    if here.is_none() {
        editor.set_status(
            StatusKind::Warning,
            format!("Not in game: can't {action} right now."),
        );
    }
    here
}

/// Captures a marker at the character's position (feet, raised if set), like TacO's "add
/// marker". The outcome is in the status.
fn capture(editor: &mut Editor, live: &Live) {
    if let Some((map_id, feet)) = here(editor, live, "capture") {
        let _ = editor.capture(map_id, feet);
    }
}

/// What the trail keybind does: start a trail, then pause or resume it, or in manual mode place
/// a point.
fn toggle_trail(editor: &mut Editor, live: &Live) {
    match editor.recording() {
        Some(_) if editor.settings.trail_manual => add_trail_point(editor, live),
        Some((_, false)) => editor.pause_trail(),
        Some((_, true)) => {
            if let Some((map_id, pos)) = here(editor, live, "resume the trail") {
                let _ = editor.resume_trail(map_id, pos);
            }
        }
        None => {
            if let Some((map_id, pos)) = here(editor, live, "start a trail") {
                let _ = editor.start_trail(map_id, pos);
            }
        }
    }
}

/// Adds a trail point at the character's position, whatever the spacing.
fn add_trail_point(editor: &mut Editor, live: &Live) {
    if let Some((map_id, pos)) = here(editor, live, "place a point") {
        let _ = editor.add_trail_point(map_id, pos);
    }
}

/// Records more of an existing trail, from the character's position.
fn continue_trail(editor: &mut Editor, live: &Live, index: usize, join: Join) {
    if let Some((map_id, pos)) = here(editor, live, "record") {
        let _ = editor.continue_trail(index, map_id, pos, join);
    }
}

/// Moves a trail point to the character's position.
fn move_trail_point(editor: &mut Editor, live: &Live, index: usize, point: usize) {
    if let Some((map_id, pos)) = here(editor, live, "move the point") {
        let _ = editor.move_trail_point(index, point, map_id, pos);
    }
}

/// Adds a trail point at the character's position, after `point`. Returns its index.
fn insert_trail_point(
    editor: &mut Editor,
    live: &Live,
    index: usize,
    point: usize,
) -> Option<usize> {
    let (map_id, pos) = here(editor, live, "add a point")?;
    editor.insert_trail_point(index, point, map_id, pos).ok()
}

/// Adds a marker in `category` at the character's position. Returns its index.
fn add_poi(editor: &mut Editor, live: &Live, category: &str) -> Option<usize> {
    let (map_id, feet) = here(editor, live, "add a marker")?;
    editor.add_poi(category, map_id, feet).ok()
}

/// Moves a marker to the character's position.
fn move_poi(editor: &mut Editor, live: &Live, index: usize) {
    if let Some((map_id, feet)) = here(editor, live, "move the marker") {
        let _ = editor.move_poi(index, map_id, feet);
    }
}

/// Moves markers together so the first is at the character's position.
fn move_pois_here(editor: &mut Editor, live: &Live, indices: &BTreeSet<usize>) {
    if let Some((map_id, feet)) = here(editor, live, "move the markers") {
        let _ = editor.move_pois_here(indices, map_id, feet);
    }
}

/// Moves trail points together so the first is at the character's position.
fn move_trail_points_here(
    editor: &mut Editor,
    live: &Live,
    index: usize,
    points: &BTreeSet<usize>,
) {
    if let Some((map_id, pos)) = here(editor, live, "move the points") {
        let _ = editor.move_trail_points_here(index, points, map_id, pos);
    }
}

/// The game state the editor needs, read once per frame.
#[derive(Debug, Default)]
struct Live {
    /// In gameplay (not loading, character select or a cutscene) with a valid map.
    in_game: bool,
    map_id: u32,
    map_type: u32,
    /// Character position (feet): metres, map space, X east, Y up, Z north.
    avatar: [f32; 3],
    /// Camera position and direction, same space.
    camera: [f32; 3],
    camera_front: [f32; 3],
    /// Vertical field of view in radians, 0 if unknown.
    fov: f32,
    /// The full-screen map is open.
    map_open: bool,
    /// The minimap, or the world map while it's open.
    map: view::MapContext,
}

/// Frames between two reads of the FOV from MumbleLink's identity JSON, which rarely changes.
const FOV_REFRESH_FRAMES: u32 = 60;

thread_local! {
    /// Last FOV and interface size read, and frames since. Render thread only.
    static IDENTITY: Cell<(f32, u32, u32)> = const { Cell::new((0.0, 1, 0)) };
}

impl Live {
    /// Map and position, if in game.
    fn here(&self) -> Option<(u32, [f32; 3])> {
        self.in_game.then_some((self.map_id, self.avatar))
    }

    /// The position, if in game on `map_id`.
    fn position_on(&self, map_id: u32) -> Option<[f32; 3]> {
        self.here()
            .filter(|&(map, _)| map == map_id)
            .map(|(_, pos)| pos)
    }

    fn read() -> Self {
        let Some(mumble) = get_mumble_link() else {
            return Self::default();
        };
        let gameplay = read_nexus_link().is_some_and(|link| link.is_gameplay);
        // Read fields one by one: `read_context()` would also read the mount byte into a Rust
        // enum, which is undefined behaviour if the game ever writes an unknown mount id.
        let map_id = mumble.read_map_id();
        let camera = mumble.read_camera();

        let (mut fov, mut ui_size, age) = IDENTITY.get();
        if fov <= 0.0 || age >= FOV_REFRESH_FRAMES {
            let identity = mumble.read_identity_string();
            let identity = identity.to_string_lossy();
            fov = view::parse_fov(&identity).unwrap_or(fov);
            ui_size = view::parse_ui_size(&identity).unwrap_or(ui_size);
            IDENTITY.set((fov, ui_size, 0));
        } else {
            IDENTITY.set((fov, ui_size, age + 1));
        }
        let ui_state = mumble.read_ui_state();
        let avatar = mumble.read_avatar().position;
        let map_open = ui_state.contains(UiState::IS_MAP_OPEN);
        let [compass_w, compass_h] = mumble.read_compass_dimensions();

        Self {
            in_game: gameplay && map_id != 0,
            map_id,
            map_type: mumble.read_map_type(),
            avatar,
            camera: camera.position,
            camera_front: camera.front,
            fov,
            map_open,
            map: view::MapContext {
                map_open,
                compass_size: [f32::from(compass_w), f32::from(compass_h)],
                compass_top_right: ui_state.contains(UiState::IS_COMPASS_TOP_RIGHT),
                compass_rotating: ui_state.contains(UiState::DOES_COMPASS_HAVE_ROTATION_ENABLED),
                compass_rotation: mumble.read_compass_rotation(),
                player: mumble.read_player_position(),
                map_center: mumble.read_map_center(),
                map_scale: mumble.read_map_scale(),
                ui_size,
                avatar,
            },
        }
    }
}
