//! Integration test for the `send.delete` 3600s delete-for-everyone window.
//!
//! `msg_timestamp = now - 7200` (2 hours ago) is past the window. The
//! handler must return `-32014 DeleteWindowExpired` before any adapter
//! contact.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use octo_whatsapp::config::{
    EventsConfig, MediaBufferConfig, RulesConfig, SecurityConfig, WhatsAppRuntimeConfig,
};
use octo_whatsapp::daemon::Daemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_delete_expired_window_returns_minus_32014() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "delete-window".to_string(),
        data_dir: tmp.path().join("data"),
        log_dir: tmp.path().join("log"),
        socket_dir: tmp.path().to_path_buf(),
        media_buffer: MediaBufferConfig::default(),
        events: EventsConfig::default(),
        security: SecurityConfig::default(),
        observability: Default::default(),
        rules: RulesConfig::default(),
        ..Default::default()
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

    let resp_json = tokio::task::spawn_blocking({
        let sock = sock.clone();
        move || {
            let mut s = UnixStream::connect(&sock).unwrap();
            let req = serde_json::json!({
                "id": 1,
                "method": "send.delete",
                "params": {
                    "peer": "+15551234567",
                    "msg_id": "ABCDEFG",
                    "msg_timestamp": now - 7200, // 2 hours ago
                },
            });
            let mut line = serde_json::to_string(&req).unwrap();
            line.push('\n');
            s.write_all(line.as_bytes()).unwrap();
            let mut reader = BufReader::new(s);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            line
        }
    })
    .await
    .unwrap();

    cancel.cancel();
    let _ = daemon_task.await;

    let resp: serde_json::Value = serde_json::from_str(resp_json.trim()).unwrap();
    assert_eq!(resp["id"], 1);
    let err = &resp["error"];
    assert_eq!(err["code"], -32014);
    assert_eq!(err["data"]["window_seconds"], 3600);
    assert!(err["data"]["elapsed_seconds"].as_i64().unwrap() > 3600);
}
