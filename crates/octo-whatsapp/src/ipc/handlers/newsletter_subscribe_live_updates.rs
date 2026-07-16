//! `newsletter.subscribe_live_updates` — subscribe to live push updates for a newsletter.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    jid: String,
}

#[derive(Debug)]
pub struct NewsletterSubscribeLiveUpdates;

#[async_trait::async_trait]
impl RpcHandler for NewsletterSubscribeLiveUpdates {
    fn name(&self) -> &'static str {
        "newsletter.subscribe_live_updates"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.jid.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "jid must be non-empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let duration_seconds = adapter
            .newsletter_subscribe_live_updates(&p.jid)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter newsletter_subscribe_live_updates failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "ok",
            "duration_seconds": duration_seconds,
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
        let err = NewsletterSubscribeLiveUpdates
            .call(
                handle(),
                serde_json::json!({"jid": "120363012345678901@newsletter"}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_param_rejected() {
        let (h, _mock) = handle_with_mock();
        let err = NewsletterSubscribeLiveUpdates
            .call(h, serde_json::json!({"jid": "   "}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let (h, mock) = handle_with_mock();
        let r = NewsletterSubscribeLiveUpdates
            .call(
                h,
                serde_json::json!({"jid": "120363012345678901@newsletter"}),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "ok");
        assert_eq!(r["duration_seconds"], 300);
        assert_eq!(mock.call_count("newsletter_subscribe_live_updates"), 1);
    }
}
