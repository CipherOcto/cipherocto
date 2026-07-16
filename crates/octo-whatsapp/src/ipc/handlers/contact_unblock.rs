//! `contact.unblock` — remove a peer from the local blocklist. Reverses
//! `contact.block` (see `contact_block.rs`). The unblock propagates to
//! all our linked devices via the WA server's blocklist IQ.

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
pub struct ContactUnblock;

#[async_trait::async_trait]
impl RpcHandler for ContactUnblock {
    fn name(&self) -> &'static str {
        "contact.unblock"
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
            .unblock_contact(jid.as_str())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("contact.unblock failed: {e}"),
                data: Some(json!({"peer": p.peer, "jid": jid})),
            })?;

        Ok(json!({
            "status": "unblocked",
            "peer": p.peer,
            "jid": jid,
        }))
    }
}
