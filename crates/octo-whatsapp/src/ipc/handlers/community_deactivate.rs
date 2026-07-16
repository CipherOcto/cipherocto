//! `community.deactivate` — deactivate (delete) a community.

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
pub struct CommunityDeactivate;

#[async_trait::async_trait]
impl RpcHandler for CommunityDeactivate {
    fn name(&self) -> &'static str {
        "community.deactivate"
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
        adapter
            .community_deactivate(&p.jid)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("community.deactivate failed: {e}"),
                data: Some(json!({"jid": p.jid})),
            })?;
        Ok(json!({
            "status": "deactivated",
            "jid": p.jid,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;
    use crate::test_mock_adapter::MockAdapter;
    use crate::OctoWhatsAppAdapter;
    use std::sync::Arc;

    fn handle() -> DaemonHandle {
        let tmp = tempfile::tempdir().expect("tempdir");
        Daemon::new_for_tests(tmp.path()).1
    }

    fn handle_with_mock() -> (DaemonHandle, Arc<MockAdapter>) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
        let mock = Arc::new(MockAdapter::new());
        handle.bind_adapter(mock.clone() as Arc<dyn OctoWhatsAppAdapter>);
        (handle, mock)
    }

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = CommunityDeactivate
            .call(handle(), serde_json::json!({"jid": "120363999999999@g.us"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_jid_rejected() {
        let (h, _mock) = handle_with_mock();
        let err = CommunityDeactivate
            .call(h, serde_json::json!({"jid": "   "}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let (h, mock) = handle_with_mock();
        let r = CommunityDeactivate
            .call(h, serde_json::json!({"jid": "120363999999999@g.us"}))
            .await
            .unwrap();
        assert_eq!(r["status"], "deactivated");
        assert_eq!(r["jid"], "120363999999999@g.us");
        assert_eq!(mock.call_count("community_deactivate"), 1);
    }
}
