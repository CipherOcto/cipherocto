//! `privacy.get` — fetch all current privacy settings as a list of
//! `{category, value}` pairs (wire string forms).

use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct PrivacyGet;

#[async_trait::async_trait]
impl RpcHandler for PrivacyGet {
    fn name(&self) -> &'static str {
        "privacy.get"
    }

    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let settings = adapter
            .fetch_privacy_settings()
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("privacy.get failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "settings": settings,
            "count": settings.len(),
        }))
    }
}
