//! `messages.get` — fetch a single message by id.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    msg_id: String,
}

#[derive(Debug)]
pub struct MessagesGet;

#[async_trait::async_trait]
impl RpcHandler for MessagesGet {
    fn name(&self) -> &'static str {
        "messages.get"
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
        // Use message_search as a probe; filter to exact id match.
        let hits = adapter
            .message_search(&p.msg_id, None)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter message_search failed: {e}"),
                data: None,
            })?;
        let exact: Vec<_> = hits.into_iter().filter(|h| h.msg_id == p.msg_id).collect();
        Ok(json!({
            "messages": exact,
            "msg_id": p.msg_id,
        }))
    }
}