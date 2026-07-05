//! `send.video` — outbound video with optional caption.

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
pub struct SendVideo;

#[async_trait::async_trait]
impl RpcHandler for SendVideo {
    fn name(&self) -> &'static str {
        "send.video"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let kind = MediaKind::Video;
        let slot = preflight::preflight(&h, kind, &p.file).await?;
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let (id, token) = adapter
            .send_video_checked(&p.peer, &p.file, p.caption.as_deref(), kind.max_bytes())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter send_video failed: {e}"),
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
