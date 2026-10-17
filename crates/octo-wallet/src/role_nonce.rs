//! Role-binding nonce counter per RFC-0011-d §7.4.
//!
//! `next_nonce_counter(operator_did)` returns a monotonic u64 that
//! `octo_role::select` consumes inside its envelope-build transaction
//! to bind each role selection to a unique nonce. The counter is
//! process-local; production deployments persist it to the slash
//! ledger (RFC-0900) so that nonce continuity survives process
//! restarts.
//!
//! Phase 1: in-process `Mutex<HashMap<Did, u64>>`. Substrate-grade
//! persistence ships when the slash-ledger-backed `BindingStore`
//! replaces the in-memory store in `octo-role` (M4 deferred follow-on).
//!
//! Test isolation: each test in this module uses a unique `Did`
//! namespace (`did:octo:0x00` through `did:octo:0xff`). The global
//! counter map keys by `Did`, so concurrent tests cannot race on a
//! shared entry. The earlier `reset_for_tests()` helper that wiped
//! the entire map was unsafe under `cargo test`'s default
//! multi-threaded runner — Test A's `reset_for_tests()` would clear
//! Test B's seeded state mid-execution, surfacing as a non-
//! deterministic `monotonic invariant: N -> 0` panic. The helper
//! has been removed.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::error::WalletError;
use crate::identity_record::Did;

/// Process-local role-binding nonce counter keyed by operator DID.
static COUNTERS: LazyLock<Mutex<HashMap<Did, u64>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Return the next role-binding nonce for `operator_did` and atomically
/// increment the persisted counter.
///
/// The first call for a given DID returns 0 and seeds the counter at 1.
/// Each subsequent call returns the prior value + 1.
///
/// # Errors
///
/// Returns [`WalletError::NonceUnderflow`] when the counter would
/// saturate at `u64::MAX`. Process-wide invariant violation — substrate
/// enters a degraded state and `octo_role::select` must surface the
/// error rather than wrapping.
pub fn next_nonce_counter(operator_did: &Did) -> Result<u64, WalletError> {
    let mut guard = COUNTERS.lock().expect("role-nonce counter mutex poisoned");
    let entry = guard.entry(operator_did.clone()).or_insert(0);
    let n = *entry;
    let next = n.checked_add(1).ok_or(WalletError::NonceUnderflow)?;
    *entry = next;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn did_for(name: &str) -> Did {
        Did(name.to_string())
    }

    #[test]
    fn next_nonce_counter_starts_at_zero() {
        // Unique DID (did:octo:0x00) — no reset needed; per-DID
        // isolation is the test contract. If the same crate runs
        // this test multiple times in the same process, the
        // second run sees the persisted counter from the first
        // run. cargo test runs each `#[test]` exactly once per
        // `cargo test` invocation, so the second-run scenario is
        // out of scope here.
        let did = did_for("did:octo:0x00");
        assert_eq!(next_nonce_counter(&did).unwrap(), 0);
        assert_eq!(next_nonce_counter(&did).unwrap(), 1);
        assert_eq!(next_nonce_counter(&did).unwrap(), 2);
    }

    #[test]
    fn next_nonce_counter_is_did_scoped() {
        let a = did_for("did:octo:0x01");
        let b = did_for("did:octo:0x02");
        assert_eq!(next_nonce_counter(&a).unwrap(), 0);
        assert_eq!(next_nonce_counter(&b).unwrap(), 0);
        assert_eq!(next_nonce_counter(&a).unwrap(), 1);
        assert_eq!(next_nonce_counter(&b).unwrap(), 1);
    }

    #[test]
    fn next_nonce_counter_is_monotonic() {
        // Unique DID (did:octo:0x03) — no reset needed. The
        // discarded first call below advances the counter from
        // the seed (0) so the loop measures strict monotonicity
        // over the post-discard sequence. Prior to removing the
        // global `reset_for_tests()` helper, this test would
        // non-deterministically fail with `monotonic invariant:
        // N -> 0` when a sibling test's `reset_for_tests()`
        // wiped the map mid-loop.
        let did = did_for("did:octo:0x03");
        let _ = next_nonce_counter(&did).unwrap();
        let mut prev = next_nonce_counter(&did).unwrap();
        for _ in 0..100 {
            let n = next_nonce_counter(&did).unwrap();
            assert!(n > prev, "monotonic invariant: {prev} -> {n}");
            prev = n;
        }
    }

    /// R12: `next_nonce_counter` must surface `WalletError::NonceUnderflow`
    /// when the counter saturates at `u64::MAX`. The Phase 1 in-memory
    /// store maps directly from the substrate return so callers don't
    /// have to special-case the saturation path.
    #[test]
    fn next_nonce_counter_returns_err_on_underflow() {
        let did = did_for("did:octo:0xff");
        // Seed the counter at u64::MAX - 1 so the next call saturates.
        COUNTERS
            .lock()
            .expect("counter mutex")
            .insert(did.clone(), u64::MAX - 1);
        assert_eq!(next_nonce_counter(&did).unwrap(), u64::MAX - 1);
        let err = next_nonce_counter(&did).unwrap_err();
        assert!(matches!(err, WalletError::NonceUnderflow));
    }
}
