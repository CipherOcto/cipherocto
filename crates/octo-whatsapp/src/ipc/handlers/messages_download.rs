//! `messages.download` — fetch media referenced by a media_ref_token.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    media_ref_token: String,
    out_path: std::path::PathBuf,
}

#[derive(Debug)]
pub struct MessagesDownload;

#[async_trait::async_trait]
impl RpcHandler for MessagesDownload {
    fn name(&self) -> &'static str {
        "messages.download"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let bytes = adapter
            .download_media(&p.media_ref_token)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter download_media failed: {e}"),
                data: None,
            })?;
        tokio::fs::write(&p.out_path, &bytes)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::Internal.as_i32(),
                message: format!("failed to write {out_path:?}: {e}", out_path = p.out_path),
                data: None,
            })?;
        Ok(json!({
            "status": "downloaded",
            "out_path": p.out_path,
            "size_bytes": bytes.len(),
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

    #[test]
    fn name_is_messages_download() {
        assert_eq!(MessagesDownload.name(), "messages.download");
    }

    #[tokio::test]
    async fn invalid_params_returns_minus_32602() {
        // No `media_ref_token` field — params deserialization fails.
        let tmp = tempfile::tempdir().unwrap();
        let err = MessagesDownload
            .call(
                handle(),
                serde_json::json!({"out_path": tmp.path().join("out.bin")}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn no_adapter_returns_not_connected() {
        // Valid params but no adapter bound — early NotConnected (-32012).
        let tmp = tempfile::tempdir().unwrap();
        let err = MessagesDownload
            .call(
                handle(),
                serde_json::json!({
                    "media_ref_token": "tok-abc",
                    "out_path": tmp.path().join("out.bin"),
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock_returns_decoded_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("dl.bin");
        let h = handle();
        // Override mock to return a known payload.
        let mock = MockAdapter::new();
        mock.set_download_media_result("download_media", vec![1, 2, 3, 4, 5]);
        h.bind_adapter(Arc::new(mock));
        let r = MessagesDownload
            .call(
                h,
                serde_json::json!({
                    "media_ref_token": "tok-xyz",
                    "out_path": out.clone(),
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "downloaded");
        assert_eq!(r["size_bytes"], 5);
        assert_eq!(r["out_path"], serde_json::json!(out));
        // Verify file was actually written.
        assert_eq!(std::fs::read(&out).unwrap(), vec![1, 2, 3, 4, 5]);
    }
}
