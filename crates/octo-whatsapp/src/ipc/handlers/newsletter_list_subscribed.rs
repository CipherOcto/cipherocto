//! `newsletter.list_subscribed` — list every newsletter this
//! account is subscribed to.

use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct NewsletterListSubscribed;

#[async_trait::async_trait]
impl RpcHandler for NewsletterListSubscribed {
    fn name(&self) -> &'static str {
        "newsletter.list_subscribed"
    }

    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let list = adapter
            .list_subscribed_newsletters()
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("newsletter.list_subscribed failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "newsletters": list,
            "count": list.len(),
        }))
    }
}
