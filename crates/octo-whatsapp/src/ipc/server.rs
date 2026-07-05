//! Unix-socket JSON-RPC server. Phase 1: handler trait + registry.
//! The actual socket plumbing arrives in Task 32.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use super::protocol::{RpcError, RpcErrorCode, RpcRequest, RpcResponse};
use crate::daemon::DaemonHandle;

/// One RPC method handler.
#[async_trait::async_trait]
pub trait RpcHandler: Send + Sync {
    fn name(&self) -> &'static str;
    async fn call(&self, handle: DaemonHandle, params: Value) -> Result<Value, RpcError>;
}

pub struct HandlerRegistry {
    handlers: HashMap<&'static str, Arc<dyn RpcHandler>>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register(mut self, h: Arc<dyn RpcHandler>) -> Self {
        self.handlers.insert(h.name(), h);
        self
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn RpcHandler>> {
        self.handlers.get(name).cloned()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    pub fn methods(&self) -> Vec<&'static str> {
        let mut v: Vec<&'static str> = self.handlers.keys().copied().collect();
        v.sort_unstable();
        v
    }

    pub async fn dispatch(&self, handle: DaemonHandle, req: RpcRequest) -> RpcResponse {
        match self.handlers.get(req.method.as_str()) {
            Some(h) => match h.call(handle, req.params).await {
                Ok(result) => RpcResponse {
                    id: req.id,
                    result: Some(result),
                    error: None,
                },
                Err(err) => RpcResponse {
                    id: req.id,
                    result: None,
                    error: Some(err),
                },
            },
            None => RpcResponse {
                id: req.id,
                result: None,
                error: Some(RpcError {
                    code: RpcErrorCode::MethodNotFound.as_i32(),
                    message: format!("method {:?} not found in this build", req.method),
                    data: Some(serde_json::json!({
                        "api_version": env!("CARGO_PKG_VERSION"),
                        "available_in": "phase2_or_later",
                    })),
                }),
            },
        }
    }
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
