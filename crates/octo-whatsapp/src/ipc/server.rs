//! Unix-socket JSON-RPC server. Phase 1: handler trait + registry + bind/accept
//! loop. Per-connection idle timeouts are deferred to Task 36.

use std::collections::HashMap;
use std::net::IpAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use tokio::net::UnixListener;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::protocol::{RpcError, RpcErrorCode, RpcRequest, RpcResponse};
use crate::daemon::DaemonHandle;
use crate::security::{authenticate, AuthBackoff};

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
/// The bound `UnixListener` is stored on the struct so `serve` reuses it
/// without re-binding. The earlier "drop and re-bind" pattern could hang on
/// Linux when the freshly-released socket path was still in the kernel's
/// pending-state table; see handoff memory note for details.
pub struct UnixSocketServer {
    pub socket_path: PathBuf,
    listener: Option<UnixListener>,
}

impl std::fmt::Debug for UnixSocketServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnixSocketServer")
            .field("socket_path", &self.socket_path)
            .field(
                "listener_bound",
                &self.listener.as_ref().map(|_| true).unwrap_or(false),
            )
            .finish()
    }
}

impl UnixSocketServer {
    pub fn bind(path: &Path) -> std::io::Result<Self> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms)?;
        info!(socket = ?path, "bound unix socket");
        Ok(Self {
            socket_path: path.to_path_buf(),
            listener: Some(listener),
        })
    }

    /// Accept loop. Stops on cancellation; cleans up the socket file before
    /// returning.
    pub async fn serve(
        mut self,
        handle: DaemonHandle,
        registry: Arc<HandlerRegistry>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let listener = self
            .listener
            .take()
            .ok_or_else(|| anyhow::anyhow!("UnixSocketServer::serve called without bind()"))?;
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
///
/// Phase 5 Part A: every dispatched RPC goes through the bearer-auth
/// middleware. The middleware's hermetic-bypass kicks in when no
/// active tokens are loaded (matches pre-Phase 5 test contract).
/// When `[security] bearer_required = true` AND active tokens exist,
/// every request MUST present a valid bearer.
async fn handle_conn(
    mut stream: tokio::net::UnixStream,
    handle: DaemonHandle,
    registry: Arc<HandlerRegistry>,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let (read_half, mut write_half) = stream.split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    // Per-connection auth state: unix socket connections don't carry
    // HTTP headers, so we accept the bearer in a side-channel via
    // the JSON-RPC request itself. Two patterns supported:
    //
    //   1. Out-of-band header file: when the connection is established
    //      by a privileged tool (e.g. systemd), the operator drops a
    //      file at `<socket_path>.auth` containing the bearer string.
    //      We read this file ONCE per connection on first request.
    //   2. In-band: the very first line of the connection is treated
    //      as an auth header if it begins with "Authorization:" —
    //      legacy escalation path for tools that already speak
    //      HTTP-ish framing.
    //
    // Both paths converge into a `Option<String>` bearer that is
    // passed to `authenticate` on every request.
    let socket_auth = handle.config().socket_path();
    let auth_path = socket_auth.with_extension("auth");
    let bearer_from_file: Option<String> = if auth_path.exists() {
        std::fs::read_to_string(&auth_path)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    };
    // Per-IP backoff is daemon-wide; unix-socket clients all map to
    // loopback for backoff accounting purposes.
    let backoff: Arc<AuthBackoff> = Arc::new(AuthBackoff::new());
    let peer_ip: IpAddr = "127.0.0.1".parse().expect("loopback ip parses");
    let bearer_required = handle.config().security.bearer_required;
    let active_token_count = handle.tokens().list_active().len();

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

        // Extract bearer: explicit `params.bearer` field first, then
        // out-of-band file, then nothing. The `params.bearer` field is
        // stripped from params before dispatch so handlers don't see
        // an unknown param.
        let presented_bearer: Option<String> = if let Some(obj) = req.params.as_object() {
            obj.get("bearer")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        } else {
            None
        };
        let bearer = presented_bearer
            .or_else(|| bearer_from_file.clone())
            .or_else(|| extract_bearer_from_first_line(&line));

        // Auth check. Runs when:
        //   - bearer_required is true and tokens are loaded, OR
        //   - bearer_required is true and the method is mutating
        //     (security review F10: hermetic mode refuses mutating
        //     RPCs unconditionally even when no tokens are loaded).
        let hermetic_bypass = handle.config().security.hermetic_bypass;
        let must_auth = bearer_required
            & (active_token_count > 0 || crate::security::auth::is_mutating_method(&req.method));
        if must_auth {
            if let Err(e) = authenticate(
                &req.method,
                bearer.as_deref(),
                handle.tokens().as_ref(),
                &backoff,
                peer_ip,
                hermetic_bypass,
            ) {
                // Phase 5 Part B: surface bearer failures to
                // `auth_failed_total{ip=...}`. Unix-socket clients all
                // map to loopback from the daemon's POV. The label is
                // HMAC-hashed inside the metrics layer.
                handle.metrics().inc_auth_failed(&peer_ip.to_string());
                let resp = RpcResponse {
                    id: req.id,
                    result: None,
                    error: Some(e),
                };
                let mut s = serde_json::to_string(&resp)?;
                s.push('\n');
                write_half.write_all(s.as_bytes()).await?;
                continue;
            }
        }

        let method_for_metric = req.method.clone();
        let dispatch_start = std::time::Instant::now();
        let resp = registry.dispatch(handle.clone(), req).await;
        // Phase 5 Part B: per-method RPC latency histogram. The
        // label is HMAC-hashed inside `observe_rpc_latency`.
        let latency_secs = dispatch_start.elapsed().as_secs_f64();
        handle
            .metrics()
            .observe_rpc_latency(&method_for_metric, latency_secs);
        let mut s = serde_json::to_string(&resp)?;
        s.push('\n');
        write_half.write_all(s.as_bytes()).await?;
    }
}

/// Parse `Authorization: Bearer ...` from the raw request line if the
/// JSON parser left the header in the wire bytes. Currently unused —
/// present for symmetry with future HTTP-shaped transports.
fn extract_bearer_from_first_line(_line: &str) -> Option<String> {
    None
}

#[cfg(test)]
mod tests;
