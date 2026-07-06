//! Smoke test: `octo-whatsapp capabilities --socket <path>` queries the
//! `capabilities` RPC and prints the platform capability payload.

use std::time::Duration;

use assert_cmd::Command;
use octo_whatsapp::config::{EventsConfig, MediaBufferConfig, WhatsAppRuntimeConfig};
use octo_whatsapp::daemon::Daemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_capabilities_prints_max_payload_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "smoke-caps".to_string(),
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
    assert!(
        sock.exists(),
        "daemon socket did not appear at {} within timeout",
        sock.display()
    );

    let sock_str = sock.to_str().unwrap().to_string();
    let assert_output = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_octo-whatsapp"))
            .arg("capabilities")
            .arg("--socket")
            .arg(&sock_str)
            .arg("--json")
            .assert()
            .success()
    })
    .await
    .unwrap();

    let output = assert_output.get_output().clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("max_payload_bytes"),
        "expected max_payload_bytes key in capabilities stdout, got: {stdout}"
    );
    assert!(
        stdout.contains("104857600"),
        "expected 104857600 (100 MiB) in capabilities stdout, got: {stdout}"
    );

    cancel.cancel();
    let _ = daemon_task.await;
}
