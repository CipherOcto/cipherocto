//! Smoke test for the `tools/call` MCP dispatcher.
//!
//! Sends `tools/call` for `capabilities` through the stdio MCP bridge
//! and asserts the daemon returns a `content[0].text` payload that
//! contains the canonical capability markers.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use octo_whatsapp::config::{
    EventsConfig, MediaBufferConfig, RulesConfig, SecurityConfig, WhatsAppRuntimeConfig,
};
use octo_whatsapp::daemon::Daemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_tools_call_capabilities_forwards_to_daemon() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "mcpdispatch".to_string(),
        data_dir: tmp.path().join("data"),
        log_dir: tmp.path().join("log"),
        socket_dir: tmp.path().to_path_buf(),
        media_buffer: MediaBufferConfig::default(),
        events: EventsConfig::default(),
        security: SecurityConfig::default(),
        observability: Default::default(),
        rules: RulesConfig::default(),
        ..Default::default()
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
    assert!(sock.exists(), "socket file was never created");

    let mut child = Command::new(env!("CARGO_BIN_EXE_octo-whatsapp"))
        .arg("mcp")
        .arg("--socket")
        .arg(&sock)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn octo-whatsapp mcp");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // initialize
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {"protocolVersion": "2025-06-18"},
    });
    let mut line = serde_json::to_string(&init).unwrap();
    line.push('\n');
    stdin.write_all(line.as_bytes()).unwrap();

    // tools/call for `capabilities` (no arguments)
    let call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {"name": "capabilities", "arguments": {}},
    });
    let mut line = serde_json::to_string(&call).unwrap();
    line.push('\n');
    stdin.write_all(line.as_bytes()).unwrap();

    // Drain both responses.
    let (init_resp, call_resp) = tokio::task::spawn_blocking(move || {
        let mut a = String::new();
        reader.read_line(&mut a).unwrap();
        let mut b = String::new();
        reader.read_line(&mut b).unwrap();
        (a, b)
    })
    .await
    .unwrap();
    let init_v: serde_json::Value = serde_json::from_str(init_resp.trim()).unwrap();
    let call_v: serde_json::Value = serde_json::from_str(call_resp.trim()).unwrap();
    assert_eq!(init_v["id"], 1);
    assert_eq!(call_v["id"], 2);

    // No error envelope.
    assert!(call_v.get("error").is_none(), "unexpected error: {call_v}");

    let content = call_v["result"]["content"]
        .as_array()
        .expect("tools/call result missing content array");
    assert_eq!(content.len(), 1, "expected single content item");
    let text = content[0]["text"]
        .as_str()
        .expect("content[0].text must be a string");
    assert!(
        text.contains("max_payload_bytes"),
        "expected max_payload_bytes in capabilities text, got: {text}"
    );
    // The static fallback capabilities report uses 100*1024*1024 for the
    // media max upload bytes — assert it surfaces through the MCP bridge.
    assert!(
        text.contains("104857600"),
        "expected 104857600 (100 MiB media cap) in capabilities text, got: {text}"
    );

    drop(stdin);
    let _ = child.wait();
    cancel.cancel();
    let _ = daemon_task.await;
}
