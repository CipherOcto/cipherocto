//! `newsletter.get_metadata` — fetch metadata for one newsletter.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    jid: String,
}

#[derive(Debug)]
pub struct NewsletterGetMetadata;

#[async_trait::async_trait]
impl RpcHandler for NewsletterGetMetadata {
    fn name(&self) -> &'static str {
        "newsletter.get_metadata"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let info = adapter
            .get_newsletter_metadata(&p.jid)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("newsletter.get_metadata failed: {e}"),
                data: Some(json!({"jid": p.jid})),
            })?;
        Ok(json!({
            "info": info,
        }))
    }
}
