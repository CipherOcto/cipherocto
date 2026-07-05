//! Integration test for `send.image` 16 MiB ceiling.
//!
//! Drives the daemon over the unix socket with a file one byte over
//! the image ceiling; asserts -32004 PayloadTooLarge with `data.max_bytes`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use octo_whatsapp::config::{MediaBufferConfig, WhatsAppRuntimeConfig};
use octo_whatsapp::daemon::Daemon;
use octo_whatsapp::limits::MediaKind;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_image_one_byte_over_ceiling_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "img".to_string(),
        data_dir: tmp.path().join("data"),
        log_dir: tmp.path().join("log"),
        socket_dir: tmp.path().to_path_buf(),
        media_buffer: MediaBufferConfig::default(),
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

    // File: ceiling + 1 byte.
    let media_file = tmp.path().join("over.jpg");
    let bytes = vec![0u8; MediaKind::Image.max_bytes() + 1];
    std::fs::write(&media_file, &bytes).unwrap();

    let resp_json = tokio::task::spawn_blocking({
        let sock = sock.clone();
        let media_path = media_file.to_string_lossy().into_owned();
        move || {
            let mut s = UnixStream::connect(&sock).unwrap();
            let req = serde_json::json!({
                "id": 1,
                "method": "send.image",
                "params": {
                    "peer": "+15551234567",
                    "file": media_path,
                },
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
    let err = &resp["error"];
    assert_eq!(err["code"], -32004);
    assert_eq!(err["data"]["max_bytes"], MediaKind::Image.max_bytes());
    assert_eq!(err["data"]["kind"], "image");
}
