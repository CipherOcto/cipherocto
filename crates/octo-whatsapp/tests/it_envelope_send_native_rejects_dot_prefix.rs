//! Integration smoke for `envelope.send-native` rejecting DOT/-prefixed input.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use octo_whatsapp::config::{MediaBufferConfig, WhatsAppRuntimeConfig};
use octo_whatsapp::daemon::Daemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn envelope_send_native_rejects_dot_prefixed_input() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "send-native-reject".to_string(),
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

    // File whose content starts with "DOT/1/" — must be rejected
    // by the pre-flight guard (design §923).
    let wire_file = tmp.path().join("already-encoded.bin");
    std::fs::write(&wire_file, b"DOT/1/aGVsbG8").unwrap();

    let resp_json = tokio::task::spawn_blocking({
        let sock = sock.clone();
        let wire_file = wire_file.clone();
        move || {
            let mut s = UnixStream::connect(&sock).unwrap();
            let req = serde_json::json!({
                "id": 1,
                "method": "envelope.send-native",
                "params": { "peer": "+15551234567", "file": wire_file },
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
    assert_eq!(resp["error"]["code"], -32602, "full response: {resp}");
    let msg = resp["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("raw wire bytes") && msg.contains("DOT/"),
        "expected guidance message, got: {msg}"
    );
    assert_eq!(
        resp["error"]["data"]["hint"],
        "use envelope.send for already-encoded DOT/1/{b64} payloads"
    );
}
