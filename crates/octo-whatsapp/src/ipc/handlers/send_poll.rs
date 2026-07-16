//! `send.poll` — outbound poll with question + options.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;
use crate::limits::MediaKind;

#[derive(Deserialize)]
struct Params {
    peer: String,
    question: String,
    options: Vec<String>,
    #[serde(default)]
    multi: bool,
    /// When `true`, send a quiz (single-select with one correct
    /// answer); `correct_option_index` then becomes required.
    #[serde(default)]
    is_quiz: bool,
    /// 0-based index into `options` of the correct answer.
    /// Required when `is_quiz=true`.
    #[serde(default)]
    correct_option_index: Option<usize>,
}

#[derive(Debug)]
pub struct SendPoll;

#[async_trait::async_trait]
impl RpcHandler for SendPoll {
    fn name(&self) -> &'static str {
        "send.poll"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let kind = MediaKind::Poll;
        let payload_size = p.question.len() + p.options.iter().map(|o| o.len()).sum::<usize>() + 32;
        if payload_size > kind.max_bytes() {
            return Err(RpcError {
                code: RpcErrorCode::PayloadTooLarge.as_i32(),
                message: format!("poll payload {payload_size} > ceiling {}", kind.max_bytes()),
                data: Some(json!({
                    "size_bytes": payload_size,
                    "max_bytes": kind.max_bytes(),
                    "kind": kind.as_str(),
                    "option_count": p.options.len(),
                })),
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let id = adapter
            .send_poll_checked(
                &p.peer,
                &p.question,
                &p.options,
                p.multi,
                p.is_quiz,
                p.correct_option_index,
                kind.max_bytes(),
            )
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter send_poll failed: {e}"),
                data: Some(json!({"kind": kind.as_str()})),
            })?;
        Ok(json!({
            "status": "sent",
            "message_id": id,
            "option_count": p.options.len(),
            "is_quiz": p.is_quiz,
            "kind": kind.as_str(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;
    use crate::test_mock_adapter::MockAdapter;
    use std::sync::Arc;

    fn handle() -> DaemonHandle {
        let tmp = tempfile::tempdir().expect("tempdir");
        Daemon::new_for_tests(tmp.path()).1
    }

    fn handle_with_mock() -> DaemonHandle {
        let h = handle();
        h.bind_adapter(Arc::new(MockAdapter::new()));
        h
    }

    #[tokio::test]
    async fn ceiling_is_enforced_pre_flight() {
        // 100 options of 100 chars each = ~10_032 bytes total (>4 KiB).
        let options: Vec<String> = (0..100).map(|_| "x".repeat(100)).collect();
        let err = SendPoll
            .call(
                handle(),
                serde_json::json!({
                    "peer": "+15551234567",
                    "question": "Q?",
                    "options": options,
                    "multi": false,
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::PayloadTooLarge.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = SendPoll
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "question": "Pick one?",
                    "options": ["A", "B"],
                    "multi": false,
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "sent");
        assert_eq!(r["message_id"], "fake-poll-msg-id");
        assert_eq!(r["option_count"], 2);
        assert_eq!(r["kind"], "poll");
        assert_eq!(r["is_quiz"], false);
    }

    #[tokio::test]
    async fn quiz_path_with_correct_option_index() {
        let r = SendPoll
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "question": "Capital of France?",
                    "options": ["London", "Paris", "Berlin"],
                    "multi": false,
                    "is_quiz": true,
                    "correct_option_index": 1,
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "sent");
        assert_eq!(r["is_quiz"], true);
        assert_eq!(r["message_id"], "fake-poll-msg-id");
    }

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = SendPoll
            .call(
                handle(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "question": "Pick one?",
                    "options": ["A", "B"],
                    "multi": false,
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }
}
