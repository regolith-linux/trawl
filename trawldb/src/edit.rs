//! `trawldb --edit`: puts the current resources into a resource file the
//! way `xrdb -edit` does.

/// Edits `listing` (trawld's `Query("")` output, one `name:<TAB>value` line
/// per resource) into the contents of a resource file.
///
/// The first entry in the file for a resource in the listing is replaced by
/// the listing's line (a continued entry is replaced as a whole). Resources
/// the file doesn't have are appended in listing order. Everything else
/// stays as it is: comments, preprocessor lines, blank lines, later
/// duplicates, entries for other resources and bytes that aren't UTF-8.
/// Every line of the result ends with a newline.
pub fn edit_into(file: &[u8], listing: &str) -> Vec<u8> {
    let mut entries: Vec<(&str, &str, bool)> = listing
        .lines()
        .filter_map(|line| {
            let (name, _) = line.split_once(':')?;
            let name = name.trim();
            (!name.is_empty()).then_some((name, line, false))
        })
        .collect();
    let mut out = Vec::with_capacity(file.len() + listing.len());
    for line in logical_lines(file) {
        let entry = entry_name(line).and_then(|name| {
            entries
                .iter_mut()
                .find(|(n, _, done)| !*done && n.as_bytes() == name)
        });
        match entry {
            Some((_, new_line, done)) => {
                out.extend_from_slice(new_line.as_bytes());
                out.push(b'\n');
                *done = true;
            }
            None => {
                out.extend_from_slice(line);
                if !line.ends_with(b"\n") {
                    out.push(b'\n');
                }
            }
        }
    }
    for (_, line, done) in entries {
        if !done {
            out.extend_from_slice(line.as_bytes());
            out.push(b'\n');
        }
    }
    out
}

/// Splits `file` into logical lines, each with its newline: a line ending
/// in a backslash continues on the next one.
fn logical_lines(file: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, &b) in file.iter().enumerate() {
        if b == b'\n' && !(i > start && file[i - 1] == b'\\') {
            lines.push(&file[start..=i]);
            start = i + 1;
        }
    }
    if start < file.len() {
        lines.push(&file[start..]);
    }
    lines
}

/// The resource name of an entry line, or None for anything else (a
/// comment, a preprocessor line, a blank line, a line without a colon).
fn entry_name(line: &[u8]) -> Option<&[u8]> {
    let line = trim_ascii_start(line);
    if line.is_empty() || line[0] == b'!' || line[0] == b'#' {
        return None;
    }
    let colon = line.iter().position(|&b| b == b':')?;
    let name = trim_ascii(&line[..colon]);
    (!name.is_empty()).then_some(name)
}

// Local equivalents of `<[u8]>::trim_ascii_start` / `trim_ascii`, which are
// only stable since Rust 1.80; Ubuntu jammy and noble ship rustc 1.75.
fn trim_ascii_start(s: &[u8]) -> &[u8] {
    let n = s.iter().take_while(|b| b.is_ascii_whitespace()).count();
    &s[n..]
}

fn trim_ascii(s: &[u8]) -> &[u8] {
    let s = trim_ascii_start(s);
    let n = s.iter().rev().take_while(|b| b.is_ascii_whitespace()).count();
    &s[..s.len() - n]
}

#[cfg(test)]
mod tests {
    use super::edit_into;

    const LISTING: &str = "k.a:\tnew\nk.b:\tnewb";

    fn edit(file: &[u8]) -> Vec<u8> {
        edit_into(file, LISTING)
    }

    #[test]
    fn keeps_the_rest_of_the_file() {
        let file = b"! my comment\nk.a: old\nother.key: keepme\n#define FOO bar\n";
        let expected = b"! my comment\nk.a:\tnew\nother.key: keepme\n#define FOO bar\nk.b:\tnewb\n";
        assert_eq!(edit(file), expected);
    }

    #[test]
    fn replaces_the_first_entry_only() {
        let file = b"k.a: 1\n  k.a  :   2\n";
        assert_eq!(edit(file), b"k.a:\tnew\n  k.a  :   2\nk.b:\tnewb\n");
    }

    #[test]
    fn replaces_a_continued_entry() {
        let file = b"k.a: one \\\n two\nlast.key: x";
        assert_eq!(edit(file), b"k.a:\tnew\nlast.key: x\nk.b:\tnewb\n");
    }

    #[test]
    fn keeps_bytes_and_blank_lines() {
        let file = b"old file\n\xff\n\n\nk.b: x\n\n";
        assert_eq!(edit(file), b"old file\n\xff\n\n\nk.b:\tnewb\n\nk.a:\tnew\n");
    }

    #[test]
    fn empty_file_gets_the_listing() {
        assert_eq!(edit(b""), b"k.a:\tnew\nk.b:\tnewb\n");
        assert_eq!(edit_into(b"a: b\n", ""), b"a: b\n");
    }

    #[test]
    fn older_listing_format() {
        // trawld versions that print "name :<TAB>value"
        let listing = "k.a :\tnew\nk.b :\tnewb";
        assert_eq!(
            edit_into(b"k.b: old\n", listing),
            b"k.b :\tnewb\nk.a :\tnew\n"
        );
    }
}
