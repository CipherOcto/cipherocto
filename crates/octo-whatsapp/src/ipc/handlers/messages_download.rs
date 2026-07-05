//! `messages.download` — fetch media referenced by a media_ref_token.

use serde::Deserialize;
use serde_json::{json, Value};

use octo_network::dot::adapters::PlatformAdapter;

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
