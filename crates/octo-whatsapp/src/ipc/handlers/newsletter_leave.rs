//! `newsletter.leave` — leave a newsletter.

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
pub struct NewsletterLeave;

#[async_trait::async_trait]
impl RpcHandler for NewsletterLeave {
    fn name(&self) -> &'static str {
        "newsletter.leave"
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
        adapter
            .leave_newsletter(&p.jid)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("newsletter.leave failed: {e}"),
                data: Some(json!({"jid": p.jid})),
            })?;
        Ok(json!({
            "status": "left",
            "jid": p.jid,
        }))
    }
}
