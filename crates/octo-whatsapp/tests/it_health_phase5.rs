//! Phase 5 Part B integration smoke tests:
//! 1. `daemon_api_version` is `"1.0.0+phase5"` over the JSON-RPC socket.
//! 2. `health.get` returns the extended Phase 5 JSON schema
//!    (`daemon_ready`, `connected`, `session_valid`, `bot_state`,
//!    `socket_bound`, `storage_state`, `uptime_seconds`,
//!    `api_version`).
//!
//! Hermetic; the daemon is bound to a per-test socket.

use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream as TokioUnixStream;
use tokio_util::sync::CancellationToken;

fn make_test_name() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("smoke-p5b-{nanos}")
}

fn make_socket_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("octo-whatsapp-test-sockets");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("octo-whatsapp-{name}.sock"))
}

fn delete_socket_quietly(p: &PathBuf) {
    let _ = std::fs::remove_file(p);
}

async fn dial(socket_path: &std::path::Path) -> TokioUnixStream {
    // Tiny retry loop in case the listener isn't quite ready.
    for _ in 0..50 {
        match TokioUnixStream::connect(socket_path).await {
            Ok(s) => return s,
            Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    }
    panic!("timed out dialing {socket_path:?}");
}

async fn round_trip(socket_path: &std::path::Path, method: &str, params: Value) -> Value {
    let mut s = dial(socket_path).await;
    let (read_half, mut write_half) = s.split();
    let req = serde_json::json!({
        "id": 1,
        "method": method,
        "params": params,
    });
    let mut line = serde_json::to_string(&req).unwrap();
    line.push('\n');
    write_half.write_all(line.as_bytes()).await.unwrap();
    write_half.flush().await.unwrap();
    let mut reader = BufReader::new(read_half);
    let mut out = String::new();
    // Block until a line comes back.
    tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut out))
        .await
        .expect("rpc timed out")
        .expect("rpc read failed");
    serde_json::from_str(&out).unwrap()
}

async fn boot_daemon(name: &str) -> (PathBuf, CancellationToken, tokio::task::JoinHandle<()>) {
    let sock = make_socket_path(name);
    delete_socket_quietly(&sock);
    let cancel = CancellationToken::new();
    let tokio_cancel = cancel.clone();
    let cfg_toml = format!(
        r#"
            name = "{name}"
            socket_dir = "{}"
            [events]
            max_rows = 1024
            retention_days = 1
        "#,
        sock.parent().unwrap().display()
    );
    let sock_clone = sock.clone();
    let handle = tokio::spawn(async move {
        let cfg = octo_whatsapp::config::WhatsAppRuntimeConfig::from_toml(cfg_toml.as_bytes())
            .expect("config parse");
        let daemon = octo_whatsapp::daemon::Daemon::new(cfg);
        let _ = daemon.run().await;
    });
    // Give the daemon a moment to bind.
    for _ in 0..50 {
        if sock_clone.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    (sock, tokio_cancel, handle)
}

#[tokio::test]
async fn version_reports_phase5() {
    let name = make_test_name();
    let (sock, cancel, h) = boot_daemon(&name).await;
    // Sanity: socket file should exist now (UnixListener creates it).
    let _ = UnixStream::connect(&sock).expect("socket file exists");
    let resp = round_trip(&sock, "version.get", Value::Null).await;
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["daemon_api_version"], "1.0.0+phase5");
    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), h).await;
    delete_socket_quietly(&sock);
}

#[tokio::test]
async fn health_get_returns_phase5_extended_schema() {
    let name = make_test_name();
    let (sock, cancel, h) = boot_daemon(&name).await;
    let resp = round_trip(&sock, "health.get", Value::Null).await;
    let res = &resp["result"];
    // Every key from the spec'd Phase 5 schema must exist.
    for key in [
        "daemon_ready",
        "connected",
        "session_valid",
        "bot_state",
        "socket_bound",
        "storage_state",
        "uptime_seconds",
        "api_version",
    ] {
        assert!(
            res.get(key).is_some(),
            "missing key {key} in health.get: {res}"
        );
    }
    assert_eq!(res["api_version"], "1.0.0+phase5");
    // Initial state — freshly booted daemon: connected & session_valid
    // are false (we haven't bound an adapter), bot_state is "booting".
    assert_eq!(res["bot_state"], "booting");
    assert_eq!(res["connected"], false);
    assert_eq!(res["daemon_ready"], false);
    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), h).await;
    delete_socket_quietly(&sock);
}

#[tokio::test]
async fn health_get_uptime_is_a_finite_number() {
    // Sanity: `uptime_seconds` must be a non-negative finite number —
    // dashboard scrapers depend on it being numeric (NaN breaks them).
    let name = make_test_name();
    let (sock, cancel, h) = boot_daemon(&name).await;
    // Wait a beat so uptime > 0.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let resp = round_trip(&sock, "health.get", Value::Null).await;
    let u = resp["result"]["uptime_seconds"]
        .as_f64()
        .expect("uptime_seconds is a number");
    assert!(u.is_finite());
    assert!(u >= 0.0);
    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), h).await;
    delete_socket_quietly(&sock);
}
