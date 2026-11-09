//! Load, Merge, LoadCpp, MergeCpp, LoadCppArgv and MergeCppArgv reply with
//! a D-Bus error when the file can't be read or the preprocessor can't be
//! run, and leave the resources unchanged.
//!
//! Like `cpp_args.rs`, this starts a private session bus and serves trawld
//! in-process, so it doesn't need a running trawld.

use clap::Parser;
use std::{
    fs,
    io::{BufRead, BufReader, ErrorKind},
    process::{Child, Command, Stdio},
};
use trawldb::{manager::ResourceManagerProxy, Client};
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

/// Calls each file method on `path`; returns the results by method name
async fn call_all(
    proxy: &ResourceManagerProxy<'_>,
    path: &str,
    cpp: &str,
) -> Vec<(&'static str, zbus::Result<()>)> {
    vec![
        ("Load", proxy.load(path, false).await),
        ("Load nocpp", proxy.load(path, true).await),
        ("Merge", proxy.merge(path, false).await),
        ("Merge nocpp", proxy.merge(path, true).await),
        ("LoadCpp", proxy.load_cpp(path, cpp, "").await),
        ("MergeCpp", proxy.merge_cpp(path, cpp, "").await),
        ("LoadCppArgv", proxy.load_cpp_argv(path, cpp, &[]).await),
        ("MergeCppArgv", proxy.merge_cpp_argv(path, cpp, &[]).await),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn file_errors_are_returned() {
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
    let proxy = client.proxy();
    proxy
        .set_resource(String::from("old.key"), String::from("x"))
        .await
        .unwrap();
    let before = proxy.query("").await.unwrap();

    let dir = std::env::temp_dir().join(format!("trawld-errors-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("res");
    fs::write(&file, "new.key: 1\n").unwrap();
    let missing = dir.join("missing");
    let cases = [
        (missing.to_str().unwrap(), "/usr/bin/cpp", "can't open file"),
        (dir.to_str().unwrap(), "/usr/bin/cpp", "it is a directory"),
    ];
    for (path, cpp, message) in cases {
        for (method, res) in call_all(proxy, path, cpp).await {
            match res {
                Err(zbus::Error::MethodError(name, Some(text), _)) => {
                    assert_eq!(name.as_str(), "org.freedesktop.DBus.Error.Failed");
                    assert!(text.contains(message), "{method} {path}: {text}");
                }
                other => panic!("{method} {path}: expected an error, got {other:?}"),
            }
        }
        assert_eq!(proxy.query("").await.unwrap(), before);
    }

    // A preprocessor that can't be run is an error too
    let no_cpp = dir.join("no-such-cpp");
    let no_cpp = no_cpp.to_str().unwrap();
    let file = file.to_str().unwrap();
    for (method, res) in call_all(proxy, file, no_cpp).await {
        // Load and Merge use trawld's own preprocessor (or none)
        if method.starts_with("Load") && !method.starts_with("LoadCpp")
            || method.starts_with("Merge") && !method.starts_with("MergeCpp")
        {
            assert!(res.is_ok(), "{method}: {res:?}");
            continue;
        }
        match res {
            Err(zbus::Error::MethodError(_, Some(text), _)) => {
                assert!(text.contains("can't run preprocessor"), "{method}: {text}")
            }
            other => panic!("{method}: expected an error, got {other:?}"),
        }
    }
    // The readable file was loaded by Load and Merge
    assert_eq!(proxy.get_resource("new.key").await.unwrap(), "1");
    assert_eq!(proxy.get_resource("old.key").await.unwrap(), "x");

    fs::remove_dir_all(&dir).unwrap();
}
