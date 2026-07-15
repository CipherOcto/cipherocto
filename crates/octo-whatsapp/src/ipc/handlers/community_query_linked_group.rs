//! `community.query_linked_group` — query one linked subgroup's metadata from the parent community.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    community_jid: String,
    subgroup_jid: String,
}

#[derive(Debug)]
pub struct CommunityQueryLinkedGroup;

#[async_trait::async_trait]
impl RpcHandler for CommunityQueryLinkedGroup {
    fn name(&self) -> &'static str {
        "community.query_linked_group"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.community_jid.trim().is_empty() || p.subgroup_jid.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "community_jid and subgroup_jid must be non-empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let meta = adapter
            .community_query_linked_group(&p.community_jid, &p.subgroup_jid)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("community.query_linked_group failed: {e}"),
                data: Some(
                    json!({"community_jid": p.community_jid, "subgroup_jid": p.subgroup_jid}),
                ),
            })?;
        Ok(json!({
            "community_jid": p.community_jid,
            "subgroup_jid": p.subgroup_jid,
            "subgroup": meta,
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
        let err = CommunityQueryLinkedGroup
            .call(
                handle(),
                serde_json::json!({
                    "community_jid": "120363999999999@g.us",
                    "subgroup_jid": "120363999999998@g.us",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_jid_rejected() {
        let (h, _mock) = handle_with_mock();
        // empty community_jid
        let err = CommunityQueryLinkedGroup
            .call(
                h.clone(),
                serde_json::json!({
                    "community_jid": "   ",
                    "subgroup_jid": "120363999999998@g.us",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
        // empty subgroup_jid
        let err = CommunityQueryLinkedGroup
            .call(
                h,
                serde_json::json!({
                    "community_jid": "120363999999999@g.us",
                    "subgroup_jid": "",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let (h, mock) = handle_with_mock();
        let r = CommunityQueryLinkedGroup
            .call(
                h,
                serde_json::json!({
                    "community_jid": "120363999999999@g.us",
                    "subgroup_jid": "120363999999998@g.us",
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["community_jid"], "120363999999999@g.us");
        assert_eq!(r["subgroup_jid"], "120363999999998@g.us");
        assert_eq!(r["subgroup"]["jid"], "120363999999998@g.us");
        assert_eq!(r["subgroup"]["subject"], "Queried Subgroup");
        assert_eq!(mock.call_count("community_query_linked_group"), 1);
    }
}
