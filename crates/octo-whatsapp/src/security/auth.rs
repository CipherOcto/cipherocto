//! Bearer-auth middleware + per-IP failure backoff.
//!
//! Phase 5 Part A §Task 5 + §Task 7. The middleware sits between the
//! IPC dispatch layer and the handler registry; on auth failure it
//! returns a JSON-RPC error with code `-32050` and `data.kind =
//! "unauthorized"` (operator hint: re-issue a token via
//! `security.rotate_token` on a privileged path).
//!
//! ## Hermetic-test bypass (security review F1, F8, F10)
//!
//! When `[security] bearer_token_env` is unset on the daemon config
//! (and the TokenStore is empty), the middleware behaviour depends on
//! `[security] hermetic_bypass`:
//!
//! - `true` (default for tests, OFF in production) — accept every
//!   request unconditionally and log at debug level.
//! - `false` (production default) — refuse ALL mutation RPCs (rules.*,
//!   triggers.*, audit.*, actions.*, security.*) with `-32050
//!   unauthorized`. Pure read-only RPCs (`health.get`, `daemon.*`)
//!   continue to work so operators can still observe state.
//!
//! The config layer REJECTS a daemon start where
//! `bearer_token_env = unset` AND `hermetic_bypass = false` AND
//! `[security] bearer_required = true` — there is no path where a
//! production daemon silently accepts unauthenticated mutations.
//!
//! ## Per-IP backoff (security review F13)
//!
//! Failed attempts per source peer IP are tracked in a
//! `HashMap<IpAddr, VecDeque<i64>>` capped at `MAX_BACKOFF_ENTRIES`
//! (10k IPs). The LRU eviction drops the oldest IP entry when the
//! map is full. If a peer exceeds 1 failure/sec sustained over a
//! 60-second window, the middleware short-circuits with
//! `Unauthorized` WITHOUT invoking `TokenStore::verify` (to avoid
//! letting an attacker burn CPU on a constant-time comparison).
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
/// Hard cap on tracked IPs to bound memory under attack (F13).
pub const MAX_BACKOFF_ENTRIES: usize = 10_000;

/// Per-IP failure tracker. Bounded by `MAX_BACKOFF_ENTRIES`; oldest
/// IP is evicted when the cap is reached.
#[derive(Debug)]
pub struct AuthBackoff {
    by_ip: Mutex<HashMap<IpAddr, VecDeque<i64>>>,
    cap_per_sec: usize,
    /// Insertion order for LRU eviction (security review F13).
    insertion_order: Mutex<VecDeque<IpAddr>>,
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
            insertion_order: Mutex::new(VecDeque::new()),
        }
    }

    /// Record one failure for `ip`. Returns the new failure count
    /// within the rolling window. Evicts the oldest tracked IP if
    /// the entry cap is reached.
    pub fn record_failure(&self, ip: IpAddr, now_secs: i64) -> usize {
        // Compute the eviction list under one lock acquisition to
        // avoid the nested-mut-borrow issue from holding both
        // `by_ip` and `insertion_order` at once. The LRU list is
        // authoritative; the `by_ip` map is rebuilt to match.
        let (new_q_len, eviction_needed) = {
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
            // Touch the now-updated queue length before potentially
            // dropping the borrow below.
            let new_q_len = q.len();
            (new_q_len, false)
        };
        // Update insertion order under its own lock; if the cap is
        // exceeded, drop the oldest entry from BOTH `by_ip` and
        // `insertion_order`.
        let mut ord = self.insertion_order.lock();
        ord.retain(|x| *x != ip);
        ord.push_back(ip);
        let mut evict: Option<IpAddr> = None;
        if ord.len() > MAX_BACKOFF_ENTRIES {
            evict = ord.pop_front();
        }
        drop(ord);
        if let Some(oldest) = evict {
            let mut g = self.by_ip.lock();
            g.remove(&oldest);
        }
        // Suppress the unused-binding warning while keeping the
        // borrow pattern self-documenting.
        let _ = eviction_needed;
        new_q_len
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

    /// Total tracked IPs (for tests + metrics).
    pub fn tracked_ip_count(&self) -> usize {
        self.insertion_order.lock().len()
    }
}

/// Methods that mutate daemon state and require a valid bearer
/// even when in hermetic mode (security review F10). The IPC layer
/// consults `is_mutating_method()` before deciding whether to allow
/// the request under hermetic bypass.
pub fn is_mutating_method(method: &str) -> bool {
    matches!(
        method,
        // security.* — even security.rotate_token requires a token
        // issued by an out-of-band mechanism.
        m if m.starts_with("security.")
            // rules.*
            || m.starts_with("rules.")
            // triggers.*
            || m.starts_with("triggers.")
            // audit.*
            || m.starts_with("audit.")
            // actions.*
            || m.starts_with("actions.")
            // outbound RPCs that hit the adapter
            || m.starts_with("send.")
            || m.starts_with("messages.edit")
            || m.starts_with("messages.mark_read")
            || m.starts_with("send.delete")
            // chat mutations
            || m.starts_with("chats.pin")
            || m.starts_with("chats.unpin")
            || m.starts_with("chats.mute")
            || m.starts_with("chats.archive")
            || m.starts_with("chats.delete")
            // envelope.send
            || m == "envelope.send"
            || m == "envelope.send-native"
    )
}

/// Decide whether `bearer` (the raw `Authorization` header value,
/// `None` when missing) authenticates successfully against `tokens`.
/// Records failures on `backoff` keyed by `peer_ip`.
///
/// `method` is the RPC method name (used for hermetic-mode
/// mutating-method gating — security review F10).
/// `hermetic_bypass` is the operator-controlled flag (default `false`
/// in production builds).
///
/// Returns `Ok(())` on success. On failure returns the `RpcError`
/// shape the middleware will surface to the client.
pub fn authenticate(
    method: &str,
    bearer: Option<&str>,
    tokens: &TokenStore,
    backoff: &AuthBackoff,
    peer_ip: IpAddr,
    hermetic_bypass: bool,
) -> Result<(), RpcError> {
    // Backoff short-circuit: do not even attempt verification if the
    // caller is already over the rate cap.
    if backoff.is_throttled(peer_ip) {
        return Err(unauthorized_error("backoff"));
    }

    let active_count = {
        let active = tokens.list_active();
        active.len()
    };
    let presented = bearer
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|s| s.trim());

    // Hermetic bypass branch (security review F1, F8, F10).
    if active_count == 0 {
        if !hermetic_bypass {
            // Refuse mutating RPCs unconditionally; allow pure reads.
            if is_mutating_method(method) {
                return Err(unauthorized_error(
                    "hermetic mode: mutating RPCs require a bearer token",
                ));
            }
            // Pure read-only: allow and log a warning so operators
            // see "auth: hermetic, read-only" in the journal.
            tracing::warn!(
                peer = %peer_ip,
                method = %method,
                "auth: hermetic mode active (no tokens loaded); read-only RPCs permitted"
            );
            return Ok(());
        }
        // Hermetic bypass enabled (tests + explicit operator opt-in):
        // accept unconditionally.
        tracing::debug!(
            peer = %peer_ip,
            "auth: bypass active (no tokens loaded — hermetic_bypass = true)"
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
            // Storage failures are NOT "wrong credential" — do NOT
            // record into the backoff (security review F14). Surface
            // as Internal so operators investigate disk / IO.
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
        // hermetic_bypass = true (test default): mutating methods allowed.
        assert!(authenticate("rules.create", None, &s, &b, ip(), true).is_ok());
        assert!(authenticate("rules.create", Some("Bearer garbage"), &s, &b, ip(), true).is_ok());
    }

    #[test]
    fn hermetic_mode_refuses_mutating_when_no_tokens() {
        // Security review F10: hermetic_bypass = false (production
        // default) MUST refuse mutating RPCs even with no tokens
        // loaded.
        let s = TokenStore::new(None, 60_000);
        let b = AuthBackoff::new();
        let err = authenticate("rules.create", None, &s, &b, ip(), false).unwrap_err();
        assert_eq!(err.code, RpcErrorCode::Internal.as_i32());
        assert!(err.data.as_ref().unwrap()["kind"] == "unauthorized");
    }

    #[test]
    fn hermetic_mode_allows_read_only_when_no_tokens() {
        let s = TokenStore::new(None, 60_000);
        let b = AuthBackoff::new();
        // Pure read RPCs still work in hermetic mode.
        assert!(authenticate("health.get", None, &s, &b, ip(), false).is_ok());
        assert!(authenticate("daemon.status", None, &s, &b, ip(), false).is_ok());
    }

    #[test]
    fn rejects_missing_bearer_when_tokens_loaded() {
        let s = TokenStore::new(None, 60_000);
        let b = AuthBackoff::new();
        let secret = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let id = crate::security::tokens::derive_token_id(secret);
        s.load_from_value(&format!("{id}.{secret}"), None).unwrap();
        let err = authenticate("rules.list", None, &s, &b, ip(), false).unwrap_err();
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
            "rules.list",
            Some(&format!(
                "Bearer {id}.0000000000000000000000000000000000000000000000000000000000000000"
            )),
            &s,
            &b,
            ip(),
            false,
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
        authenticate("rules.list", Some(&bearer), &s, &b, ip(), false).unwrap();
    }

    #[test]
    fn rejects_non_bearer_scheme() {
        let s = TokenStore::new(None, 60_000);
        let b = AuthBackoff::new();
        let secret = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let id = crate::security::tokens::derive_token_id(secret);
        s.load_from_value(&format!("{id}.{secret}"), None).unwrap();
        let err = authenticate(
            "rules.list",
            Some(&format!("Basic {id}:{secret}")),
            &s,
            &b,
            ip(),
            false,
        )
        .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::Internal.as_i32());
    }

    #[test]
    fn backoff_caps_tracked_ips_at_max_entries() {
        // Security review F13: bounded LRU on the per-IP map.
        let b = AuthBackoff::new();
        // Insert MAX_BACKOFF_ENTRIES + 100 distinct IPs.
        let extra = MAX_BACKOFF_ENTRIES + 100;
        for i in 0..extra {
            let ip = std::net::IpAddr::from([10, 0, (i >> 8) as u8, (i & 0xff) as u8]);
            b.record_failure(ip, 1_000);
        }
        // Map must be capped.
        assert!(b.tracked_ip_count() <= MAX_BACKOFF_ENTRIES);
    }
}
