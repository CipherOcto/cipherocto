//! `send.text` — pre-flight size ceiling + peer validation + real dispatch.
//!
//! **Load-bearing test of Phase 1.** The 65,536-byte ceiling MUST be enforced
//! here, pre-flight, so that over-size text never reaches WhatsApp. Phase 2
//! replaced the stub with real adapter dispatch via
//! `OctoWhatsAppAdapter::send_text`.

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

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
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

        // Validate peer shape — also produces a canonical JID we forward
        // to the adapter. Phase 1's pre-flight check; still load-bearing
        // because over-size text must never reach the adapter.
        let jid = crate::jids::peer_to_jid(&p.peer).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid peer: {e}"),
            data: Some(serde_json::json!({
                "expected_format": "E.164 or <digits>@s.whatsapp.net or <digits>@lid"
            })),
        })?;

        // Real dispatch. Surface adapter errors verbatim — the daemon's
        // error mapping layer (see `RpcErrorCode::*`) translates the
        // `PlatformAdapterError` variants the trait returns.
        let adapter = h.adapter().ok_or_else(|| RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound; daemon.start must precede send.text".into(),
            data: None,
        })?;
        let message_id = adapter
            .send_text(jid.as_str(), &p.text, p.reply_to.as_deref(), &p.mentions)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("send.text dispatch failed: {e}"),
                data: None,
            })?;

        Ok(serde_json::json!({
            "message_id": message_id,
            "peer": p.peer,
            "size_bytes": bytes,
            "ts_unix_ms": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;
    use crate::test_mock_adapter::MockAdapter;
    use crate::OctoWhatsAppAdapter;
    use std::sync::Arc;

    fn handle_with_mock() -> (DaemonHandle, Arc<MockAdapter>) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
        let mock = Arc::new(MockAdapter::new());
        handle.bind_adapter(mock.clone() as Arc<dyn OctoWhatsAppAdapter>);
        (handle, mock)
    }

    #[tokio::test]
    async fn dispatches_to_adapter_with_message_id() {
        let (h, mock) = handle_with_mock();
        let v = SendText
            .call(
                h,
                serde_json::json!({"peer": "+15551234567", "text": "hello"}),
            )
            .await
            .unwrap();
        assert_eq!(v["message_id"], "fake-text-msg-id");
        assert_eq!(v["peer"], "+15551234567");
        assert_eq!(v["size_bytes"], 5);
        assert_eq!(mock.call_count("send_text"), 1);
    }

    #[tokio::test]
    async fn passes_reply_to_and_mentions_through() {
        let (h, mock) = handle_with_mock();
        // The mock doesn't introspect args — we assert only that the
        // call happened with the right name and returned the canned id.
        let v = SendText
            .call(
                h,
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "text": "reply",
                    "reply_to": "orig-msg-id",
                    "mentions": ["1111111111@s.whatsapp.net"]
                }),
            )
            .await
            .unwrap();
        assert_eq!(v["message_id"], "fake-text-msg-id");
        assert_eq!(mock.call_count("send_text"), 1);
    }

    #[tokio::test]
    async fn accepts_exactly_65536() {
        let (h, _mock) = handle_with_mock();
        let text = "a".repeat(MAX_TEXT_BYTES);
        let v = SendText
            .call(h, serde_json::json!({"peer": "+15551234567", "text": text}))
            .await
            .unwrap();
        assert_eq!(v["message_id"], "fake-text-msg-id");
        assert_eq!(v["size_bytes"], MAX_TEXT_BYTES);
    }

    #[tokio::test]
    async fn rejects_65537() {
        let (h, _mock) = handle_with_mock();
        let text = "a".repeat(MAX_TEXT_BYTES + 1);
        let err = SendText
            .call(h, serde_json::json!({"peer": "+15551234567", "text": text}))
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
        let (h, _mock) = handle_with_mock();
        let err = SendText
            .call(h, serde_json::json!({"peer": "not-a-peer", "text": "hi"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, -32602);
    }

    #[tokio::test]
    async fn rejects_when_no_adapter_bound() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
        let err = SendText
            .call(
                handle,
                serde_json::json!({"peer": "+15551234567", "text": "hi"}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, -32012); // NotConnected
    }
}
