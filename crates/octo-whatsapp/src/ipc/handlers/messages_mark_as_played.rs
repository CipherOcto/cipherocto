//! `messages.mark_as_played` — send a `played` receipt for one or
//! more messages. Used after a voice note or video has been played
//! by our daemon so the peer sees the blue double-tick `Played`
//! indicator.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    /// Chat JID (must end in `@s.whatsapp.net`, `@g.us`, or `@lid`).
    chat: String,
    msg_ids: Vec<String>,
}

#[derive(Debug)]
pub struct MessagesMarkAsPlayed;

#[async_trait::async_trait]
impl RpcHandler for MessagesMarkAsPlayed {
    fn name(&self) -> &'static str {
        "messages.mark_as_played"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.msg_ids.is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "msg_ids must be non-empty".into(),
                data: None,
            });
        }
        let chat_jid = crate::jids::peer_to_jid(&p.chat).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid chat: {e}"),
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
            .mark_as_played(chat_jid.as_str(), &p.msg_ids)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("messages.mark_as_played failed: {e}"),
                data: Some(json!({"chat": chat_jid, "msg_count": p.msg_ids.len()})),
            })?;
        Ok(json!({
            "status": "played",
            "chat": chat_jid,
            "msg_ids": p.msg_ids,
            "count": p.msg_ids.len(),
        }))
    }
}
