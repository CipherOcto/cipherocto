//! `events.respond` — RSVP to a WA calendar event.
//!
//! The 32-byte `message_secret_b64` is the per-event secret
//! generated when the event was created (returned by
//! `events.create` in a future commit, or extracted from the
//! inbound event's `MessageContextInfo`).
//!
//! `response` is one of:
//! - `"going"`     — RSVP yes
//! - `"not_going"` — RSVP no
//! - `"maybe"`     — tentative

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    /// Chat JID where the event lives (1:1 or group).
    peer: String,
    /// Message id of the event creation.
    event_msg_id: String,
    /// JID of whoever created the event (the RSVP encryption
    /// AAD is keyed off it).
    event_creator_jid: String,
    /// Base64-encoded 32-byte secret from event creation.
    message_secret_b64: String,
    /// `"going"`, `"not_going"`, or `"maybe"`.
    response: String,
    /// Optional extra-guest count (`+1`, `+2`, ...). `None`
    /// means the responder is attending solo.
    #[serde(default)]
    extra_guest_count: Option<i32>,
}

#[derive(Debug)]
pub struct EventsRespond;

#[async_trait::async_trait]
impl RpcHandler for EventsRespond {
    fn name(&self) -> &'static str {
        "events.respond"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let response = match p.response.as_str() {
            "going" => Ok(octo_adapter_whatsapp::waproto::whatsapp::message::event_response_message::EventResponseType::Going),
            "not_going" => Ok(octo_adapter_whatsapp::waproto::whatsapp::message::event_response_message::EventResponseType::NotGoing),
            "maybe" => Ok(octo_adapter_whatsapp::waproto::whatsapp::message::event_response_message::EventResponseType::Maybe),
            other => Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!(
                    "response must be one of going/not_going/maybe; got {other:?}"
                ),
                data: None,
            }),
        }?;
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let msg_id = adapter
            .respond_event(
                &p.peer,
                &p.event_msg_id,
                &p.event_creator_jid,
                &p.message_secret_b64,
                response,
                p.extra_guest_count,
            )
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter respond_event failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "responded",
            "peer": p.peer,
            "event_msg_id": p.event_msg_id,
            "response": p.response,
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
        let err = EventsRespond
            .call(
                handle(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "event_msg_id": "EVT1",
                    "event_creator_jid": "1234567890@s.whatsapp.net",
                    "message_secret_b64": SAMPLE_SECRET,
                    "response": "going",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn invalid_response_rejected() {
        let err = EventsRespond
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "event_msg_id": "EVT1",
                    "event_creator_jid": "1234567890@s.whatsapp.net",
                    "message_secret_b64": SAMPLE_SECRET,
                    "response": "perhaps",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_going() {
        let r = EventsRespond
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "event_msg_id": "EVT1",
                    "event_creator_jid": "1234567890@s.whatsapp.net",
                    "message_secret_b64": SAMPLE_SECRET,
                    "response": "going",
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "responded");
        assert_eq!(r["response"], "going");
        assert_eq!(r["message_id"], "fake-event-respond-msg-id");
    }

    #[tokio::test]
    async fn success_not_going_with_extra_guests() {
        let r = EventsRespond
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "event_msg_id": "EVT1",
                    "event_creator_jid": "1234567890@s.whatsapp.net",
                    "message_secret_b64": SAMPLE_SECRET,
                    "response": "not_going",
                    "extra_guest_count": 2,
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "responded");
        assert_eq!(r["response"], "not_going");
    }
}
