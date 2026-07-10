//! `tctoken.get` — read a locally-stored privacy token by JID.

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
pub struct TcTokenGet;

#[async_trait::async_trait]
impl RpcHandler for TcTokenGet {
    fn name(&self) -> &'static str {
        "tctoken.get"
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
        let entry = adapter.get_tc_token(&p.jid).await.map_err(|e| RpcError {
            code: RpcErrorCode::InternalError.as_i32(),
            message: format!("adapter get_tc_token failed: {e}"),
            data: None,
        })?;
        match entry {
            Some(e) => Ok(json!({
                "status": "found",
                "jid": p.jid,
                "entry": e,
            })),
            None => Ok(json!({
                "status": "not_found",
                "jid": p.jid,
            })),
        }
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
        let err = TcTokenGet
            .call(handle(), serde_json::json!({"jid": "9999@s.whatsapp.net"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = TcTokenGet
            .call(
                handle_with_mock(),
                serde_json::json!({"jid": "9999@s.whatsapp.net"}),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "not_found");
    }
}
