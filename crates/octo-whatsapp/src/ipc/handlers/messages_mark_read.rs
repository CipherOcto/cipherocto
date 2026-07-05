//! `messages.mark_read` — mark all messages in a peer up to a given msg_id as read.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    peer: String,
    up_to_msg_id: String,
}

#[derive(Debug)]
pub struct MessagesMarkRead;

#[async_trait::async_trait]
impl RpcHandler for MessagesMarkRead {
    fn name(&self) -> &'static str {
        "messages.mark_read"
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
        adapter.mark_read(&p.peer, &p.up_to_msg_id).await.map_err(|e| {
            RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter mark_read failed: {e}"),
                data: None,
            }
        })?;
        Ok(json!({
            "status": "marked_read",
            "peer": p.peer,
            "up_to_msg_id": p.up_to_msg_id,
        }))
    }
}