//! Integration smoke for `chats.info`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use octo_whatsapp::config::{
    EventsConfig, MediaBufferConfig, SecurityConfig, WhatsAppRuntimeConfig,
};
use octo_whatsapp::daemon::Daemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chats_info_is_registered_in_registry() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "chats-info".to_string(),
        data_dir: tmp.path().join("data"),
        log_dir: tmp.path().join("log"),
        socket_dir: tmp.path().to_path_buf(),
        media_buffer: MediaBufferConfig::default(),
        events: EventsConfig::default(),
        security: SecurityConfig::default(),
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
                "method": "chats.info",
                "params": { "jid": "12345@g.us" },
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
    if let Some(err) = resp.get("error") {
        assert_ne!(
            err["code"].as_i64().unwrap_or(0),
            -32601,
            "chats.info should be registered, got {err}"
        );
    }
    // chats.info requires a bound adapter → expect -32012 NotConnected.
    if let Some(err) = resp.get("error") {
        assert_eq!(err["code"], -32012);
    }
}
