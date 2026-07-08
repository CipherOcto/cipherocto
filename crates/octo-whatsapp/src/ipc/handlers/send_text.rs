//! `send.text` — pre-flight size ceiling + peer validation.
//!
//! **Load-bearing test of Phase 1.** The 65,536-byte ceiling MUST be enforced
//! here, pre-flight, so that over-size text never reaches WhatsApp. Real
//! adapter dispatch arrives in Phase 2.

use serde::Deserialize;
use serde_json::Value;

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

/// Maximum raw text payload size (inclusive), per RFC-0850 §8.6.
///
/// `pub` so Part J's `it_send_text_ceiling.rs` integration test can reference
/// the same constant the handler enforces.
pub const MAX_TEXT_BYTES: usize = 65_536;

#[derive(Deserialize)]
#[allow(dead_code)] // `reply_to` / `mentions` are reserved for Phase 2 quoting/reply routing.
struct Params {
    peer: String,
    text: String,
    #[serde(default)]
    reply_to: Option<String>,
    #[serde(default)]
    mentions: Vec<String>,
}

#[derive(Debug)]
pub struct SendText;

#[async_trait::async_trait]
impl RpcHandler for SendText {
    fn name(&self) -> &'static str {
        "send.text"
    }

    async fn call(&self, _h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;

        // byte length, not char count — we transmit the raw bytes over the
        // adapter; round-trip UTF-8 char-counting is the receiver's problem.
        let bytes = p.text.len();
        if bytes > MAX_TEXT_BYTES {
            return Err(RpcError {
                code: RpcErrorCode::PayloadTooLarge.as_i32(),
                message: format!(
                    "text payload is {bytes} bytes; ceiling is {MAX_TEXT_BYTES}; \
                     use send.doc for larger payloads"
                ),
                data: Some(serde_json::json!({
                    "size_bytes": bytes,
                    "max_bytes": MAX_TEXT_BYTES,
                    "hint": "use send.doc",
                })),
            });
        }

        // Phase 1: validate peer, do not actually send. The actual call into
        // CoordinatorAdmin happens in Task 33.
        let _jid = crate::jids::peer_to_jid(&p.peer).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid peer: {e}"),
            data: Some(serde_json::json!({
                "expected_format": "E.164 or <digits>@s.whatsapp.net or <digits>@lid"
            })),
        })?;

        Ok(serde_json::json!({
            "status": "queued_for_phase2",
            "peer": p.peer,
            "size_bytes": bytes,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;

    fn handle() -> DaemonHandle {
        let tmp = tempfile::tempdir().expect("tempdir");
        Daemon::new_for_tests(tmp.path()).1
    }

    #[tokio::test]
    async fn accepts_exactly_65536() {
        let text = "a".repeat(MAX_TEXT_BYTES);
        let v = SendText
            .call(
                handle(),
                serde_json::json!({"peer": "+15551234567", "text": text}),
            )
            .await
            .unwrap();
        assert_eq!(v["status"], "queued_for_phase2");
        assert_eq!(v["size_bytes"], MAX_TEXT_BYTES);
    }

    #[tokio::test]
    async fn rejects_65537() {
        let text = "a".repeat(MAX_TEXT_BYTES + 1);
        let err = SendText
            .call(
                handle(),
                serde_json::json!({"peer": "+15551234567", "text": text}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, -32004);
        let data = err.data.unwrap();
        assert_eq!(data["max_bytes"], MAX_TEXT_BYTES);
        assert_eq!(data["size_bytes"], MAX_TEXT_BYTES + 1);
        assert_eq!(data["hint"], "use send.doc");
    }

    #[tokio::test]
    async fn rejects_invalid_peer() {
        let err = SendText
            .call(
                handle(),
                serde_json::json!({"peer": "not-a-peer", "text": "hi"}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, -32602);
    }
}
