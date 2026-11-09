//! A Load/Merge whose file blocks (here a FIFO nobody writes to yet) must
//! not stop trawld from answering other callers, and must finish once the
//! input arrives.
//!
//! Like `cpp_args.rs`, this starts a private session bus and serves trawld
//! in-process, so it doesn't need a running trawld.

use clap::Parser;
use std::{
    fs,
    io::{BufRead, BufReader, ErrorKind, Write},
    process::{Child, Command, Stdio},
    time::Duration,
};
use tokio::time::timeout;
use trawldb::Client;
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

/// Serves trawld on its own thread and runtime, so that a trawld that does
/// block (as before this fix) makes the test fail instead of hanging it
fn serve_trawld() {
    let (ready, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let args = trawld::parser::CliArgs::try_parse_from(["trawld"]).unwrap();
            let _daemon = trawld::ResourceManager::from_args(&args)
                .run_server()
                .await
                .unwrap();
            ready.send(()).unwrap();
            std::future::pending::<()>().await;
        });
    });
    wait.recv().unwrap();
}

/// While a call is blocked, other calls must answer within this
const RESPOND: Duration = Duration::from_secs(2);
/// Once the input arrives, the blocked call must finish within this
const COMPLETE: Duration = Duration::from_secs(5);

#[tokio::test(flavor = "multi_thread")]
async fn blocked_input_does_not_block_others() {
    let Some(_bus) = start_session_bus() else {
        eprintln!("dbus-daemon not found, skipping");
        return;
    };
    serve_trawld();
    let dir = std::env::temp_dir().join(format!("trawldb-fifo-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();

    for (n, nocpp) in [true, false].into_iter().enumerate() {
        let fifo = dir.join(format!("fifo{n}"));
        let status = Command::new("mkfifo").arg(&fifo).status().unwrap();
        assert!(status.success());
        let fifo = fifo.to_str().unwrap().to_string();

        let blocked = {
            let fifo = fifo.clone();
            tokio::spawn(async move {
                let client = Client::new().await.unwrap();
                client.proxy().merge(&fifo, nocpp).await
            })
        };
        // Give the call time to reach trawld and block there
        tokio::time::sleep(Duration::from_millis(300)).await;

        let client = Client::new().await.unwrap();
        let key = format!("other.key{n}");
        let set = client.proxy().set_resource(key.clone(), String::from("x"));
        timeout(RESPOND, set)
            .await
            .expect("SetResource blocked")
            .unwrap();
        let get = timeout(RESPOND, client.proxy().get_resource(&key))
            .await
            .expect("GetResource blocked");
        assert_eq!(get.unwrap(), "x");
        timeout(RESPOND, client.proxy().query(""))
            .await
            .expect("Query blocked")
            .unwrap();
        assert!(!blocked.is_finished(), "Merge returned before its input");

        // Now give it its input
        let line = format!("fifo.key{n}: {n}\n");
        tokio::task::spawn_blocking(move || {
            let mut writer = fs::OpenOptions::new().write(true).open(&fifo).unwrap();
            writer.write_all(line.as_bytes()).unwrap();
        })
        .await
        .unwrap();
        timeout(COMPLETE, blocked)
            .await
            .expect("Merge didn't finish after its input arrived")
            .unwrap()
            .unwrap();
        // The reply comes after the resources are in place
        let value = client
            .proxy()
            .get_resource(&format!("fifo.key{n}"))
            .await
            .unwrap();
        assert_eq!(value, n.to_string(), "nocpp: {nocpp}");
    }

    fs::remove_dir_all(&dir).unwrap();
}
