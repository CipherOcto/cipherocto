//! `contacts.get_user_info` — fetch rich user info for one peer:
//! status text, picture id, business flag, verified business name,
//! linked device ids. Returns `null` when the WA server has no record
//! (e.g. privacy-hidden contact).
//!
//! **Tier 6 of the live coverage matrix.**

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
pub struct ContactsGetUserInfo;

#[async_trait::async_trait]
impl RpcHandler for ContactsGetUserInfo {
    fn name(&self) -> &'static str {
        "contacts.get_user_info"
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
        let info = adapter
            .get_user_info(jid.as_str())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("contacts.get_user_info failed: {e}"),
                data: Some(json!({"peer": p.peer, "jid": jid})),
            })?;

        match info {
            Some(snap) => Ok(json!({
                "peer": p.peer,
                "found": true,
                "info": snap,
            })),
            None => Ok(json!({
                "peer": p.peer,
                "found": false,
                "info": null,
            })),
        }
    }
}
