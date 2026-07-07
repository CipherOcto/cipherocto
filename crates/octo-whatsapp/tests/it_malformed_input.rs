//! End-to-end tests for malformed input handling.
//!
//! The daemon's line-delimited JSON parser must surface parse failures
//! as JSON-RPC `-32700 ParseError` responses. These hermetic tests send
//! each malformed input and assert the error code.
//!
//! Each test spawns its own daemon in a fresh TempDir so socket paths
//! never collide.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use octo_whatsapp::config::{
    EventsConfig, MediaBufferConfig, RulesConfig, SecurityConfig, WhatsAppRuntimeConfig,
};
use octo_whatsapp::daemon::Daemon;

async fn drive_daemon(input: String) -> serde_json::Value {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "bad".to_string(),
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
            s.write_all(input.as_bytes()).unwrap();
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

    serde_json::from_str(resp_json.trim()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn string_id_is_rejected_with_parse_error() {
    let input = "{\"id\":\"not-an-int\",\"method\":\"x\"}\n".to_string();
    let resp = drive_daemon(input).await;
    assert_eq!(resp["error"]["code"], -32700);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_method_is_rejected_with_parse_error() {
    let input = "{\"id\":1}\n".to_string();
    let resp = drive_daemon(input).await;
    assert_eq!(resp["error"]["code"], -32700);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_json_is_rejected_with_parse_error() {
    // Include a trailing newline so the server's line-delimited parser
    // can hand the line off to the JSON parser; without it both sides
    // would block on `read_line` (server waits for \n, client waits for
    // response).
    let input = "{not valid json\n".to_string();
    let resp = drive_daemon(input).await;
    assert_eq!(resp["error"]["code"], -32700);
}
