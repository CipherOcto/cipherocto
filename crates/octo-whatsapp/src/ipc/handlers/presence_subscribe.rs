//! `presence.subscribe` — subscribe to a peer's presence updates.
//! After this succeeds, inbound `InboundEvent::Presence { peer, kind }`
//! events will fire on the daemon whenever the peer changes state
//! (online / offline / last-seen updates).

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
pub struct PresenceSubscribe;

#[async_trait::async_trait]
impl RpcHandler for PresenceSubscribe {
    fn name(&self) -> &'static str {
        "presence.subscribe"
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
            .subscribe_presence(jid.as_str())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("presence.subscribe failed: {e}"),
                data: Some(json!({"peer": p.peer, "jid": jid})),
            })?;

        Ok(json!({
            "status": "subscribed",
            "peer": p.peer,
            "jid": jid,
        }))
    }
}
