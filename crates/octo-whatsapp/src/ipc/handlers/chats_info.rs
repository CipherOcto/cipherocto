//! `chats.info` — fetch metadata for a single chat.

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
pub struct ChatsInfo;

#[async_trait::async_trait]
impl RpcHandler for ChatsInfo {
    fn name(&self) -> &'static str {
        "chats.info"
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
        let info = adapter.chat_info(&p.jid).await.map_err(|e| RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: format!("adapter chat_info failed: {e}"),
            data: Some(json!({"jid": p.jid})),
        })?;
        Ok(json!({
            "chat": info,
            "jid": p.jid,
        }))
    }
}
