//! Reading resource input: a file, read raw or through the preprocessor.
//!
//! Everything here blocks (a FIFO with no writer, a stalled mount or a slow
//! preprocessor can block for as long as they like), so the daemon only calls
//! it from a blocking task, never while it holds the resources.

use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};

/// Result of reading one input: the text to parse and the preprocessor's
/// diagnostics, if it printed any.
pub struct ReadInput {
    pub text: String,
    pub diagnostics: String,
}

/// Bytes to text. Resource files are bytes (Xlib stores them as they are);
/// D-Bus strings must be UTF-8, so a byte that isn't part of a valid UTF-8
/// sequence becomes U+FFFD instead of rejecting the whole input.
pub fn to_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// GCC's preprocessor, in its default C mode, writes every non-ASCII
/// character outside quotes as a universal character name: `café` comes out
/// as `caf\U000000e9`. Resource values aren't C and valid UTF-8 must stay
/// unchanged, so each such sequence (`\U` and eight lowercase hex digits, as
/// GCC writes them) becomes the character it stands for again.
pub fn undo_ucns(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("\\U") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 2..];
        match ucn_char(after.as_bytes()) {
            Some(c) => {
                out.push(c);
                rest = &after[8..];
            }
            None => {
                out.push_str("\\U");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The character named by the eight lowercase hex digits at the start of
/// `hex`, if `\U` followed by them is a sequence `undo_ucns` decodes.
fn ucn_char(hex: &[u8]) -> Option<char> {
    let hex = hex.get(..8)?;
    if !hex.iter().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return None;
    }
    let code = u32::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
    char::from_u32(code).filter(|c| !c.is_ascii())
}

/// `\U` sequences the user wrote are not the preprocessor's and must come
/// back as written, as with xrdb. Before preprocessing, each one that
/// `undo_ucns` would decode has its `\U` replaced by a marker made of
/// identifier characters (so the preprocessor tokenizes and passes it like
/// the escape it replaces, inside quotes or not) that occurs nowhere in the
/// input; `restore_ucns` puts the `\U` back after decoding. Include
/// directives are left alone: their file names must stay as written.
/// No marker when there was nothing to hide.
pub fn protect_ucns(input: &[u8]) -> (Vec<u8>, Option<String>) {
    let mut marker: Option<String> = None;
    let mut out = Vec::with_capacity(input.len());
    for (n, line) in input.split(|&b| b == b'\n').enumerate() {
        if n > 0 {
            out.push(b'\n');
        }
        if is_include(line) {
            out.extend_from_slice(line);
            continue;
        }
        let mut i = 0;
        while i < line.len() {
            if line[i..].starts_with(b"\\U") && ucn_char(&line[i + 2..]).is_some() {
                let marker = marker.get_or_insert_with(|| new_marker(input));
                out.extend_from_slice(marker.as_bytes());
                i += 2;
            } else {
                out.push(line[i]);
                i += 1;
            }
        }
    }
    (out, marker)
}

/// Decodes the preprocessor's universal character names, then gives back
/// the `\U` sequences `protect_ucns` hid.
pub fn restore_ucns(text: &str, marker: Option<&str>) -> String {
    let decoded = undo_ucns(text);
    match marker {
        Some(marker) => decoded.replace(marker, "\\U"),
        None => decoded,
    }
}

/// An include directive (`#include`, `#include_next`, `#import`, `#embed`)
fn is_include(line: &[u8]) -> bool {
    let rest = line.trim_ascii_start();
    let Some(rest) = rest.strip_prefix(b"#") else {
        return false;
    };
    let rest = rest.trim_ascii_start();
    [&b"include"[..], b"import", b"embed"]
        .iter()
        .any(|word| rest.starts_with(word))
}

/// A marker that isn't in `input`: random, so that it isn't in the files the
/// preprocessor pulls in either.
fn new_marker(input: &[u8]) -> String {
    use std::hash::{BuildHasher, Hasher};
    loop {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_usize(input.len());
        let marker = format!("_trawlU{:016x}_", hasher.finish());
        if !input.windows(marker.len()).any(|w| w == marker.as_bytes()) {
            return marker;
        }
    }
}

/// Why the preprocessor's output can't be had
pub enum CppError {
    /// The preprocessor couldn't be started at all
    NotRun(String),
    /// Anything else
    Failed(String),
}

/// Runs the preprocessor on `input` (fed through its stdin, as `-`), with
/// the user's `\U` sequences kept as written. Its diagnostics go to
/// `stderr`; with `Stdio::piped()` they come back in the second field.
pub fn preprocess(
    cpp: &str,
    cpp_args: &[String],
    include_dir: Option<&Path>,
    input: &[u8],
    stderr: Stdio,
) -> Result<(String, String), CppError> {
    let (protected, marker) = protect_ucns(input);
    let mut command = Command::new(cpp);
    command.args(cpp_args);
    if let Some(dir) = include_dir {
        command.arg("-iquote").arg(dir);
    }
    let mut child = command
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(stderr)
        .spawn()
        .map_err(|e| CppError::NotRun(format!("can't run preprocessor '{cpp}': {e}")))?;
    // Written from another thread while the output is read here, so neither
    // side can fill its pipe and wait for the other. A preprocessor that
    // stops reading early just loses the rest.
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&protected);
    });
    let output = child
        .wait_with_output()
        .map_err(|e| CppError::Failed(format!("preprocessor '{cpp}' failed: {e}")))?;
    let _ = writer.join();
    Ok((
        restore_ucns(&to_text(&output.stdout), marker.as_deref()),
        to_text(&output.stderr),
    ))
}

/// Reads `path` as given (symlinks are not resolved, so `#include "..."`
/// resolves against the directory of the path the caller named, as with xrdb).
pub fn read_file(
    path: &str,
    nocpp: bool,
    cpp: &str,
    cpp_args: &[String],
) -> Result<ReadInput, String> {
    let mut file = File::open(path).map_err(|e| format!("can't open file '{path}': {e}"))?;
    if file.metadata().map(|m| m.is_dir()).unwrap_or(false) {
        return Err(format!("can't read file '{path}': it is a directory"));
    }
    // Read once, whole: a FIFO can't be opened twice.
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .map_err(|e| format!("can't read file '{path}': {e}"))?;
    if nocpp {
        return Ok(ReadInput {
            text: to_text(&buf),
            diagnostics: String::new(),
        });
    }
    // Quoted includes resolve against the directory of the path as given:
    // symlinks are not resolved first, as with xrdb.
    match preprocess(cpp, cpp_args, include_dir(path), &buf, Stdio::piped()) {
        Ok((text, diagnostics)) => Ok(ReadInput { text, diagnostics }),
        Err(CppError::NotRun(e) | CppError::Failed(e)) => Err(e),
    }
}

/// The directory of `path` as given (not resolved through symlinks), for
/// quoted includes. None when the path has no directory part: the
/// preprocessor already searches the current directory.
pub fn include_dir(path: &str) -> Option<&Path> {
    Path::new(path)
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_bytes_become_replacement_characters() {
        assert_eq!(to_text(b"a.key: caf\xe9\n"), "a.key: caf\u{fffd}\n");
        assert_eq!(to_text("b.key: café\n".as_bytes()), "b.key: café\n");
    }

    #[test]
    fn undoes_gcc_ucns() {
        assert_eq!(undo_ucns("caf\\U000000e9"), "café");
        assert_eq!(undo_ucns("\\U0001f600!"), "\u{1f600}!");
        // Not what GCC writes: kept as is
        assert_eq!(undo_ucns("\\U000000E9"), "\\U000000E9");
        assert_eq!(undo_ucns("\\U00000041"), "\\U00000041");
        assert_eq!(undo_ucns("\\U0000e9"), "\\U0000e9");
        assert_eq!(undo_ucns("\\Uffffffff"), "\\Uffffffff");
        assert_eq!(undo_ucns("tail\\U"), "tail\\U");
    }

    #[test]
    fn protects_user_ucns() {
        let input = b"a.key: \\U000000e9\n#include \"\\U000000e9.x\"\n";
        let (protected, marker) = protect_ucns(input);
        let marker = marker.expect("a marker");
        let protected = String::from_utf8(protected).unwrap();
        assert!(!protected.starts_with("a.key: \\U"));
        assert!(protected.contains(&marker));
        // include lines stay as written
        assert!(protected.ends_with("#include \"\\U000000e9.x\"\n"));
        assert!(restore_ucns(&protected, Some(&marker)).starts_with("a.key: \\U000000e9\n"));
        // nothing to hide, no marker
        assert_eq!(protect_ucns(b"a: b\n"), (b"a: b\n".to_vec(), None));
    }

    #[test]
    fn non_ascii_survives_cpp() {
        let input = "a.key: café\nb.key: \"naïve\"\nc.key: \\U000000e9\n";
        let (text, _) = preprocess("/usr/bin/cpp", &[], None, input.as_bytes(), Stdio::null())
            .ok()
            .expect("cpp runs");
        assert!(text.contains("a.key: café\n"), "{text}");
        assert!(text.contains("b.key: \"naïve\"\n"), "{text}");
        assert!(text.contains("c.key: \\U000000e9\n"), "{text}");
    }
}
