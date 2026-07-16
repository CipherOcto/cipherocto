//! `newsletter.join` — join (subscribe to) a newsletter by its JID.

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
pub struct NewsletterJoin;

#[async_trait::async_trait]
impl RpcHandler for NewsletterJoin {
    fn name(&self) -> &'static str {
        "newsletter.join"
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
        let meta = adapter
            .join_newsletter(&p.jid)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter join_newsletter failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "joined",
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

    fn handle_with_mock() -> DaemonHandle {
        let h = handle();
        h.bind_adapter(Arc::new(MockAdapter::new()));
        h
    }

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = NewsletterJoin
            .call(
                handle(),
                serde_json::json!({"jid": "120363012345678901@newsletter"}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = NewsletterJoin
            .call(
                handle_with_mock(),
                serde_json::json!({"jid": "120363012345678901@newsletter"}),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "joined");
        assert_eq!(r["newsletter"]["jid"], "120363012345678901@newsletter");
        assert_eq!(r["newsletter"]["name"], "Fake Newsletter");
    }
}
