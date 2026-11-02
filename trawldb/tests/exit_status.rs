//! Runs the trawldb binary to check that a failed command is reported and
//! exits with a non-zero status, like xrdb.
//!
//! Like `cpp_args.rs`, this starts a private session bus, so it doesn't
//! need a running trawld. Without one on that bus, every command fails.

use clap::Parser;
use std::{
    fs,
    io::{BufRead, BufReader, ErrorKind},
    process::{Child, Command, Output, Stdio},
};
use uuid::Uuid;

/// Private dbus-daemon, killed when dropped
struct SessionBus(Child);

impl Drop for SessionBus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Starts a private session bus and points DBUS_SESSION_BUS_ADDRESS at it.
/// Returns None if dbus-daemon isn't installed.
fn start_session_bus() -> Option<SessionBus> {
    let listen = format!("--address=unix:tmpdir={}", std::env::temp_dir().display());
    let spawned = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address", &listen])
        .stdout(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) if e.kind() == ErrorKind::NotFound => return None,
        Err(e) => panic!("failed to start dbus-daemon: {e}"),
    };
    let mut address = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    std::env::set_var("DBUS_SESSION_BUS_ADDRESS", address.trim());
    Some(SessionBus(child))
}

/// Runs the trawldb binary on the private bus
async fn trawldb(args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_trawldb"))
            .args(&args)
            .stdin(Stdio::null())
            .output()
            .expect("failed to run trawldb")
    })
    .await
    .unwrap()
}

fn assert_fails(out: &Output, what: &str) {
    assert_eq!(out.status.code(), Some(1), "{what}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("trawldb: "), "{what}: {stderr}");
}

#[tokio::test(flavor = "multi_thread")]
async fn failures_exit_1() {
    let Some(_bus) = start_session_bus() else {
        eprintln!("dbus-daemon not found, skipping");
        return;
    };
    let dir = std::env::temp_dir().join(format!("trawldb-exit-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("res");
    fs::write(&file, "a.key: 1\n").unwrap();
    let file = file.to_str().unwrap();

    // trawld isn't running on this bus
    for args in [
        &["--query"][..],
        &["--get", "a.key"],
        &["--merge", file],
        &["--load", file],
        &["--remove"],
    ] {
        assert_fails(&trawldb(args).await, &args.join(" "));
    }

    // --edit leaves the file alone when trawld can't be asked
    let out = trawldb(&["--edit", file]).await;
    assert_fails(&out, "--edit");
    assert_eq!(fs::read_to_string(file).unwrap(), "a.key: 1\n");
    assert!(!dir.join("res.bak").exists());

    // With trawld running, a file that can't be opened is a failure too
    let daemon_args = trawld::parser::CliArgs::try_parse_from(["trawld"]).unwrap();
    let _daemon = trawld::ResourceManager::from_args(&daemon_args)
        .run_server()
        .await
        .unwrap();
    let missing = dir.join("missing");
    let missing = missing.to_str().unwrap();
    assert_fails(&trawldb(&["--merge", missing]).await, "--merge missing");

    // and success is still 0
    let out = trawldb(&["--merge", file]).await;
    assert_eq!(out.status.code(), Some(0));
    let out = trawldb(&["--get", "a.key"]).await;
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "1\n");

    fs::remove_dir_all(&dir).unwrap();
}
