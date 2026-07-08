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
    use crate::daemon::Daemon;
    use crate::test_mock_adapter::MockAdapter;
    use std::sync::Arc;

    fn handle() -> DaemonHandle {
        // Leak the TempDir so the media buffer root survives the helper
        // return. `new_for_tests` creates `data` + `sock` but not `media`;
        // pre-flight writes a probe file under the media buffer root, so
        // the directory must exist before preflight runs.
        let tmp = Box::leak(Box::new(tempfile::tempdir().expect("tempdir")));
        std::fs::create_dir_all(tmp.path().join("media")).expect("mkdir media");
        Daemon::new_for_tests(tmp.path()).1
    }

    fn handle_with_mock() -> DaemonHandle {
        let h = handle();
        h.bind_adapter(Arc::new(MockAdapter::new()));
        h
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

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        // Adapter is None — pre-flight passes (small real file), but
        // h.adapter().ok_or(NotConnected)? must fire before any adapter call.
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("img.bin");
        std::fs::write(&f, b"hello").unwrap();
        let err = SendImage
            .call(
                handle(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "file": f,
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("img.bin");
        std::fs::write(&f, b"hello").unwrap();
        let r = SendImage
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "file": f,
                    "caption": "look at this",
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "sent");
        assert_eq!(r["message_id"], "fake-img-msg-id");
        assert_eq!(r["media_ref_token"], "fake-img-token");
        assert_eq!(r["size_bytes"], 5);
        assert_eq!(r["kind"], "image");
    }
}
