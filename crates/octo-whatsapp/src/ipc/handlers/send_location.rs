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
        // Location max is 1 KiB; flood the `name` field to overshoot.
        let err = SendLocation
            .call(
                handle(),
                serde_json::json!({
                    "peer": "+15551234567",
                    "lat": 0.0,
                    "lon": 0.0,
                    "name": "X".repeat(MediaKind::Location.max_bytes() + 100),
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::PayloadTooLarge.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = SendLocation
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "lat": 51.5074,
                    "lon": -0.1278,
                    "name": "London",
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "sent");
        assert_eq!(r["message_id"], "fake-loc-msg-id");
        assert_eq!(r["kind"], "location");
    }

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = SendLocation
            .call(
                handle(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "lat": 51.5074,
                    "lon": -0.1278,
                    "name": "London",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }
}
