//! Packs as `.zip` / `.taco` archives (the same thing, docs/01 §2): exporting the pack being
//! edited, backing up the addon folder, and importing a pack in its place.
//!
//! Archive paths are relative to the archive root, like the pack's paths are relative to the
//! addon folder, so files keep their paths both ways. Every entry name goes through
//! [`files::resolve`]: nothing is written outside the addon folder.

use crate::{
    editor::{CAPTURES_FILE, SETTINGS_FILE},
    files::{resolve, write_atomic},
    pack::{self, Category, Pack},
    scripts::LuaScan,
};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

/// Where the user puts packs to import, and where exports and backups go. Both are in the
/// addon folder, and never part of the pack.
pub const IMPORT_DIR: &str = "import";
pub const EXPORT_DIR: &str = "export";
pub const EXTENSIONS: [&str; 2] = ["zip", "taco"];

/// Limits against broken or hostile archives.
const MAX_ENTRIES: usize = 50_000;
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_XML_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_DEPTH: usize = 16;

/// Top-level names in the addon folder that belong to the addon, not to the pack.
fn is_addon_file(top: &str) -> bool {
    [IMPORT_DIR, EXPORT_DIR, SETTINGS_FILE]
        .iter()
        .any(|n| n.eq_ignore_ascii_case(top))
}

/// `name` with `\` turned into `/`, without `./` and empty segments.
fn normalize(name: &str) -> String {
    name.replace('\\', "/")
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn has_extension(name: &str, ext: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, e)| e.eq_ignore_ascii_case(ext))
}

/// `name` as a file name: letters, digits, `-`, `_`, `.` and spaces, else `_`.
pub fn safe_file_name(name: &str) -> String {
    let name: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "-_. ".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim_matches(['.', ' ']).to_owned();
    if name.is_empty() {
        "pack".to_owned()
    } else {
        name
    }
}

/// The `.zip` and `.taco` files in `<dir>/import/`, by name.
pub fn list_imports(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir.join(IMPORT_DIR)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| EXTENSIONS.iter().any(|ext| has_extension(n, ext)))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

// ---- Export ----

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExportReport {
    /// Files written next to the XML, in archive order.
    pub files: Vec<String>,
    /// Paths the pack uses that aren't in the addon folder.
    pub missing: Vec<String>,
}

/// Every file the pack refers to: icons, textures, `.trl` files, and the `.lua` scripts.
pub fn pack_files(dir: &Path, pack: &Pack) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut add = |path: &str| {
        let path = normalize(path);
        if !path.is_empty() && seen.insert(path.to_lowercase()) {
            out.push(path);
        }
    };
    let file_attr = |name: &str| {
        let name = name.strip_prefix("bh-").unwrap_or(name);
        name.eq_ignore_ascii_case("iconFile") || name.eq_ignore_ascii_case("texture")
    };
    fn walk(categories: &[Category], f: &mut impl FnMut(&[(String, String)])) {
        for category in categories {
            f(&category.attrs);
            walk(&category.children, f);
        }
    }
    let mut from_attrs = |attrs: &[(String, String)]| {
        for (name, value) in attrs {
            if file_attr(name) {
                add(value);
            }
        }
    };
    walk(&pack.categories, &mut from_attrs);
    for poi in &pack.pois {
        from_attrs(&poi.attrs);
    }
    for trail in &pack.trails {
        from_attrs(&trail.attrs);
    }
    for trail in &pack.trails {
        add(&trail.file);
    }
    for file in LuaScan::scan(dir).files {
        add(&file.path);
    }
    out
}

/// Writes the pack as an archive at `out`: the XML as `<xml_name>` at the root, then the files
/// it uses, at their paths.
pub fn export(dir: &Path, pack: &Pack, out: &Path, xml_name: &str) -> Result<ExportReport, String> {
    let mut report = ExportReport::default();
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    for path in pack_files(dir, pack) {
        match resolve(dir, &path).filter(|p| p.is_file()) {
            Some(file) => entries.push((path, file)),
            None => report.missing.push(path),
        }
    }
    write_archive(out, |zip| {
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file(xml_name, options)
            .map_err(|e| e.to_string())?;
        zip.write_all(pack.to_document().as_bytes())
            .map_err(|e| e.to_string())?;
        for (name, file) in &entries {
            add_file(zip, name, file)?;
            report.files.push(name.clone());
        }
        Ok(())
    })?;
    Ok(report)
}

/// Writes every file of the addon folder that belongs to the pack (everything but `import/`,
/// `export/` and the settings) into an archive at `out`. Returns how many.
pub fn backup(dir: &Path, out: &Path) -> Result<usize, String> {
    let files = folder_files(dir);
    write_archive(out, |zip| {
        for (name, file) in &files {
            add_file(zip, name, file)?;
        }
        Ok(())
    })?;
    Ok(files.len())
}

/// The pack's files in the addon folder, relative, with `/`. Links aren't followed.
pub fn folder_files(dir: &Path) -> Vec<(String, PathBuf)> {
    fn walk(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<(String, PathBuf)>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if prefix.is_empty() && is_addon_file(&name) {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if kind.is_symlink() {
                continue;
            } else if kind.is_dir() && depth < MAX_DEPTH {
                walk(&entry.path(), &relative, depth + 1, out);
            } else if kind.is_file() && !relative.ends_with(".tmp") {
                out.push((relative, entry.path()));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, "", 0, &mut out);
    out
}

/// Writes an archive through a temporary file, so a failure leaves no half archive at `out`.
fn write_archive(
    out: &Path,
    fill: impl FnOnce(&mut ZipWriter<File>) -> Result<(), String>,
) -> Result<(), String> {
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let mut tmp = out.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let result = (|| {
        let file = File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
        let mut zip = ZipWriter::new(file);
        fill(&mut zip)?;
        zip.finish().map_err(|e| e.to_string())?;
        fs::rename(&tmp, out).map_err(|e| format!("{}: {e}", out.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn add_file(zip: &mut ZipWriter<File>, name: &str, file: &Path) -> Result<(), String> {
    // PNGs and the like are compressed already.
    let method = if has_extension(name, "png") || has_extension(name, "jpg") {
        CompressionMethod::Stored
    } else {
        CompressionMethod::Deflated
    };
    let options = SimpleFileOptions::default()
        .compression_method(method)
        .large_file(fs::metadata(file).is_ok_and(|m| m.len() > u32::MAX as u64));
    zip.start_file(name, options).map_err(|e| e.to_string())?;
    let mut source = File::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    io::copy(&mut source, zip).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(())
}

// ---- Import ----

/// An archive read and checked, before anything in the addon folder changes.
pub struct Import {
    archive: ZipArchive<File>,
    /// The categories, markers and trails of all its XML files.
    pub pack: Pack,
    /// Entries to extract: index in the archive and path.
    files: Vec<(usize, String)>,
    pub xml_files: usize,
    /// Things the user should know (skipped entries, unreadable XML).
    pub warnings: Vec<String>,
}

/// Opens `path` and reads its XML files. Nothing is written.
pub fn read_archive(path: &Path) -> Result<Import, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| format!("{} isn't a zip archive: {e}", path.display()))?;
    if archive.len() > MAX_ENTRIES {
        return Err(format!(
            "{} has more than {MAX_ENTRIES} files.",
            path.display()
        ));
    }
    let mut warnings = Vec::new();
    let mut xml = Vec::new();
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    let mut unsafe_names = 0;
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let Ok(raw) = entry.name().map(|n| n.into_owned()) else {
            unsafe_names += 1;
            continue;
        };
        let name = normalize(&raw);
        if entry.is_symlink() || resolve(Path::new("."), &raw).is_none() {
            unsafe_names += 1;
            continue;
        }
        let top = name.split('/').next().unwrap_or_default();
        if is_addon_file(top) {
            warnings.push(format!("Skipped {name}: that path is the addon's own."));
            continue;
        }
        // Windows paths are case-insensitive: the first entry wins, like the overlays read it.
        if !seen.insert(name.to_lowercase()) {
            continue;
        }
        let size = entry.size();
        if has_extension(&name, "xml") {
            if size > MAX_XML_BYTES {
                warnings.push(format!("Skipped {name}: too big for an XML file."));
            } else {
                xml.push((i, name));
            }
            continue;
        }
        if size > MAX_FILE_BYTES {
            warnings.push(format!(
                "Skipped {name}: bigger than {} MB.",
                MAX_FILE_BYTES >> 20
            ));
            continue;
        }
        total += size;
        if total > MAX_TOTAL_BYTES {
            return Err(format!(
                "{} unpacks to more than {} GB.",
                path.display(),
                MAX_TOTAL_BYTES >> 30
            ));
        }
        files.push((i, name));
    }
    if unsafe_names > 0 {
        warnings.push(format!(
            "Skipped {unsafe_names} entr(ies) with paths outside the pack (absolute, .., links)."
        ));
    }

    // A Blish "optimized" pack (docs/01 §2.1): only these XML files count.
    let manifested = xml
        .iter()
        .any(|(_, n)| n.eq_ignore_ascii_case("_markercategories.xml"));
    if manifested {
        xml.retain(|(_, n)| {
            let n = n.to_lowercase();
            n == "_markercategories.xml"
                || n.strip_prefix("map")
                    .and_then(|r| r.strip_suffix(".xml"))
                    .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        });
    }

    let mut pack = Pack::default();
    let mut skipped = 0;
    for (i, name) in &xml {
        let mut text = String::new();
        let read = archive
            .by_index(*i)
            .map_err(|e| e.to_string())
            .and_then(|e| {
                e.take(MAX_XML_BYTES)
                    .read_to_string(&mut text)
                    .map_err(|e| e.to_string())
            });
        if let Err(e) = read {
            warnings.push(format!("Couldn't read {name}: {e}."));
            continue;
        }
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        match pack::parse_document(text) {
            Ok(parsed) => {
                skipped += parsed.skipped;
                pack.merge(parsed.pack);
            }
            Err(e) => warnings.push(format!("{name} isn't valid XML, skipped ({e}).")),
        }
    }
    if skipped > 0 {
        warnings.push(format!(
            "{skipped} incomplete POI/Trail element(s) dropped (no MapID/position or no trailData)."
        ));
    }
    if xml.is_empty() {
        return Err(format!(
            "{} has no XML file: it isn't a marker pack.",
            path.display()
        ));
    }
    Ok(Import {
        archive,
        pack,
        files,
        xml_files: xml.len(),
        warnings,
    })
}

impl Import {
    /// Deletes the pack's files from the addon folder (keeping `import/`, `export/` and the
    /// settings), then writes the archive's files and the merged XML as `captures.xml`.
    /// Returns how many files were written.
    pub fn replace_folder(mut self, dir: &Path) -> Result<usize, String> {
        clear_folder(dir)?;
        let mut written = 0;
        let mut total = 0u64;
        for (i, name) in &self.files {
            let Some(path) = resolve(dir, name) else {
                continue;
            };
            let entry = self
                .archive
                .by_index(*i)
                .map_err(|e| format!("{name}: {e}"))?;
            let n = write_entry(entry, &path).map_err(|e| format!("{name}: {e}"))?;
            total += n;
            if total > MAX_TOTAL_BYTES {
                return Err("The archive unpacks to more than it says.".to_owned());
            }
            written += 1;
        }
        pack::save_file(&dir.join(CAPTURES_FILE), &self.pack)
            .map_err(|e| format!("{CAPTURES_FILE}: {e}"))?;
        Ok(written)
    }
}

fn write_entry(entry: impl Read, path: &Path) -> io::Result<u64> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut limited = entry.take(MAX_FILE_BYTES + 1);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(io::Error::other("bigger than it says"));
    }
    write_atomic(path, &bytes)?;
    Ok(bytes.len() as u64)
}

/// Deletes the pack's files ([`folder_files`]) and the folders left empty.
fn clear_folder(dir: &Path) -> Result<(), String> {
    let files = folder_files(dir);
    let mut folders = BTreeSet::new();
    for (name, path) in &files {
        fs::remove_file(path).map_err(|e| format!("Couldn't delete {name}: {e}"))?;
        let mut parent = Path::new(name).parent();
        while let Some(p) = parent.filter(|p| !p.as_os_str().is_empty()) {
            folders.insert(p.to_path_buf());
            parent = p.parent();
        }
    }
    // Deepest first; folders that still hold something (links, other files) stay.
    for folder in folders.iter().rev() {
        let _ = fs::remove_dir(dir.join(folder));
    }
    Ok(())
}

/// `backup-YYYY-MM-DD-HHMMSS.zip` for now (UTC).
pub fn backup_name() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let t = secs % 86_400;
    format!(
        "backup-{y:04}-{m:02}-{d:02}-{:02}{:02}{:02}.zip",
        t / 3600,
        t / 60 % 60,
        t % 60
    )
}

/// Days since 1970-01-01 to a date (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The archive names a reader sees, for tests.
#[cfg(test)]
fn names(path: &Path) -> Vec<String> {
    let mut archive = ZipArchive::new(File::open(path).unwrap()).unwrap();
    (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().unwrap().into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{attrs, pack::Poi, trail::Trail};

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pathing_editor_zip_{}",
            crate::guid::new_guid().replace(['/', '+', '='], "")
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, name: &str, contents: &str) {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn dates_and_names() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_415), (2025, 11, 23));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert!(backup_name().starts_with("backup-20"));
        assert_eq!(safe_file_name(" My pack: v2 "), "My pack_ v2");
        assert_eq!(safe_file_name("../.."), "_");
        assert_eq!(safe_file_name(""), "pack");
        assert_eq!(normalize(".\\Data//a.png"), "Data/a.png");
    }

    #[test]
    fn export_then_import() {
        let dir = temp_dir();
        let mut pack = Pack::default();
        pack.ensure_category("ep.chests");
        if let Some(c) = pack.category_mut("ep") {
            attrs::set(&mut c.attrs, "iconFile", "images\\chest.png");
        }
        let mut poi = Poi::new(50, [1.0, 2.0, 3.0], "ep.chests");
        attrs::set(&mut poi.attrs, "iconFile", "images/missing.png");
        pack.pois.push(poi);
        let mut trail = Trail::new(50, "ep.chests", "trails/map50_1.trl");
        trail.points = vec![[0.0; 3], [1.0; 3]];
        crate::trail::save(&dir, &trail).unwrap();
        pack.trails.push(trail);
        write(&dir, "images/chest.png", "png");
        write(&dir, "images/unused.png", "png");
        write(&dir, "pack.lua", "function Go(m) end");
        write(&dir, "settings.ini", "x=1");
        write(&dir, "export/old.zip", "old");
        pack::save_file(&dir.join(CAPTURES_FILE), &pack).unwrap();

        let out = dir.join(EXPORT_DIR).join("ep.taco");
        let report = export(&dir, &pack, &out, "ep.xml").unwrap();
        assert_eq!(
            report.files,
            vec!["images/chest.png", "trails/map50_1.trl", "pack.lua"]
        );
        assert_eq!(report.missing, vec!["images/missing.png"]);
        assert_eq!(
            names(&out),
            vec![
                "ep.xml",
                "images/chest.png",
                "trails/map50_1.trl",
                "pack.lua"
            ]
        );

        let backup_path = dir.join(EXPORT_DIR).join("backup.zip");
        assert_eq!(backup(&dir, &backup_path).unwrap(), 5);
        let mut backed_up = names(&backup_path);
        backed_up.sort();
        assert_eq!(
            backed_up,
            vec![
                "captures.xml",
                "images/chest.png",
                "images/unused.png",
                "pack.lua",
                "trails/map50_1.trl"
            ]
        );

        // Import the export in place of the folder.
        let import = read_archive(&out).unwrap();
        assert_eq!(import.xml_files, 1);
        assert_eq!(import.pack.pois.len(), 1);
        assert!(import.warnings.is_empty(), "{:?}", import.warnings);
        assert_eq!(import.replace_folder(&dir).unwrap(), 3);
        assert!(!dir.join("images/unused.png").exists(), "replaced");
        assert!(dir.join("images/chest.png").exists());
        assert!(dir.join("settings.ini").exists(), "kept");
        assert!(dir.join("export/old.zip").exists(), "kept");
        let reloaded = pack::load_file(&dir.join(CAPTURES_FILE)).unwrap().pack;
        assert_eq!(reloaded.pois, pack.pois);
        assert_eq!(reloaded.trails[0].file, "trails/map50_1.trl");

        // And the backup brings the old folder back.
        read_archive(&backup_path)
            .unwrap()
            .replace_folder(&dir)
            .unwrap();
        assert!(dir.join("images/unused.png").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hostile_and_odd_archives() {
        let dir = temp_dir();
        let path = dir.join("odd.zip");
        let file = File::create(&path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        for (name, body) in [
            ("../evil.png", "x"),
            ("/abs.png", "x"),
            ("settings.ini", "x"),
            ("export/x.png", "x"),
            ("Data/A.png", "first"),
            ("data/a.png", "second"),
            (
                "cats.xml",
                "<OverlayData><MarkerCategory name=\"a\"/></OverlayData>",
            ),
            (
                "pois.xml",
                "<OverlayData><POIs><POI MapID=\"1\" xpos=\"0\" ypos=\"0\" zpos=\"0\" type=\"a\"/></POIs></OverlayData>",
            ),
            ("broken.xml", "<OverlayData>"),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();

        let import = read_archive(&path).unwrap();
        assert_eq!(import.xml_files, 3);
        assert_eq!(import.pack.categories.len(), 1);
        assert_eq!(import.pack.pois.len(), 1);
        assert_eq!(import.warnings.len(), 4, "{:?}", import.warnings);
        let target = dir.join("pack");
        fs::create_dir_all(&target).unwrap();
        assert_eq!(import.replace_folder(&target).unwrap(), 1);
        assert_eq!(
            fs::read_to_string(target.join("Data/A.png")).unwrap(),
            "first"
        );
        assert!(!dir.join("evil.png").exists());
        assert!(!target.join("settings.ini").exists());

        fs::write(&path, "not a zip").unwrap();
        assert!(read_archive(&path).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn listing_imports() {
        let dir = temp_dir();
        assert!(list_imports(&dir).is_empty());
        write(&dir, "import/b.TACO", "");
        write(&dir, "import/a.zip", "");
        write(&dir, "import/notes.txt", "");
        assert_eq!(list_imports(&dir), vec!["a.zip", "b.TACO"]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
