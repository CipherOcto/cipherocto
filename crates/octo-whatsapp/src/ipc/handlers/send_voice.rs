//! `send.voice` — outbound voice note. No caption.

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
pub struct SendVoice;

#[async_trait::async_trait]
impl RpcHandler for SendVoice {
    fn name(&self) -> &'static str {
        "send.voice"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let kind = MediaKind::Voice;
        let slot = preflight::preflight(&h, kind, &p.file).await?;
        let jid = crate::jids::peer_to_jid(&p.peer).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid peer: {e}"),
            data: Some(serde_json::json!({
                "expected_format": "E.164 or <digits>@s.whatsapp.net or <digits>@lid"
            })),
        })?;
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let jid = crate::jids::apply_self_routing(&jid, adapter.self_jid_full().as_deref());
        let (id, token) = adapter
            .send_voice_checked(&jid, &p.file, kind.max_bytes())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter send_voice failed: {e}"),
                data: Some(json!({"kind": kind.as_str()})),
            })?;
        let ts_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let ts_mono_ns = crate::events::now_mono_ns();
        let outbound_event = crate::events::InboundEvent::from_outbound_media(
            id.clone(),
            p.peer.clone(),
            jid.clone(),
            crate::events::MessageKind::Voice,
            None,
            Some(token.clone()),
            ts_unix_ms,
            ts_mono_ns,
        );
        h.events_buffer().push(outbound_event);
        Ok(json!({
            "status": "sent",
            "message_id": id,
            "media_ref_token": token,
            "size_bytes": slot.size_bytes,
            "kind": kind.as_str(),
            "ts_unix_ms": ts_unix_ms as u64,
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
    async fn not_connected_returns_minus_32012() {
        // Adapter is None — pre-flight passes (small real file), but
        // h.adapter().ok_or(NotConnected)? must fire before any adapter call.
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("voice.bin");
        std::fs::write(&f, b"hello-voice").unwrap();
        let err = SendVoice
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
        let f = tmp.path().join("voice.bin");
        std::fs::write(&f, b"hello-voice").unwrap();
        let r = SendVoice
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "file": f,
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "sent");
        assert_eq!(r["message_id"], "fake-voice-msg-id");
        assert_eq!(r["media_ref_token"], "fake-voice-token");
        assert_eq!(r["size_bytes"], 11);
        assert_eq!(r["kind"], "voice");
    }
}
