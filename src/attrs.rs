//! XML attribute lists. Names are matched case-insensitively, like all three overlays do, and
//! kept in their original order and casing so a round trip doesn't change the file.

pub type Attrs = Vec<(String, String)>;

pub fn get<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Sets `name` to `value`. An existing attribute keeps its position and casing; a new one is
/// appended as `name` (pass the canonical casing, docs/01 §14.2).
pub fn set(attrs: &mut Attrs, name: &str, value: impl Into<String>) {
    let value = value.into();
    match attrs.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
        Some((_, v)) => *v = value,
        None => attrs.push((name.to_owned(), value)),
    }
}

/// Removes every attribute called `name`. Returns whether there was one.
pub fn remove(attrs: &mut Attrs, name: &str) -> bool {
    let before = attrs.len();
    attrs.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
    attrs.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_set_remove() {
        let mut attrs: Attrs = vec![("IconFile".into(), "a.png".into())];
        assert_eq!(get(&attrs, "iconfile"), Some("a.png"));
        set(&mut attrs, "iconFile", "b.png");
        assert_eq!(
            attrs,
            vec![("IconFile".into(), "b.png".into())],
            "casing kept"
        );
        set(&mut attrs, "DisplayName", "Chests");
        assert_eq!(attrs[1], ("DisplayName".into(), "Chests".into()));
        assert!(remove(&mut attrs, "ICONFILE"));
        assert!(!remove(&mut attrs, "iconFile"));
        assert_eq!(get(&attrs, "iconFile"), None);
    }
}
