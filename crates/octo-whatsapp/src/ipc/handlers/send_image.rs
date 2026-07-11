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
        // Peer validation + canonical JID. Mirrors `send.text`'s pre-flight:
        // over-size text must never reach the adapter; same invariant for
        // media — we want a canonical JID before dispatch so the synthetic
        // outbound event surfaces a consistent peer shape.
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
        // Self-send routing (digit-prefix match against session's canonical JID).
        // Companion-linked sessions (device suffix `:N`) would otherwise
        // dispatch to a different WA account — same gap class as the
        // `send.text` fix in commit `9fd44984`.
        let jid = crate::jids::apply_self_routing(&jid, adapter.self_jid_full().as_deref());
        let (id, token) = adapter
            .send_image_checked(&jid, &p.file, p.caption.as_deref(), kind.max_bytes())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter send_image failed: {e}"),
                data: Some(json!({"kind": kind.as_str()})),
            })?;
        // Operator mandate: every dispatched media MUST surface in the
        // events table so every linked WA client mirrors the bubble. WA's
        // own self-echo path is unreliable on single-device sessions and
        // filtered for 1:1 chats by the adapter's `accept_message` policy,
        // so we synthesise a typed outbound `Message` event from this
        // project's handler layer (independent of the adapter's
        // `octo-adapter-whatsapp` crate). Two functions, two data flows,
        // isolated to `octo-whatsapp`.
        let ts_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let ts_mono_ns = crate::events::now_mono_ns();
        let outbound_event = crate::events::InboundEvent::from_outbound_media(
            id.clone(),
            p.peer.clone(),
            jid.clone(),
            crate::events::MessageKind::Image,
            p.caption.clone(),
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
            "peer": p.peer,
            "routed_jid": jid,
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
