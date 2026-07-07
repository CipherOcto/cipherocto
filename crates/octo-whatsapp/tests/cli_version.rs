//! Smoke test: `octo-whatsapp version --socket <path> --json` queries the
//! running daemon and prints its `daemon_api_version`.
//!
//! Pattern: spawn a `Daemon` in a tmpdir, wait for the socket to appear,
//! then drive the actual `octo-whatsapp` binary against that socket via
//! `assert_cmd`. Asserts the JSON-formatted stdout contains the Phase 1
//! marker `1.0.0+phase5`.

use std::time::Duration;

use assert_cmd::Command;
use octo_whatsapp::config::{
    EventsConfig, MediaBufferConfig, RulesConfig, SecurityConfig, WhatsAppRuntimeConfig,
};
use octo_whatsapp::daemon::Daemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_version_reads_daemon() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "smoke-version".to_string(),
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

    let sock_str = sock.to_str().unwrap().to_string();
    let assert_output = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_octo-whatsapp"))
            .arg("version")
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
        stdout.contains("1.0.0+phase5"),
        "expected daemon_api_version marker in stdout, got: {stdout}"
    );
    assert!(
        stdout.contains("daemon_api_version"),
        "expected daemon_api_version key in stdout, got: {stdout}"
    );

    cancel.cancel();
    let _ = daemon_task.await;
}
