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
            "tools": tool_descriptors(),
            "_phase": "phase2",
        },
    }))
}

/// Canonical tool descriptor list mirrored from the daemon's RPC method
/// registry. Each tool's `name` is forwarded as the RPC `method` field;
/// arguments map 1:1 to RPC params (the JSON Schema is the source of truth).
pub fn tool_descriptors() -> Vec<Value> {
    let mut v: Vec<Value> = Vec::new();
    // ─── Lifecycle (3) ────────────────────────────────────────────────
    v.push(td(
        "version",
        "Get the daemon version info.",
        schema_empty(),
    ));
    v.push(td(
        "status",
        "Get daemon runtime status (boot state, bot state, session health).",
        schema_empty(),
    ));
    v.push(td(
        "health",
        "Get daemon health (liveness/readiness summary).",
        schema_empty(),
    ));
    // ─── Send media + control (11) ────────────────────────────────────
    v.push(td(
        "send.text",
        "Send a text message to a peer.",
        schema_props_required(
            &[("peer", "string"), ("text", "string")],
            &["peer", "text"],
        ),
    ));
    v.push(td(
        "send.image",
        "Send an image with optional caption.",
        schema_props_required(
            &[("peer", "string"), ("file", "string"), ("caption", "string")],
            &["peer", "file"],
        ),
    ));
    v.push(td(
        "send.video",
        "Send a video with optional caption.",
        schema_props_required(
            &[("peer", "string"), ("file", "string"), ("caption", "string")],
            &["peer", "file"],
        ),
    ));
    v.push(td(
        "send.audio",
        "Send an audio file.",
        schema_props_required(
            &[("peer", "string"), ("file", "string")],
            &["peer", "file"],
        ),
    ));
    v.push(td(
        "send.voice",
        "Send a voice-note (PTT) audio file.",
        schema_props_required(
            &[("peer", "string"), ("file", "string")],
            &["peer", "file"],
        ),
    ));
    v.push(td(
        "send.sticker",
        "Send a sticker image (WEBP).",
        schema_props_required(
            &[("peer", "string"), ("file", "string")],
            &["peer", "file"],
        ),
    ));
    v.push(td(
        "send.reaction",
        "React to a message with an emoji.",
        schema_props_required(
            &[("peer", "string"), ("msg_id", "string"), ("emoji", "string")],
            &["peer", "msg_id", "emoji"],
        ),
    ));
    v.push(td(
        "send.poll",
        "Send a poll to a peer.",
        schema_props_required(
            &[
                ("peer", "string"),
                ("question", "string"),
                ("options", "array"),
                ("multi", "boolean"),
            ],
            &["peer", "question", "options"],
        ),
    ));
    v.push(td(
        "send.contact",
        "Send a vCard contact.",
        schema_props_required(
            &[("peer", "string"), ("vcard", "string")],
            &["peer", "vcard"],
        ),
    ));
    v.push(td(
        "send.location",
        "Send a location pin.",
        schema_props_required(
            &[
                ("peer", "string"),
                ("lat", "number"),
                ("lon", "number"),
                ("name", "string"),
            ],
            &["peer", "lat", "lon"],
        ),
    ));
    v.push(td(
        "send.delete",
        "Delete (revoke) a previously sent message.",
        schema_props_required(
            &[
                ("peer", "string"),
                ("msg_id", "string"),
                ("msg_timestamp", "integer"),
            ],
            &["peer", "msg_id", "msg_timestamp"],
        ),
    ));
    v
}

async fn handle_tools_call(id: Value, req: &Value, socket: &Path) -> anyhow::Result<Value> {
    let tool_name = req
        .get("params")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("");
    // MCP spec: client passes tool args under `params.arguments`. Fall back
    // to the whole `params` object for legacy clients that inline args.
    let arguments = req
        .get("params")
        .and_then(|p| p.get("arguments"))
        .cloned()
        .unwrap_or(Value::Null);
    let daemon_method = match tool_name {
        "version" => "version.get",
        "status" => "status.get",
        "health" => "health.get",
        "send.text" => "send.text",
        "send.image" => "send.image",
        "send.video" => "send.video",
        "send.audio" => "send.audio",
        "send.voice" => "send.voice",
        "send.sticker" => "send.sticker",
        "send.reaction" => "send.reaction",
        "send.poll" => "send.poll",
        "send.contact" => "send.contact",
        "send.location" => "send.location",
        "send.delete" => "send.delete",
        other => {
            return Ok(jsonrpc_error(
                id,
                -32601,
                &format!("tool {:?} not implemented in Phase 2", other),
            ));
        }
    };
    let daemon_result = forward_to_daemon(socket, daemon_method, arguments).await?;
    let text = if daemon_result.is_null() {
        "null".to_string()
    } else {
        serde_json::to_string(&daemon_result)?
    };
    Ok(serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {"content": [{"type": "text", "text": text}]},
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

// ── Tool descriptor helpers ────────────────────────────────────────────

fn td(name: &str, description: &str, input_schema: Value) -> Value {
    serde_json::json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
    })
}

fn schema_empty() -> Value {
    serde_json::json!({"type": "object", "properties": {}})
}

fn schema_props_required(props: &[(&str, &str)], required: &[&str]) -> Value {
    let mut p = serde_json::Map::new();
    for (k, ty) in props {
        p.insert((*k).to_string(), serde_json::json!({"type": *ty}));
    }
    serde_json::json!({
        "type": "object",
        "properties": Value::Object(p),
        "required": required,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Task 51: send.* tool descriptors must be present.
    #[test]
    fn send_tools_are_advertised() {
        let descs = tool_descriptors();
        let names: std::collections::BTreeSet<&str> = descs
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        for m in &[
            "send.text",
            "send.image",
            "send.video",
            "send.audio",
            "send.voice",
            "send.sticker",
            "send.reaction",
            "send.poll",
            "send.contact",
            "send.location",
            "send.delete",
        ] {
            assert!(names.contains(m), "send tool {m:?} not advertised");
        }
    }
}