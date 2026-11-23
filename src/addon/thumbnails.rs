//! Textures of the images in `images/`, loaded by Nexus from the files: previews in the pack
//! window, icons and trail textures in the world.

use crate::files::resolve;
use nexus::{
    imgui::{Image, Ui},
    texture::{Texture, get_texture, load_texture_from_file},
};
use std::{cell::RefCell, collections::HashMap, fs, path::Path, time::UNIX_EPOCH};

/// Load requests per frame, so opening the picker on a big folder doesn't flood Nexus.
const REQUESTS_PER_FRAME: usize = 8;

/// Bigger files aren't previewed.
const MAX_BYTES: u64 = 8 << 20;

#[derive(Default)]
struct Cache {
    /// Path relative to the addon dir → Nexus texture identifier, or `None` if it can't be
    /// previewed.
    textures: HashMap<String, Option<String>>,
    frame: i32,
    requests: usize,
}

thread_local! {
    /// Render thread only. Holds strings only, nothing to release on unload.
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

/// Forgets every preview, so changed files are loaded again (under new identifiers: Nexus keeps
/// the old textures).
pub fn refresh() {
    CACHE.with_borrow_mut(|cache| cache.textures.clear());
}

/// Draws the image at `rel` (relative to `dir`), scaled to fit a `size` square, or an empty
/// square while it loads or if it can't be previewed. Returns whether the image was drawn.
pub fn draw(ui: &Ui, dir: &Path, rel: &str, size: f32) -> bool {
    match texture(ui, dir, rel) {
        Some(texture) => {
            let [w, h] = texture.size();
            let scale = size / w.max(h).max(1.0);
            Image::new(texture.id(), [w * scale, h * scale]).build(ui);
            true
        }
        None => {
            ui.dummy([size, size]);
            false
        }
    }
}

/// The texture of the image at `rel` (relative to `dir`), once Nexus has loaded it.
pub fn texture(ui: &Ui, dir: &Path, rel: &str) -> Option<Texture> {
    let identifier = CACHE.with_borrow_mut(|cache| {
        let frame = ui.frame_count();
        if cache.frame != frame {
            cache.frame = frame;
            cache.requests = 0;
        }
        if let Some(identifier) = cache.textures.get(rel) {
            return identifier.clone();
        }
        if cache.requests >= REQUESTS_PER_FRAME {
            return None;
        }
        cache.requests += 1;
        let identifier = request(dir, rel);
        cache.textures.insert(rel.to_owned(), identifier.clone());
        identifier
    });
    identifier.and_then(get_texture)
}

/// Asks Nexus to load the file, under an identifier that changes when the file does.
fn request(dir: &Path, rel: &str) -> Option<String> {
    // nexus-rs panics on a NUL in an identifier.
    if rel.contains('\0') {
        return None;
    }
    let path = resolve(dir, rel)?;
    // nexus-rs panics if the path can't be converted to the system code page. An ASCII path
    // always can.
    if !path.to_str().is_some_and(|p| p.is_ascii()) {
        return None;
    }
    let meta = fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return None;
    }
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let identifier = format!("PATHING_EDITOR_IMG_{rel}_{}_{modified}", meta.len());
    if get_texture(&identifier).is_none() {
        load_texture_from_file(&identifier, &path, None);
    }
    Some(identifier)
}
