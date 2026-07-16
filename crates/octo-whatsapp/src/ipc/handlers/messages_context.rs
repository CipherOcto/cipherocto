//! `messages.context` — return the surrounding window of messages
//! around a given event_id. Backed by `QueryService::context`.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize, Default)]
struct Params {
    /// The pivot event_id.
    event_id: i64,
    #[serde(default)]
    before: Option<usize>,
    #[serde(default)]
    after: Option<usize>,
}

#[derive(Debug)]
pub struct MessagesContext;

#[async_trait::async_trait]
impl RpcHandler for MessagesContext {
    fn name(&self) -> &'static str {
        "messages.context"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let svc = h.query_service().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "query subsystem not installed".into(),
            data: None,
        })?;
        let before = p.before.unwrap_or(5).min(50);
        let after = p.after.unwrap_or(5).min(50);
        let hits = svc
            .context(p.event_id, before, after)
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("context lookup failed: {e}"),
                data: None,
            })?;
        let hits_json: Vec<Value> = hits
            .iter()
            .filter_map(|h| serde_json::to_value(h).ok())
            .collect();
        Ok(json!({
            "event_id": p.event_id,
            "hits": hits_json,
            "count": hits_json.len(),
            "before": before,
            "after": after,
        }))
    }
}
