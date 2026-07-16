//! `events.create` — create a WA calendar event. The `message_secret`
//! returned by the WA client (used for RSVP decryption) is internal
//! to the protocol and not surfaced.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    /// Recipient JID — the chat where the event is created.
    /// Typically the operator's own JID or a group JID.
    to: String,
    name: String,
    /// Event start time, UNIX epoch seconds.
    start_time: i64,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug)]
pub struct EventsCreate;

#[async_trait::async_trait]
impl RpcHandler for EventsCreate {
    fn name(&self) -> &'static str {
        "events.create"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.name.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "name cannot be empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let msg_id = adapter
            .create_event(&p.to, &p.name, p.start_time, p.description.as_deref())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("events.create failed: {e}"),
                data: Some(json!({
                    "to": p.to,
                    "name": p.name,
                    "start_time": p.start_time,
                })),
            })?;
        Ok(json!({
            "status": "created",
            "message_id": msg_id,
            "to": p.to,
            "name": p.name,
            "start_time": p.start_time,
        }))
    }
}
