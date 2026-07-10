//! `polls.vote` — submit a vote on an existing poll.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    /// Chat JID where the poll lives (1:1 or group).
    peer: String,
    /// Message id of the poll-creation message we are voting on.
    poll_msg_id: String,
    /// JID of whoever created the poll (the encryption AAD is
    /// keyed off it; getting it wrong makes the vote
    /// undecryptable on the receiver).
    poll_creator_jid: String,
    /// Base64-encoded 32-byte secret generated when the poll was
    /// created. Without it the WA server cannot encrypt the vote.
    message_secret_b64: String,
    /// Names of the options the voter is selecting. Multi-select
    /// polls accept more than one entry; the WA crate derives the
    /// cryptographic commitment per option name.
    selected_options: Vec<String>,
}

#[derive(Debug)]
pub struct PollsVote;

#[async_trait::async_trait]
impl RpcHandler for PollsVote {
    fn name(&self) -> &'static str {
        "polls.vote"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.selected_options.is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "selected_options must be non-empty".into(),
                data: None,
            });
        }
        if p.peer.trim().is_empty() || p.poll_msg_id.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "peer and poll_msg_id must be non-empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let msg_id = adapter
            .vote_poll(
                &p.peer,
                &p.poll_msg_id,
                &p.poll_creator_jid,
                &p.message_secret_b64,
                &p.selected_options,
            )
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter vote_poll failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "voted",
            "peer": p.peer,
            "poll_msg_id": p.poll_msg_id,
            "selected_options": p.selected_options,
            "message_id": msg_id,
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

    const SAMPLE_SECRET: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = PollsVote
            .call(
                handle(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "poll_msg_id": "POLL1",
                    "poll_creator_jid": "1234567890@s.whatsapp.net",
                    "message_secret_b64": SAMPLE_SECRET,
                    "selected_options": ["A"],
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_options_rejected() {
        let err = PollsVote
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "poll_msg_id": "POLL1",
                    "poll_creator_jid": "1234567890@s.whatsapp.net",
                    "message_secret_b64": SAMPLE_SECRET,
                    "selected_options": [],
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = PollsVote
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "poll_msg_id": "POLL1",
                    "poll_creator_jid": "1234567890@s.whatsapp.net",
                    "message_secret_b64": SAMPLE_SECRET,
                    "selected_options": ["A", "C"],
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "voted");
        assert_eq!(r["poll_msg_id"], "POLL1");
        assert_eq!(r["message_id"], "fake-poll-vote-msg-id");
        assert_eq!(r["selected_options"], serde_json::json!(["A", "C"]));
    }
}
