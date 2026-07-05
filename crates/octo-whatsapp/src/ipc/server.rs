//! Unix-socket JSON-RPC server. Phase 1: handler trait + registry + bind/accept
//! loop. Per-connection idle timeouts are deferred to Task 36.

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use tokio::net::UnixListener;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

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

impl std::fmt::Debug for HandlerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut v: Vec<&'static str> = self.handlers.keys().copied().collect();
        v.sort_unstable();
        f.debug_struct("HandlerRegistry")
            .field("methods", &v)
            .finish()
    }
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

// --- unix socket server ---

/// A bound unix-domain socket at a known path.
///
/// `bind` removes any existing socket file (unix sockets can't be rebound
/// over an existing file), binds the socket, and locks down permissions to
/// `0600` so only the owning UID can talk to the daemon.
///
/// The bound `UnixListener` is not stored on the struct so `bind` stays a
/// synchronous, infallible-ish setup step. `serve` re-binds at the start
/// of its loop; the re-bind races with no one because `bind` already holds
/// the path and the per-connection handlers don't touch `socket_path`.
pub struct UnixSocketServer {
    pub socket_path: PathBuf,
}

impl std::fmt::Debug for UnixSocketServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnixSocketServer")
            .field("socket_path", &self.socket_path)
            .finish()
    }
}

impl UnixSocketServer {
    pub fn bind(path: &Path) -> std::io::Result<Self> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        drop(listener);
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms)?;
        info!(socket = ?path, "bound unix socket");
        Ok(Self {
            socket_path: path.to_path_buf(),
        })
    }

    /// Bind a fresh listener on the same path. Used by `serve` to obtain an
    /// async listener — `bind` above is the canonical path-write setup.
    pub fn listener(&self) -> std::io::Result<UnixListener> {
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }
        let listener = UnixListener::bind(&self.socket_path)?;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&self.socket_path, perms)?;
        Ok(listener)
    }

    /// Accept loop. Stops on cancellation; cleans up the socket file before
    /// returning.
    pub async fn serve(
        self,
        handle: DaemonHandle,
        registry: Arc<HandlerRegistry>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let listener = self.listener()?;
        info!(socket = ?self.socket_path, "unix socket server: accept loop starting");
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("unix socket server: cancel observed, exiting");
                    let _ = std::fs::remove_file(&self.socket_path);
                    return Ok(());
                }
                accept = listener.accept() => {
                    let (stream, _addr) = match accept {
                        Ok(p) => p,
                        Err(e) => {
                            warn!(error = %e, "accept failed");
                            continue;
                        }
                    };
                    let h = handle.clone();
                    let r = registry.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_conn(stream, h, r).await {
                            warn!(error = %e, "connection handler error");
                        }
                    });
                }
            }
        }
    }
}

/// One connection: line-delimited JSON-RPC. EOF (read returns 0) is the
/// client's signal to close. A parse error becomes a JSON-RPC `-32700`
/// response so the client can recover and continue.
async fn handle_conn(
    mut stream: tokio::net::UnixStream,
    handle: DaemonHandle,
    registry: Arc<HandlerRegistry>,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let (read_half, mut write_half) = stream.split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(());
        }
        let req = match RpcRequest::from_json(line.as_bytes()) {
            Ok(r) => r,
            Err(e) => {
                let resp = RpcResponse {
                    id: 0,
                    result: None,
                    error: Some(RpcError {
                        code: RpcErrorCode::ParseError.as_i32(),
                        message: format!("parse error: {e}"),
                        data: None,
                    }),
                };
                let mut s = serde_json::to_string(&resp)?;
                s.push('\n');
                write_half.write_all(s.as_bytes()).await?;
                continue;
            }
        };
        let resp = registry.dispatch(handle.clone(), req).await;
        let mut s = serde_json::to_string(&resp)?;
        s.push('\n');
        write_half.write_all(s.as_bytes()).await?;
    }
}

#[cfg(test)]
mod tests;
