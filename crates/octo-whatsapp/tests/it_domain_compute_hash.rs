//! Integration smoke for `domain.compute-hash`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use octo_whatsapp::config::{
    EventsConfig, MediaBufferConfig, RulesConfig, SecurityConfig, WhatsAppRuntimeConfig,
};
use octo_whatsapp::daemon::Daemon;

fn manual_blake3_hex(input: &str) -> String {
    use blake3::Hasher;
    let mut h = Hasher::new();
    h.update(b"whatsapp:");
    h.update(input.trim().to_lowercase().as_bytes());
    h.finalize().to_hex().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn domain_compute_hash_matches_manual_blake3() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "domain-hash".to_string(),
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

    let resp_json = tokio::task::spawn_blocking({
        let sock = sock.clone();
        move || {
            let mut s = UnixStream::connect(&sock).unwrap();
            let req = serde_json::json!({
                "id": 1,
                "method": "domain.compute-hash",
                "params": { "jid": "1234567890@g.us" },
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
    assert!(resp.get("error").is_none(), "expected success: {resp}");
    assert_eq!(resp["result"]["input"], "1234567890@g.us");
    let got = resp["result"]["domain_id"].as_str().unwrap();
    let expected = manual_blake3_hex("1234567890@g.us");
    assert_eq!(got, expected);
    assert_eq!(got.len(), 64); // BLAKE3-256 hex length
}
