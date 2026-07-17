//! `contacts.save_contact` — save or rename a contact in the local
//! address book. Cross-device sync via the `critical_unblock_low`
//! app-state collection. JID must be a phone-number JID (LIDs are
//! rejected by the WA server).

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    peer: String,
    full_name: String,
}

#[derive(Debug)]
pub struct ContactsSaveContact;

#[async_trait::async_trait]
impl RpcHandler for ContactsSaveContact {
    fn name(&self) -> &'static str {
        "contacts.save_contact"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.full_name.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "full_name cannot be empty".into(),
                data: None,
            });
        }
        let jid = crate::jids::peer_to_jid(&p.peer).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid peer: {e}"),
            data: Some(json!({
                "expected_format": "E.164 or <digits>@s.whatsapp.net (LIDs not supported by WA server for save_contact)"
            })),
        })?;

        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter
            .save_contact(jid.as_str(), &p.full_name)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("contacts.save_contact failed: {e}"),
                data: Some(json!({"peer": p.peer, "jid": jid, "full_name": p.full_name})),
            })?;
        Ok(json!({
            "status": "saved",
            "peer": p.peer,
            "jid": jid,
            "full_name": p.full_name,
        }))
    }
}
