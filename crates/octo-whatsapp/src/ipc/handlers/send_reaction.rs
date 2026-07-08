//! `send.reaction` — emoji reaction to a message.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;
use crate::limits::MediaKind;

#[derive(Deserialize)]
struct Params {
    peer: String,
    msg_id: String,
    emoji: String,
}

#[derive(Debug)]
pub struct SendReaction;

#[async_trait::async_trait]
impl RpcHandler for SendReaction {
    fn name(&self) -> &'static str {
        "send.reaction"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let kind = MediaKind::Reaction;
        // Reaction has no file pre-flight; just size-check the payload.
        let payload_size = p.msg_id.len() + p.emoji.len() + 16;
        if payload_size > kind.max_bytes() {
            return Err(RpcError {
                code: RpcErrorCode::PayloadTooLarge.as_i32(),
                message: format!(
                    "reaction payload {payload_size} > ceiling {}",
                    kind.max_bytes()
                ),
                data: Some(json!({
                    "size_bytes": payload_size,
                    "max_bytes": kind.max_bytes(),
                    "kind": kind.as_str(),
                })),
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let id = adapter
            .send_reaction_checked(&p.peer, &p.msg_id, &p.emoji, kind.max_bytes())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter send_reaction failed: {e}"),
                data: Some(json!({"kind": kind.as_str()})),
            })?;
        Ok(json!({
            "status": "sent",
            "message_id": id,
            "kind": kind.as_str(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;
    use crate::test_mock_adapter::MockAdapter;
    use std::sync::Arc;

    fn handle() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        Daemon::new(cfg).handle()
    }

    fn handle_with_mock() -> DaemonHandle {
        let h = handle();
        h.bind_adapter(Arc::new(MockAdapter::new()));
        h
    }

    #[tokio::test]
    async fn ceiling_is_enforced_pre_flight() {
        // Reaction max is 1 KiB; flood emoji to overshoot.
        let err = SendReaction
            .call(
                handle(),
                serde_json::json!({
                    "peer": "+15551234567",
                    "msg_id": "ABCDEFGHIJ",
                    "emoji": "X".repeat(MediaKind::Reaction.max_bytes() + 100),
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::PayloadTooLarge.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = SendReaction
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "msg_id": "3EB0B1234567890ABCDEF",
                    "emoji": "\u{1F44D}",
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "sent");
        assert_eq!(r["message_id"], "fake-rxn-msg-id");
        assert_eq!(r["kind"], "reaction");
    }

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = SendReaction
            .call(
                handle(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "msg_id": "3EB0B1234567890ABCDEF",
                    "emoji": "\u{1F44D}",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }
}
