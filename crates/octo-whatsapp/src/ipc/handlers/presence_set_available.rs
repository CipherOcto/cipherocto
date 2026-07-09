//! `presence.set_available` — broadcast our presence as
//! `<presence type="available" name="..."/>`. Peers that have subscribed
//! to us will see the change within ~1 s and the daemon will emit an
//! `InboundEvent::Presence { peer: self, kind: Available }` event for
//! our own audit trail.

use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct PresenceSetAvailable;

#[async_trait::async_trait]
impl RpcHandler for PresenceSetAvailable {
    fn name(&self) -> &'static str {
        "presence.set_available"
    }

    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter
            .set_presence_available()
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("presence.set_available failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "available",
            "state": "available",
        }))
    }
}
