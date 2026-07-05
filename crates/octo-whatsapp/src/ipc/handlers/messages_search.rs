//! `messages.search` — query the local message index.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize, Default)]
#[allow(dead_code)] // `since` / `limit` are reserved for Phase 3 event-router persistence.
struct Params {
    query: String,
    #[serde(default)]
    peer: Option<String>,
    #[serde(default)]
    since: Option<i64>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Debug)]
pub struct MessagesSearch;

#[async_trait::async_trait]
impl RpcHandler for MessagesSearch {
    fn name(&self) -> &'static str {
        "messages.search"
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
        let hits = adapter
            .message_search(&p.query, p.peer.as_deref())
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter message_search failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "hits": hits,
            "query": p.query,
            "limit": p.limit.unwrap_or(50),
        }))
    }
}