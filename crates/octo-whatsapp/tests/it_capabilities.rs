//! Integration smoke for `capabilities`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use octo_whatsapp::config::{
    EventsConfig, MediaBufferConfig, SecurityConfig, WhatsAppRuntimeConfig,
    RulesConfig,
};
use octo_whatsapp::daemon::Daemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capabilities_returns_expected_static_report() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "capabilities".to_string(),
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
                "method": "capabilities",
                "params": {},
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
    let r = &resp["result"];
    assert_eq!(r["platform"], "whatsapp");
    assert_eq!(r["max_payload_bytes"], 65_536);
    assert_eq!(r["supports_fragmentation"], false);
    assert_eq!(r["supports_raw_binary"], false);
    assert_eq!(r["supports_encryption"], true);
    assert_eq!(r["rate_limit_per_second"], 20);
    assert_eq!(r["media_capabilities"]["max_upload_bytes"], 104_857_600);
    assert_eq!(
        r["media_capabilities"]["supported_mime_types"][0],
        "application/octet-stream"
    );
}
