//! The pack being edited (categories, POIs, trails) and `captures.xml`, the file it persists in.
//!
//! `captures.xml` is a complete, minimal marker pack (safe writer profile, docs/01 §14): the
//! category tree, then the POIs and trails. With the `trails/` and `images/` folders next to it,
//! the addon folder's content can be dropped into an overlay's marker folder as is.

use crate::{
    attrs::{self, Attrs},
    files::write_atomic,
    format::{escape_attr, fmt_metres},
    geometry::distance,
    guid, namespace,
    trail::Trail,
};
use std::{
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// A marker (`<POI>`).
#[derive(Debug, Clone, PartialEq)]
pub struct Poi {
    /// GW2 API map id (MumbleLink `Context.MapID`).
    pub map_id: u32,
    /// `xpos`, `ypos`, `zpos`: metres in MumbleLink map space (X east, Y up, Z north).
    pub pos: [f32; 3],
    /// Category namespace written to `type`. Empty means no `type` attribute.
    pub category: String,
    /// Base64 GUID. Kept as read so hand-written values survive a round trip.
    pub guid: String,
    /// Every other attribute (`iconFile`, ...), written after ours.
    pub attrs: Attrs,
}

impl Poi {
    pub fn new(map_id: u32, pos: [f32; 3], category: impl Into<String>) -> Self {
        Self {
            map_id,
            pos,
            category: category.into(),
            guid: guid::new_guid(),
            attrs: Attrs::new(),
        }
    }

    /// One `<POI .../>` element, ready to paste into a pack's `<POIs>` section.
    pub fn poi_xml(&self) -> String {
        let mut s = format!(
            "<POI MapID=\"{}\" {}",
            self.map_id,
            position_attrs(self.pos)
        );
        if !self.category.is_empty() {
            let _ = write!(s, " type=\"{}\"", escape_attr(&self.category));
        }
        let _ = write!(s, " GUID=\"{}\"", escape_attr(&self.guid));
        write_attrs(&mut s, &self.attrs);
        s.push_str("/>");
        s
    }

    /// Straight-line distance in metres.
    pub fn distance_to(&self, pos: [f32; 3]) -> f32 {
        distance(self.pos, pos)
    }
}

/// `xpos="…" ypos="…" zpos="…"`, for moving an existing marker of a pack by hand.
pub fn position_attrs(pos: [f32; 3]) -> String {
    format!(
        "xpos=\"{}\" ypos=\"{}\" zpos=\"{}\"",
        fmt_metres(pos[0]),
        fmt_metres(pos[1]),
        fmt_metres(pos[2])
    )
}

pub(crate) fn write_attrs(out: &mut String, attrs: &[(String, String)]) {
    for (name, value) in attrs {
        let _ = write!(out, " {name}=\"{}\"", escape_attr(value));
    }
}

/// A `<MarkerCategory>`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Category {
    /// One namespace segment.
    pub name: String,
    /// Every attribute but `name` (`DisplayName`, `iconFile`, ...).
    pub attrs: Attrs,
    pub children: Vec<Category>,
}

impl Category {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// `DisplayName` if set, else the name.
    pub fn display_name(&self) -> &str {
        attrs::get(&self.attrs, "DisplayName")
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(&self.name)
    }

    fn write(&self, out: &mut String, depth: usize) {
        let indent = "  ".repeat(depth);
        let _ = write!(
            out,
            "{indent}<MarkerCategory name=\"{}\"",
            escape_attr(&self.name)
        );
        write_attrs(out, &self.attrs);
        if self.children.is_empty() {
            out.push_str("/>\n");
        } else {
            out.push_str(">\n");
            for child in &self.children {
                child.write(out, depth + 1);
            }
            let _ = writeln!(out, "{indent}</MarkerCategory>");
        }
    }
}

fn find<'a>(nodes: &'a [Category], ns: &str) -> Option<&'a Category> {
    let mut nodes = nodes;
    let mut found = None;
    for segment in namespace::segments(ns) {
        let node = nodes
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(segment))?;
        nodes = &node.children;
        found = Some(node);
    }
    found
}

fn find_mut<'a>(nodes: &'a mut [Category], ns: &str) -> Option<&'a mut Category> {
    let (first, rest) = match ns.split_once('.') {
        Some((first, rest)) => (first, Some(rest)),
        None => (ns, None),
    };
    let node = nodes
        .iter_mut()
        .find(|c| c.name.eq_ignore_ascii_case(first))?;
    match rest {
        Some(rest) if !rest.is_empty() => find_mut(&mut node.children, rest),
        _ => Some(node),
    }
}

/// Deepest category kept. Real packs use a handful of levels; this bounds the recursion over
/// the tree (a pasted or hand-written pack could nest thousands).
pub const MAX_CATEGORY_DEPTH: usize = 32;

/// Creates the missing categories of `ns` (down to [`MAX_CATEGORY_DEPTH`]).
fn ensure(nodes: &mut Vec<Category>, ns: &str) {
    let mut nodes = nodes;
    for segment in namespace::segments(ns).take(MAX_CATEGORY_DEPTH) {
        let index = match nodes
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(segment))
        {
            Some(i) => i,
            None => {
                nodes.push(Category::new(segment));
                nodes.len() - 1
            }
        };
        nodes = &mut nodes[index].children;
    }
}

/// Adds `category` to `nodes`, merging it into a category of the same name (later attribute
/// values win, like a second declaration in a pack).
fn merge(nodes: &mut Vec<Category>, category: Category) {
    match nodes
        .iter_mut()
        .find(|c| c.name.eq_ignore_ascii_case(&category.name))
    {
        Some(existing) => {
            for (name, value) in category.attrs {
                attrs::set(&mut existing.attrs, &name, value);
            }
            for child in category.children {
                merge(&mut existing.children, child);
            }
        }
        None => nodes.push(category),
    }
}

/// Categories, POIs and trails.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pack {
    pub categories: Vec<Category>,
    pub pois: Vec<Poi>,
    pub trails: Vec<Trail>,
}

impl Pack {
    pub fn category(&self, ns: &str) -> Option<&Category> {
        find(&self.categories, ns)
    }

    pub fn category_mut(&mut self, ns: &str) -> Option<&mut Category> {
        find_mut(&mut self.categories, ns)
    }

    /// The value of attribute `name` for an item of category `category` with attributes `own`:
    /// its own, or else that of the closest category above it that sets it (docs/01 §4).
    pub fn inherited<'a>(
        &'a self,
        category: &str,
        own: &'a [(String, String)],
        name: &str,
    ) -> Option<&'a str> {
        if let Some(value) = attrs::get(own, name) {
            return Some(value);
        }
        let mut chain = Vec::new();
        let mut nodes = &self.categories[..];
        for segment in namespace::segments(category) {
            let Some(node) = nodes.iter().find(|c| c.name.eq_ignore_ascii_case(segment)) else {
                break;
            };
            chain.push(node);
            nodes = &node.children;
        }
        chain.iter().rev().find_map(|c| attrs::get(&c.attrs, name))
    }

    /// Adds another document's categories (merged by name, later attributes win, like the
    /// overlays), markers and trails.
    pub fn merge(&mut self, other: Pack) {
        for category in other.categories {
            merge(&mut self.categories, category);
        }
        self.pois.extend(other.pois);
        self.trails.extend(other.trails);
    }

    /// Creates the missing categories of `ns`.
    pub fn ensure_category(&mut self, ns: &str) {
        ensure(&mut self.categories, ns);
    }

    /// Gives every namespace used by a POI or trail a category.
    pub fn ensure_used_categories(&mut self) {
        let used: Vec<String> = self.used_namespaces().map(str::to_owned).collect();
        for ns in used {
            ensure(&mut self.categories, &ns);
        }
    }

    fn used_namespaces(&self) -> impl Iterator<Item = &str> {
        self.pois
            .iter()
            .map(|p| p.category.as_str())
            .chain(self.trails.iter().map(|t| t.category.as_str()))
    }

    /// Number of POIs and trails in `ns` or below it.
    pub fn count_within(&self, ns: &str) -> (usize, usize) {
        (
            self.pois
                .iter()
                .filter(|p| namespace::is_within(&p.category, ns))
                .count(),
            self.trails
                .iter()
                .filter(|t| namespace::is_within(&t.category, ns))
                .count(),
        )
    }

    /// Removes the category `ns` and its subcategories, if no POI or trail is in them.
    pub fn remove_category(&mut self, ns: &str) -> Result<(), String> {
        let (pois, trails) = self.count_within(ns);
        if pois + trails > 0 {
            return Err(format!(
                "{ns} still holds {pois} marker(s) and {trails} trail(s)."
            ));
        }
        let (siblings, name) = match ns.rsplit_once('.') {
            Some((parent, name)) => (
                &mut self
                    .category_mut(parent)
                    .ok_or_else(|| format!("No category {parent}."))?
                    .children,
                name,
            ),
            None => (&mut self.categories, ns),
        };
        let before = siblings.len();
        siblings.retain(|c| !c.name.eq_ignore_ascii_case(name));
        if siblings.len() == before {
            return Err(format!("No category {ns}."));
        }
        Ok(())
    }

    /// The whole OverlayData document: the category tree (plus any category a POI or trail
    /// uses but the tree lacks), then the POIs and trails.
    pub fn to_document(&self) -> String {
        let mut categories = self.categories.clone();
        for ns in self.used_namespaces() {
            ensure(&mut categories, ns);
        }

        let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<OverlayData>\n");
        for category in &categories {
            category.write(&mut out, 1);
        }
        out.push_str("  <POIs>\n");
        for poi in &self.pois {
            let _ = writeln!(out, "    {}", poi.poi_xml());
        }
        for trail in &self.trails {
            let _ = writeln!(out, "    {}", trail.trail_xml());
        }
        out.push_str("  </POIs>\n</OverlayData>\n");
        out
    }
}

/// Result of reading a document.
#[derive(Debug, Default)]
pub struct ParsedDocument {
    /// Trails as written in the XML: their `.trl` files are not read, so `map_id` is 0 and
    /// `points` is empty.
    pub pack: Pack,
    /// POIs without a usable `MapID` / `xpos` / `ypos` / `zpos`, trails without `trailData`.
    pub skipped: usize,
}

/// Reads the categories, POIs and trails of a document (element and attribute names are matched
/// case-insensitively, like all three overlays do).
pub fn parse_document(xml: &str) -> Result<ParsedDocument, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let mut parsed = ParsedDocument::default();

    for node in doc.root_element().children() {
        if let Some(category) = parse_category(node, 1) {
            merge(&mut parsed.pack.categories, category);
        }
    }

    for node in doc.descendants().filter(|n| n.is_element()) {
        let tag = node.tag_name().name();
        if tag.eq_ignore_ascii_case("trail") {
            match parse_trail(node) {
                Some(trail) => parsed.pack.trails.push(trail),
                None => parsed.skipped += 1,
            }
        } else if tag.eq_ignore_ascii_case("poi") {
            match parse_poi(node) {
                Some(poi) => parsed.pack.pois.push(poi),
                None => parsed.skipped += 1,
            }
        }
    }
    Ok(parsed)
}

fn is_element(node: roxmltree::Node, name: &str) -> bool {
    node.is_element() && node.tag_name().name().eq_ignore_ascii_case(name)
}

/// A `<MarkerCategory>` and its subcategories, `depth` levels down from the top.
fn parse_category(node: roxmltree::Node, depth: usize) -> Option<Category> {
    if !is_element(node, "markercategory") || depth > MAX_CATEGORY_DEPTH {
        return None;
    }
    let mut category = Category::default();
    for attr in node.attributes() {
        if attr.name().eq_ignore_ascii_case("name") {
            category.name = attr.value().trim().to_owned();
        } else {
            category
                .attrs
                .push((attr.name().to_owned(), attr.value().to_owned()));
        }
    }
    if category.name.is_empty() {
        return None;
    }
    for child in node
        .children()
        .filter_map(|child| parse_category(child, depth + 1))
    {
        merge(&mut category.children, child);
    }
    Some(category)
}

fn parse_poi(node: roxmltree::Node) -> Option<Poi> {
    let mut map_id = None;
    let mut pos = [None; 3];
    let mut category = String::new();
    let mut guid = None;
    let mut attrs = Attrs::new();

    for attr in node.attributes() {
        let value = attr.value().trim();
        match attr.name().to_ascii_lowercase().as_str() {
            "mapid" => map_id = value.parse::<u32>().ok(),
            "xpos" => pos[0] = value.parse::<f32>().ok(),
            "ypos" => pos[1] = value.parse::<f32>().ok(),
            "zpos" => pos[2] = value.parse::<f32>().ok(),
            "type" => category = value.to_owned(),
            "guid" => guid = Some(value.to_owned()),
            _ => attrs.push((attr.name().to_owned(), attr.value().to_owned())),
        }
    }

    let [Some(x), Some(y), Some(z)] = pos else {
        return None;
    };
    Some(Poi {
        map_id: map_id?,
        pos: [x, y, z],
        category,
        guid: guid
            .filter(|g| !g.is_empty())
            .unwrap_or_else(guid::new_guid),
        attrs,
    })
}

fn parse_trail(node: roxmltree::Node) -> Option<Trail> {
    let mut trail = Trail::new(0, "", "");
    trail.attrs.clear();
    let mut guid = None;
    for attr in node.attributes() {
        let value = attr.value().trim();
        match attr.name().to_ascii_lowercase().as_str() {
            "traildata" => trail.file = value.to_owned(),
            "type" => trail.category = value.to_owned(),
            "guid" => guid = Some(value.to_owned()),
            _ => trail
                .attrs
                .push((attr.name().to_owned(), attr.value().to_owned())),
        }
    }
    if let Some(guid) = guid.filter(|g| !g.is_empty()) {
        trail.guid = guid;
    }
    (!trail.file.is_empty()).then_some(trail)
}

/// Outcome of [`load_file`].
#[derive(Debug, Default)]
pub struct LoadReport {
    /// Trails without their points: see [`ParsedDocument::pack`].
    pub pack: Pack,
    /// Something the user should know about (skipped elements, file moved aside).
    pub warning: Option<String>,
}

/// Loads the pack from `path`. A missing file gives an empty pack. A file that doesn't parse is
/// renamed to `<name>.broken-<unix time>` so that the next save can't overwrite it.
///
/// Returns `Err` only when the file exists but can't be read. The caller must then not save to
/// `path`, or it would destroy data it never saw.
pub fn load_file(path: &Path) -> Result<LoadReport, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(LoadReport::default()),
        Err(e) => return Err(format!("couldn't read {}: {e}", path.display())),
    };

    match parse_document(&text) {
        Ok(parsed) => Ok(LoadReport {
            warning: (parsed.skipped > 0).then(|| {
                format!(
                    "{} element(s) in {} were incomplete (a POI without MapID/xpos/ypos/zpos or a Trail without trailData) and will be dropped on the next save",
                    parsed.skipped,
                    path.display()
                )
            }),
            pack: parsed.pack,
        }),
        Err(parse_error) => {
            let aside = broken_path(path);
            fs::rename(path, &aside).map_err(|e| {
                format!("{} is not valid XML ({parse_error}) and couldn't be moved aside: {e}", path.display())
            })?;
            Ok(LoadReport {
                warning: Some(format!(
                    "{} was not valid XML ({parse_error}). It was renamed to {}",
                    path.display(),
                    aside.display()
                )),
                ..LoadReport::default()
            })
        }
    }
}

/// Writes the document to `path` (not the trails' `.trl` files).
pub fn save_file(path: &Path, pack: &Pack) -> io::Result<()> {
    write_atomic(path, pack.to_document())
}

fn broken_path(path: &Path) -> PathBuf {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".broken-{secs}"));
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(category: &str) -> Poi {
        Poi {
            map_id: 50,
            pos: [-194.2794, 1.37639, -161.135],
            category: category.into(),
            guid: "xM5D+V2CUEGt0qIpbEik4g==".into(),
            attrs: Attrs::new(),
        }
    }

    fn pack(pois: Vec<Poi>, trails: Vec<Trail>) -> Pack {
        Pack {
            categories: Vec::new(),
            pois,
            trails,
        }
    }

    #[test]
    fn poi_line() {
        assert_eq!(
            sample("mypack.jps.chalice").poi_xml(),
            r#"<POI MapID="50" xpos="-194.279" ypos="1.376" zpos="-161.135" type="mypack.jps.chalice" GUID="xM5D+V2CUEGt0qIpbEik4g=="/>"#
        );
        let mut poi = sample("");
        poi.attrs
            .push(("iconFile".into(), "images/chest.png".into()));
        assert_eq!(
            poi.poi_xml(),
            r#"<POI MapID="50" xpos="-194.279" ypos="1.376" zpos="-161.135" GUID="xM5D+V2CUEGt0qIpbEik4g==" iconFile="images/chest.png"/>"#
        );
    }

    #[test]
    fn position_only() {
        assert_eq!(
            position_attrs([1.0, -2.5, 3.25]),
            r#"xpos="1" ypos="-2.5" zpos="3.25""#
        );
    }

    #[test]
    fn document_has_category_tree_before_pois() {
        let mut trail = Trail::new(50, "mypack.trails", "trails/map50_1.trl");
        trail.guid = "xM5D+V2CUEGt0qIpbEik4g==".into();
        let mut pack = pack(
            vec![
                sample("mypack.jps.chalice"),
                sample("mypack.jps.other"),
                sample("mypack.chests"),
                sample("second"),
            ],
            vec![trail],
        );
        // An explicit category with attributes, and an empty one: both written.
        pack.categories.push(Category {
            name: "mypack".into(),
            attrs: vec![("DisplayName".into(), "My Pack".into())],
            children: vec![Category::new("empty")],
        });
        let doc = pack.to_document();
        let expected = r#"<?xml version="1.0" encoding="utf-8"?>
<OverlayData>
  <MarkerCategory name="mypack" DisplayName="My Pack">
    <MarkerCategory name="empty"/>
    <MarkerCategory name="jps">
      <MarkerCategory name="chalice"/>
      <MarkerCategory name="other"/>
    </MarkerCategory>
    <MarkerCategory name="chests"/>
    <MarkerCategory name="trails"/>
  </MarkerCategory>
  <MarkerCategory name="second"/>
  <POIs>
"#;
        assert!(doc.starts_with(expected), "got:\n{doc}");
        assert!(doc.ends_with(
            "\n    <Trail type=\"mypack.trails\" trailData=\"trails/map50_1.trl\" GUID=\"xM5D+V2CUEGt0qIpbEik4g==\" fadeNear=\"5000\" fadeFar=\"5100\"/>\n  </POIs>\n</OverlayData>\n"
        ), "got:\n{doc}");
        assert_eq!(doc.matches("<POI ").count(), 4);
    }

    #[test]
    fn round_trip_keeps_everything() {
        let mut poi = sample("mypack.markers");
        poi.attrs
            .push(("tip-name".into(), "Chest <1> & \"friends\"".into()));
        let mut trail = Trail::new(50, "mypack.trails", "trails/map50_1.trl");
        trail
            .attrs
            .push(("texture".into(), "images/arrow.png".into()));
        let mut original = pack(vec![poi], vec![trail]);
        original.categories.push(Category {
            name: "mypack".into(),
            attrs: vec![("iconFile".into(), "images/a.png".into())],
            children: Vec::new(),
        });
        original.ensure_used_categories();

        let parsed = parse_document(&original.to_document()).unwrap();
        assert_eq!(parsed.skipped, 0);
        let mut back = parsed.pack;
        // Floats are written with 3 decimals, and the .trl (map id, points) isn't read here.
        assert!(back.pois[0].distance_to(original.pois[0].pos) < 0.001);
        back.pois[0].pos = original.pois[0].pos;
        back.trails[0].map_id = original.trails[0].map_id;
        assert_eq!(back, original);
    }

    #[test]
    fn parse_is_case_insensitive_and_counts_unusable_elements() {
        let xml = r#"<OverlayData>
            <markercategory Name="a" displayname="A"><MarkerCategory name="b"/></markercategory>
            <MarkerCategory name="A" iconFile="x.png"><MarkerCategory name="c"/></MarkerCategory>
            <MarkerCategory DisplayName="nameless"/>
            <POIs>
            <POI mapid="15" XPOS="1" ypos="2" zpos="3" Type="a.b"/>
            <POI MapID="15" xpos="1"/>
            <poi MapID="x" xpos="1" ypos="2" zpos="3"/>
            <TRAIL TrailData="a.trl" type="a.c"/>
            <Trail type="a.c"/>
        </POIs></OverlayData>"#;
        let pack = parse_document(xml).unwrap();
        assert_eq!(pack.skipped, 3);
        let pack = pack.pack;

        // Both declarations of "a" merged.
        assert_eq!(pack.categories.len(), 1);
        let a = &pack.categories[0];
        assert_eq!(a.display_name(), "A");
        assert_eq!(attrs::get(&a.attrs, "iconfile"), Some("x.png"));
        assert_eq!(a.children.len(), 2);
        assert!(pack.category("A.B").is_some());

        assert_eq!(pack.pois.len(), 1);
        assert_eq!(pack.pois[0].category, "a.b");
        assert_eq!(pack.pois[0].guid.len(), 24, "missing GUID gets a fresh one");
        assert_eq!(pack.trails.len(), 1);
        assert_eq!(pack.trails[0].file, "a.trl");
        assert!(pack.trails[0].attrs.is_empty(), "no default fade on read");
    }

    #[test]
    fn category_depth_is_bounded() {
        let deep = 100;
        let xml = format!(
            "<OverlayData>{}{}</OverlayData>",
            "<MarkerCategory name=\"c\">".repeat(deep),
            "</MarkerCategory>".repeat(deep)
        );
        let mut pack = parse_document(&xml).unwrap().pack;
        let depth = |pack: &Pack| {
            let (mut depth, mut nodes) = (0, &pack.categories);
            while let Some(first) = nodes.first() {
                depth += 1;
                nodes = &first.children;
            }
            depth
        };
        assert_eq!(depth(&pack), MAX_CATEGORY_DEPTH);
        pack.categories.clear();
        pack.ensure_category(&vec!["x"; deep].join("."));
        assert_eq!(depth(&pack), MAX_CATEGORY_DEPTH);
    }

    #[test]
    fn category_operations() {
        let mut pack = pack(vec![sample("mypack.chests")], Vec::new());
        pack.ensure_used_categories();
        pack.ensure_category("mypack.jps.chalice");
        assert!(pack.category("mypack.jps").is_some());
        assert_eq!(pack.count_within("mypack"), (1, 0));
        assert_eq!(pack.count_within("mypack.jps"), (0, 0));

        pack.category_mut("mypack.jps.chalice")
            .unwrap()
            .attrs
            .push(("DisplayName".into(), "Chalice".into()));
        assert_eq!(
            pack.category("mypack.jps.chalice").unwrap().display_name(),
            "Chalice"
        );

        assert!(pack.remove_category("mypack").is_err(), "not empty");
        assert!(pack.remove_category("mypack.nothing").is_err());
        pack.remove_category("mypack.jps").unwrap();
        assert!(pack.category("mypack.jps").is_none());
        assert!(pack.category("mypack.chests").is_some());
    }

    #[test]
    fn files() {
        let dir = std::env::temp_dir().join(format!(
            "pathing_editor_test_{}",
            guid::new_guid().replace(['/', '+', '='], "")
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("captures.xml");

        // Missing file = empty.
        let report = load_file(&path).unwrap();
        assert!(report.pack.pois.is_empty() && report.warning.is_none());

        // Save + load.
        save_file(
            &path,
            &pack(vec![sample("mypack.a"), sample("mypack.b")], Vec::new()),
        )
        .unwrap();
        let report = load_file(&path).unwrap();
        assert_eq!(report.pack.pois.len(), 2);
        assert!(report.warning.is_none());

        // A broken file is moved aside, not lost.
        fs::write(&path, "<OverlayData><POIs>").unwrap();
        let report = load_file(&path).unwrap();
        assert!(report.pack.pois.is_empty());
        assert!(report.warning.unwrap().contains("renamed"));
        assert!(!path.exists());
        let aside: Vec<_> = fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(aside.len(), 1);
        assert_eq!(
            fs::read_to_string(aside[0].path()).unwrap(),
            "<OverlayData><POIs>"
        );

        fs::remove_dir_all(&dir).unwrap();
    }
}
