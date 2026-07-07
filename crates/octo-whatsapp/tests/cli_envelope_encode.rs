//! Smoke test: `octo-whatsapp envelope encode --file <path> --socket <path>`
//! forwards to the `envelope.encode` RPC. The handler does not require an
//! adapter, so it should return a DOT/1 envelope successfully when given
//! valid bytes.

use std::time::Duration;

use assert_cmd::Command;
use octo_whatsapp::config::{
    EventsConfig, MediaBufferConfig, SecurityConfig, WhatsAppRuntimeConfig,
    RulesConfig,
};
use octo_whatsapp::daemon::Daemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_envelope_encode_emits_dot1_envelope() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "smoke-env-enc".to_string(),
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
    assert!(
        sock.exists(),
        "daemon socket did not appear at {} within timeout",
        sock.display()
    );

    let payload = tmp.path().join("payload.bin");
    std::fs::write(&payload, b"hello-dot-envelope").unwrap();

    let sock_str = sock.to_str().unwrap().to_string();
    let payload_str = payload.to_str().unwrap().to_string();
    let assert_output = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_octo-whatsapp"))
            .arg("envelope")
            .arg("encode")
            .arg("--file")
            .arg(&payload_str)
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
        stdout.contains("DOT/1/"),
        "expected DOT/1 envelope marker in stdout, got: {stdout}"
    );

    cancel.cancel();
    let _ = daemon_task.await;
}
