//! `community.unlink_subgroups` — remove groups from a community.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    community_jid: String,
    subgroup_jids: Vec<String>,
    #[serde(default)]
    remove_orphan_members: bool,
}

#[derive(Debug)]
pub struct CommunityUnlinkSubgroups;

#[async_trait::async_trait]
impl RpcHandler for CommunityUnlinkSubgroups {
    fn name(&self) -> &'static str {
        "community.unlink_subgroups"
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
        if p.subgroup_jids.is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "subgroup_jids must contain at least one JID".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let result = adapter
            .community_unlink_subgroups(&p.community_jid, &p.subgroup_jids, p.remove_orphan_members)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("community.unlink_subgroups failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "unlinked",
            "community_jid": p.community_jid,
            "result": result,
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

    fn handle_with_mock() -> (DaemonHandle, Arc<MockAdapter>) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (_daemon, handle) = Daemon::new_for_tests(tmp.path());
        let mock = Arc::new(MockAdapter::new());
        handle.bind_adapter(mock.clone() as Arc<dyn OctoWhatsAppAdapter>);
        (handle, mock)
    }

    #[tokio::test]
    async fn empty_community_jid_rejected() {
        let (h, _mock) = handle_with_mock();
        let err = CommunityUnlinkSubgroups
            .call(
                h,
                serde_json::json!({
                    "community_jid": "   ",
                    "subgroup_jids": ["120363999999998@g.us"],
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn empty_subgroup_jids_rejected() {
        let (h, _mock) = handle_with_mock();
        let err = CommunityUnlinkSubgroups
            .call(
                h,
                serde_json::json!({
                    "community_jid": "120363999999999@g.us",
                    "subgroup_jids": [],
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let (h, mock) = handle_with_mock();
        let r = CommunityUnlinkSubgroups
            .call(
                h,
                serde_json::json!({
                    "community_jid": "120363999999999@g.us",
                    "subgroup_jids": ["120363999999998@g.us"],
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "unlinked");
        assert_eq!(mock.call_count("community_unlink_subgroups"), 1);
    }
}
