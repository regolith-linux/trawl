//! End-to-end checks of the trawldb client against trawld.
//!
//! Every test starts its own private session bus and serves trawld in-process
//! on it, so no running trawld is needed and the user's session bus is left
//! alone. The tests take turns: the bus address is the process-wide
//! DBUS_SESSION_BUS_ADDRESS, so two tests running at once would share one
//! daemon and clear each other's resources.

mod common;

use clap::Parser;
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, ErrorKind},
    process::{Child, Command, Stdio},
};
use tokio::sync::{Mutex, MutexGuard};

use trawldb::Client;
use common::*;

/// Held by the running test, so the tests run one at a time
static ONE_AT_A_TIME: Mutex<()> = Mutex::const_new(());

/// Private dbus-daemon, killed when dropped
struct SessionBus(Child);

impl Drop for SessionBus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A trawld of the test's own on a private session bus. The fields drop in
/// order: the daemon, then the bus, then the turn.
struct Trawld {
    _daemon: zbus::Connection,
    _bus: SessionBus,
    _turn: MutexGuard<'static, ()>,
}

/// Waits for this test's turn, starts a private session bus, points
/// DBUS_SESSION_BUS_ADDRESS at it and serves trawld there.
/// Returns None if dbus-daemon isn't installed.
async fn start_trawld() -> Option<Trawld> {
    let turn = ONE_AT_A_TIME.lock().await;
    let listen = format!("--address=unix:tmpdir={}", std::env::temp_dir().display());
    let spawned = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address", &listen])
        .stdout(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            eprintln!("dbus-daemon not found, skipping");
            return None;
        }
        Err(e) => panic!("failed to start dbus-daemon: {e}"),
    };
    let mut address = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    std::env::set_var("DBUS_SESSION_BUS_ADDRESS", address.trim());
    let bus = SessionBus(child);
    let daemon_args = trawld::parser::CliArgs::try_parse_from(["trawld"]).unwrap();
    let daemon = trawld::ResourceManager::from_args(&daemon_args)
        .run_server()
        .await
        .unwrap();
    Some(Trawld {
        _daemon: daemon,
        _bus: bus,
        _turn: turn,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn filename() {
    let Some(_trawld) = start_trawld().await else {
        return;
    };
    let expected = arg_filename_default().await;
    let curr_resources = get_resources_prop().await.unwrap();
    clear_resources().await;
    assert_eq!(expected, curr_resources);
}

#[tokio::test(flavor = "multi_thread")]
async fn merge() {
    let Some(_trawld) = start_trawld().await else {
        return;
    };
    let expected = arg_merge().await;
    let curr_resources = get_resources_prop().await.unwrap();
    clear_resources().await;
    assert_eq!(expected, curr_resources);
}

#[tokio::test(flavor = "multi_thread")]
async fn load() {
    let Some(_trawld) = start_trawld().await else {
        return;
    };
    let expected = arg_load().await;
    let curr_resources = get_resources_prop().await.unwrap();
    clear_resources().await;
    assert_eq!(expected, curr_resources);

}

#[tokio::test(flavor = "multi_thread")]
async fn query() {
    let Some(_trawld) = start_trawld().await else {
        return;
    };
    arg_filename_default().await;
    let (q, expected) = arg_query().await;
    let client = Client::new().await.unwrap();
    let actual_result = client.proxy().query(&q).await.unwrap();
    clear_resources().await;
    assert_eq!(expected, actual_result);
}

#[tokio::test(flavor = "multi_thread")]
async fn query_all() {
    let Some(_trawld) = start_trawld().await else {
        return;
    };
    arg_filename_default().await;
    let expected = query_all_result().await;
    let client = Client::new().await.unwrap();
    let actual_result = client.proxy().query("").await.unwrap();
    clear_resources().await;
    assert_eq!(expected, actual_result);
}

#[tokio::test(flavor = "multi_thread")]
async fn edit() {
    let Some(_trawld) = start_trawld().await else {
        return;
    };
    arg_filename_default().await;
    let client = Client::new().await.unwrap();
    let expected = client.proxy().query("").await.unwrap();
    let (path, bak, bak_expected) = arg_edit(None).await;
    let contents = fs::read_to_string(&path).unwrap();
    // The file's own line stays, the resources are appended
    let expected: String = std::iter::once(bak_expected.as_str())
        .chain(expected.lines())
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(expected, contents);
    let bak_content = fs::read_to_string(&bak).unwrap();
    assert_eq!(bak_expected, bak_content);
    clear_resources().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn clear() {
    let Some(_trawld) = start_trawld().await else {
        return;
    };
    arg_filename_default().await;
    clear_resources().await;
    let expected = HashMap::<String, String>::new();
    let actual = get_resources_prop().await.unwrap();
    assert_eq!(expected, actual);
}
