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

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::identity_record::Did;

/// Process-local role-binding nonce counter keyed by operator DID.
static COUNTERS: LazyLock<Mutex<HashMap<Did, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Return the next role-binding nonce for `operator_did` and atomically
/// increment the persisted counter.
///
/// The first call for a given DID returns 0 and seeds the counter at 1.
/// Each subsequent call returns the prior value + 1.
///
/// # Panics
///
/// Panics if the internal mutex is poisoned (process-wide invariant
/// violation; substrate enters a degraded state).
pub fn next_nonce_counter(operator_did: &Did) -> u64 {
    let mut guard = COUNTERS.lock().expect("role-nonce counter mutex poisoned");
    let entry = guard.entry(operator_did.clone()).or_insert(0);
    let n = *entry;
    *entry = n.saturating_add(1);
    n
}

/// Test-only: reset the global counter map. NOT exposed outside `cfg(test)`.
#[cfg(test)]
pub fn _reset_for_tests() {
    COUNTERS.lock().expect("role-nonce counter mutex poisoned").clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn did_for(name: &str) -> Did {
        Did(name.to_string())
    }

    #[test]
    fn next_nonce_counter_starts_at_zero() {
        _reset_for_tests();
        let did = did_for("did:octo:0x00");
        assert_eq!(next_nonce_counter(&did), 0);
        assert_eq!(next_nonce_counter(&did), 1);
        assert_eq!(next_nonce_counter(&did), 2);
    }

    #[test]
    fn next_nonce_counter_is_did_scoped() {
        _reset_for_tests();
        let a = did_for("did:octo:0x01");
        let b = did_for("did:octo:0x02");
        assert_eq!(next_nonce_counter(&a), 0);
        assert_eq!(next_nonce_counter(&b), 0);
        assert_eq!(next_nonce_counter(&a), 1);
        assert_eq!(next_nonce_counter(&b), 1);
    }

    #[test]
    fn next_nonce_counter_is_monotonic() {
        _reset_for_tests();
        let did = did_for("did:octo:0x03");
        let mut prev = next_nonce_counter(&did);
        for _ in 0..100 {
            let n = next_nonce_counter(&did);
            assert!(n > prev, "monotonic invariant: {prev} -> {n}");
            prev = n;
        }
    }
}
