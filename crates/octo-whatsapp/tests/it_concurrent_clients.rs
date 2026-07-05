//! End-to-end test for concurrent clients.
//!
//! The daemon's accept loop spawns a per-connection task, so it should
//! safely multiplex RPC traffic across many clients simultaneously.
//! This hermetic stress test fires 8 concurrent clients, each making 5
//! `version.get` calls on its own blocking stream (40 RPC calls total),
//! and asserts every response has the right id and shape.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Duration;

use octo_whatsapp::config::WhatsAppRuntimeConfig;
use octo_whatsapp::daemon::Daemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_clients_each_get_correct_responses() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "concurrent".to_string(),
        data_dir: tmp.path().join("data"),
        log_dir: tmp.path().join("log"),
        socket_dir: tmp.path().to_path_buf(),
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

    let sock = Arc::new(sock);

    // 8 concurrent clients, each making 5 RPC calls.
    let mut tasks = Vec::new();
    for client_id in 0..8 {
        let sock = sock.clone();
        tasks.push(tokio::task::spawn_blocking(move || {
            let mut stream = UnixStream::connect(sock.as_ref()).unwrap();
            for call_id in 0..5 {
                let req = serde_json::json!({
                    "id": client_id * 100 + call_id,
                    "method": "version.get",
                });
                let mut line = serde_json::to_string(&req).unwrap();
                line.push('\n');
                stream.write_all(line.as_bytes()).unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut resp_line = String::new();
                reader.read_line(&mut resp_line).unwrap();
                let resp: serde_json::Value = serde_json::from_str(resp_line.trim()).unwrap();
                assert_eq!(resp["id"], client_id * 100 + call_id);
                assert_eq!(resp["result"]["daemon_api_version"], "1.0.0+phase1");
            }
        }));
    }

    for t in tasks {
        t.await.unwrap();
    }

    cancel.cancel();
    let _ = daemon_task.await;
}
