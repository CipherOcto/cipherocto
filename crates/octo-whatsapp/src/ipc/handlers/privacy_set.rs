//! `privacy.set` — update one privacy setting.
//!
//! Wire-string forms of category + value:
//! - category: `last | online | profile | status | groupadd | readreceipts |
//!   calladd | messages | defense`
//! - value:    `all | contacts | none | contact_blacklist | match_last_seen |
//!   known | off | on_standard`
//!
//! The server rejects invalid (category, value) combinations; the
//! handler surfaces those rejections as `InternalError`.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    category: String,
    value: String,
}

#[derive(Debug)]
pub struct PrivacySet;

#[async_trait::async_trait]
impl RpcHandler for PrivacySet {
    fn name(&self) -> &'static str {
        "privacy.set"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.category.trim().is_empty() || p.value.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "category and value must be non-empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter
            .set_privacy_setting(&p.category, &p.value)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("privacy.set failed: {e}"),
                data: Some(json!({"category": p.category, "value": p.value})),
            })?;
        Ok(json!({
            "status": "set",
            "category": p.category,
            "value": p.value,
        }))
    }
}
