//! Runs the trawldb binary to check that it reads its input like xrdb:
//! standard input, pipes, symlinked files, `--nocpp`, and bytes that aren't
//! UTF-8. Also checks the LoadText and MergeText methods it uses.
//!
//! Like `cpp_args.rs`, this starts a private session bus and serves trawld
//! in-process, so it doesn't need a running trawld.

use clap::Parser;
use std::{
    fs,
    io::{BufRead, BufReader, ErrorKind, Write},
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
};
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

/// Runs the trawldb binary in `dir` on the private bus, with `stdin` as
/// its standard input
async fn trawldb(dir: &Path, args: &[&str], stdin: &[u8]) -> Output {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (dir, stdin) = (dir.to_path_buf(), stdin.to_vec());
    tokio::task::spawn_blocking(move || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_trawldb"))
            .args(&args)
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to run trawldb");
        child.stdin.take().unwrap().write_all(&stdin).unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "trawldb {args:?}: {out:?}");
        out
    })
    .await
    .unwrap()
}

async fn get(client: &Client<'_>, key: &str) -> String {
    client.proxy().get_resource(key).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_input_like_xrdb() {
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
    let dir: PathBuf = std::env::temp_dir().join(format!("trawldb-input-{}", Uuid::new_v4()));
    fs::create_dir_all(dir.join("dotfiles")).unwrap();
    fs::create_dir_all(dir.join("inc")).unwrap();

    // Standard input: no file, "-", the positional "-", /dev/stdin
    for (n, args) in [
        &["--merge"][..],
        &["--merge", "-"],
        &["--load"],
        &["-"],
        &["--merge", "/dev/stdin"],
    ]
    .into_iter()
    .enumerate()
    {
        let input = format!("stdin.key{n}: {n}\n");
        trawldb(&dir, args, input.as_bytes()).await;
        assert_eq!(get(&client, &format!("stdin.key{n}")).await, n.to_string());
    }

    // Quoted includes from standard input are searched in the current
    // directory; -D still applies
    fs::write(dir.join("local"), "local.key: NAME\n").unwrap();
    trawldb(
        &dir,
        &["-D", "NAME=a b", "--merge"],
        b"#include \"local\"\n",
    )
    .await;
    assert_eq!(get(&client, "local.key").await, "a b");

    // A symlinked file includes relative to the link, not its target
    fs::write(dir.join("theme"), "theme.key: nord\n").unwrap();
    fs::write(
        dir.join("dotfiles/Xresources"),
        "#include \"theme\"\nfont.key: Mono\n",
    )
    .unwrap();
    symlink("dotfiles/Xresources", dir.join("Xresources")).unwrap();
    trawldb(&dir, &["--merge", "Xresources"], b"").await;
    assert_eq!(get(&client, "theme.key").await, "nord");
    assert_eq!(get(&client, "font.key").await, "Mono");

    // --nocpp wins over -I and -D: the file is taken as it is
    fs::write(dir.join("inc/x"), "x.key: 1\n").unwrap();
    fs::write(dir.join("n.x"), "#include <x>\nn.key: N\n").unwrap();
    trawldb(
        &dir,
        &["--nocpp", "-I", "inc", "-D", "N=2", "--merge", "n.x"],
        b"",
    )
    .await;
    assert_eq!(get(&client, "n.key").await, "N");
    assert_eq!(get(&client, "x.key").await, "");

    // Bytes that aren't UTF-8 become U+FFFD; valid UTF-8 survives cpp
    trawldb(
        &dir,
        &["--merge"],
        b"latin.key: caf\xe9\nutf8.key: caf\xc3\xa9\n",
    )
    .await;
    assert_eq!(get(&client, "latin.key").await, "caf\u{fffd}");
    assert_eq!(get(&client, "utf8.key").await, "café");

    // The preprocessor's messages reach trawldb's stderr
    let out = trawldb(&dir, &["--merge"], b"#include \"nope.h\"\n").await;
    assert!(String::from_utf8_lossy(&out.stderr).contains("nope.h"));

    // LoadText adds only new keys, MergeText replaces
    let proxy = client.proxy();
    proxy
        .load_text("text.key: 1\nlocal.key: x\n")
        .await
        .unwrap();
    assert_eq!(get(&client, "text.key").await, "1");
    assert_eq!(get(&client, "local.key").await, "a b");
    proxy.merge_text("text.key: 2\n").await.unwrap();
    assert_eq!(get(&client, "text.key").await, "2");

    fs::remove_dir_all(&dir).unwrap();
}
