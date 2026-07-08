//! `send.contact` — outbound vCard contact file.

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
    vcard: std::path::PathBuf,
}

#[derive(Debug)]
pub struct SendContact;

#[async_trait::async_trait]
impl RpcHandler for SendContact {
    fn name(&self) -> &'static str {
        "send.contact"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let kind = MediaKind::Contact;
        let slot = preflight::preflight(&h, kind, &p.vcard).await?;
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let id = adapter
            .send_contact_checked(&p.peer, &p.vcard, kind.max_bytes())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter send_contact failed: {e}"),
                data: Some(json!({"kind": kind.as_str()})),
            })?;
        Ok(json!({
            "status": "sent",
            "message_id": id,
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
        let tmp = tempfile::tempdir().expect("tempdir");
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
        let f = tmp.path().join("contact.vcf");
        std::fs::write(&f, b"BEGIN:VCARD\nEND:VCARD\n").unwrap();
        let err = SendContact
            .call(
                handle(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "vcard": f,
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("contact.vcf");
        std::fs::write(&f, b"BEGIN:VCARD\nVERSION:3.0\nFN:Alice\nEND:VCARD\n").unwrap();
        let r = SendContact
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "vcard": f,
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "sent");
        assert_eq!(r["message_id"], "fake-contact-msg-id");
        assert_eq!(r["kind"], "contact");
        assert!(r["media_ref_token"].is_null());
    }
}
