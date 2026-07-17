//! `newsletter.create` — create a new newsletter (channel).

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug)]
pub struct NewsletterCreate;

#[async_trait::async_trait]
impl RpcHandler for NewsletterCreate {
    fn name(&self) -> &'static str {
        "newsletter.create"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.name.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "name must be non-empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let meta = adapter
            .create_newsletter(&p.name, p.description.as_deref())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter create_newsletter failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "created",
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
        let err = NewsletterCreate
            .call(handle(), serde_json::json!({"name": "Foo"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_name_rejected() {
        let err = NewsletterCreate
            .call(handle_with_mock(), serde_json::json!({"name": "   "}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = NewsletterCreate
            .call(
                handle_with_mock(),
                serde_json::json!({"name": "Foo", "description": "bar"}),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "created");
        assert_eq!(r["newsletter"]["name"], "Fake Newsletter");
        assert_eq!(r["newsletter"]["state"], "active");
    }
}
