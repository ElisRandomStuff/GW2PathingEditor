//! Value encoding for the XML we write (docs/01 §10 and the safe writer profile, §14).

/// Formats a float for an attribute value: `.` decimal point, no exponent, at most `decimals`
/// decimals, trailing zeros removed. Non-finite values become `0`.
///
/// TacO reads floats from a 20-char buffer, so values must stay within 19 chars. Map-space
/// positions (metres) are far below that limit.
pub fn fmt_float(value: f32, decimals: usize) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    let mut s = format!("{value:.decimals$}");
    if s.contains('.') {
        let trimmed = s.trim_end_matches('0').trim_end_matches('.').len();
        s.truncate(trimmed);
    }
    if s == "-0" {
        s = "0".into();
    }
    s
}

/// Positions (metres) are written with millimetre precision.
pub fn fmt_metres(value: f32) -> String {
    fmt_float(value, 3)
}

/// Escapes text for use inside a double-quoted XML attribute. Newlines become `&#xA;`, which is
/// how packs encode multi-line `info` text.
pub fn escape_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => {}
            '\t' => out.push_str("&#x9;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats() {
        assert_eq!(fmt_metres(-194.2794), "-194.279");
        assert_eq!(fmt_metres(1.5), "1.5");
        assert_eq!(fmt_metres(2.0), "2");
        assert_eq!(fmt_metres(-0.0001), "0");
        assert_eq!(fmt_metres(0.0), "0");
        assert_eq!(fmt_metres(12345.678), "12345.678");
        assert_eq!(fmt_metres(f32::NAN), "0");
        assert_eq!(fmt_float(100.0, 0), "100");
    }

    #[test]
    fn escaping() {
        assert_eq!(
            escape_attr(r#"a<b & "c" 'd'>"#),
            "a&lt;b &amp; &quot;c&quot; &apos;d&apos;&gt;"
        );
        assert_eq!(escape_attr("line1\r\nline2"), "line1&#xA;line2");
        assert_eq!(
            escape_attr("xM5D+V2CUEGt0qIpbEik4g=="),
            "xM5D+V2CUEGt0qIpbEik4g=="
        );
    }
}
