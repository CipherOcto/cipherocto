//! `groups.*` handlers — Phase 6.12 Task 2.
//!
//! These four handlers route through `CoordinatorAdmin::create_group`,
//! `list_own_groups`, `get_group_metadata`, and `leave_group`. The
//! `CoordinatorAdmin` view is fetched from the bound adapter via
//! `PlatformAdapter::as_coordinator_admin`; if no adapter is bound,
//! or the adapter does not implement `CoordinatorAdmin`, the call
//! fails with `NotConnected`.

use std::sync::Arc;
use std::time::Duration;

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

// --- groups.destroy ---

#[derive(Debug)]
pub struct GroupsDestroy;

#[async_trait::async_trait]
impl RpcHandler for GroupsDestroy {
    fn name(&self) -> &'static str {
        "groups.destroy"
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
            .destroy_group(&gid)
            .await
            .map_err(|e| map_err("groups.destroy", e))?;
        let _keep_alive = adapter;
        Ok(json!({}))
    }
}

// --- groups.add_member (singular) ---

#[derive(Debug)]
pub struct GroupsAddMember;

#[derive(Deserialize)]
struct AddMemberParams {
    jid: String,
    member: String,
    #[serde(default)]
    is_admin: bool,
}

#[async_trait::async_trait]
impl RpcHandler for GroupsAddMember {
    fn name(&self) -> &'static str {
        "groups.add_member"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: AddMemberParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let gid = GroupId::new(p.jid.clone());
        let spec = GroupMemberSpec {
            handle: p.member.clone(),
            display_name: None,
            is_admin: p.is_admin,
        };
        let out = coord
            .add_member(&gid, &spec)
            .await
            .map_err(|e| map_err("groups.add_member", e))?;
        let _keep_alive = adapter;
        Ok(json!({
            "jid": p.jid,
            "member": p.member,
            "added": out.added,
            "promoted": out.promoted.map(|r| r.is_ok()),
        }))
    }
}

// --- groups.add_members (array, partial-success) ---

#[derive(Debug)]
pub struct GroupsAddMembers;

#[derive(Deserialize)]
struct AddMembersParams {
    jid: String,
    #[serde(default)]
    members: Vec<AddMemberBatchEntry>,
}

#[derive(Deserialize)]
struct AddMemberBatchEntry {
    handle: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    is_admin: bool,
}

#[async_trait::async_trait]
impl RpcHandler for GroupsAddMembers {
    fn name(&self) -> &'static str {
        "groups.add_members"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: AddMembersParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let gid = GroupId::new(p.jid.clone());
        let mut added: Vec<Value> = Vec::new();
        let mut errors: Vec<Value> = Vec::new();
        for m in p.members.iter() {
            let spec = GroupMemberSpec {
                handle: m.handle.clone(),
                display_name: m.display_name.clone(),
                is_admin: m.is_admin,
            };
            match coord.add_member(&gid, &spec).await {
                Ok(out) => added.push(json!({
                    "handle": m.handle,
                    "is_admin": m.is_admin,
                    "added": out.added,
                })),
                Err(e) => errors.push(json!({
                    "handle": m.handle,
                    "error": e.to_string(),
                })),
            }
        }
        let _keep_alive = adapter;
        Ok(json!({
            "added": added,
            "errors": errors,
            "group_id": p.jid,
        }))
    }
}

// --- groups.remove_member (singular) ---

#[derive(Debug)]
pub struct GroupsRemoveMember;

#[derive(Deserialize)]
struct RemoveMemberParams {
    jid: String,
    member: String,
}

#[async_trait::async_trait]
impl RpcHandler for GroupsRemoveMember {
    fn name(&self) -> &'static str {
        "groups.remove_member"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: RemoveMemberParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let gid = GroupId::new(p.jid);
        let peer = PeerId::new(p.member);
        coord
            .remove_member(&gid, &peer)
            .await
            .map_err(|e| map_err("groups.remove_member", e))?;
        let _keep_alive = adapter;
        Ok(json!({}))
    }
}

// --- groups.remove_members (array, partial-success) ---

#[derive(Debug)]
pub struct GroupsRemoveMembers;

#[derive(Deserialize)]
struct RemoveMembersParams {
    jid: String,
    #[serde(default)]
    members: Vec<String>,
}

#[async_trait::async_trait]
impl RpcHandler for GroupsRemoveMembers {
    fn name(&self) -> &'static str {
        "groups.remove_members"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: RemoveMembersParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let gid = GroupId::new(p.jid);
        let mut removed: Vec<String> = Vec::new();
        let mut errors: Vec<Value> = Vec::new();
        for handle in p.members.iter() {
            let peer = PeerId::new(handle.clone());
            match coord.remove_member(&gid, &peer).await {
                Ok(()) => removed.push(handle.clone()),
                Err(e) => errors.push(json!({
                    "member": handle,
                    "error": e.to_string(),
                })),
            }
        }
        let _keep_alive = adapter;
        Ok(json!({
            "removed": removed,
            "errors": errors,
        }))
    }
}

// --- groups.promote ---

#[derive(Debug)]
pub struct GroupsPromote;

#[async_trait::async_trait]
impl RpcHandler for GroupsPromote {
    fn name(&self) -> &'static str {
        "groups.promote"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: RemoveMemberParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let gid = GroupId::new(p.jid);
        let peer = PeerId::new(p.member);
        coord
            .promote_to_admin(&gid, &peer)
            .await
            .map_err(|e| map_err("groups.promote", e))?;
        let _keep_alive = adapter;
        Ok(json!({}))
    }
}

// --- groups.demote ---

#[derive(Debug)]
pub struct GroupsDemote;

#[async_trait::async_trait]
impl RpcHandler for GroupsDemote {
    fn name(&self) -> &'static str {
        "groups.demote"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: RemoveMemberParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let gid = GroupId::new(p.jid);
        let peer = PeerId::new(p.member);
        coord
            .demote_from_admin(&gid, &peer)
            .await
            .map_err(|e| map_err("groups.demote", e))?;
        let _keep_alive = adapter;
        Ok(json!({}))
    }
}

// --- groups.ban ---

#[derive(Debug)]
pub struct GroupsBan;

#[derive(Deserialize)]
struct BanParams {
    jid: String,
    member: String,
    #[serde(default)]
    duration_seconds: Option<u64>,
}

#[async_trait::async_trait]
impl RpcHandler for GroupsBan {
    fn name(&self) -> &'static str {
        "groups.ban"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: BanParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let gid = GroupId::new(p.jid);
        let peer = PeerId::new(p.member);
        let duration = p.duration_seconds.map(Duration::from_secs);
        coord
            .ban_member(&gid, &peer, duration)
            .await
            .map_err(|e| map_err("groups.ban", e))?;
        let _keep_alive = adapter;
        Ok(json!({}))
    }
}

// --- groups.approve_join ---

#[derive(Debug)]
pub struct GroupsApproveJoin;

#[async_trait::async_trait]
impl RpcHandler for GroupsApproveJoin {
    fn name(&self) -> &'static str {
        "groups.approve_join"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: RemoveMemberParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let gid = GroupId::new(p.jid);
        let peer = PeerId::new(p.member);
        coord
            .approve_join_request(&gid, &peer)
            .await
            .map_err(|e| map_err("groups.approve_join", e))?;
        let _keep_alive = adapter;
        Ok(json!({}))
    }
}

// --- groups.rename ---

#[derive(Debug)]
pub struct GroupsRename;

#[derive(Deserialize)]
struct RenameParams {
    jid: String,
    subject: String,
}

#[async_trait::async_trait]
impl RpcHandler for GroupsRename {
    fn name(&self) -> &'static str {
        "groups.rename"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: RenameParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let gid = GroupId::new(p.jid);
        coord
            .rename_group(&gid, &p.subject)
            .await
            .map_err(|e| map_err("groups.rename", e))?;
        let _keep_alive = adapter;
        Ok(json!({}))
    }
}

// --- groups.set_description ---

#[derive(Debug)]
pub struct GroupsSetDescription;

#[derive(Deserialize)]
struct SetDescriptionParams {
    jid: String,
    description: String,
}

#[async_trait::async_trait]
impl RpcHandler for GroupsSetDescription {
    fn name(&self) -> &'static str {
        "groups.set_description"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: SetDescriptionParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let gid = GroupId::new(p.jid);
        coord
            .set_group_description(&gid, &p.description)
            .await
            .map_err(|e| map_err("groups.set_description", e))?;
        let _keep_alive = adapter;
        Ok(json!({}))
    }
}

// --- groups.set_locked ---

#[derive(Debug)]
pub struct GroupsSetLocked;

#[derive(Deserialize)]
struct SetLockedParams {
    jid: String,
    locked: bool,
}

#[async_trait::async_trait]
impl RpcHandler for GroupsSetLocked {
    fn name(&self) -> &'static str {
        "groups.set_locked"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: SetLockedParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let gid = GroupId::new(p.jid);
        coord
            .set_locked(&gid, p.locked)
            .await
            .map_err(|e| map_err("groups.set_locked", e))?;
        let _keep_alive = adapter;
        Ok(json!({}))
    }
}

// --- groups.transfer_ownership ---

#[derive(Debug)]
pub struct GroupsTransferOwnership;

#[async_trait::async_trait]
impl RpcHandler for GroupsTransferOwnership {
    fn name(&self) -> &'static str {
        "groups.transfer_ownership"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: RemoveMemberParams = serde_json::from_value(params).map_err(invalid_params)?;
        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;
        let gid = GroupId::new(p.jid);
        let peer = PeerId::new(p.member);
        coord
            .transfer_ownership(&gid, &peer)
            .await
            .map_err(|e| map_err("groups.transfer_ownership", e))?;
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

    // --- groups.destroy ---

    #[tokio::test]
    async fn groups_destroy_happy_path() {
        let h = fresh_daemon_with_mock();
        let v = GroupsDestroy
            .call(h, json!({"jid": "x@g.us"}))
            .await
            .unwrap();
        assert_eq!(v, json!({}));
    }

    #[tokio::test]
    async fn groups_destroy_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsDestroy
            .call(h, json!({"jid": "x@g.us"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn groups_destroy_missing_jid() {
        let h = fresh_daemon_with_mock();
        let e = GroupsDestroy.call(h, json!({})).await.unwrap_err();
        assert_eq!(e.code, RpcErrorCode::InvalidParams.as_i32());
    }

    // --- groups.add_member (singular) ---

    #[tokio::test]
    async fn groups_add_member_happy_path() {
        let h = fresh_daemon_with_mock();
        let v = GroupsAddMember
            .call(
                h,
                json!({"jid": "x@g.us", "member": "5511", "is_admin": false}),
            )
            .await
            .unwrap();
        assert_eq!(v["jid"], "x@g.us");
        assert_eq!(v["member"], "5511");
        assert_eq!(v["added"], true);
        assert_eq!(v["promoted"], Value::Null);
    }

    #[tokio::test]
    async fn groups_add_member_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsAddMember
            .call(h, json!({"jid": "x@g.us", "member": "5511"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn groups_add_member_missing_member() {
        let h = fresh_daemon_with_mock();
        let e = GroupsAddMember
            .call(h, json!({"jid": "x@g.us"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::InvalidParams.as_i32());
    }

    // --- groups.add_members (array) ---

    #[tokio::test]
    async fn groups_add_members_empty_array() {
        let h = fresh_daemon_with_mock();
        let v = GroupsAddMembers
            .call(h, json!({"jid": "x@g.us", "members": []}))
            .await
            .unwrap();
        assert_eq!(v["added"].as_array().unwrap().len(), 0);
        assert_eq!(v["errors"].as_array().unwrap().len(), 0);
        assert_eq!(v["group_id"], "x@g.us");
    }

    #[tokio::test]
    async fn groups_add_members_partial_success() {
        // First member succeeds (no canned error), second member
        // fails because `set_canned_err` is single-shot and gets
        // consumed by whichever call hits it first. To force the
        // second to fail, we issue one warm-up add_member so the
        // canned error is consumed, then expect the next two calls
        // (members 0 and 1) to both succeed.
        //
        // Real partial-success test: pre-seed the error AFTER the
        // first element has been processed. We do that by spawning
        // the loop, but `set_canned_err` is global, not per-call.
        //
        // Workaround: seed the canned error and have the first
        // element FAIL; verify partial-success by checking that the
        // array reports both `added` (empty) and `errors` (populated).
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "t""#).unwrap();
        let daemon = Daemon::new(cfg);
        let h = daemon.handle();
        let mock = Arc::new(MockAdapter::new());
        // Pre-seed error so EVERY add_member call returns Err until
        // consumed. Single-shot semantics: first call fails, rest succeed.
        mock.coord_admin.set_canned_err(
            "add_member",
            octo_network::dot::error::PlatformAdapterError::Unreachable {
                platform: "mock".into(),
                reason: "test".into(),
            },
        );
        h.set_adapter_for_tests(mock);
        let v = GroupsAddMembers
            .call(
                h,
                json!({
                    "jid": "x@g.us",
                    "members": [
                        {"handle": "5511"},
                        {"handle": "5522"}
                    ]
                }),
            )
            .await
            .unwrap();
        // First call consumes the error → fails. Second call succeeds.
        assert_eq!(v["added"].as_array().unwrap().len(), 1);
        assert_eq!(v["errors"].as_array().unwrap().len(), 1);
        assert_eq!(v["errors"][0]["handle"], "5511");
        assert_eq!(v["added"][0]["handle"], "5522");
        assert_eq!(v["group_id"], "x@g.us");
    }

    #[tokio::test]
    async fn groups_add_members_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsAddMembers
            .call(h, json!({"jid": "x@g.us", "members": [{"handle": "5511"}]}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    // --- groups.remove_member (singular) ---

    #[tokio::test]
    async fn groups_remove_member_happy_path() {
        let h = fresh_daemon_with_mock();
        let v = GroupsRemoveMember
            .call(h, json!({"jid": "x@g.us", "member": "5511"}))
            .await
            .unwrap();
        assert_eq!(v, json!({}));
    }

    #[tokio::test]
    async fn groups_remove_member_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsRemoveMember
            .call(h, json!({"jid": "x@g.us", "member": "5511"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn groups_remove_member_missing_member() {
        let h = fresh_daemon_with_mock();
        let e = GroupsRemoveMember
            .call(h, json!({"jid": "x@g.us"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::InvalidParams.as_i32());
    }

    // --- groups.remove_members (array) ---

    #[tokio::test]
    async fn groups_remove_members_empty_array() {
        let h = fresh_daemon_with_mock();
        let v = GroupsRemoveMembers
            .call(h, json!({"jid": "x@g.us", "members": []}))
            .await
            .unwrap();
        assert_eq!(v["removed"].as_array().unwrap().len(), 0);
        assert_eq!(v["errors"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn groups_remove_members_partial_success() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "t""#).unwrap();
        let daemon = Daemon::new(cfg);
        let h = daemon.handle();
        let mock = Arc::new(MockAdapter::new());
        mock.coord_admin.set_canned_err(
            "remove_member",
            octo_network::dot::error::PlatformAdapterError::Unreachable {
                platform: "mock".into(),
                reason: "test".into(),
            },
        );
        h.set_adapter_for_tests(mock);
        let v = GroupsRemoveMembers
            .call(h, json!({"jid": "x@g.us", "members": ["5511", "5522"]}))
            .await
            .unwrap();
        // First call consumes the canned error → fails. Second succeeds.
        assert_eq!(v["removed"].as_array().unwrap().len(), 1);
        assert_eq!(v["errors"].as_array().unwrap().len(), 1);
        assert_eq!(v["errors"][0]["member"], "5511");
        assert_eq!(v["removed"][0], "5522");
    }

    #[tokio::test]
    async fn groups_remove_members_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsRemoveMembers
            .call(h, json!({"jid": "x@g.us", "members": ["5511"]}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    // --- groups.promote ---

    #[tokio::test]
    async fn groups_promote_happy_path() {
        let h = fresh_daemon_with_mock();
        let v = GroupsPromote
            .call(h, json!({"jid": "x@g.us", "member": "5511"}))
            .await
            .unwrap();
        assert_eq!(v, json!({}));
    }

    #[tokio::test]
    async fn groups_promote_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsPromote
            .call(h, json!({"jid": "x@g.us", "member": "5511"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    // --- groups.demote ---

    #[tokio::test]
    async fn groups_demote_happy_path() {
        let h = fresh_daemon_with_mock();
        let v = GroupsDemote
            .call(h, json!({"jid": "x@g.us", "member": "5511"}))
            .await
            .unwrap();
        assert_eq!(v, json!({}));
    }

    #[tokio::test]
    async fn groups_demote_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsDemote
            .call(h, json!({"jid": "x@g.us", "member": "5511"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    // --- groups.ban ---

    #[tokio::test]
    async fn groups_ban_with_duration() {
        let h = fresh_daemon_with_mock();
        let v = GroupsBan
            .call(
                h,
                json!({"jid": "x@g.us", "member": "5511", "duration_seconds": 3600}),
            )
            .await
            .unwrap();
        assert_eq!(v, json!({}));
    }

    #[tokio::test]
    async fn groups_ban_indefinite() {
        let h = fresh_daemon_with_mock();
        let v = GroupsBan
            .call(h, json!({"jid": "x@g.us", "member": "5511"}))
            .await
            .unwrap();
        assert_eq!(v, json!({}));
    }

    #[tokio::test]
    async fn groups_ban_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsBan
            .call(h, json!({"jid": "x@g.us", "member": "5511"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    // --- groups.approve_join ---

    #[tokio::test]
    async fn groups_approve_join_happy_path() {
        let h = fresh_daemon_with_mock();
        let v = GroupsApproveJoin
            .call(h, json!({"jid": "x@g.us", "member": "5511"}))
            .await
            .unwrap();
        assert_eq!(v, json!({}));
    }

    #[tokio::test]
    async fn groups_approve_join_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsApproveJoin
            .call(h, json!({"jid": "x@g.us", "member": "5511"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    // --- groups.rename ---

    #[tokio::test]
    async fn groups_rename_happy_path() {
        let h = fresh_daemon_with_mock();
        let v = GroupsRename
            .call(h, json!({"jid": "x@g.us", "subject": "new name"}))
            .await
            .unwrap();
        assert_eq!(v, json!({}));
    }

    #[tokio::test]
    async fn groups_rename_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsRename
            .call(h, json!({"jid": "x@g.us", "subject": "x"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    // --- groups.set_description ---

    #[tokio::test]
    async fn groups_set_description_happy_path() {
        let h = fresh_daemon_with_mock();
        let v = GroupsSetDescription
            .call(h, json!({"jid": "x@g.us", "description": "new desc"}))
            .await
            .unwrap();
        assert_eq!(v, json!({}));
    }

    #[tokio::test]
    async fn groups_set_description_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsSetDescription
            .call(h, json!({"jid": "x@g.us", "description": "x"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    // --- groups.set_locked ---

    #[tokio::test]
    async fn groups_set_locked_true() {
        let h = fresh_daemon_with_mock();
        let v = GroupsSetLocked
            .call(h, json!({"jid": "x@g.us", "locked": true}))
            .await
            .unwrap();
        assert_eq!(v, json!({}));
    }

    #[tokio::test]
    async fn groups_set_locked_false() {
        let h = fresh_daemon_with_mock();
        let v = GroupsSetLocked
            .call(h, json!({"jid": "x@g.us", "locked": false}))
            .await
            .unwrap();
        assert_eq!(v, json!({}));
    }

    #[tokio::test]
    async fn groups_set_locked_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsSetLocked
            .call(h, json!({"jid": "x@g.us", "locked": true}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }

    // --- groups.transfer_ownership ---

    #[tokio::test]
    async fn groups_transfer_ownership_happy_path() {
        let h = fresh_daemon_with_mock();
        let v = GroupsTransferOwnership
            .call(h, json!({"jid": "x@g.us", "member": "5511"}))
            .await
            .unwrap();
        assert_eq!(v, json!({}));
    }

    #[tokio::test]
    async fn groups_transfer_ownership_no_adapter() {
        let h = fresh_daemon_no_adapter();
        let e = GroupsTransferOwnership
            .call(h, json!({"jid": "x@g.us", "member": "5511"}))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::NotConnected.as_i32());
    }
}
