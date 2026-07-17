//! `blocking.get_blocklist` — return the current local blocklist as a
//! list of JID strings.

use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct BlockingGetBlocklist;

#[async_trait::async_trait]
impl RpcHandler for BlockingGetBlocklist {
    fn name(&self) -> &'static str {
        "blocking.get_blocklist"
    }

    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let jids = adapter.get_blocklist().await.map_err(|e| RpcError {
            code: RpcErrorCode::InternalError.as_i32(),
            message: format!("blocking.get_blocklist failed: {e}"),
            data: None,
        })?;
        Ok(json!({
            "jids": jids,
            "count": jids.len(),
        }))
    }
}
