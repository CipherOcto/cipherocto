//! `identity.get_lid` — return our LID (local identifier) JID as a
//! string, or `null` if migration has not occurred.

use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct IdentityGetLid;

#[async_trait::async_trait]
impl RpcHandler for IdentityGetLid {
    fn name(&self) -> &'static str {
        "identity.get_lid"
    }

    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let lid = adapter.get_lid().await.map_err(|e| RpcError {
            code: RpcErrorCode::InternalError.as_i32(),
            message: format!("identity.get_lid failed: {e}"),
            data: None,
        })?;
        Ok(json!({
            "lid": lid,
            "migrated": lid.is_some(),
        }))
    }
}
