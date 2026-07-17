//! `messages.delete_for_me` — local-only delete (not for everyone).
//! Reverses the visible state on this device only; the peer still
//! sees the message on their own device.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    peer: String,
    msg_id: String,
    #[serde(default = "default_from_me")]
    from_me: bool,
}

fn default_from_me() -> bool {
    true
}

#[derive(Debug)]
pub struct MessagesDeleteForMe;

#[async_trait::async_trait]
impl RpcHandler for MessagesDeleteForMe {
    fn name(&self) -> &'static str {
        "messages.delete_for_me"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let peer_jid = crate::jids::peer_to_jid(&p.peer).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid peer: {e}"),
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
            .delete_message_for_me(peer_jid.as_str(), &p.msg_id, p.from_me)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("messages.delete_for_me failed: {e}"),
                data: Some(json!({
                    "peer": peer_jid,
                    "msg_id": p.msg_id,
                    "from_me": p.from_me,
                })),
            })?;
        Ok(json!({
            "status": "deleted_for_me",
            "peer": peer_jid,
            "msg_id": p.msg_id,
            "from_me": p.from_me,
        }))
    }
}
