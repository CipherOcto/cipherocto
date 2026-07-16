//! `events.find` — pure-SQL filter over the `events` table. Backed
//! by `QueryService::find`. Use cases: ops dashboards showing
//! "all receipt events for peer X in the last hour", or
//! "all group_change events of kind Subject".

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize, Default)]
struct Params {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    variant: Option<String>,
    #[serde(default)]
    peer: Option<String>,
    #[serde(default)]
    since_ts_unix_ms: Option<i64>,
    #[serde(default)]
    until_ts_unix_ms: Option<i64>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug)]
pub struct EventsFind;

#[async_trait::async_trait]
impl RpcHandler for EventsFind {
    fn name(&self) -> &'static str {
        "events.find"
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
        let limit = p.limit.unwrap_or(100).min(1000);
        let hits = svc
            .find(
                p.kind.as_deref(),
                p.variant.as_deref(),
                p.peer.as_deref(),
                p.since_ts_unix_ms,
                p.until_ts_unix_ms,
                limit,
            )
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("events.find failed: {e}"),
                data: None,
            })?;
        let hits_json: Vec<Value> = hits
            .iter()
            .filter_map(|h| serde_json::to_value(h).ok())
            .collect();
        Ok(json!({
            "hits": hits_json,
            "count": hits_json.len(),
            "limit": limit,
        }))
    }
}
