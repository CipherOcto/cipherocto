//! Integration test for the `send.text` 65,536-byte ceiling.
//!
//! The ceiling is enforced pre-flight by the handler (it returns
//! `-32004 PayloadTooLarge` before any adapter contact). This hermetic
//! test spawns the daemon, drives the full socket flow with text of
//! exactly the ceiling and one byte over, and asserts the response codes.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use octo_whatsapp::config::{EventsConfig, MediaBufferConfig, WhatsAppRuntimeConfig};
use octo_whatsapp::daemon::Daemon;
use octo_whatsapp::ipc::handlers::send_text::MAX_TEXT_BYTES;

async fn drive_daemon_send(text: String) -> serde_json::Value {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "ceiling".to_string(),
        data_dir: tmp.path().join("data"),
        log_dir: tmp.path().join("log"),
        socket_dir: tmp.path().to_path_buf(),
        media_buffer: MediaBufferConfig::default(),
        events: EventsConfig::default(),
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
                "method": "send.text",
                "params": {"peer": "+15551234567", "text": text},
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

    serde_json::from_str(resp_json.trim()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_text_at_exact_ceiling_is_accepted() {
    let text = "a".repeat(MAX_TEXT_BYTES);
    let resp = drive_daemon_send(text).await;
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["status"], "queued_for_phase2");
    assert_eq!(resp["result"]["size_bytes"], MAX_TEXT_BYTES);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_text_one_byte_over_ceiling_is_rejected_with_payload_too_large() {
    let text = "a".repeat(MAX_TEXT_BYTES + 1);
    let resp = drive_daemon_send(text).await;
    assert_eq!(resp["id"], 1);
    let err = &resp["error"];
    assert_eq!(err["code"], -32004);
    assert_eq!(err["data"]["max_bytes"], MAX_TEXT_BYTES);
}
