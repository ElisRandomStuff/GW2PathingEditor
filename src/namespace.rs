//! Category namespaces (`type="mypack.jps.chalice"`), docs/01 §5.1.
//!
//! TacO and TmfLib sanitise category names differently for non-ASCII text, so we only ever write
//! segments matching `[a-z0-9_]+`. Both overlays then agree on the namespace.

/// Turns user input into a namespace both TacO and Pathing resolve identically: segments split
/// on `.`, trimmed, lowercased, every char outside `[a-z0-9_]` replaced by `_`, empty segments
/// dropped.
pub fn sanitize(input: &str) -> String {
    input
        .split('.')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            segment
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() {
                        c.to_ascii_lowercase()
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Splits a namespace into its category names.
pub fn segments(ns: &str) -> impl Iterator<Item = &str> {
    ns.split('.').filter(|s| !s.is_empty())
}

/// Whether `ns` is `ancestor` or one of its subcategories (case-insensitive, like the overlays).
pub fn is_within(ns: &str, ancestor: &str) -> bool {
    let mut ns = segments(ns);
    segments(ancestor).all(|a| ns.next().is_some_and(|s| s.eq_ignore_ascii_case(a)))
        && segments(ancestor).next().is_some()
}

/// `parent.child`, or `child` at the top level.
pub fn join(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_owned()
    } else {
        format!("{parent}.{child}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizing() {
        assert_eq!(sanitize("mypack.jps.chalice"), "mypack.jps.chalice");
        assert_eq!(
            sanitize(" My Pack . Jumping Puzzles "),
            "my_pack.jumping_puzzles"
        );
        assert_eq!(sanitize("a..b."), "a.b");
        assert_eq!(sanitize("Café-2"), "caf__2");
        assert_eq!(sanitize(""), "");
        assert_eq!(sanitize(" . "), "");
    }

    #[test]
    fn within() {
        assert!(is_within("a.b.c", "a.b"));
        assert!(is_within("a.b", "A.B"));
        assert!(!is_within("a.bc", "a.b"));
        assert!(!is_within("a", "a.b"));
        assert!(!is_within("a", ""));
        assert_eq!(join("", "a"), "a");
        assert_eq!(join("a", "b"), "a.b");
    }
}
