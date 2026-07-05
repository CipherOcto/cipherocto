//! MCP server (stdio JSON-RPC). Phase 1: thin proxy to the daemon.
//!
//! Receives JSON-RPC on stdin, forwards `tools/list` / `tools/call` /
//! `initialize` / `ping` to daemon-side counterparts (or directly answers
//! initialize + ping), writes responses on stdout. Multiple MCP clients
//! may share one daemon.

use std::io::{self, BufRead, Write};
use std::path::Path;

use serde_json::Value;

pub async fn serve(socket: &Path) -> anyhow::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            return Ok(());
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {"code": -32700, "message": format!("parse: {e}")},
                });
                writeln!(stdout, "{}", err)?;
                stdout.flush()?;
                continue;
            }
        };
        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let id = req.get("id").cloned().unwrap_or(Value::Null);

        let response = match method {
            "initialize" => handle_initialize(id, &req).await?,
            "ping" => handle_ping(id).await?,
            "tools/list" => handle_tools_list(id, socket).await?,
            "tools/call" => handle_tools_call(id, &req, socket).await?,
            _ => jsonrpc_error(
                id,
                -32601,
                &format!("method {:?} not implemented in Phase 1", method),
            ),
        };
        writeln!(stdout, "{}", response)?;
        stdout.flush()?;
    }
}

async fn handle_initialize(id: Value, _req: &Value) -> anyhow::Result<Value> {
    Ok(serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2025-06-18",
            "serverInfo": {"name": "octo-whatsapp", "version": env!("CARGO_PKG_VERSION")},
            "capabilities": {"tools": {}},
        },
    }))
}

async fn handle_ping(id: Value) -> anyhow::Result<Value> {
    Ok(serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {},
    }))
}

async fn handle_tools_list(id: Value, _socket: &Path) -> anyhow::Result<Value> {
    Ok(serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": [
                {
                    "name": "version",
                    "description": "Get the daemon version info.",
                    "inputSchema": {"type": "object", "properties": {}},
                },
                {
                    "name": "health",
                    "description": "Get daemon health.",
                    "inputSchema": {"type": "object", "properties": {}},
                },
            ],
            "_phase": "phase1",
        },
    }))
}

async fn handle_tools_call(id: Value, req: &Value, socket: &Path) -> anyhow::Result<Value> {
    let tool_name = req
        .get("params")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("");
    let daemon_method = match tool_name {
        "version" => "version.get",
        "health" => "health.get",
        "status" => "status.get",
        "send_text" => "send.text",
        other => {
            return Ok(jsonrpc_error(
                id,
                -32601,
                &format!("tool {:?} not implemented in Phase 1", other),
            ));
        }
    };
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let daemon_params = if tool_name == "send_text" {
        // MCP tool params are already in the right shape; forward as-is.
        params
    } else {
        serde_json::json!({})
    };
    let daemon_result = forward_to_daemon(socket, daemon_method, daemon_params).await?;
    Ok(serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {"content": [{"type": "text", "text": serde_json::to_string(&daemon_result)?}]},
    }))
}

async fn forward_to_daemon(socket: &Path, method: &str, params: Value) -> anyhow::Result<Value> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    let mut s = UnixStream::connect(socket)?;
    let req = serde_json::json!({"id": 1, "method": method, "params": params});
    let mut line = serde_json::to_string(&req)?;
    line.push('\n');
    s.write_all(line.as_bytes())?;
    let mut buf = String::new();
    s.read_to_string(&mut buf)?;
    let resp: Value = serde_json::from_str(buf.trim())?;
    Ok(resp.get("result").cloned().unwrap_or(Value::Null))
}

fn jsonrpc_error(id: Value, code: i32, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message},
    })
}