//! `polls.aggregate` — tally an existing poll's votes by
//! decrypting each encrypted vote and resolving which option each
//! voter picked.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct VoteParam {
    voter_jid: String,
    /// Base64-encoded encrypted payload from
    /// `PollEncValue.enc_payload`.
    enc_payload_b64: String,
    /// Base64-encoded IV from `PollEncValue.enc_iv`.
    enc_iv_b64: String,
}

#[derive(Deserialize)]
struct Params {
    /// Original options the poll creator posted (the strings the
    /// tallier is matching hashes against).
    options: Vec<String>,
    /// Each encrypted vote, harvested from inbound
    /// `PollUpdateMessage`s.
    votes: Vec<VoteParam>,
    message_secret_b64: String,
    poll_msg_id: String,
    poll_creator_jid: String,
}

#[derive(Debug)]
pub struct PollsAggregate;

#[async_trait::async_trait]
impl RpcHandler for PollsAggregate {
    fn name(&self) -> &'static str {
        "polls.aggregate"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.options.is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "options must be non-empty".into(),
                data: None,
            });
        }
        // Decode base64 vote ciphertexts up front; report a single
        // InvalidParams error for any malformed entry so the
        // operator can see which vote is bad.
        let mut votes: Vec<(String, Vec<u8>, Vec<u8>)> = Vec::with_capacity(p.votes.len());
        for v in &p.votes {
            let enc_payload = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                v.enc_payload_b64.as_bytes(),
            )
            .map_err(|e| RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!(
                    "vote.enc_payload_b64 for {} invalid base64: {e}",
                    v.voter_jid
                ),
                data: None,
            })?;
            let enc_iv = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                v.enc_iv_b64.as_bytes(),
            )
            .map_err(|e| RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!("vote.enc_iv_b64 for {} invalid base64: {e}", v.voter_jid),
                data: None,
            })?;
            votes.push((v.voter_jid.clone(), enc_payload, enc_iv));
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let results = adapter
            .aggregate_poll_votes(
                &p.options,
                &votes,
                &p.message_secret_b64,
                &p.poll_msg_id,
                &p.poll_creator_jid,
            )
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter aggregate_poll_votes failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "aggregated",
            "poll_msg_id": p.poll_msg_id,
            "results": results,
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
    // 16 bytes of zero, base64 = "AAAAAAAAAAAAAAAAAAAAAA=="
    const SAMPLE_PAYLOAD: &str = "AAAAAAAAAAAAAAAAAAAAAA==";
    const SAMPLE_IV: &str = "AAAAAAAAAAAAAAAAAAAAAA==";

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = PollsAggregate
            .call(
                handle(),
                serde_json::json!({
                    "options": ["A", "B"],
                    "votes": [{
                        "voter_jid": "9999@s.whatsapp.net",
                        "enc_payload_b64": SAMPLE_PAYLOAD,
                        "enc_iv_b64": SAMPLE_IV,
                    }],
                    "message_secret_b64": SAMPLE_SECRET,
                    "poll_msg_id": "POLL1",
                    "poll_creator_jid": "1234567890@s.whatsapp.net",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_options_rejected() {
        let err = PollsAggregate
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "options": [],
                    "votes": [],
                    "message_secret_b64": SAMPLE_SECRET,
                    "poll_msg_id": "POLL1",
                    "poll_creator_jid": "1234567890@s.whatsapp.net",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn invalid_base64_vote_rejected() {
        let err = PollsAggregate
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "options": ["A"],
                    "votes": [{
                        "voter_jid": "9999@s.whatsapp.net",
                        "enc_payload_b64": "!!!not-base64!!!",
                        "enc_iv_b64": SAMPLE_IV,
                    }],
                    "message_secret_b64": SAMPLE_SECRET,
                    "poll_msg_id": "POLL1",
                    "poll_creator_jid": "1234567890@s.whatsapp.net",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
        assert!(
            err.message.contains("9999@s.whatsapp.net"),
            "error must name the offending voter; got {}",
            err.message
        );
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = PollsAggregate
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "options": ["A", "B"],
                    "votes": [{
                        "voter_jid": "9999@s.whatsapp.net",
                        "enc_payload_b64": SAMPLE_PAYLOAD,
                        "enc_iv_b64": SAMPLE_IV,
                    }],
                    "message_secret_b64": SAMPLE_SECRET,
                    "poll_msg_id": "POLL1",
                    "poll_creator_jid": "1234567890@s.whatsapp.net",
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "aggregated");
        assert_eq!(r["poll_msg_id"], "POLL1");
        let results = r["results"].as_array().expect("results must be array");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["name"], "A");
        assert!(results[0]["voters"].is_array());
    }
}
