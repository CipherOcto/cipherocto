//! `chats.typing` — emit a typing-indicator presence update.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    jid: String,
    on: bool,
}

#[derive(Debug)]
pub struct ChatsTyping;

#[async_trait::async_trait]
impl RpcHandler for ChatsTyping {
    fn name(&self) -> &'static str {
        "chats.typing"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter
            .send_typing(&p.jid, p.on)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter send_typing failed: {e}"),
                data: Some(json!({"jid": p.jid, "on": p.on})),
            })?;
        let status = if p.on {
            "typing_started"
        } else {
            "typing_stopped"
        };
        Ok(json!({
            "status": status,
            "jid": p.jid,
            "on": p.on,
        }))
    }
}
