//! Integration smoke for `envelope.encode` + `envelope.decode` round-trip.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use octo_whatsapp::config::{
    EventsConfig, MediaBufferConfig, SecurityConfig, WhatsAppRuntimeConfig,
    RulesConfig,
};
use octo_whatsapp::daemon::Daemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn envelope_encode_decode_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "envelope-roundtrip".to_string(),
        data_dir: tmp.path().join("data"),
        log_dir: tmp.path().join("log"),
        socket_dir: tmp.path().to_path_buf(),
        media_buffer: MediaBufferConfig::default(),
        events: EventsConfig::default(),
        security: SecurityConfig::default(),
        observability: Default::default(),
        rules: RulesConfig::default(),
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

    // Write a known byte sequence to a temp file.
    let wire_tmp = tempfile::tempdir().unwrap();
    let wire_file = wire_tmp.path().join("wire.bin");
    std::fs::write(&wire_file, b"hello world").unwrap();

    // 1) envelope.encode — verify DOT/1/ prefix + wire_bytes.
    let encode_resp_json = tokio::task::spawn_blocking({
        let sock = sock.clone();
        let wire_file = wire_file.clone();
        move || {
            let mut s = UnixStream::connect(&sock).unwrap();
            let req = serde_json::json!({
                "id": 1,
                "method": "envelope.encode",
                "params": { "file": wire_file },
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

    let encode_resp: serde_json::Value = serde_json::from_str(encode_resp_json.trim()).unwrap();
    assert_eq!(encode_resp["id"], 1);
    assert!(
        encode_resp.get("error").is_none(),
        "encode error: {encode_resp}"
    );
    let encoded = encode_resp["result"]["encoded"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        encoded.starts_with("DOT/1/"),
        "encoded must start with DOT/1/: {encoded}"
    );
    assert_eq!(encode_resp["result"]["wire_bytes"], 11);

    // 2) envelope.decode — round-trip the encoded payload back to
    // wire bytes and verify the original "hello world" bytes.
    let decode_resp_json = tokio::task::spawn_blocking({
        let sock = sock.clone();
        move || {
            let mut s = UnixStream::connect(&sock).unwrap();
            let req = serde_json::json!({
                "id": 2,
                "method": "envelope.decode",
                "params": { "encoded": encoded },
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

    let decode_resp: serde_json::Value = serde_json::from_str(decode_resp_json.trim()).unwrap();
    assert_eq!(decode_resp["id"], 2);
    assert!(
        decode_resp.get("error").is_none(),
        "decode error: {decode_resp}"
    );
    let wire_hex = decode_resp["result"]["wire_hex"].as_str().unwrap();
    assert_eq!(wire_hex.len(), 22); // 11 bytes * 2 hex chars
    assert_eq!(wire_hex, "68656c6c6f20776f726c64"); // "hello world" hex
    assert_eq!(decode_resp["result"]["wire_bytes"], 11);
}
