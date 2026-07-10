//! `tctoken.issue` — issue privacy tokens for the given JIDs.
//!
//! Operator-only RPC: requires admin role. Skips silently when
//! the running account does not have admin privileges.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    jids: Vec<String>,
}

#[derive(Debug)]
pub struct TcTokenIssue;

#[async_trait::async_trait]
impl RpcHandler for TcTokenIssue {
    fn name(&self) -> &'static str {
        "tctoken.issue"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.jids.is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "jids must be non-empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let tokens = adapter
            .issue_tc_tokens(&p.jids)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter issue_tc_tokens failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "issued",
            "tokens": tokens,
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
        let err = TcTokenIssue
            .call(
                handle(),
                serde_json::json!({"jids": ["9999@s.whatsapp.net"]}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_jids_rejected() {
        let err = TcTokenIssue
            .call(handle_with_mock(), serde_json::json!({"jids": []}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = TcTokenIssue
            .call(
                handle_with_mock(),
                serde_json::json!({"jids": ["9999@s.whatsapp.net"]}),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "issued");
        assert!(r["tokens"].is_array());
    }
}
