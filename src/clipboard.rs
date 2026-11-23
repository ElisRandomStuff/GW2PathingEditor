//! What copy and paste put on and read from the system clipboard: `<POI>` and `<Trail>` lines
//! (so they paste from and into pack files too), `x, y, z` lines for trail points.

use crate::{
    format::fmt_metres,
    pack::{self, Poi},
    trail::Trail,
};

/// Something pasteable found on the clipboard.
#[derive(Debug, Clone, PartialEq)]
pub enum Clip {
    /// Markers and trails. The trails only have what their XML line says: their points are in
    /// the `.trl` file `trailData` names.
    Items(Vec<Poi>, Vec<Trail>),
    /// Trail points.
    Points(Vec<[f32; 3]>),
}

/// Reads markers and trails (`<POI>` and `<Trail>` lines, or a whole pack file) or trail points
/// (`x, y, z` per line).
pub fn parse(text: &str) -> Option<Clip> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if text.starts_with('<') {
        let lower = text.to_ascii_lowercase();
        let parsed = if lower.contains("<overlaydata") {
            pack::parse_document(text)
        } else {
            pack::parse_document(&format!("<OverlayData><POIs>{text}</POIs></OverlayData>"))
        }
        .ok()?;
        let pack = parsed.pack;
        let found = !pack.pois.is_empty() || !pack.trails.is_empty();
        return found.then_some(Clip::Items(pack.pois, pack.trails));
    }
    let points: Option<Vec<[f32; 3]>> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(parse_point)
        .collect();
    points.filter(|p| !p.is_empty()).map(Clip::Points)
}

/// `x, y, z` (commas, spaces or tabs between the numbers).
fn parse_point(line: &str) -> Option<[f32; 3]> {
    let mut numbers = line
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<f32>().ok().filter(|v| v.is_finite()));
    let point = [numbers.next()??, numbers.next()??, numbers.next()??];
    numbers.next().is_none().then_some(point)
}

/// Markers as `<POI>` lines.
pub fn pois_text<'a>(pois: impl IntoIterator<Item = &'a Poi>) -> String {
    pois.into_iter()
        .map(Poi::poi_xml)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Trails as `<Trail>` lines.
pub fn trails_text<'a>(trails: impl IntoIterator<Item = &'a Trail>) -> String {
    trails
        .into_iter()
        .map(Trail::trail_xml)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Trail points as `x, y, z` lines.
pub fn points_text(points: &[[f32; 3]]) -> String {
    points
        .iter()
        .map(|p| {
            format!(
                "{}, {}, {}",
                fmt_metres(p[0]),
                fmt_metres(p[1]),
                fmt_metres(p[2])
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers() {
        let pois = vec![
            Poi::new(50, [1.0, 2.0, 3.0], "a.b"),
            Poi::new(50, [4.0, 5.0, 6.0], ""),
        ];
        let Some(Clip::Items(back, trails)) = parse(&pois_text(&pois)) else {
            panic!("not markers");
        };
        assert_eq!(back, pois);
        assert!(trails.is_empty());

        // A whole file, with a trail line.
        let file = r#"<?xml version="1.0"?><OverlayData><POIs>
            <POI MapID="1" xpos="1" ypos="2" zpos="3"/><Trail trailData="a.trl"/>
        </POIs></OverlayData>"#;
        assert!(matches!(parse(file), Some(Clip::Items(p, t)) if p.len() == 1 && t.len() == 1));
        // Trails alone.
        let Some(Clip::Items(pois, trails)) =
            parse(r#"<Trail type="a.b" trailData="Data\x.trl" texture="t.png"/>"#)
        else {
            panic!("not a trail");
        };
        assert!(pois.is_empty());
        assert_eq!(trails[0].file, "Data\\x.trl");
        assert_eq!(parse("<POI MapID=\"1\"/>"), None, "unusable");
        assert_eq!(parse("<not xml"), None);
    }

    #[test]
    fn points() {
        let points = vec![[1.0, -2.5, 3.25], [0.5, 0.0, 100.0]];
        assert_eq!(parse(&points_text(&points)), Some(Clip::Points(points)));
        assert_eq!(
            parse("1 2 3\n\n4\t5\t6\n"),
            Some(Clip::Points(vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]))
        );
        assert_eq!(parse("1, 2"), None);
        assert_eq!(parse("1, 2, 3, 4"), None);
        assert_eq!(parse("hello world"), None);
        assert_eq!(parse("   "), None);
    }
}
