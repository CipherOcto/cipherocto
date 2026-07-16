//! `presence.set_unavailable` — broadcast our presence as
//! `<presence type="unavailable"/>`. Reversed by `presence.set_available`.

use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct PresenceSetUnavailable;

#[async_trait::async_trait]
impl RpcHandler for PresenceSetUnavailable {
    fn name(&self) -> &'static str {
        "presence.set_unavailable"
    }

    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter
            .set_presence_unavailable()
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("presence.set_unavailable failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "unavailable",
            "state": "unavailable",
        }))
    }
}
