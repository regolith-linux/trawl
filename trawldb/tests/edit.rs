//! End-to-end check of `--edit`: like `xrdb -edit`, the current values are
//! edited into the file, and everything else in it stays.
//!
//! Like `cpp_args.rs`, this starts a private session bus and serves trawld
//! in-process, so it doesn't need a running trawld.

use clap::Parser;
use std::{
    fs,
    io::{BufRead, BufReader, ErrorKind},
    process::{Child, Command, Stdio},
};
use trawldb::{parser::CliArgs, Client};
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

/// Parses a trawldb command line
fn trawldb(args: &[&str]) -> CliArgs {
    CliArgs::try_parse_from([&["trawldb"], args].concat()).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn edit_keeps_the_file() {
    let Some(_bus) = start_session_bus() else {
        eprintln!("dbus-daemon not found, skipping");
        return;
    };
    let daemon_args = trawld::parser::CliArgs::try_parse_from(["trawld"]).unwrap();
    let _daemon = trawld::ResourceManager::from_args(&daemon_args)
        .run_server()
        .await
        .unwrap();
    let client = Client::new().await.unwrap();
    for (key, value) in [("k.a", "new"), ("k.b", "newb")] {
        client
            .proxy()
            .set_resource(key.to_string(), value.to_string())
            .await
            .unwrap();
    }
    let listing = client.proxy().query("").await.unwrap();
    let line = |key: &str| {
        listing
            .lines()
            .find(|l| l.starts_with(key))
            .unwrap()
            .to_string()
    };
    let (line_a, line_b) = (line("k.a"), line("k.b"));

    let dir = std::env::temp_dir().join(format!("trawldb-edit-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("Xresources");
    let original = "! my comment\nk.a: old\nother.key: keepme\n#define FOO bar\n";
    fs::write(&file, original).unwrap();
    let path = file.to_str().unwrap();

    client.run(&trawldb(&["--edit", path])).await.unwrap();
    let expected =
        format!("! my comment\n{line_a}\nother.key: keepme\n#define FOO bar\n{line_b}\n");
    assert_eq!(fs::read_to_string(&file).unwrap(), expected);
    assert_eq!(
        fs::read_to_string(dir.join("Xresources.bak")).unwrap(),
        original
    );

    // A second edit changes nothing but the values
    client
        .proxy()
        .set_resource(String::from("k.a"), String::from("newer"))
        .await
        .unwrap();
    let listing = client.proxy().query("").await.unwrap();
    let line_a = listing.lines().find(|l| l.starts_with("k.a")).unwrap();
    client
        .run(&trawldb(&["--edit", path, "--backup", ".saved"]))
        .await
        .unwrap();
    let expected2 =
        format!("! my comment\n{line_a}\nother.key: keepme\n#define FOO bar\n{line_b}\n");
    assert_eq!(fs::read_to_string(&file).unwrap(), expected2);
    assert_eq!(
        fs::read_to_string(dir.join("Xresources.saved")).unwrap(),
        expected
    );

    // A file that doesn't exist yet gets the resources
    let new_file = dir.join("new");
    client
        .run(&trawldb(&["--edit", new_file.to_str().unwrap()]))
        .await
        .unwrap();
    assert_eq!(
        fs::read_to_string(&new_file).unwrap(),
        format!("{line_a}\n{line_b}\n")
    );
    assert!(!dir.join("new.bak").exists());

    fs::remove_dir_all(&dir).unwrap();
}
