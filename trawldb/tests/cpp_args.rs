//! End-to-end check of preprocessor arguments that contain spaces:
//! trawldb client -> D-Bus -> trawld -> cpp.
//!
//! Unlike `integration_tests.rs`, this doesn't need a running trawld: it
//! starts a private session bus and serves trawld in-process.

use clap::Parser;
use std::{
    fs,
    io::{BufRead, BufReader, ErrorKind},
    path::PathBuf,
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

fn client_args(
    load: Option<&str>,
    merge: Option<&str>,
    include: &[&str],
    define: &[&str],
) -> CliArgs {
    CliArgs {
        nocpp: false,
        filename: None,
        cpp: None,
        load: load.map(String::from),
        merge: merge.map(String::from),
        edit: None,
        backup: None,
        get: None,
        query: None,
        remove: false,
        include: include.iter().map(|s| s.to_string()).collect(),
        define: define.iter().map(|s| s.to_string()).collect(),
    }
}

async fn get(client: &Client<'_>, key: &str) -> String {
    client.proxy().get_resource(key).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn cpp_args_with_spaces() {
    let Some(_bus) = start_session_bus() else {
        eprintln!("dbus-daemon not found, skipping");
        return;
    };
    let daemon_args = trawld::parser::CliArgs::try_parse_from(["trawld"]).unwrap();
    let _daemon = trawld::ResourceManager::from_args(&daemon_args)
        .run_server()
        .await
        .unwrap();

    // A home directory with a space, as in "/home/Jane Doe"
    let mut home: PathBuf = std::env::temp_dir();
    home.push(format!("trawldb-{} home", Uuid::new_v4()));
    let include_dir = home.join("Xresources.d");
    fs::create_dir_all(&include_dir).unwrap();
    fs::write(include_dir.join("extra"), "extra.key: 1\n").unwrap();
    fs::write(home.join("local"), "local.key: 2\n").unwrap();
    let contents = "#include <extra>\n#include \"local\"\nname.key: NAME\n";
    let file = home.join("Xresources");
    fs::write(&file, contents).unwrap();
    let plain_contents = "plain.key: NAME\n";
    let plain = home.join("plain");
    fs::write(&plain, plain_contents).unwrap();
    let file_str = file.to_str().unwrap();
    let plain_str = plain.to_str().unwrap();
    let include_str = include_dir.to_str().unwrap();

    let client = Client::new().await.unwrap();

    // -I with a space
    let args = client_args(None, Some(file_str), &[include_str], &[]);
    client.run(&args).await.unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), contents);
    assert_eq!(get(&client, "extra.key").await, "1");
    assert_eq!(get(&client, "local.key").await, "2");
    assert_eq!(get(&client, "name.key").await, "NAME");

    // -D with a space
    client.proxy().remove_all().await.unwrap();
    let args = client_args(None, Some(plain_str), &[], &["NAME=a b"]);
    client.run(&args).await.unwrap();
    assert_eq!(fs::read_to_string(&plain).unwrap(), plain_contents);
    assert_eq!(get(&client, "plain.key").await, "a b");

    // Both, through load
    client.proxy().remove_all().await.unwrap();
    let args = client_args(Some(file_str), None, &[include_str], &["NAME=a b"]);
    client.run(&args).await.unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), contents);
    assert_eq!(get(&client, "extra.key").await, "1");
    assert_eq!(get(&client, "local.key").await, "2");
    assert_eq!(get(&client, "name.key").await, "a b");

    fs::remove_dir_all(&home).unwrap();
}
