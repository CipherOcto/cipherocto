//! `daemon.set_wanted_pre_key_count` — set the per-upload
//! pre-key batch size. Maps to
//! `Client::set_wanted_pre_key_count`.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    /// Per-batch one-time pre-key count. Defaults to 812
    /// (UPLOAD_KEYS_COUNT).
    count: u32,
}

#[derive(Debug)]
pub struct DaemonSetWantedPreKeyCount;

#[async_trait::async_trait]
impl RpcHandler for DaemonSetWantedPreKeyCount {
    fn name(&self) -> &'static str {
        "daemon.set_wanted_pre_key_count"
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
        adapter
            .set_wanted_pre_key_count(p.count)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter set_wanted_pre_key_count failed: {e}"),
                data: None,
            })?;
        Ok(json!({"status": "set", "count": p.count}))
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
        let err = DaemonSetWantedPreKeyCount
            .call(handle(), serde_json::json!({"count": 812}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = DaemonSetWantedPreKeyCount
            .call(handle_with_mock(), serde_json::json!({"count": 1624}))
            .await
            .unwrap();
        assert_eq!(r["status"], "set");
        assert_eq!(r["count"], 1624);
    }
}
