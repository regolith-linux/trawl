//! Reading resource input: a file, read raw or through the preprocessor.
//!
//! Everything here blocks (a FIFO with no writer, a stalled mount or a slow
//! preprocessor can block for as long as they like), so the daemon only calls
//! it from a blocking task, never while it holds the resources.

use std::{
    fs::File,
    io::Read,
    path::Path,
    process::{Command, Stdio},
};

/// Result of reading one input: the text to parse and the preprocessor's
/// diagnostics, if it printed any.
pub struct ReadInput {
    pub text: String,
    pub diagnostics: String,
}

/// Reads `path`, through the preprocessor unless `nocpp` is set. Fails if
/// the file can't be opened or read, or the preprocessor can't be run.
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
    if nocpp {
        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|e| format!("can't read file '{path}': {e}"))?;
        return Ok(ReadInput {
            text,
            diagnostics: String::new(),
        });
    }
    let output = preprocessor_command(cpp, cpp_args, path, file)
        .output()
        .map_err(|e| format!("can't run preprocessor '{cpp}': {e}"))?;
    let text =
        String::from_utf8(output.stdout).map_err(|e| format!("can't read file '{path}': {e}"))?;
    Ok(ReadInput {
        text,
        diagnostics: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Builds the preprocessor command for a resource file.
///
/// The file is passed on stdin and never named on the command line, so
/// the preprocessor can't take it for its output file, whatever the
/// arguments are. The trailing "-" (stdin) keeps cpp's optional output
/// operand from naming a file. The command runs in the file's directory
/// so that `#include "..."` stays relative to the file.
fn preprocessor_command(cpp: &str, cpp_args: &[String], file_path: &str, input: File) -> Command {
    let mut command = Command::new(cpp);
    command
        .args(cpp_args)
        .arg("-")
        .stdin(input)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = Path::new(file_path).parent() {
        if !dir.as_os_str().is_empty() {
            command.current_dir(dir);
        }
    }
    command
}
