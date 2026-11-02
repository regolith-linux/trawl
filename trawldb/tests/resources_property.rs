//! End-to-end check of the `Resources` property through the trawldb proxy.
//!
//! Like `cpp_args.rs`, this starts a private session bus and serves trawld
//! in-process, so it doesn't need a running trawld.

use clap::Parser;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, ErrorKind},
    process::{Child, Command, Stdio},
};
use trawldb::Client;

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

#[tokio::test(flavor = "multi_thread")]
async fn resources_property() {
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
    assert_eq!(client.proxy().resources().await.unwrap(), HashMap::new());

    let proxy = client.proxy();
    proxy
        .set_resource(String::from("k.a"), String::from("1"))
        .await
        .unwrap();
    proxy
        .set_resource(String::from("k.b"), String::from("2"))
        .await
        .unwrap();
    let expected: HashMap<String, String> = [("k.a", "1"), ("k.b", "2")]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    // A new client reads the current value from trawld
    let other = Client::new().await.unwrap();
    assert_eq!(other.proxy().resources().await.unwrap(), expected);

    // The first client's cached value follows the PropertiesChanged signals
    let mut current = HashMap::new();
    for _ in 0..50 {
        current = client.proxy().resources().await.unwrap();
        if current == expected {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(current, expected);
}
