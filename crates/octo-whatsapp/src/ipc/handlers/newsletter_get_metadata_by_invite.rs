//! `newsletter.get_metadata_by_invite` — fetch newsletter metadata by invite code.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    invite: String,
}

#[derive(Debug)]
pub struct NewsletterGetMetadataByInvite;

#[async_trait::async_trait]
impl RpcHandler for NewsletterGetMetadataByInvite {
    fn name(&self) -> &'static str {
        "newsletter.get_metadata_by_invite"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.invite.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "invite must be non-empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let meta = adapter
            .newsletter_get_metadata_by_invite(&p.invite)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter newsletter_get_metadata_by_invite failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "ok",
            "newsletter": meta,
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

    fn handle_with_mock() -> (DaemonHandle, Arc<MockAdapter>) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
        let mock = Arc::new(MockAdapter::new());
        handle.bind_adapter(mock.clone() as Arc<dyn crate::OctoWhatsAppAdapter>);
        (handle, mock)
    }

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = NewsletterGetMetadataByInvite
            .call(handle(), serde_json::json!({"invite": "ABCD1234"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_param_rejected() {
        let (h, _mock) = handle_with_mock();
        let err = NewsletterGetMetadataByInvite
            .call(h, serde_json::json!({"invite": ""}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let (h, mock) = handle_with_mock();
        let r = NewsletterGetMetadataByInvite
            .call(h, serde_json::json!({"invite": "ABCD1234"}))
            .await
            .unwrap();
        assert_eq!(r["status"], "ok");
        assert_eq!(r["newsletter"]["name"], "Fake Newsletter");
        assert_eq!(r["newsletter"]["jid"], "0000@newsletter");
        assert_eq!(mock.call_count("newsletter_get_metadata_by_invite"), 1);
    }
}
