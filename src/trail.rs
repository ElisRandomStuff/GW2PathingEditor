//! Trails: `<Trail>` elements (docs/01 §7) and the `.trl` point files they reference (§8).

use crate::{
    attrs::Attrs,
    files::{resolve, write_atomic},
    format::escape_attr,
    geometry::distance,
    guid,
    pack::write_attrs,
};
use std::{fmt::Write as _, fs, io, path::Path};

/// Folder of the addon directory that recorded `.trl` files go in. `trailData` paths are
/// relative to the addon directory, the way they are relative to a pack's root, so
/// `captures.xml` and this folder together form a working pack.
pub const TRAILS_DIR: &str = "trails";

/// Bigger `.trl` files aren't read: 64 MiB is over 5 million points.
const MAX_TRL_BYTES: u64 = 64 << 20;

/// The point that separates two sections of a trail (docs/01 §8).
pub const SECTION_BREAK: [f32; 3] = [0.0; 3];

/// Fade distances (game units, i.e. inches) written on new trails, from TacO's editor. The
/// overlays' defaults disagree, so the safe writer profile always writes them (docs/01 §14.9).
const DEFAULT_FADE: [(&str, &str); 2] = [("fadeNear", "5000"), ("fadeFar", "5100")];

#[derive(Debug, Clone, PartialEq)]
pub struct Trail {
    /// GW2 API map id, from the `.trl` header. 0 if the file couldn't be read.
    pub map_id: u32,
    /// Category namespace written to `type`. Empty means no `type` attribute.
    pub category: String,
    /// `trailData`: the `.trl` path, relative to the pack root (here the addon directory).
    pub file: String,
    pub guid: String,
    /// Metres, map space. [`SECTION_BREAK`] separates sections.
    pub points: Vec<[f32; 3]>,
    /// Every other attribute (`texture`, `fadeNear`, ...), written after ours.
    pub attrs: Attrs,
}

impl Trail {
    pub fn new(map_id: u32, category: impl Into<String>, file: impl Into<String>) -> Self {
        Self {
            map_id,
            category: category.into(),
            file: file.into(),
            guid: guid::new_guid(),
            points: Vec::new(),
            attrs: DEFAULT_FADE
                .iter()
                .map(|&(name, value)| (name.into(), value.into()))
                .collect(),
        }
    }

    /// One `<Trail .../>` element, ready to paste into a pack's `<POIs>` section.
    pub fn trail_xml(&self) -> String {
        let mut s = String::from("<Trail");
        if !self.category.is_empty() {
            let _ = write!(s, " type=\"{}\"", escape_attr(&self.category));
        }
        let _ = write!(
            s,
            " trailData=\"{}\" GUID=\"{}\"",
            escape_attr(&self.file),
            escape_attr(&self.guid)
        );
        write_attrs(&mut s, &self.attrs);
        s.push_str("/>");
        s
    }

    /// The pieces of the trail that are drawn connected.
    pub fn sections(&self) -> impl Iterator<Item = &[[f32; 3]]> {
        self.points
            .split(|p| is_break(*p))
            .filter(|section| !section.is_empty())
    }

    /// Number of points, not counting section breaks.
    pub fn point_count(&self) -> usize {
        self.points.iter().filter(|&&p| !is_break(p)).count()
    }

    pub fn section_count(&self) -> usize {
        self.sections().count()
    }

    /// Drawn length in metres. The gaps between sections don't count.
    pub fn length(&self) -> f32 {
        self.sections()
            .flat_map(|section| section.windows(2).map(|w| distance(w[0], w[1])))
            .sum()
    }

    pub fn last_point(&self) -> Option<[f32; 3]> {
        self.points.iter().rev().copied().find(|&p| !is_break(p))
    }

    /// Appends a point to the current section. A point exactly at the origin would read back as a
    /// section break, so it is refused (no real position is ever exactly there).
    pub fn push_point(&mut self, pos: [f32; 3]) -> bool {
        if is_break(pos) {
            return false;
        }
        self.points.push(pos);
        true
    }

    /// Ends the current section: the next point starts a new, unconnected one.
    pub fn break_section(&mut self) {
        if self.points.last().is_some_and(|&p| !is_break(p)) {
            self.points.push(SECTION_BREAK);
        }
    }

    /// Undoes the last step: removes a trailing section break, or else the last point (and the
    /// section break before it if it was alone in its section).
    pub fn pop_point(&mut self) -> bool {
        if self.points.last().is_some_and(|&p| is_break(p)) {
            self.trim_breaks();
            return true;
        }
        let popped = self.points.pop().is_some();
        self.trim_breaks();
        popped
    }

    /// Removes section breaks at the end, which separate nothing.
    pub fn trim_breaks(&mut self) {
        while self.points.last().is_some_and(|&p| is_break(p)) {
            self.points.pop();
        }
    }

    /// Every point that isn't a section break: its index in `points` and its section number
    /// (from 1).
    pub fn numbered_points(&self) -> Vec<(usize, usize)> {
        let mut section = 1;
        let mut out = Vec::with_capacity(self.points.len());
        for (i, &p) in self.points.iter().enumerate() {
            if is_break(p) {
                if out.last().is_some_and(|&(_, s)| s == section) {
                    section += 1;
                }
            } else {
                out.push((i, section));
            }
        }
        out
    }

    /// The point (index in `points`) closest to `pos`.
    pub fn nearest_point(&self, pos: [f32; 3]) -> Option<usize> {
        self.points
            .iter()
            .enumerate()
            .filter(|&(_, &p)| !is_break(p))
            .min_by(|a, b| distance(*a.1, pos).total_cmp(&distance(*b.1, pos)))
            .map(|(i, _)| i)
    }

    /// Inserts `pos` right after the point at `index`, in its section. Returns the new point's
    /// index.
    pub fn insert_point(&mut self, index: usize, pos: [f32; 3]) -> Option<usize> {
        if index >= self.points.len() || is_break(pos) {
            return None;
        }
        self.points.insert(index + 1, pos);
        Some(index + 1)
    }

    /// Removes the points at `indices` (indices in `points`; breaks and out-of-range ones are
    /// ignored), then the section breaks left at the start, at the end or next to each other.
    /// Returns how many points were removed.
    pub fn remove_points(&mut self, indices: &[usize]) -> usize {
        let mut indices: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&i| self.points.get(i).is_some_and(|&p| !is_break(p)))
            .collect();
        indices.sort_unstable();
        indices.dedup();
        // Highest first, so the others keep their place.
        for &i in indices.iter().rev() {
            self.points.remove(i);
        }
        self.tidy_breaks();
        indices.len()
    }

    /// Removes the section breaks that separate nothing: at the start, at the end, and all but
    /// one of several in a row.
    fn tidy_breaks(&mut self) {
        let mut previous_was_break = true; // so breaks at the start go
        self.points.retain(|&p| {
            let keep = !(is_break(p) && previous_was_break);
            previous_was_break = is_break(p);
            keep
        });
        self.trim_breaks();
    }

    /// The range in `points` of each section, in order.
    pub fn section_ranges(&self) -> Vec<std::ops::Range<usize>> {
        let mut ranges = Vec::new();
        let mut start = None;
        for (i, &p) in self.points.iter().enumerate() {
            match (is_break(p), start) {
                (false, None) => start = Some(i),
                (true, Some(s)) => {
                    ranges.push(s..i);
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            ranges.push(s..self.points.len());
        }
        ranges
    }

    /// Moves section `section` (0-based) after the others, so that new points continue it.
    /// Sections are drawn separately, so their order changes nothing on screen.
    pub fn move_section_to_end(&mut self, section: usize) -> bool {
        let ranges = self.section_ranges();
        let Some(range) = ranges.get(section).cloned() else {
            return false;
        };
        if section + 1 == ranges.len() {
            return true;
        }
        let moved: Vec<[f32; 3]> = self.points.drain(range).collect();
        self.tidy_breaks();
        self.points.push(SECTION_BREAK);
        self.points.extend(moved);
        true
    }
}

pub fn is_break(pos: [f32; 3]) -> bool {
    pos == SECTION_BREAK
}

/// `.trl` v0: `i32 version = 0`, `i32 map id`, then `f32 x, y, z` per point, little-endian.
pub fn encode_trl(map_id: u32, points: &[[f32; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + 12 * points.len());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&map_id.to_le_bytes());
    for point in points {
        for value in point {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out
}

/// Reads a `.trl`: map id and points. Like TacO, ignores the version, and like every reader,
/// ignores a trailing partial point.
pub fn decode_trl(bytes: &[u8]) -> Result<(u32, Vec<[f32; 3]>), String> {
    let Some((header, body)) = bytes.split_first_chunk::<8>() else {
        return Err(format!(
            "{} bytes is too short for a .trl header",
            bytes.len()
        ));
    };
    let map_id = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    let (points, _partial) = body.as_chunks::<12>();
    let points = points
        .iter()
        .map(|c| {
            let f = |i: usize| f32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]);
            [f(0), f(4), f(8)]
        })
        .collect();
    Ok((map_id, points))
}

/// Whether `file` is one the editor created (`trails/<name>.trl`), so it may delete it.
pub fn is_own_file(file: &str) -> bool {
    file.strip_prefix(TRAILS_DIR)
        .and_then(|rest| rest.strip_prefix('/'))
        .is_some_and(|name| {
            name.ends_with(".trl") && !name.contains(['/', '\\']) && !name.starts_with('.')
        })
}

/// A `trailData` path no trail in `taken` uses and no file occupies: `trails/map<id>_<n>.trl`.
pub fn new_file_name(dir: &Path, map_id: u32, taken: &[Trail]) -> String {
    (1u32..)
        .map(|n| format!("{TRAILS_DIR}/map{map_id}_{n}.trl"))
        .find(|file| {
            !taken.iter().any(|t| t.file.eq_ignore_ascii_case(file))
                && resolve(dir, file).is_some_and(|path| !path.exists())
        })
        .expect("some number is free")
}

/// Reads `trail.file` (relative to `dir`) into `trail.map_id` and `trail.points`.
pub fn load(dir: &Path, trail: &mut Trail) -> Result<(), String> {
    let path = resolve(dir, &trail.file)
        .ok_or_else(|| format!("trailData=\"{}\" is not a relative path", trail.file))?;
    let read_error = |e: io::Error| format!("couldn't read {}: {e}", path.display());
    let size = fs::metadata(&path).map_err(read_error)?.len();
    if size > MAX_TRL_BYTES {
        return Err(format!(
            "{} is {size} bytes, too big for a trail",
            path.display()
        ));
    }
    let bytes = fs::read(&path).map_err(read_error)?;
    let (map_id, points) = decode_trl(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    trail.map_id = map_id;
    trail.points = points;
    Ok(())
}

/// Writes `trail.points` to `trail.file` (relative to `dir`), creating the folder if needed.
pub fn save(dir: &Path, trail: &Trail) -> io::Result<()> {
    let path = resolve(dir, &trail.file).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("trailData=\"{}\" is not a relative path", trail.file),
        )
    })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_atomic(&path, encode_trl(trail.map_id, &trail.points))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Trail {
        Trail {
            map_id: 50,
            category: "mypack.trails".into(),
            file: "trails/map50_1.trl".into(),
            guid: "xM5D+V2CUEGt0qIpbEik4g==".into(),
            points: vec![
                [0.0, 1.0, 0.0],
                [3.0, 1.0, 4.0],
                SECTION_BREAK,
                [10.0, 0.0, 0.0],
                [10.0, 0.0, 2.0],
            ],
            attrs: Attrs::new(),
        }
    }

    #[test]
    fn trail_line() {
        let mut trail = Trail::new(50, "mypack.trails", "trails/map50_1.trl");
        trail.guid = "xM5D+V2CUEGt0qIpbEik4g==".into();
        assert_eq!(
            trail.trail_xml(),
            r#"<Trail type="mypack.trails" trailData="trails/map50_1.trl" GUID="xM5D+V2CUEGt0qIpbEik4g==" fadeNear="5000" fadeFar="5100"/>"#
        );
    }

    #[test]
    fn sections_and_length() {
        let trail = sample();
        assert_eq!(trail.point_count(), 4);
        assert_eq!(trail.section_count(), 2);
        assert_eq!(
            trail.length(),
            5.0 + 2.0,
            "the gap between sections doesn't count"
        );
        assert_eq!(trail.last_point(), Some([10.0, 0.0, 2.0]));
    }

    #[test]
    fn editing_points() {
        let mut trail = Trail::new(50, "", "trails/x.trl");
        assert!(!trail.push_point(SECTION_BREAK));
        trail.break_section(); // nothing to break yet
        assert!(trail.points.is_empty());

        trail.push_point([1.0, 0.0, 0.0]);
        trail.break_section();
        trail.break_section(); // only one break
        trail.push_point([2.0, 0.0, 0.0]);
        assert_eq!(trail.points.len(), 3);

        // Popping the only point of a section also drops its break.
        assert!(trail.pop_point());
        assert_eq!(trail.points, vec![[1.0, 0.0, 0.0]]);
        // A pending break is undone on its own.
        trail.break_section();
        assert!(trail.pop_point());
        assert_eq!(trail.points, vec![[1.0, 0.0, 0.0]]);
        assert!(trail.pop_point());
        assert!(!trail.pop_point());
    }

    #[test]
    fn point_editing() {
        let mut trail = sample();
        assert_eq!(
            trail.numbered_points(),
            vec![(0, 1), (1, 1), (3, 2), (4, 2)]
        );
        assert_eq!(trail.nearest_point([9.0, 0.0, 0.0]), Some(3));

        assert_eq!(trail.insert_point(0, [1.0, 1.0, 1.0]), Some(1));
        assert_eq!(trail.points[1], [1.0, 1.0, 1.0]);
        assert_eq!(trail.insert_point(99, [1.0, 1.0, 1.0]), None);
        assert_eq!(trail.insert_point(0, SECTION_BREAK), None);

        // Removing both points of the second section also removes its break.
        assert_eq!(trail.remove_points(&[3]), 0, "a break isn't a point");
        assert_eq!(trail.remove_points(&[4]), 1);
        assert_eq!(trail.remove_points(&[4]), 1);
        assert_eq!(trail.section_count(), 1);
        assert!(trail.points.iter().all(|&p| !is_break(p)));

        // Removing the first section's points leaves no leading break.
        let mut trail = sample();
        assert_eq!(trail.remove_points(&[0]), 1);
        assert_eq!(trail.remove_points(&[0]), 1);
        assert_eq!(trail.points, vec![[10.0, 0.0, 0.0], [10.0, 0.0, 2.0]]);
    }

    #[test]
    fn several_points_and_sections() {
        let mut trail = sample();
        assert_eq!(trail.section_ranges(), vec![0..2, 3..5]);
        // Points of both sections, a break and a bad index: the second section disappears.
        assert_eq!(trail.remove_points(&[4, 1, 3, 2, 99, 3]), 3);
        assert_eq!(trail.points, vec![[0.0, 1.0, 0.0]]);

        let mut trail = sample();
        assert!(trail.move_section_to_end(0));
        assert_eq!(
            trail.points,
            vec![
                [10.0, 0.0, 0.0],
                [10.0, 0.0, 2.0],
                SECTION_BREAK,
                [0.0, 1.0, 0.0],
                [3.0, 1.0, 4.0],
            ]
        );
        assert_eq!(trail.last_point(), Some([3.0, 1.0, 4.0]));
        assert!(trail.move_section_to_end(1), "already last");
        assert!(!trail.move_section_to_end(2));
    }

    #[test]
    fn trl_round_trip() {
        let trail = sample();
        let bytes = encode_trl(trail.map_id, &trail.points);
        assert_eq!(bytes.len(), 8 + 12 * 5);
        assert_eq!(&bytes[..8], &[0, 0, 0, 0, 50, 0, 0, 0]);
        assert_eq!(&bytes[8..12], &0f32.to_le_bytes());
        assert_eq!(&bytes[12..16], &1f32.to_le_bytes());

        let mut with_tail = bytes.clone();
        with_tail.extend_from_slice(&[1, 2, 3]); // partial point: ignored
        assert_eq!(decode_trl(&with_tail), Ok((50, trail.points.clone())));
        assert_eq!(decode_trl(&bytes[..8]), Ok((50, Vec::new())));
        assert!(decode_trl(&bytes[..7]).is_err());
    }

    #[test]
    fn own_files() {
        assert!(is_own_file("trails/map50_1.trl"));
        for other in [
            "data/a.trl",
            "trails/sub/a.trl",
            "trails/../a.trl",
            "trails/a.png",
            "trails/.trl",
        ] {
            assert!(!is_own_file(other), "{other}");
        }
    }

    #[test]
    fn files() {
        let dir = std::env::temp_dir().join(format!(
            "pathing_editor_test_{}",
            guid::new_guid().replace(['/', '+', '='], "")
        ));
        assert_eq!(new_file_name(&dir, 50, &[]), "trails/map50_1.trl");

        let mut trail = sample();
        save(&dir, &trail).unwrap();
        assert_eq!(
            new_file_name(&dir, 50, &[]),
            "trails/map50_2.trl",
            "file exists"
        );
        let mut other = sample();
        other.file = "trails/map50_2.trl".into();
        assert_eq!(
            new_file_name(&dir, 50, &[other]),
            "trails/map50_3.trl",
            "name taken"
        );

        let saved = trail.points.clone();
        trail.points.clear();
        trail.map_id = 0;
        load(&dir, &mut trail).unwrap();
        assert_eq!((trail.map_id, &trail.points), (50, &saved));

        trail.file = "trails/missing.trl".into();
        assert!(load(&dir, &mut trail).is_err());

        fs::remove_dir_all(&dir).unwrap();
    }
}
