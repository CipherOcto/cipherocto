//! `daemon.set_resend_rate_limit` — retune the per-chat
//! outbound resend rate limiter live. Maps to
//! `Client::set_resend_rate_limit`.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    /// Instantaneous allowance per chat. 0 disables the limiter.
    burst: u32,
    /// Sustained ceiling per chat per minute.
    refill_per_min: u32,
}

#[derive(Debug)]
pub struct DaemonSetResendRateLimit;

#[async_trait::async_trait]
impl RpcHandler for DaemonSetResendRateLimit {
    fn name(&self) -> &'static str {
        "daemon.set_resend_rate_limit"
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
            .set_resend_rate_limit(p.burst, p.refill_per_min)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter set_resend_rate_limit failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "set",
            "burst": p.burst,
            "refill_per_min": p.refill_per_min,
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
        let err = DaemonSetResendRateLimit
            .call(
                handle(),
                serde_json::json!({"burst": 8, "refill_per_min": 60}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = DaemonSetResendRateLimit
            .call(
                handle_with_mock(),
                serde_json::json!({"burst": 16, "refill_per_min": 120}),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "set");
        assert_eq!(r["burst"], 16);
        assert_eq!(r["refill_per_min"], 120);
    }
}
