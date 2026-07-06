//! Test the MCP initialize handshake end-to-end.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use octo_whatsapp::config::{EventsConfig, MediaBufferConfig, WhatsAppRuntimeConfig};
use octo_whatsapp::daemon::Daemon;
use octo_whatsapp::mcp_server::EXPECTED_TOOL_COUNT;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_initialize_returns_protocol_version_2025_06_18() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "mcptest".to_string(),
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
    assert!(sock.exists(), "socket file was never created");

    // Spawn the MCP server binary; pipe in/out.
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

    // Send initialize.
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {"protocolVersion": "2025-06-18"},
    });
    let mut line = serde_json::to_string(&req).unwrap();
    line.push('\n');
    stdin.write_all(line.as_bytes()).unwrap();

    // Read response (with a generous timeout to detect hangs).
    let read = tokio::task::spawn_blocking(move || {
        let mut resp_line = String::new();
        reader.read_line(&mut resp_line).unwrap();
        resp_line
    })
    .await
    .unwrap();
    let resp: serde_json::Value = serde_json::from_str(read.trim()).unwrap();
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(resp["result"]["serverInfo"]["name"], "octo-whatsapp");

    // Close stdin so the MCP server exits cleanly.
    drop(stdin);
    let _ = child.wait();

    cancel.cancel();
    let _ = daemon_task.await;
}

/// `tools/list` must advertise at least `EXPECTED_TOOL_COUNT` tools
/// (the full Phase 1 + Phase 2 RPC surface). Sends `initialize` first so
/// the MCP server is in a steady state, then a `tools/list`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_tools_list_advertises_full_surface() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WhatsAppRuntimeConfig {
        name: "mcptools".to_string(),
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

    // initialize.
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {"protocolVersion": "2025-06-18"},
    });
    let mut line = serde_json::to_string(&req).unwrap();
    line.push('\n');
    stdin.write_all(line.as_bytes()).unwrap();

    // tools/list.
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {},
    });
    let mut line = serde_json::to_string(&req).unwrap();
    line.push('\n');
    stdin.write_all(line.as_bytes()).unwrap();

    // Drain the two responses.
    let (init_resp, tools_resp) = tokio::task::spawn_blocking(move || {
        let mut a = String::new();
        reader.read_line(&mut a).unwrap();
        let mut b = String::new();
        reader.read_line(&mut b).unwrap();
        (a, b)
    })
    .await
    .unwrap();
    let init: serde_json::Value = serde_json::from_str(init_resp.trim()).unwrap();
    let tools: serde_json::Value = serde_json::from_str(tools_resp.trim()).unwrap();
    assert_eq!(init["id"], 1);
    assert_eq!(tools["id"], 2);

    let tools_arr = tools["result"]["tools"]
        .as_array()
        .expect("tools/list response missing tools array");
    assert!(
        tools_arr.len() >= EXPECTED_TOOL_COUNT,
        "tools/list returned {} tools, expected at least {EXPECTED_TOOL_COUNT}",
        tools_arr.len()
    );
    // Spot-check that representative Phase 2 tools are present.
    let names: std::collections::BTreeSet<&str> = tools_arr
        .iter()
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
        .collect();
    for must in &["send.image", "messages.search", "chats.pin", "capabilities"] {
        assert!(
            names.contains(must),
            "expected tool {must:?} in tools/list, got {names:?}"
        );
    }

    drop(stdin);
    let _ = child.wait();
    cancel.cancel();
    let _ = daemon_task.await;
}
