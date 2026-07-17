//! `daemon.set_passive` — toggle passive mode.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    passive: bool,
}

#[derive(Debug)]
pub struct DaemonSetPassive;

#[async_trait::async_trait]
impl RpcHandler for DaemonSetPassive {
    fn name(&self) -> &'static str {
        "daemon.set_passive"
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
        adapter.set_passive(p.passive).await.map_err(|e| RpcError {
            code: RpcErrorCode::InternalError.as_i32(),
            message: format!("adapter set_passive failed: {e}"),
            data: None,
        })?;
        Ok(json!({"status": "set", "passive": p.passive}))
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
        let err = DaemonSetPassive
            .call(handle(), serde_json::json!({"passive": true}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = DaemonSetPassive
            .call(handle_with_mock(), serde_json::json!({"passive": true}))
            .await
            .unwrap();
        assert_eq!(r["status"], "set");
        assert_eq!(r["passive"], true);
    }
}
