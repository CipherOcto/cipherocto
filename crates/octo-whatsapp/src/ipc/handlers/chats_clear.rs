//! `chats.clear` — clear all messages in a chat but keep the chat
//! entry. Distinct from `chats.delete` which removes the chat
//! entirely.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    jid: String,
    #[serde(default)]
    delete_starred: bool,
    #[serde(default)]
    delete_media: bool,
}

#[derive(Debug)]
pub struct ChatsClear;

#[async_trait::async_trait]
impl RpcHandler for ChatsClear {
    fn name(&self) -> &'static str {
        "chats.clear"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let jid = crate::jids::peer_to_jid(&p.jid)
            .or_else(|_| {
                if p.jid.ends_with("@g.us")
                    || p.jid.ends_with("@lid")
                    || p.jid.ends_with("@broadcast")
                {
                    Ok(p.jid.clone())
                } else {
                    Err(crate::jids::JidError::InvalidPeerFormat(p.jid.clone()))
                }
            })
            .map_err(|e| RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!("invalid jid: {e}"),
                data: Some(json!({
                    "expected_format": "E.164, <digits>@s.whatsapp.net, <digits>@g.us, <digits>@lid"
                })),
            })?;

        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter
            .clear_chat(jid.as_str(), p.delete_starred, p.delete_media)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("chats.clear failed: {e}"),
                data: Some(json!({
                    "jid": jid,
                    "delete_starred": p.delete_starred,
                    "delete_media": p.delete_media,
                })),
            })?;
        Ok(json!({
            "status": "cleared",
            "jid": jid,
            "delete_starred": p.delete_starred,
            "delete_media": p.delete_media,
        }))
    }
}
