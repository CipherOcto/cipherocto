//! `profile.set_status` — update our "About" status text. Persists
//! cross-device via app-state sync. NOT the ephemeral text status
//! update — for that, see the Status API (Tier 6.x backlog).
//!
//! **Tier 6 of the live coverage matrix.**

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    text: String,
}

#[derive(Debug)]
pub struct ProfileSetStatus;

#[async_trait::async_trait]
impl RpcHandler for ProfileSetStatus {
    fn name(&self) -> &'static str {
        "profile.set_status"
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
            .set_status_text(&p.text)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("profile.set_status failed: {e}"),
                data: Some(json!({"text_len": p.text.len()})),
            })?;
        Ok(json!({
            "status": "status_set",
            "text": p.text,
            "length_bytes": p.text.len(),
        }))
    }
}
