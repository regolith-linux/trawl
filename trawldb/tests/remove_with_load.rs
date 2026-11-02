//! End-to-end check of `--remove` combined with `--load`/`--merge`:
//! the database is cleared first, then the file is read, so the file's
//! resources replace the old ones.
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

/// The values of the keys this test uses ("" for a missing key)
async fn values(client: &Client<'_>) -> Vec<String> {
    let mut values = Vec::new();
    for key in ["old.key", "new.a", "new.b"] {
        values.push(client.proxy().get_resource(key).await.unwrap());
    }
    values
}

#[tokio::test(flavor = "multi_thread")]
async fn remove_then_load_or_merge() {
    let Some(_bus) = start_session_bus() else {
        eprintln!("dbus-daemon not found, skipping");
        return;
    };
    let daemon_args = trawld::parser::CliArgs::try_parse_from(["trawld"]).unwrap();
    let _daemon = trawld::ResourceManager::from_args(&daemon_args)
        .run_server()
        .await
        .unwrap();

    let file = std::env::temp_dir().join(format!("{}.res", Uuid::new_v4()));
    fs::write(&file, "new.a: 1\nnew.b: 2\n").unwrap();
    let file = file.to_str().unwrap();
    // old.key is gone, the file's resources are there
    let expected = ["", "1", "2"];

    let client = Client::new().await.unwrap();
    let cases: [&[&str]; 5] = [
        &["--remove", "--merge", file],
        &["--merge", file, "--remove"],
        &["--remove", "--load", file],
        &["--remove", file],
        &["--remove", "--nocpp", "--merge", file],
    ];
    for case in cases {
        client
            .proxy()
            .set_resource(String::from("old.key"), String::from("x"))
            .await
            .unwrap();
        client.run(&trawldb(case)).await.unwrap();
        assert_eq!(
            values(&client).await,
            expected,
            "trawldb {}",
            case.join(" ")
        );
    }

    // --remove alone still empties the database
    client.run(&trawldb(&["--remove"])).await.unwrap();
    assert_eq!(client.proxy().query("").await.unwrap(), "");

    fs::remove_file(file).unwrap();
}
