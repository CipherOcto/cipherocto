//! `chats.mute` — mute a chat until a given epoch-second timestamp.
//!
//! Pass `until_epoch_secs = 0` to unmute.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    jid: String,
    until_epoch_secs: i64,
}

#[derive(Debug)]
pub struct ChatsMute;

#[async_trait::async_trait]
impl RpcHandler for ChatsMute {
    fn name(&self) -> &'static str {
        "chats.mute"
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
            .set_chat_muted(&p.jid, p.until_epoch_secs)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter set_chat_muted failed: {e}"),
                data: Some(json!({"jid": p.jid, "until_epoch_secs": p.until_epoch_secs})),
            })?;
        let status = if p.until_epoch_secs == 0 {
            "unmuted"
        } else {
            "muted"
        };
        Ok(json!({
            "status": status,
            "jid": p.jid,
            "until_epoch_secs": p.until_epoch_secs,
        }))
    }
}
