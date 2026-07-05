//! `send.location` — outbound location pin.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;
use crate::limits::MediaKind;

#[derive(Deserialize)]
struct Params {
    peer: String,
    lat: f64,
    lon: f64,
    name: String,
}

#[derive(Debug)]
pub struct SendLocation;

#[async_trait::async_trait]
impl RpcHandler for SendLocation {
    fn name(&self) -> &'static str {
        "send.location"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let kind = MediaKind::Location;
        let payload_size = p.name.len() + 64;
        if payload_size > kind.max_bytes() {
            return Err(RpcError {
                code: RpcErrorCode::PayloadTooLarge.as_i32(),
                message: format!(
                    "location payload {payload_size} > ceiling {}",
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
            .send_location_checked(&p.peer, p.lat, p.lon, &p.name, kind.max_bytes())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter send_location failed: {e}"),
                data: Some(json!({"kind": kind.as_str()})),
            })?;
        Ok(json!({
            "status": "sent",
            "message_id": id,
            "kind": kind.as_str(),
        }))
    }
}
