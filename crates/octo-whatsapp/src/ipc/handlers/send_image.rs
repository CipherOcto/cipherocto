//! `send.image` — outbound image with optional caption.
//!
//! Pre-flight ceiling is enforced by `preflight::preflight` (16 MiB for
//! images). On success the request is forwarded to the adapter's
//! `send_image_checked` method which re-checks size and dispatches.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use super::preflight;
use crate::daemon::DaemonHandle;
use crate::limits::MediaKind;

#[derive(Deserialize)]
struct Params {
    peer: String,
    file: std::path::PathBuf,
    #[serde(default)]
    caption: Option<String>,
}

#[derive(Debug)]
pub struct SendImage;

#[async_trait::async_trait]
impl RpcHandler for SendImage {
    fn name(&self) -> &'static str {
        "send.image"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let kind = MediaKind::Image;
        let slot = preflight::preflight(&h, kind, &p.file).await?;
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let (id, token) = adapter
            .send_image_checked(&p.peer, &p.file, p.caption.as_deref(), kind.max_bytes())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter send_image failed: {e}"),
                data: Some(json!({"kind": kind.as_str()})),
            })?;
        Ok(json!({
            "status": "sent",
            "message_id": id,
            "media_ref_token": token,
            "size_bytes": slot.size_bytes,
            "kind": kind.as_str(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    fn handle() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        Daemon::new(cfg).handle()
    }

    #[tokio::test]
    async fn ceiling_is_enforced_pre_flight() {
        // 16 MiB + 1 byte over the ceiling — pre-flight rejects with
        // -32004 before any adapter contact.
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("big.bin");
        let bytes = vec![0u8; MediaKind::Image.max_bytes() + 1];
        std::fs::write(&f, &bytes).unwrap();
        let err = SendImage
            .call(
                handle(),
                serde_json::json!({"peer": "+15551234567", "file": f}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::PayloadTooLarge.as_i32());
    }
}
