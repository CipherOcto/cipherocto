//! `groups.*` handlers — Phase 6.12 Task 2.
//!
//! These four handlers route through `CoordinatorAdmin::create_group`,
//! `list_own_groups`, `get_group_metadata`, and `leave_group`. The
//! `CoordinatorAdmin` view is fetched from the bound adapter via
//! `PlatformAdapter::as_coordinator_admin`; if no adapter is bound,
//! or the adapter does not implement `CoordinatorAdmin`, the call
//! fails with `NotConnected`.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use octo_network::dot::adapters::coordinator_admin::{
    GroupHandle, GroupId, GroupMemberSpec, GroupMetadata, PeerId,
};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::adapter_trait::OctoWhatsAppAdapter;
use crate::daemon::DaemonHandle;

// --- shared helpers ---

/// Acquire the `Arc<dyn OctoWhatsAppAdapter>` bound to the daemon.
/// Returns a `NotConnected` `RpcError` if no adapter is bound.
fn require_adapter(h: &DaemonHandle) -> Result<Arc<dyn OctoWhatsAppAdapter>, RpcError> {
    h.adapter().ok_or(RpcError {
        code: RpcErrorCode::NotConnected.as_i32(),
        message: "no adapter bound".into(),
        data: None,
    })
}

/// Map a `PlatformAdapterError` into an `RpcError` with the
/// `Internal` code, prefixed by the method name for easier triage.
fn map_err(method: &str, e: octo_network::dot::error::PlatformAdapterError) -> RpcError {
    RpcError {
        code: RpcErrorCode::Internal.as_i32(),
        message: format!("{method} failed: {e}"),
        data: None,
    }
}

fn invalid_params(e: serde_json::Error) -> RpcError {
    RpcError {
        code: RpcErrorCode::InvalidParams.as_i32(),
        message: format!("invalid params: {e}"),
        data: None,
    }
}

fn group_handle_to_json(h: &GroupHandle) -> Value {
    json!({
        "jid": h.id.as_str(),
        "subject": h.subject,
        "invite_url": h.invite_url,
        "is_admin": h.is_admin,
        "member_count": h.member_count,
    })
}

fn peer_ids_to_json(p: &[PeerId]) -> Vec<&str> {
    p.iter().map(|p| p.as_str()).collect()
}

fn group_metadata_to_json(m: &GroupMetadata) -> Value {
    json!({
        "jid": m.id.as_str(),
        "subject": m.subject,
        "description": m.description,
        "members": peer_ids_to_json(&m.members),
        "admins": peer_ids_to_json(&m.admins),
        "invite_url": m.invite_url,
        "mode_flags": {
            "locked": m.mode_flags.locked,
            "announce_only": m.mode_flags.announce_only,
            "ephemeral_seconds": m.mode_flags.ephemeral_ttl.map(|d| d.as_secs()),
            "requires_approval": m.mode_flags.requires_approval,
        },
    })
}

// --- groups.create ---

#[derive(Debug)]
pub struct GroupsCreate;

#[derive(Deserialize)]
struct CreateParams {
    subject: String,
    #[serde(default)]
    members: Vec<MemberInput>,
}

#[derive(Deserialize)]
struct MemberInput {
    handle: String,
    #[serde(default)]
    is_admin: bool,
}

#[async_trait::async_trait]
impl RpcHandler for GroupsCreate {
    fn name(&self) -> &'static str {
        "groups.create"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: CreateParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let specs: Vec<GroupMemberSpec> = p
            .members
            .iter()
            .map(|m| GroupMemberSpec {
                handle: m.handle.clone(),
                display_name: None,
                is_admin: m.is_admin,
            })
            .collect();
        let handle = coord
            .create_group(&p.subject, &specs)
            .await
            .map_err(|e| map_err("groups.create", e))?;
        let _keep_alive = adapter;
        Ok(group_handle_to_json(&handle))
    }
}

// --- groups.list ---

#[derive(Debug)]
pub struct GroupsList;

#[async_trait::async_trait]
impl RpcHandler for GroupsList {
    fn name(&self) -> &'static str {
        "groups.list"
    }
    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let handles = coord
            .list_own_groups()
            .await
            .map_err(|e| map_err("groups.list", e))?;
        let _keep_alive = adapter;
        let groups: Vec<Value> = handles.iter().map(group_handle_to_json).collect();
        Ok(json!({ "groups": groups }))
    }
}

// --- groups.info ---

#[derive(Debug)]
pub struct GroupsInfo;

#[derive(Deserialize)]
struct InfoParams {
    jid: String,
}

#[async_trait::async_trait]
impl RpcHandler for GroupsInfo {
    fn name(&self) -> &'static str {
        "groups.info"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: InfoParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let gid = GroupId::new(p.jid);
        let meta = coord
            .get_group_metadata(&gid)
            .await
            .map_err(|e| map_err("groups.info", e))?;
        let _keep_alive = adapter;
        Ok(group_metadata_to_json(&meta))
    }
}

// --- groups.leave ---

#[derive(Debug)]
pub struct GroupsLeave;

#[async_trait::async_trait]
impl RpcHandler for GroupsLeave {
    fn name(&self) -> &'static str {
        "groups.leave"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: InfoParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let gid = GroupId::new(p.jid);
        coord
            .leave_group(&gid)
            .await
            .map_err(|e| map_err("groups.leave", e))?;
        let _keep_alive = adapter;
        Ok(json!({}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;
    use crate::test_mock_adapter::MockAdapter;

    fn fresh_daemon_with_mock() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "t""#).unwrap();
        let h = Daemon::new(cfg).handle();
        h.set_adapter_for_tests(Arc::new(MockAdapter::new()));
        h
    }

    fn fresh_daemon_no_adapter() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "t""#).unwrap();
        Daemon::new(cfg).handle()
    }

    // --- groups.create ---

    #[tokio::test]
    async fn groups_create_happy_path() {
        let h = fresh_daemon_with_mock();
        let v = GroupsCreate
            .call(
                h,
                json!({"subject": "test", "members": [{"handle": "5511"}]}),
            )
            .await
            .unwrap();
        assert_eq!(v["subject"], "test");
        assert!(v["jid"].as_str().unwrap().contains("@g.us"));
    }

    #[tokio::test]
    async fn groups_create_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsCreate
            .call(h, json!({"subject": "test"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn groups_create_missing_subject() {
        let h = fresh_daemon_with_mock();
        let e = GroupsCreate.call(h, json!({})).await.unwrap_err();
        assert_eq!(e.code, RpcErrorCode::InvalidParams.as_i32());
    }

    // --- groups.list ---

    #[tokio::test]
    async fn groups_list_happy_path() {
        let h = fresh_daemon_with_mock();
        let v = GroupsList.call(h, Value::Null).await.unwrap();
        assert!(v["groups"].is_array());
    }

    #[tokio::test]
    async fn groups_list_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsList.call(h, Value::Null).await.unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    // --- groups.info ---

    #[tokio::test]
    async fn groups_info_happy_path() {
        let h = fresh_daemon_with_mock();
        let v = GroupsInfo.call(h, json!({"jid": "x@g.us"})).await.unwrap();
        assert!(v["members"].is_array());
        assert!(v["mode_flags"]["locked"].is_boolean());
    }

    #[tokio::test]
    async fn groups_info_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsInfo
            .call(h, json!({"jid": "x@g.us"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn groups_info_missing_jid() {
        let h = fresh_daemon_with_mock();
        let e = GroupsInfo.call(h, json!({})).await.unwrap_err();
        assert_eq!(e.code, RpcErrorCode::InvalidParams.as_i32());
    }

    // --- groups.leave ---

    #[tokio::test]
    async fn groups_leave_happy_path() {
        let h = fresh_daemon_with_mock();
        let v = GroupsLeave.call(h, json!({"jid": "x@g.us"})).await.unwrap();
        assert_eq!(v, json!({}));
    }

    #[tokio::test]
    async fn groups_leave_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsLeave
            .call(h, json!({"jid": "x@g.us"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn groups_leave_missing_jid() {
        let h = fresh_daemon_with_mock();
        let e = GroupsLeave.call(h, json!({})).await.unwrap_err();
        assert_eq!(e.code, RpcErrorCode::InvalidParams.as_i32());
    }
}
