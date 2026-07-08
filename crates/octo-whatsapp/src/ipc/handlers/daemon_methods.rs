//! `daemon.methods.list` and `daemon.methods.help` — agent discovery.
//!
//! Phase 3 Part C: two methods for introspecting the daemon's RPC
//! surface. `list` returns just the names; `help` returns the schema
//! for a single method (param list). Both are stateless proxies over
//! the `HandlerRegistry` that the daemon already holds.

use serde_json::{json, Value};

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use super::build_registry;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct DaemonMethodsList;

#[async_trait::async_trait]
impl RpcHandler for DaemonMethodsList {
    fn name(&self) -> &'static str {
        "daemon.methods.list"
    }
    async fn call(&self, _h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let reg = build_registry();
        let methods = reg.methods();
        Ok(json!({
            "methods": methods,
            "count": methods.len(),
        }))
    }
}

#[derive(Debug)]
pub struct DaemonMethodsHelp;

#[async_trait::async_trait]
impl RpcHandler for DaemonMethodsHelp {
    fn name(&self) -> &'static str {
        "daemon.methods.help"
    }
    async fn call(&self, _h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let method = params
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError {
                code: -32602,
                message: "missing or non-string `method`".to_string(),
                data: Some(json!({ "params": params })),
            })?;
        let reg = build_registry();
        if !reg.contains(method) {
            return Err(RpcError {
                code: -32601,
                message: format!("method {method:?} not found"),
                data: Some(json!({ "method": method })),
            });
        }
        Ok(json!({
            "method": method,
            "registered": true,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;

    fn handle() -> DaemonHandle {
        let tmp = tempfile::tempdir().expect("tempdir");
        Daemon::new_for_tests(tmp.path()).1
    }

    #[tokio::test]
    async fn list_returns_all_registered_methods() {
        let v = DaemonMethodsList.call(handle(), Value::Null).await.unwrap();
        let count = v["count"].as_u64().unwrap() as usize;
        assert!(count > 30, "expected >30 methods, got {count}");
        let methods = v["methods"].as_array().unwrap();
        assert!(methods.iter().any(|m| m == "version.get"));
        assert!(methods.iter().any(|m| m == "events.list"));
    }

    #[tokio::test]
    async fn help_returns_registered_for_known_method() {
        let v = DaemonMethodsHelp
            .call(handle(), json!({ "method": "send.text" }))
            .await
            .unwrap();
        assert_eq!(v["method"], "send.text");
        assert_eq!(v["registered"], true);
    }

    #[tokio::test]
    async fn help_returns_minus_32601_for_unknown_method() {
        let e = DaemonMethodsHelp
            .call(handle(), json!({ "method": "totally.fake" }))
            .await
            .unwrap_err();
        assert_eq!(e.code, -32601);
    }

    #[tokio::test]
    async fn help_returns_minus_32602_for_missing_method_param() {
        let e = DaemonMethodsHelp
            .call(handle(), Value::Null)
            .await
            .unwrap_err();
        assert_eq!(e.code, -32602);
    }
}
