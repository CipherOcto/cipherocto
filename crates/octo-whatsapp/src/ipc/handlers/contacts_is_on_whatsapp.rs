//! `contacts.is_on_whatsapp` — check whether a JID is a registered
//! WhatsApp user. Thin wrapper over
//! `wacore::Client::contacts().is_on_whatsapp(...)`.
//!
//! **Tier 4 of the live coverage matrix.** The peer JID must be the
//! canonical `<digits>@s.whatsapp.net` form (validated up-front via
//! `peer_to_jid`); bare E.164 returns `InvalidParams` so the request
//! never reaches the adapter.

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
pub struct ContactsIsOnWhatsApp;

#[async_trait::async_trait]
impl RpcHandler for ContactsIsOnWhatsApp {
    fn name(&self) -> &'static str {
        "contacts.is_on_whatsapp"
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
        let on_whatsapp = adapter
            .is_on_whatsapp(jid.as_str())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("contacts.is_on_whatsapp failed: {e}"),
                data: Some(json!({"peer": p.peer, "jid": jid})),
            })?;

        Ok(json!({
            "peer": p.peer,
            "jid": jid,
            "on_whatsapp": on_whatsapp,
        }))
    }
}
