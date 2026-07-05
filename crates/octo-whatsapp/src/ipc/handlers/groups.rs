//! `groups.*` handlers. Phase 1 returns `NotConnected` for all four — the
//! adapter is not wired in Phase 1. Phase 2 will route through
//! `CoordinatorAdmin::create_group/list/info/leave`.

use serde_json::Value;

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct GroupsCreate;
#[derive(Debug)]
pub struct GroupsList;
#[derive(Debug)]
pub struct GroupsInfo;
#[derive(Debug)]
pub struct GroupsLeave;

fn not_connected(method: &str) -> RpcError {
    RpcError {
        code: RpcErrorCode::NotConnected.as_i32(),
        message: format!("adapter not wired in Phase 1 ({method} arrives in Phase 2)"),
        data: None,
    }
}

#[async_trait::async_trait]
impl RpcHandler for GroupsCreate {
    fn name(&self) -> &'static str {
        "groups.create"
    }
    async fn call(&self, _h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        Err(not_connected("groups.create"))
    }
}

#[async_trait::async_trait]
impl RpcHandler for GroupsList {
    fn name(&self) -> &'static str {
        "groups.list"
    }
    async fn call(&self, _h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        Err(not_connected("groups.list"))
    }
}

#[async_trait::async_trait]
impl RpcHandler for GroupsInfo {
    fn name(&self) -> &'static str {
        "groups.info"
    }
    async fn call(&self, _h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        Err(not_connected("groups.info"))
    }
}

#[async_trait::async_trait]
impl RpcHandler for GroupsLeave {
    fn name(&self) -> &'static str {
        "groups.leave"
    }
    async fn call(&self, _h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        Err(not_connected("groups.leave"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    fn handle() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        Daemon::new(cfg).handle()
    }

    #[tokio::test]
    async fn groups_create_returns_not_connected_in_phase1() {
        let err = GroupsCreate
            .call(
                handle(),
                serde_json::json!({"subject": "ops", "members": ["+15551234567"]}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, -32012);
    }

    #[tokio::test]
    async fn groups_list_returns_not_connected_in_phase1() {
        let err = GroupsList.call(handle(), Value::Null).await.unwrap_err();
        assert_eq!(err.code, -32012);
    }
}
