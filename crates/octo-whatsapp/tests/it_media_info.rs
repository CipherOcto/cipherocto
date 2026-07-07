//! Integration smoke for `media.info`.
//!
//! `media.info` is a Phase 2 stub that returns `{info: null}` — there is
//! no media metadata cache in Phase 2. The handler is wired and returns
//! a successful result without consulting the adapter, so we expect a
//! 200-shape JSON-RPC response (NOT -32012).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use octo_whatsapp::config::{
    EventsConfig, MediaBufferConfig, SecurityConfig, WhatsAppRuntimeConfig,
};
use octo_whatsapp::daemon::Daemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn media_info_returns_null_in_phase2() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "media-info".to_string(),
        data_dir: tmp.path().join("data"),
        log_dir: tmp.path().join("log"),
        socket_dir: tmp.path().to_path_buf(),
        media_buffer: MediaBufferConfig::default(),
        events: EventsConfig::default(),
        security: SecurityConfig::default(),
        observability: Default::default(),
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
                "method": "media.info",
                "params": { "media_ref_token": "test" },
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
    // media.info is a stub that does NOT consult the adapter, so we
    // expect a successful result with `info: null`.
    let result = resp
        .get("result")
        .unwrap_or_else(|| panic!("media.info should return a result, got {resp}"));
    assert!(result["info"].is_null(), "info must be null in Phase 2");
    assert_eq!(result["phase"], "phase2");
}
