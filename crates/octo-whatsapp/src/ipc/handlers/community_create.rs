//! `community.create` — create a new community (parent group).

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
    #[serde(default)]
    closed: bool,
    #[serde(default)]
    allow_non_admin_sub_group_creation: bool,
    #[serde(default = "default_true")]
    create_general_chat: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug)]
pub struct CommunityCreate;

#[async_trait::async_trait]
impl RpcHandler for CommunityCreate {
    fn name(&self) -> &'static str {
        "community.create"
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
            .community_create(
                &p.name,
                p.description.as_deref(),
                p.closed,
                p.allow_non_admin_sub_group_creation,
                p.create_general_chat,
            )
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("community.create failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "created",
            "community": meta,
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
        let err = CommunityCreate
            .call(handle(), serde_json::json!({"name": "Test"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_name_rejected() {
        let (h, _mock) = handle_with_mock();
        let err = CommunityCreate
            .call(h, serde_json::json!({"name": "   "}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let (h, mock) = handle_with_mock();
        let r = CommunityCreate
            .call(h, serde_json::json!({"name": "My Community"}))
            .await
            .unwrap();
        assert_eq!(r["status"], "created");
        assert_eq!(r["community"]["is_parent_group"], true);
        assert_eq!(r["community"]["subject"], "My Community");
        assert_eq!(mock.call_count("community_create"), 1);
    }
}
