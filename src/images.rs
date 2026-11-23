//! The images players put in `<addon dir>/images/` to use as `iconFile` and `texture`.

use std::{fs, path::Path};

pub const IMAGES_DIR: &str = "images";

/// How deep subfolders of `images/` are listed.
const MAX_DEPTH: usize = 4;

/// Every `.png` under `dir/images/`, as paths relative to `dir` with forward slashes (the way
/// `iconFile` and `texture` reference them), sorted. Marker icons must be PNG for TacO
/// (docs/01 §2), so only PNGs are listed.
pub fn scan(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    walk(&dir.join(IMAGES_DIR), IMAGES_DIR, 0, &mut found);
    found.sort_by_key(|path| path.to_ascii_lowercase());
    found
}

fn walk(folder: &Path, rel: &str, depth: usize, found: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        // Names that aren't valid Unicode can't be written into the XML.
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = format!("{rel}/{name}");
        if kind.is_dir() {
            if depth < MAX_DEPTH {
                walk(&entry.path(), &path, depth + 1, found);
            }
        } else if name.to_ascii_lowercase().ends_with(".png") {
            found.push(path);
        }
    }
}

/// A problem with an image path for some overlay, if there is one.
pub fn path_warning(path: &str) -> Option<&'static str> {
    if !path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c))
    {
        Some(
            "Use only a-z, 0-9, _, - and . in file names: some overlays mishandle other characters.",
        )
    } else if path.chars().any(|c| c.is_ascii_uppercase()) {
        Some(
            "Rename the file in lowercase: the safe profile writes lowercase paths, and not every overlay matches them case-insensitively.",
        )
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanning() {
        let dir = std::env::temp_dir().join(format!(
            "pathing_editor_test_{}",
            crate::guid::new_guid().replace(['/', '+', '='], "")
        ));
        assert!(scan(&dir).is_empty(), "no folder: nothing");

        let images = dir.join(IMAGES_DIR);
        fs::create_dir_all(images.join("icons")).unwrap();
        for file in ["b.png", "A.PNG", "notes.txt", "icons/chest.png"] {
            fs::write(images.join(file), b"").unwrap();
        }
        assert_eq!(
            scan(&dir),
            vec!["images/A.PNG", "images/b.png", "images/icons/chest.png"]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn warnings() {
        assert_eq!(path_warning("images/icons/chest_1-b.png"), None);
        assert!(path_warning("images/Chest.png").is_some());
        assert!(path_warning("images/my chest.png").is_some());
        assert!(path_warning("images/café.png").is_some());
    }
}
