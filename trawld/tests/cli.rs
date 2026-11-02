//! Runs the trawld binary to check how it handles its command line

use std::process::{Command, Output};

fn trawld(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_trawld"))
        .args(args)
        .output()
        .expect("failed to run trawld")
}

#[test]
fn version_exits_0() {
    let out = trawld(&["--version"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.trim(),
        format!("trawld {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn help_exits_0() {
    for flag in ["--help", "-h"] {
        let out = trawld(&[flag]);
        assert_eq!(out.status.code(), Some(0), "{flag}");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("--nocpp"),
            "{flag}"
        );
        assert!(
            out.stderr.is_empty(),
            "{flag}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn bad_option_still_fails() {
    let out = trawld(&["--no-such-option"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("Error parsing:"));
}
