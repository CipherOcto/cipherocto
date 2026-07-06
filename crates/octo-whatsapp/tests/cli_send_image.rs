//! Smoke test: `octo-whatsapp send image <peer> <file> --socket <path>`
//! calls the `send.image` RPC. No adapter is bound, so the daemon must
//! surface `-32012 NotConnected` and the CLI should propagate it as a
//! non-zero exit with that marker in stderr/stdout.

use std::time::Duration;

use assert_cmd::Command;
use octo_whatsapp::config::{
    EventsConfig, MediaBufferConfig, SecurityConfig, WhatsAppRuntimeConfig,
};
use octo_whatsapp::daemon::Daemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_send_image_without_adapter_reports_not_connected() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "smoke-send-image".to_string(),
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
    assert!(
        sock.exists(),
        "daemon socket did not appear at {} within timeout",
        sock.display()
    );

    // Create a small temp image file (just bytes; we never reach the
    // transport layer because the adapter isn't bound).
    let img = tmp.path().join("tiny.png");
    std::fs::write(&img, b"\x89PNG\r\n\x1a\nfakeimagebytes").unwrap();

    let sock_str = sock.to_str().unwrap().to_string();
    let img_str = img.to_str().unwrap().to_string();
    let assert_output = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_octo-whatsapp"))
            .arg("send")
            .arg("image")
            .arg("+15551234567")
            .arg(&img_str)
            .arg("--socket")
            .arg(&sock_str)
            .arg("--json")
            .assert()
            .failure()
    })
    .await
    .unwrap();

    let output = assert_output.get_output().clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("NotConnected")
            || combined.contains("not connected")
            || combined.contains("no adapter bound"),
        "expected NotConnected marker for send.image w/o adapter, got: {combined}"
    );

    cancel.cancel();
    let _ = daemon_task.await;
}
