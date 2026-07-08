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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;
    use crate::test_mock_adapter::MockAdapter;
    use octo_adapter_whatsapp::MessageHit;
    use std::sync::Arc;

    fn handle() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        Daemon::new(cfg).handle()
    }

    fn handle_with_mock() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        h.bind_adapter(Arc::new(MockAdapter::new()));
        h
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = MessagesSearch
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "query": "hello",
                    "peer": "1234567890@s.whatsapp.net",
                }),
            )
            .await
            .unwrap();
        assert!(r.is_object());
        assert!(r["hits"].is_array());
        assert_eq!(r["hits"].as_array().unwrap().len(), 0);
        assert_eq!(r["query"], "hello");
        assert_eq!(r["limit"], 50);
    }

    #[tokio::test]
    async fn success_path_with_mock_and_override_hits() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        let mock = Arc::new(MockAdapter::new());
        mock.set_message_search_result(
            "message_search",
            vec![MessageHit {
                msg_id: "msg-1".into(),
                peer: "1234567890@s.whatsapp.net".into(),
                ts: 1_700_000_000,
                snippet: "hello world".into(),
            }],
        );
        h.bind_adapter(mock);

        let r = MessagesSearch
            .call(
                h,
                serde_json::json!({
                    "query": "hello",
                    "peer": "1234567890@s.whatsapp.net",
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["hits"].as_array().unwrap().len(), 1);
        assert_eq!(r["hits"][0]["msg_id"], "msg-1");
        assert_eq!(r["hits"][0]["snippet"], "hello world");
    }

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = MessagesSearch
            .call(
                handle(),
                serde_json::json!({
                    "query": "hello",
                    "peer": "1234567890@s.whatsapp.net",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }
}
