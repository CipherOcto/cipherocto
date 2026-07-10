//! `messages.unstar` — unstar a message. Mirrors `messages.star`.

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
pub struct MessagesUnstar;

#[async_trait::async_trait]
impl RpcHandler for MessagesUnstar {
    fn name(&self) -> &'static str {
        "messages.unstar"
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
            .unstar_message(peer_jid.as_str(), &p.msg_id, p.from_me)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("messages.unstar failed: {e}"),
                data: Some(json!({
                    "peer": peer_jid,
                    "msg_id": p.msg_id,
                    "from_me": p.from_me,
                })),
            })?;
        Ok(json!({
            "status": "unstarred",
            "peer": peer_jid,
            "msg_id": p.msg_id,
            "from_me": p.from_me,
        }))
    }
}
