//! Smoke test: `octo-whatsapp status --socket <path> --json` queries the
//! running daemon and prints the `status.get` payload.
//!
//! Pattern: spawn a `Daemon` in a tmpdir, wait for the socket to appear,
//! then drive the actual `octo-whatsapp` binary against that socket via
//! `assert_cmd`. Asserts the JSON-formatted stdout reports the daemon's
//! phase / bot_state fields.

use std::time::Duration;

use assert_cmd::Command;
use octo_whatsapp::config::{
    EventsConfig, MediaBufferConfig, SecurityConfig, WhatsAppRuntimeConfig,
};
use octo_whatsapp::daemon::Daemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_status_reads_daemon() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "smoke-status".to_string(),
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
    assert!(
        sock.exists(),
        "daemon socket did not appear at {} within timeout",
        sock.display()
    );

    let sock_str = sock.to_str().unwrap().to_string();
    let assert_output = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_octo-whatsapp"))
            .arg("status")
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
    // Phase 1 daemon reports "booting" + "Disconnected" + readiness false.
    // We assert the JSON payload includes at least one of these canonical
    // status keys/values so the test stays stable across future phases.
    assert!(
        stdout.contains("\"phase\"")
            || stdout.contains("booting")
            || stdout.contains("connected")
            || stdout.contains("Disconnected"),
        "expected status payload in stdout, got: {stdout}"
    );
    assert!(
        stdout.contains("ready"),
        "expected readiness field in stdout, got: {stdout}"
    );

    cancel.cancel();
    let _ = daemon_task.await;
}
