//! Layer B `GovernanceSession` substrate (RFC-0011-g §7.4
//! stateless caller-owned state container).
//!
//! ## Why a session object?
//!
//! RFC-0011-g §7.4 declares the substrate functions (`snapshot`,
//! `attest`, `vote`) as stateless signatures — the function
//! arguments carry only the per-call inputs, no log / registry /
//! clock references. The state those functions consult (the
//! append-only ledgers, the capability registry, the operator's
//! active DID, the substrate clock) lives in a **caller-owned**
//! `GovernanceSession` object that the substrate reads by
//! reference. This preserves the RFC §7.4 contract while moving
//! the substrate from "global statics" to "dependency
//! injection" — the same pattern the CLI already uses to thread
//! the `WalletSignerAdapter` through the capability signer
//! surface (RFC-0015-a §6.5 paired-acceptance bridge).
//!
//! ## Layer discipline
//!
//! This module is **Layer B (RFC-driven additive only)**. It
//! owns no IO, no randomness, no storage; the `Clock` trait is
//! the only abstraction over time. The substrate uses
//! `Arc<dyn Clock>` so the CLI can swap in a `SystemClock` in
//! production and a `FixedClock` in tests / audit replays
//! without touching the substrate callers.
//!
//! ## Caller-owned
//!
//! The `GovernanceSession` is constructed once per CLI invocation
//! and held by the CLI dispatch layer; the substrate functions
//! borrow `&GovernanceSession` and never own one. This keeps the
//! "stateless substrate" promise intact: every substrate call is
//! pure w.r.t. its arguments + the session the caller provided.

use std::sync::{Arc, Mutex};

use octo_governance_core::GovernanceError;

use crate::attest::AttestationLog;
use crate::vote::{CapabilityRegistry, VoteLog};

/// Trait abstraction over the substrate clock. The substrate
/// uses this to read `now_unix` for envelope canonicalization
/// and receipt timestamps. `Send + Sync` so the trait object
/// lives behind `Arc<dyn Clock>` and is shareable across the
/// CLI's worker threads.
///
/// Per RFC-0011-g §Clock, the substrate MUST NOT call
/// `SystemTime::now()` directly; the clock is injected via the
/// `GovernanceSession`. This keeps the substrate deterministic
/// in tests + auditable in replay scenarios.
pub trait Clock: Send + Sync {
    /// Current unix epoch seconds. The substrate assumes the
    /// clock returns a non-decreasing sequence — clock
    /// regressions are surfaced by the operator at the OS layer,
    /// not in the substrate.
    fn now_unix(&self) -> u64;
}

/// Production clock backed by `std::time::SystemTime`. The CLI
/// constructs one of these per session in production paths and
/// the substrate consults it on every receipt mint.
///
/// Returns `0` if the system clock is before unix epoch (which
/// "cannot happen" on a sane host, but the substrate fails-soft
/// rather than panicking — the audit ledger records the anomaly).
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
}

/// Deterministic clock for tests + audit replays. Holds a
/// `Mutex<u64>` so multiple substrate calls in the same test
/// can advance the clock forward (or hold it fixed) by calling
/// [`FixedClock::set`].
#[derive(Debug)]
pub struct FixedClock {
    now: Mutex<u64>,
}

impl FixedClock {
    /// Construct a `FixedClock` that initially returns `start_unix`.
    #[must_use]
    pub fn new(start_unix: u64) -> Self {
        Self {
            now: Mutex::new(start_unix),
        }
    }

    /// Advance the clock to `now_unix`. Panics if the mutex is
    /// poisoned (substrate invariant: never poison the clock).
    pub fn set(&self, now_unix: u64) {
        *self.now.lock().expect("fixed clock mutex poisoned") = now_unix;
    }

    /// Read the current clock value without advancing.
    #[must_use]
    pub fn current(&self) -> u64 {
        *self.now.lock().expect("fixed clock mutex poisoned")
    }
}

impl Clock for FixedClock {
    fn now_unix(&self) -> u64 {
        self.current()
    }
}

/// Caller-owned governance substrate state container. Holds
/// the append-only ledgers, the capability registry, the
/// substrate clock, and the operator's active DID.
///
/// ## Construction
///
/// ```ignore
/// let session = GovernanceSession::new(
///     "did:octo:z6MkAlice...",
///     Arc::new(SystemClock),
/// );
/// session.register_capability("cap-001", Arc::new(my_signer));
/// ```
///
/// The CLI constructs one session per invocation and threads
/// `&session` through every substrate call. The substrate
/// functions (`vote_v2`, `attest_v2`, future `snapshot_v2`)
/// accept `&GovernanceSession` as the implicit-state carrier.
pub struct GovernanceSession {
    /// Append-only vote ledger. `vote_v2` consults + extends.
    vote_log: VoteLog,
    /// Append-only attestation ledger. `attest_v2` consults + extends.
    attestation_log: AttestationLog,
    /// Capability registry mapping `voter_cap_id → Arc<dyn
    /// CapabilitySigner>`. The CLI populates this at session
    /// construction from the wallet's capability set.
    capability_registry: CapabilityRegistry,
    /// Substrate clock (deterministic for tests via `FixedClock`,
    /// system clock in production via `SystemClock`).
    clock: Arc<dyn Clock>,
    /// Operator's active local DID. Surfaced to substrate
    /// functions for prereq gates + audit ledger stamps.
    active_did: String,
}

impl GovernanceSession {
    /// Construct a fresh session. `active_did` is the operator's
    /// local DID; `clock` is the substrate clock abstraction.
    #[must_use]
    pub fn new(active_did: impl Into<String>, clock: Arc<dyn Clock>) -> Self {
        Self {
            vote_log: VoteLog::new(),
            attestation_log: AttestationLog::new(),
            capability_registry: CapabilityRegistry::new(),
            clock,
            active_did: active_did.into(),
        }
    }

    /// Register a capability under the given `voter_cap_id`.
    /// Last-writer-wins on duplicate registrations. Mirrors the
    /// `CapabilityRegistry::register` surface so the CLI can
    /// populate the registry at session construction time.
    /// Fail-closed on mutex poisoning via `GovernanceError::Internal`.
    pub fn register_capability(
        &self,
        voter_cap_id: impl Into<String>,
        signer: Arc<dyn crate::attest::CapabilitySigner>,
    ) -> Result<(), GovernanceError> {
        self.capability_registry
            .register(voter_cap_id.into(), signer)
    }

    /// Borrow the append-only vote ledger (read-only inspection).
    #[must_use]
    pub fn vote_log(&self) -> &VoteLog {
        &self.vote_log
    }

    /// Borrow the append-only attestation ledger (read-only inspection).
    #[must_use]
    pub fn attestation_log(&self) -> &AttestationLog {
        &self.attestation_log
    }

    /// Borrow the capability registry (read-only inspection).
    #[must_use]
    pub fn capability_registry(&self) -> &CapabilityRegistry {
        &self.capability_registry
    }

    /// Operator's active local DID (read-only).
    #[must_use]
    pub fn active_did(&self) -> &str {
        &self.active_did
    }

    /// Read the substrate clock. Convenience accessor so
    /// callers can stamp receipts / audit logs without holding
    /// the `Arc<dyn Clock>` separately.
    #[must_use]
    pub fn now_unix(&self) -> u64 {
        self.clock.now_unix()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_starts_at_seed_value() {
        let clk = FixedClock::new(1_700_000_000);
        assert_eq!(clk.now_unix(), 1_700_000_000);
        assert_eq!(clk.current(), 1_700_000_000);
    }

    #[test]
    fn fixed_clock_set_advances_value() {
        let clk = FixedClock::new(1_700_000_000);
        clk.set(1_700_000_500);
        assert_eq!(clk.now_unix(), 1_700_000_500);
        assert_eq!(clk.current(), 1_700_000_500);
    }

    #[test]
    fn system_clock_returns_nonzero() {
        // Smoke test: SystemClock must return a sane unix
        // timestamp (well past the year 2000 = 946_684_800).
        let now = SystemClock.now_unix();
        assert!(
            now > 946_684_800,
            "SystemClock returned pre-2000 timestamp: {now}"
        );
    }

    #[test]
    fn governance_session_new_carries_state() {
        let session = GovernanceSession::new("did:octo:test", Arc::new(SystemClock));
        assert_eq!(session.active_did(), "did:octo:test");
        assert_eq!(session.vote_log().proposal_count().expect("unpoisoned"), 0);
        assert_eq!(session.attestation_log().len(), 0);
        assert_eq!(session.capability_registry().len(), 0);
        assert!(session.now_unix() > 0);
    }

    #[test]
    fn governance_session_register_capability_populates_registry() {
        use crate::attest::CapabilitySigner;

        struct DummySigner;
        impl CapabilitySigner for DummySigner {
            fn sign_envelope(&self, _: &[u8]) -> Result<[u8; 64], String> {
                Ok([0u8; 64])
            }
        }

        let session = GovernanceSession::new("did:octo:test", Arc::new(FixedClock::new(0)));
        assert_eq!(session.capability_registry().len(), 0);
        session
            .register_capability("cap-001", Arc::new(DummySigner))
            .expect("register unpoisoned");
        assert_eq!(session.capability_registry().len(), 1);
    }

    #[test]
    fn governance_session_with_fixed_clock_is_deterministic() {
        let session =
            GovernanceSession::new("did:octo:test", Arc::new(FixedClock::new(1_700_000_000)));
        assert_eq!(session.now_unix(), 1_700_000_000);
    }
}
