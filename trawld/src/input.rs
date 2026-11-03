//! Reading resource input: a file, read raw or through the preprocessor.
//!
//! Everything here blocks (a FIFO with no writer, a stalled mount or a slow
//! preprocessor can block for as long as they like), so the daemon only calls
//! it from a blocking task, never while it holds the resources.

use std::{
    fs::{self, File},
    path::Path,
    process::Command,
};

/// Reads `path`, through the preprocessor unless `nocpp` is set.
pub fn read_file(
    path: &str,
    nocpp: bool,
    cpp: &str,
    cpp_args: &[String],
) -> Result<String, String> {
    if nocpp {
        return fs::read_to_string(path).map_err(|e| e.to_string());
    }
    let output = preprocessor_command(cpp, cpp_args, path)
        .and_then(|mut command| command.output())
        .map_err(|e| e.to_string())?;
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}

/// Builds the preprocessor command for a resource file.
///
/// The file is passed on stdin and never named on the command line, so
/// the preprocessor can't take it for its output file, whatever the
/// arguments are. The trailing "-" (stdin) keeps cpp's optional output
/// operand from naming a file. The command runs in the file's directory
/// so that `#include "..."` stays relative to the file.
fn preprocessor_command(
    cpp: &str,
    cpp_args: &[String],
    file_path: &str,
) -> std::io::Result<Command> {
    let input = File::open(file_path)?;
    let mut command = Command::new(cpp);
    command.args(cpp_args).arg("-").stdin(input);
    if let Some(dir) = Path::new(file_path).parent() {
        if !dir.as_os_str().is_empty() {
            command.current_dir(dir);
        }
    }
    Ok(command)
}
