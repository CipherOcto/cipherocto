//! `presence.unsubscribe` — revoke an outstanding
//! `presence.subscribe`. After this returns the daemon stops receiving
//! presence updates for the peer.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    peer: String,
}

#[derive(Debug)]
pub struct PresenceUnsubscribe;

#[async_trait::async_trait]
impl RpcHandler for PresenceUnsubscribe {
    fn name(&self) -> &'static str {
        "presence.unsubscribe"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;

        let jid = crate::jids::peer_to_jid(&p.peer).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid peer: {e}"),
            data: Some(json!({
                "expected_format": "E.164 or <digits>@s.whatsapp.net or <digits>@lid"
            })),
        })?;

        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter
            .unsubscribe_presence(jid.as_str())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("presence.unsubscribe failed: {e}"),
                data: Some(json!({"peer": p.peer, "jid": jid})),
            })?;

        Ok(json!({
            "status": "unsubscribed",
            "peer": p.peer,
            "jid": jid,
        }))
    }
}
