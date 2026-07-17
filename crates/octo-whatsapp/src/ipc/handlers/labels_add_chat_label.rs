//! `labels.add_chat_label` — attach a label to a chat.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    label_id: String,
    chat_jid: String,
}

#[derive(Debug)]
pub struct LabelsAddChatLabel;

#[async_trait::async_trait]
impl RpcHandler for LabelsAddChatLabel {
    fn name(&self) -> &'static str {
        "labels.add_chat_label"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let chat_jid = crate::jids::peer_to_jid(&p.chat_jid)
            .or_else(|_| {
                // allow @g.us / @lid / @broadcast directly
                if p.chat_jid.ends_with("@g.us")
                    || p.chat_jid.ends_with("@lid")
                    || p.chat_jid.ends_with("@broadcast")
                {
                    Ok(p.chat_jid.clone())
                } else {
                    Err(crate::jids::JidError::InvalidPeerFormat(p.chat_jid.clone()))
                }
            })
            .map_err(|e| RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!("invalid chat_jid: {e}"),
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
            .add_chat_label(&p.label_id, &chat_jid)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("labels.add_chat_label failed: {e}"),
                data: Some(json!({"label_id": p.label_id, "chat_jid": chat_jid})),
            })?;
        Ok(json!({
            "status": "added",
            "label_id": p.label_id,
            "chat_jid": chat_jid,
        }))
    }
}
