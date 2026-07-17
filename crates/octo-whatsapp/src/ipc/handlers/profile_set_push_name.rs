//! `profile.set_push_name` — update our display name. Peers see the
//! new name within ~1 s on their next chat list refresh; the change
//! also propagates to our other linked devices via app-state sync.
//!
//! **Tier 6 of the live coverage matrix.**

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    name: String,
}

#[derive(Debug)]
pub struct ProfileSetPushName;

#[async_trait::async_trait]
impl RpcHandler for ProfileSetPushName {
    fn name(&self) -> &'static str {
        "profile.set_push_name"
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
                message: "name cannot be empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter.set_push_name(&p.name).await.map_err(|e| RpcError {
            code: RpcErrorCode::InternalError.as_i32(),
            message: format!("profile.set_push_name failed: {e}"),
            data: Some(json!({"name": p.name})),
        })?;
        Ok(json!({
            "status": "renamed",
            "name": p.name,
        }))
    }
}
