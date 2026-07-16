//! `identity.is_lid_migrated` — return `true` if the device has
//! completed the LID (local-identifier) migration.

use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct IdentityIsLidMigrated;

#[async_trait::async_trait]
impl RpcHandler for IdentityIsLidMigrated {
    fn name(&self) -> &'static str {
        "identity.is_lid_migrated"
    }

    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let migrated = adapter.is_lid_migrated().await.map_err(|e| RpcError {
            code: RpcErrorCode::InternalError.as_i32(),
            message: format!("identity.is_lid_migrated failed: {e}"),
            data: None,
        })?;
        Ok(json!({ "migrated": migrated }))
    }
}
