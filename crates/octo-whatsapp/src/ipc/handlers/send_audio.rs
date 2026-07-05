//! `send.audio` — outbound audio file. No caption.

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
}

#[derive(Debug)]
pub struct SendAudio;

#[async_trait::async_trait]
impl RpcHandler for SendAudio {
    fn name(&self) -> &'static str {
        "send.audio"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let kind = MediaKind::Audio;
        let slot = preflight::preflight(&h, kind, &p.file).await?;
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let (id, token) = adapter
            .send_audio_checked(&p.peer, &p.file, kind.max_bytes())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter send_audio failed: {e}"),
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
