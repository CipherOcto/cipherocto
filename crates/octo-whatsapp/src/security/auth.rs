//! Bearer-auth middleware + per-IP failure backoff.
//!
//! Phase 5 Part A §Task 5 + §Task 7. The middleware sits between the
//! IPC dispatch layer and the handler registry; on auth failure it
//! returns a JSON-RPC error with code `-32050` and `data.kind =
//! "unauthorized"` (operator hint: re-issue a token via
//! `security.rotate_token` on a privileged path).
//!
//! ## Hermetic-test bypass
//!
//! When `[security] bearer_token_env` is unset on the daemon config
//! (and the TokenStore is empty), the middleware accepts every
//! request unconditionally and logs at debug level. This matches the
//! pre-Phase 5 contract: hermetic tests in CI do not need to plumb
//! bearer tokens. Production deployments ALWAYS set the env var.
//!
//! ## Per-IP backoff
//!
//! Failed attempts per source peer IP are tracked in a
//! `HashMap<IpAddr, VecDeque<i64>>`. If a peer exceeds 1 failure/sec
//! sustained over a 60-second window, the middleware short-circuits
//! with `Unauthorized` WITHOUT invoking `TokenStore::verify` (to
//! avoid letting an attacker burn CPU on a constant-time comparison).
//!
//! On the unix socket path, the "peer IP" is `127.0.0.1` (or `::1`)
//! since unix sockets don't carry a real network peer — but the
//! middleware still records failures so operator tools that invoke
//! `security.*` after a misconfiguration can see the backoff in logs.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::Value;

use super::tokens::{TokenError, TokenStore};
use crate::ipc::protocol::{RpcError, RpcErrorCode, RpcRequest, RpcResponse};

/// Maximum failures per second per IP. Exceeding this triggers
/// fail-closed short-circuit.
pub const BACKOFF_CAP_PER_SEC: usize = 1;
/// Window for backoff measurement (seconds).
pub const BACKOFF_WINDOW_SECS: i64 = 60;

/// Per-IP failure tracker.
#[derive(Debug)]
pub struct AuthBackoff {
    by_ip: Mutex<HashMap<IpAddr, VecDeque<i64>>>,
    cap_per_sec: usize,
}

impl Default for AuthBackoff {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthBackoff {
    pub fn new() -> Self {
        Self {
            by_ip: Mutex::new(HashMap::new()),
            cap_per_sec: BACKOFF_CAP_PER_SEC,
        }
    }

    /// Record one failure for `ip`. Returns the new failure count
    /// within the rolling window.
    pub fn record_failure(&self, ip: IpAddr, now_secs: i64) -> usize {
        let mut g = self.by_ip.lock();
        let q = g.entry(ip).or_default();
        q.push_back(now_secs);
        let cutoff = now_secs - BACKOFF_WINDOW_SECS;
        while let Some(&front) = q.front() {
            if front < cutoff {
                q.pop_front();
            } else {
                break;
            }
        }
        q.len()
    }

    /// Returns true if the IP has exceeded `cap_per_sec` sustained
    /// failures over the rolling window.
    pub fn is_throttled(&self, ip: IpAddr) -> bool {
        let g = self.by_ip.lock();
        match g.get(&ip) {
            None => false,
            Some(q) => q.len() > self.cap_per_sec * (BACKOFF_WINDOW_SECS as usize),
        }
    }

    /// Snapshot the failure count for `ip` (for tests + metrics).
    pub fn failure_count(&self, ip: IpAddr) -> usize {
        let g = self.by_ip.lock();
        g.get(&ip).map(|q| q.len()).unwrap_or(0)
    }
}

/// Decide whether `bearer` (the raw `Authorization` header value,
/// `None` when missing) authenticates successfully against `tokens`.
/// Records failures on `backoff` keyed by `peer_ip`.
///
/// Returns `Ok(())` on success. On failure returns the `RpcError`
/// shape the middleware will surface to the client.
pub fn authenticate(
    bearer: Option<&str>,
    tokens: &TokenStore,
    backoff: &AuthBackoff,
    peer_ip: IpAddr,
) -> Result<(), RpcError> {
    // Backoff short-circuit: do not even attempt verification if the
    // caller is already over the rate cap.
    if backoff.is_throttled(peer_ip) {
        return Err(unauthorized_error("backoff"));
    }

    // Hermetic bypass: when the store is empty (no env var was set
    // during boot), accept unconditionally. This preserves the
    // pre-Phase 5 test contract.
    let active_count = {
        let active = tokens.list_active();
        active.len()
    };
    let presented = bearer
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|s| s.trim());
    if active_count == 0 {
        // Debug-only log; production deployments always have ≥ 1 token.
        tracing::debug!(
            peer = %peer_ip,
            "auth: bypass active (no tokens loaded — hermetic mode)"
        );
        return Ok(());
    }

    let bearer = match presented {
        Some(s) if !s.is_empty() => s,
        _ => {
            backoff.record_failure(peer_ip, unix_secs_now());
            return Err(unauthorized_error("missing bearer"));
        }
    };

    match tokens.verify(bearer) {
        Ok(_) => Ok(()),
        Err(TokenError::Revoked(id)) => {
            backoff.record_failure(peer_ip, unix_secs_now());
            Err(unauthorized_error(&format!("token revoked: {id}")))
        }
        Err(TokenError::Expired) => {
            backoff.record_failure(peer_ip, unix_secs_now());
            Err(unauthorized_error("token expired"))
        }
        Err(TokenError::UnknownToken(_)) => {
            backoff.record_failure(peer_ip, unix_secs_now());
            Err(unauthorized_error("unknown token"))
        }
        Err(TokenError::Invalid(msg)) => {
            backoff.record_failure(peer_ip, unix_secs_now());
            Err(unauthorized_error(&format!("invalid: {msg}")))
        }
        Err(TokenError::WeakToken { .. }) => {
            // Should be unreachable at runtime (validated on load).
            backoff.record_failure(peer_ip, unix_secs_now());
            Err(unauthorized_error("weak token"))
        }
        Err(TokenError::GraceInvalid(msg)) => {
            backoff.record_failure(peer_ip, unix_secs_now());
            Err(unauthorized_error(&format!("grace invalid: {msg}")))
        }
        Err(TokenError::Storage(msg)) => {
            // Storage failures are not "wrong credential" — surface as
            // internal error so operators investigate, but still record
            // the failure to avoid log-spam loops.
            backoff.record_failure(peer_ip, unix_secs_now());
            Err(RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("auth storage: {msg}"),
                data: None,
            })
        }
    }
}

/// Build a `-32050` (Internal) JSON-RPC error tagged as `unauthorized`.
/// We use `-32050` per the plan's spec rather than defining a new
/// `Unauthorized` variant — `data.kind` carries the discriminator.
pub fn unauthorized_error(reason: &str) -> RpcError {
    let data: Value = serde_json::json!({
        "kind": "unauthorized",
        "reason": reason,
    });
    RpcError {
        code: RpcErrorCode::Internal.as_i32(),
        message: format!("unauthorized: {reason}"),
        data: Some(data),
    }
}

fn unix_secs_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Wrap an `Arc<AuthBackoff>` for use as a sub-resource on
/// `DaemonInner`. Carries no public state beyond the inner Arc.
#[derive(Debug, Clone, Default)]
pub struct AuthBackoffHandle {
    pub backoff: Arc<AuthBackoff>,
}

impl AuthBackoffHandle {
    pub fn new() -> Self {
        Self {
            backoff: Arc::new(AuthBackoff::new()),
        }
    }
}

/// Helper: build an `RpcResponse` carrying an unauthorized error.
pub fn unauthorized_response(req_id: u64, reason: &str) -> RpcResponse {
    RpcResponse {
        id: req_id,
        result: None,
        error: Some(unauthorized_error(reason)),
    }
}

/// Re-export for tests.
pub fn _req_id_for_test(req: &RpcRequest) -> u64 {
    req.id
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip() -> IpAddr {
        "127.0.0.1".parse().unwrap()
    }

    #[test]
    fn backoff_is_empty_for_new_ip() {
        let b = AuthBackoff::new();
        assert_eq!(b.failure_count(ip()), 0);
        assert!(!b.is_throttled(ip()));
    }

    #[test]
    fn backoff_records_failures() {
        let b = AuthBackoff::new();
        b.record_failure(ip(), 1000);
        b.record_failure(ip(), 1001);
        assert_eq!(b.failure_count(ip()), 2);
    }

    #[test]
    fn backoff_throttles_when_over_cap() {
        let b = AuthBackoff::new();
        // The cap is `cap_per_sec * BACKOFF_WINDOW_SECS` = 1 * 60 = 60.
        for s in 0..200 {
            b.record_failure(ip(), s);
        }
        assert!(b.is_throttled(ip()));
    }

    #[test]
    fn backoff_old_failures_expire() {
        let b = AuthBackoff::new();
        b.record_failure(ip(), 1_000);
        b.record_failure(ip(), 1_000 + BACKOFF_WINDOW_SECS + 1);
        // First failure is now outside the window.
        assert_eq!(b.failure_count(ip()), 1);
    }

    #[test]
    fn hermetic_bypass_accepts_when_no_tokens_loaded() {
        let s = TokenStore::new(None, 60_000);
        let b = AuthBackoff::new();
        // No bearer presented, no active tokens → OK.
        assert!(authenticate(None, &s, &b, ip()).is_ok());
        // Garbage bearer, still no tokens → OK.
        assert!(authenticate(Some("Bearer garbage"), &s, &b, ip()).is_ok());
    }

    #[test]
    fn rejects_missing_bearer_when_tokens_loaded() {
        let s = TokenStore::new(None, 60_000);
        let b = AuthBackoff::new();
        let secret = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let id = crate::security::tokens::derive_token_id(secret);
        s.load_from_value(&format!("{id}.{secret}"), None).unwrap();
        let err = authenticate(None, &s, &b, ip()).unwrap_err();
        assert_eq!(err.code, RpcErrorCode::Internal.as_i32());
        assert!(err.data.as_ref().unwrap()["kind"] == "unauthorized");
    }

    #[test]
    fn rejects_wrong_bearer_when_tokens_loaded() {
        let s = TokenStore::new(None, 60_000);
        let b = AuthBackoff::new();
        let secret = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let id = crate::security::tokens::derive_token_id(secret);
        s.load_from_value(&format!("{id}.{secret}"), None).unwrap();
        let err = authenticate(
            Some(&format!(
                "Bearer {id}.0000000000000000000000000000000000000000000000000000000000000000"
            )),
            &s,
            &b,
            ip(),
        )
        .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::Internal.as_i32());
        assert!(err.data.as_ref().unwrap()["kind"] == "unauthorized");
    }

    #[test]
    fn accepts_valid_bearer_when_tokens_loaded() {
        let s = TokenStore::new(None, 60_000);
        let b = AuthBackoff::new();
        let secret = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let id = crate::security::tokens::derive_token_id(secret);
        s.load_from_value(&format!("{id}.{secret}"), None).unwrap();
        let bearer = format!("Bearer {id}.{secret}");
        authenticate(Some(&bearer), &s, &b, ip()).unwrap();
    }

    #[test]
    fn rejects_non_bearer_scheme() {
        let s = TokenStore::new(None, 60_000);
        let b = AuthBackoff::new();
        let secret = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let id = crate::security::tokens::derive_token_id(secret);
        s.load_from_value(&format!("{id}.{secret}"), None).unwrap();
        let err = authenticate(Some(&format!("Basic {id}:{secret}")), &s, &b, ip()).unwrap_err();
        assert_eq!(err.code, RpcErrorCode::Internal.as_i32());
    }
}
