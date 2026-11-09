//! Xresources semantics as implemented by Xlib (and so by `xrdb`,
//! `xrescat` and i3): resource lines with continued lines and value
//! escapes, resource specifications with tight (`.`) and loose (`*`)
//! bindings and the `?` wildcard, and lookups with Xlib precedence.

use std::collections::HashMap;

/// Parse resource lines (the preprocessed file, or the file itself with
/// `--nocpp`) into `(name, value)` pairs in file order. Names are returned
/// as written; values have their escapes decoded.
///
/// Like Xlib: a backslash at the end of a line continues the line, lines
/// starting with `!` or `#` are skipped, whitespace around the name and
/// before the value is ignored, and in values `\\`, `\n`, `\ooo` (three
/// octal digits) and a backslash before a space or tab are decoded.
/// Trailing whitespace of a value is dropped unless it was escaped.
pub fn parse(text: &str) -> Vec<(String, String)> {
    let bytes = text.as_bytes();
    let mut entries = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        i = skip_blanks(bytes, i);
        match bytes.get(i) {
            None => break,
            Some(b'\n') => {
                i += 1;
                continue;
            }
            Some(b'!') | Some(b'#') => {
                i = end_of_line(bytes, i);
                continue;
            }
            _ => {}
        }

        let mut name = Vec::new();
        let mut has_colon = false;
        while i < bytes.len() && bytes[i] != b'\n' {
            match bytes[i] {
                b'\\' if bytes.get(i + 1) == Some(&b'\n') => i += 2,
                b':' => {
                    has_colon = true;
                    i += 1;
                    break;
                }
                c => {
                    name.push(c);
                    i += 1;
                }
            }
        }
        if !has_colon {
            i = end_of_line(bytes, i);
            continue;
        }

        i = skip_blanks(bytes, i);
        let mut value = Vec::new();
        // Length of the value without its unescaped trailing whitespace
        let mut kept = 0;
        while i < bytes.len() && bytes[i] != b'\n' {
            if bytes[i] != b'\\' {
                value.push(bytes[i]);
                if !bytes[i].is_ascii_whitespace() {
                    kept = value.len();
                }
                i += 1;
                continue;
            }
            let (decoded, len) = match bytes.get(i + 1) {
                Some(b'\n') => {
                    i += 2;
                    continue;
                }
                Some(b'\\') => (b'\\', 2),
                Some(b'n') => (b'\n', 2),
                Some(&c @ (b' ' | b'\t')) => (c, 2),
                Some(_) if is_octal_escape(&bytes[i + 1..]) => {
                    let code = bytes[i + 1..i + 4]
                        .iter()
                        .fold(0u32, |acc, d| acc * 8 + (d - b'0') as u32);
                    (code as u8, 4)
                }
                // Not an escape: the backslash is kept as written
                _ => (b'\\', 1),
            };
            value.push(decoded);
            kept = value.len();
            i += len;
        }
        value.truncate(kept);

        entries.push((
            String::from_utf8_lossy(&name).trim().to_string(),
            String::from_utf8_lossy(&value).into_owned(),
        ));
    }
    entries
}

fn skip_blanks(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    i
}

fn end_of_line(bytes: &[u8], i: usize) -> usize {
    match bytes[i..].iter().position(|&c| c == b'\n') {
        Some(pos) => i + pos,
        None => bytes.len(),
    }
}

fn is_octal_escape(bytes: &[u8]) -> bool {
    bytes.len() >= 3 && bytes[..3].iter().all(|c| (b'0'..=b'7').contains(c))
}

/// Escape a value for a resource line, so that `parse` reads it back
/// unchanged: backslashes and newlines are escaped, control characters and
/// leading or trailing blanks are written as octal escapes.
pub fn escape_value(value: &str) -> String {
    let start = value.len() - value.trim_start_matches([' ', '\t']).len();
    let end = value
        .trim_end_matches(|c: char| c.is_ascii_whitespace())
        .len();
    let mut escaped = String::with_capacity(value.len());
    for (i, c) in value.char_indices() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            ' ' if i < start || i >= end => escaped.push_str("\\040"),
            c if c.is_ascii_control() => escaped.push_str(&format!("\\{:03o}", c as u32)),
            c => escaped.push(c),
        }
    }
    escaped
}

/// Normalize a resource specification: leading tight bindings are dropped
/// and repeated bindings collapsed (`.a..b**c` becomes `a.b*c`). Returns
/// `None` if `spec` is not a valid specification: components are made of
/// letters, digits, `-` and `_`, or are a single `?`.
pub fn normalize_spec(spec: &str) -> Option<String> {
    let mut normalized = String::with_capacity(spec.len());
    let mut component = String::new();
    let mut prev = None;
    for c in spec.chars() {
        match c {
            '.' | '*' => {
                if !component.is_empty() {
                    push_component(&mut normalized, &mut component)?;
                }
                let leading_tight = normalized.is_empty() && c == '.';
                if prev != Some(c) && !leading_tight {
                    normalized.push(c);
                }
            }
            c if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '?' => component.push(c),
            _ => return None,
        }
        prev = Some(c);
    }
    if component.is_empty() {
        // empty, or ends with a binding
        return None;
    }
    push_component(&mut normalized, &mut component)?;
    Some(normalized)
}

fn push_component(normalized: &mut String, component: &mut String) -> Option<()> {
    if component.contains('?') && component != "?" {
        return None;
    }
    normalized.push_str(component);
    component.clear();
    Some(())
}

/// Split a specification into `(loose, component)` pairs: a component is
/// bound loosely if any `*` precedes it since the previous component.
fn components(spec: &str) -> Vec<(bool, &str)> {
    let mut parts = Vec::new();
    let mut loose = false;
    let mut rest = spec;
    while !rest.is_empty() {
        let start = rest.find(|c| c != '.' && c != '*').unwrap_or(rest.len());
        loose |= rest[..start].contains('*');
        rest = &rest[start..];
        let end = rest.find(['.', '*']).unwrap_or(rest.len());
        if end > 0 {
            parts.push((loose, &rest[..end]));
            loose = false;
        }
        rest = &rest[end..];
    }
    parts
}

/// Whether two normalized specifications denote the same entry for Xlib
/// (`*.a` and `*a` do).
pub fn same_spec(a: &str, b: &str) -> bool {
    a == b || (a.contains('*') && b.contains('*') && components(a) == components(b))
}

/// The key under which the entry for the normalized specification `spec`
/// is stored, if there is one.
pub fn find_spec<'a>(resources: &'a HashMap<String, String>, spec: &str) -> Option<&'a String> {
    match resources.get_key_value(spec) {
        Some((key, _)) => Some(key),
        None if spec.contains('*') => resources.keys().find(|key| same_spec(key, spec)),
        None => None,
    }
}

/// Rank of an entry component against a name component, per Xlib's
/// precedence rules: a matching component beats an elided level, a name
/// beats `?`, and a tight binding beats a loose one.
fn rank(loose: bool, component: &str, name: &str) -> Option<u8> {
    match (component == name, component == "?", loose) {
        (true, _, false) => Some(4),
        (true, _, true) => Some(3),
        (false, true, false) => Some(2),
        (false, true, true) => Some(1),
        _ => None,
    }
}

/// Best precedence, one rank per level of `name` (0 for an elided level),
/// with which `spec` matches `name`; `None` if it does not match.
/// Ranks compare left to right, as Xlib decides at the leftmost level.
fn precedence(spec: &[(bool, &str)], name: &[&str]) -> Option<Vec<u8>> {
    // best[i][j]: best ranks for spec[i..] matching name[j..]
    let mut best = vec![vec![None; name.len() + 1]; spec.len() + 1];
    best[spec.len()][name.len()] = Some(Vec::new());
    for i in (0..spec.len()).rev() {
        for j in (0..name.len()).rev() {
            let (loose, component) = spec[i];
            let matched = rank(loose, component, name[j])
                .and_then(|r| best[i + 1][j + 1].as_ref().map(|rest| (r, rest)));
            let elided = if loose {
                best[i][j + 1].as_ref().map(|rest| (0, rest))
            } else {
                None
            };
            best[i][j] = matched
                .into_iter()
                .chain(elided)
                .map(|(r, rest): (u8, &Vec<u8>)| {
                    let mut ranks = Vec::with_capacity(rest.len() + 1);
                    ranks.push(r);
                    ranks.extend_from_slice(rest);
                    ranks
                })
                .max();
        }
    }
    best[0][0].take()
}

/// Look `name` up among `(specification, value)` entries like Xlib's
/// `XrmGetResource` with the name as class, and return the value of the
/// entry that wins.
pub fn lookup<'a, I>(entries: I, name: &str) -> Option<&'a String>
where
    I: IntoIterator<Item = (&'a String, &'a String)>,
{
    let name: Vec<&str> = name.split(['.', '*']).filter(|c| !c.is_empty()).collect();
    if name.is_empty() {
        return None;
    }
    entries
        .into_iter()
        .filter_map(|(spec, value)| {
            precedence(&components(spec), &name).map(|ranks| (ranks, value))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, value)| value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(db: &[(&str, &str)], name: &str) -> Option<String> {
        let db: HashMap<String, String> = db
            .iter()
            .map(|(k, v)| (normalize_spec(k).unwrap(), v.to_string()))
            .collect();
        lookup(&db, name).cloned()
    }

    #[test]
    fn continued_lines() {
        let parsed =
            parse("k.a: one \\\ntwo\nk.\\\nb: v\napp.v: first\\\n  second\napp.eof: end\\\npoint");
        assert_eq!(
            parsed,
            [
                ("k.a".to_string(), "one two".to_string()),
                ("k.b".to_string(), "v".to_string()),
                ("app.v".to_string(), "first  second".to_string()),
                ("app.eof".to_string(), "endpoint".to_string()),
            ]
        );
    }

    #[test]
    fn value_escapes() {
        let parsed = parse(
            "a: a\\\\b\nb: \\040lead\nc: x\\101y\nd: x\\ny\ne: \\ \\ two\nf: a\\\\nb\ng: end\\\\\nh: \\1012\ni: v   \nj: v\\040\n",
        );
        let values: Vec<&str> = parsed.iter().map(|(_, v)| v.as_str()).collect();
        assert_eq!(
            values,
            ["a\\b", " lead", "xAy", "x\ny", "  two", "a\\nb", "end\\", "A2", "v", "v "]
        );
    }

    #[test]
    fn comments_and_invalid_lines() {
        let parsed = parse("! k.x: no\n#define X y\n  k.a  :  pango:Sans 10\nno colon\nk.b:: x\n");
        assert_eq!(
            parsed,
            [
                ("k.a".to_string(), "pango:Sans 10".to_string()),
                ("k.b".to_string(), ": x".to_string()),
            ]
        );
    }

    #[test]
    fn escape_round_trip() {
        for value in [
            "",
            "plain",
            "  pad",
            "a\\nb",
            "x\ny",
            "end\\",
            "\ttab",
            "a\tb",
            "trail ",
            "\"q\"",
            "五 café",
        ] {
            let line = format!("k: {}\n", escape_value(value));
            assert_eq!(
                parse(&line),
                [("k".to_string(), value.to_string())],
                "{line:?}"
            );
        }
    }

    #[test]
    fn normalized_specs() {
        assert_eq!(normalize_spec(".app.key").as_deref(), Some("app.key"));
        assert_eq!(normalize_spec("app**leaf").as_deref(), Some("app*leaf"));
        assert_eq!(normalize_spec("*.empty").as_deref(), Some("*.empty"));
        assert_eq!(normalize_spec("?.b").as_deref(), Some("?.b"));
        assert_eq!(normalize_spec("a?.b"), None);
        assert_eq!(normalize_spec("a."), None);
        assert_eq!(normalize_spec("a b"), None);
        assert!(same_spec("*.empty", "*empty"));
        assert!(!same_spec("k.a", "k*a"));
    }

    #[test]
    fn xlib_precedence() {
        assert_eq!(
            get(&[("*b", "loose"), ("a.b", "tight")], "x.b").as_deref(),
            Some("loose")
        );
        assert_eq!(
            get(&[("*b", "loose"), ("a.b", "tight")], "a.b").as_deref(),
            Some("tight")
        );
        assert_eq!(
            get(&[("a*c", "left"), ("*b.c", "right")], "a.b.c").as_deref(),
            Some("left")
        );
        assert_eq!(
            get(&[("*b", "star"), ("?.b", "q")], "a.b").as_deref(),
            Some("q")
        );
        assert_eq!(
            get(&[("*b", "star"), ("?.b", "q")], "b").as_deref(),
            Some("star")
        );
        assert_eq!(
            get(&[("a*b.c", "loose"), ("a.b*c", "tight")], "a.b.c").as_deref(),
            Some("tight")
        );
        assert_eq!(
            get(&[("i3-wm*color", "red")], "i3-wm.bar.x.color").as_deref(),
            Some("red")
        );
        assert_eq!(get(&[("i3-wm*color", "red")], "x.i3-wm.color"), None);
        assert_eq!(get(&[("a*?.leaf", "mixed")], "a.leaf"), None);
        assert_eq!(
            get(&[("a*a.b", "repeated")], "a.a.x.a.b").as_deref(),
            Some("repeated")
        );
    }
}
