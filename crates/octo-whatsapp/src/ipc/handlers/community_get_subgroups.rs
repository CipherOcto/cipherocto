//! `community.get_subgroups` — list all subgroups of a community.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    community_jid: String,
}

#[derive(Debug)]
pub struct CommunityGetSubgroups;

#[async_trait::async_trait]
impl RpcHandler for CommunityGetSubgroups {
    fn name(&self) -> &'static str {
        "community.get_subgroups"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.community_jid.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "community_jid must be non-empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let subgroups = adapter
            .community_get_subgroups(&p.community_jid)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("community.get_subgroups failed: {e}"),
                data: Some(json!({"community_jid": p.community_jid})),
            })?;
        Ok(json!({
            "community_jid": p.community_jid,
            "subgroups": subgroups,
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
        let err = CommunityGetSubgroups
            .call(
                handle(),
                serde_json::json!({"community_jid": "120363999999999@g.us"}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_jid_rejected() {
        let (h, _mock) = handle_with_mock();
        let err = CommunityGetSubgroups
            .call(h, serde_json::json!({"community_jid": "   "}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_returns_subgroups() {
        let (h, mock) = handle_with_mock();
        let r = CommunityGetSubgroups
            .call(
                h,
                serde_json::json!({"community_jid": "120363999999999@g.us"}),
            )
            .await
            .unwrap();
        assert_eq!(r["community_jid"], "120363999999999@g.us");
        assert!(r["subgroups"].is_array());
        assert_eq!(r["subgroups"].as_array().unwrap().len(), 1);
        assert_eq!(r["subgroups"][0]["jid"], "120363999999998@g.us");
        assert_eq!(r["subgroups"][0]["subject"], "Fake Subgroup");
        assert_eq!(mock.call_count("community_get_subgroups"), 1);
    }
}
