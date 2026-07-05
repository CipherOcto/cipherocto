//! `chats.pin` — pin a chat.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    jid: String,
}

#[derive(Debug)]
pub struct ChatsPin;

#[async_trait::async_trait]
impl RpcHandler for ChatsPin {
    fn name(&self) -> &'static str {
        "chats.pin"
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
            .set_chat_pinned(&p.jid, true)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter set_chat_pinned failed: {e}"),
                data: Some(json!({"jid": p.jid, "pinned": true})),
            })?;
        Ok(json!({
            "status": "pinned",
            "jid": p.jid,
        }))
    }
}
