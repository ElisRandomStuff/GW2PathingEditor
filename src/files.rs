//! File helpers.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Writes `contents` to `path` through a temporary file, so a crash mid-write can't leave a
/// truncated file behind.
pub fn write_atomic(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)
}

/// `file` (a `trailData`, `iconFile` or `texture` value, possibly from another pack) as a path
/// under `dir`, or `None` if it could point anywhere else: absolute, drive letter, `..`, or a
/// Windows device name.
pub fn resolve(dir: &Path, file: &str) -> Option<PathBuf> {
    let file = file.replace('\\', "/");
    if file.starts_with('/') || file.contains(':') {
        return None;
    }
    let mut path = dir.to_path_buf();
    let mut any = false;
    for part in file.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            part if is_device_name(part) => return None,
            part => {
                path.push(part);
                any = true;
            }
        }
    }
    any.then_some(path)
}

/// Windows opens these as devices whatever the folder or extension (`trails/aux.trl` is the
/// AUX port): reading one could block the game.
fn is_device_name(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part).trim_end();
    let stem = stem.to_ascii_uppercase();
    match stem.as_bytes() {
        b"CON" | b"PRN" | b"AUX" | b"NUL" | b"CONIN$" | b"CONOUT$" => true,
        [b'C', b'O', b'M', n] | [b'L', b'P', b'T', n] => (b'1'..=b'9').contains(n),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside() {
        let dir = Path::new("base");
        assert_eq!(
            resolve(dir, "trails/a.trl"),
            Some(dir.join("trails").join("a.trl"))
        );
        assert_eq!(
            resolve(dir, "data\\a.trl"),
            Some(dir.join("data").join("a.trl"))
        );
        assert_eq!(resolve(dir, "./com0.trl"), Some(dir.join("com0.trl")));
        for bad in [
            "",
            "/etc/a.trl",
            "C:/a.trl",
            "a.trl:stream",
            "../a.trl",
            "trails/../../a.trl",
            "trails/aux.trl",
            "CON",
            "data/Com1.png",
            "lpt9",
        ] {
            assert_eq!(resolve(dir, bad), None, "{bad}");
        }
    }
}
