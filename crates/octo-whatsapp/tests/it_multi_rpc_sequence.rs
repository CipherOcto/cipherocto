//! End-to-end test for multi-RPC sequence on a single connection.
//!
//! Phase 1 daemon keeps each connection open after responding, so a
//! client may issue multiple requests on the same blocking stream.
//! This hermetic test exercises that path:
//!
//! 1. Bind a daemon in a TempDir.
//! 2. Spawn `Daemon::run` on a background task.
//! 3. Connect from the test via blocking `UnixStream` in `spawn_blocking`.
//! 4. Send `version.get` -> `health.get` -> `shutdown` on the same stream.
//! 5. Assert all three responses have the right shape and contents.
//! 6. Trigger cancellation; the daemon must exit cleanly.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use octo_whatsapp::config::{MediaBufferConfig, WhatsAppRuntimeConfig};
use octo_whatsapp::daemon::Daemon;

fn rpc_call(stream: &mut UnixStream, method: &str, params: serde_json::Value) -> serde_json::Value {
    let req = serde_json::json!({"id": 1, "method": method, "params": params});
    let mut line = serde_json::to_string(&req).unwrap();
    line.push('\n');
    stream.write_all(line.as_bytes()).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut resp_line = String::new();
    reader.read_line(&mut resp_line).unwrap();
    serde_json::from_str(resp_line.trim()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multi_rpc_sequence_on_single_connection() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "seq".to_string(),
        data_dir: tmp.path().join("data"),
        log_dir: tmp.path().join("log"),
        socket_dir: tmp.path().to_path_buf(),
        media_buffer: MediaBufferConfig::default(),
    };
    cfg.validate().unwrap();
    std::fs::create_dir_all(cfg.data_dir.clone()).unwrap();
    std::fs::create_dir_all(cfg.log_dir.clone()).unwrap();

    let daemon = Daemon::new(cfg.clone());
    let cancel = daemon.cancel_token();
    let daemon_task = tokio::spawn(async move { daemon.run().await });

    let sock = cfg.socket_path();
    for _ in 0..50 {
        if sock.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let results = tokio::task::spawn_blocking({
        let sock = sock.clone();
        move || {
            let mut stream = UnixStream::connect(&sock).unwrap();
            let r1 = rpc_call(&mut stream, "version.get", serde_json::json!({}));
            let r2 = rpc_call(&mut stream, "health.get", serde_json::json!({}));
            let r3 = rpc_call(&mut stream, "shutdown", serde_json::json!({}));
            (r1, r2, r3)
        }
    })
    .await
    .unwrap();

    assert_eq!(results.0["result"]["daemon_api_version"], "1.0.0+phase2");
    assert_eq!(results.1["result"]["ok"], true);
    assert_eq!(results.2["result"]["ok"], true);

    cancel.cancel();
    let _ = daemon_task.await;
}
