//! `SpendLedger` trait + `InMemorySpendLedger` impl (RFC-0862 atomic
//! transaction substrate for paid-query drain).
//!
//! ## Layer E (per-extension)
//!
//! This module sits at Layer E per [[cipherocto-design-principles]].
//! The trait defines the **drain primitive** that downstream consumers
//! (the wallet-node `WALLET_PAID_QUERY_VERIFY` handler) call after a
//! `Proceed` decision. The default backend is in-memory; production
//! deployments inject a Stoolap-backed ledger via `Arc<dyn SpendLedger>`.
//!
//! ## Why a separate trait (not extending `HolderRegistry`)?
//!
//! `HolderRegistry` (Layer A substrate) owns `HolderRecord`-shaped
//! capability metadata per RFC-0957-A1 §Algorithms. Per
//! [[cipherocto-design-principles]] ("no parallel abstractions" +
//! "single responsibility"), the spend ledger is a separate concern
//! from capability storage — different storage layout, different
//! consistency model (drain is atomic-per-(holder, macaroon); lookup
//! is content-addressable). Splitting the two lets each substrate
//! evolve at its own cadence.
//!
//! ## Atomicity guarantee
//!
//! `try_deduct` is atomic with respect to the (holder_did,
//! macaroon_id) key: either the full `cost` is deducted AND the
//! remaining balance returned, or the balance is unchanged AND a
//! `SpendLedgerError` is returned. Concurrent calls serialize on the
//! per-key lock so a concurrent drain cannot double-spend.
//!
//! ## Adversary closures
//!
//! - **A7 (arithmetic overflow):** cost and balance use `u128` —
//!   overflow impossible at worst-case scale.
//! - **A10 (post-expiry queries):** wallet handler refuses to drain
//!   after `caveat.expires_at_unix_ms`; this trait only enforces
//!   the balance gate (expiry is upstream in `verify_paid_query`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use octo_cap_macaroon::MacaroonId;
use octo_ident::WireDid;

use crate::{MicroOCTO_W, PaidQueryError};

/// Spend-ledger backend trait.
///
/// Implementations MUST guarantee atomicity per
/// `(holder_did, macaroon_id)` key (see module docs).
pub trait SpendLedger: Send + Sync + std::fmt::Debug {
    /// Seed the balance for a `(holder_did, macaroon_id)` pair. Called
    /// by the wallet at mint time when a `PaymentCaveat` is attached
    /// to a capability.
    ///
    /// # Errors
    /// Returns `SpendLedgerError::Storage` on underlying storage
    /// failure.
    fn seed(
        &self,
        holder_did: &WireDid,
        macaroon_id: &MacaroonId,
        budget: MicroOCTO_W,
    ) -> Result<(), SpendLedgerError>;

    /// Atomically deduct `cost` from the `(holder_did, macaroon_id)`
    /// balance. Returns the new remaining balance on success.
    ///
    /// # Errors
    /// - `SpendLedgerError::UnknownHolder` if no balance record exists
    ///   for the `(holder_did, macaroon_id)` pair.
    /// - `SpendLedgerError::InsufficientBalance` if balance < cost.
    /// - `SpendLedgerError::Storage` on underlying storage failure.
    fn try_deduct(
        &self,
        holder_did: &WireDid,
        macaroon_id: &MacaroonId,
        cost: MicroOCTO_W,
    ) -> Result<MicroOCTO_W, SpendLedgerError>;

    /// Read-only balance lookup. Returns `None` if no record exists.
    ///
    /// # Errors
    /// Returns `SpendLedgerError::Storage` on underlying storage
    /// failure.
    fn balance(
        &self,
        holder_did: &WireDid,
        macaroon_id: &MacaroonId,
    ) -> Result<Option<MicroOCTO_W>, SpendLedgerError>;
}

/// Spend-ledger errors (trait-impl-shared; `PaidQueryError` re-exports
/// the per-call-site variants).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SpendLedgerError {
    /// No balance record exists for the `(holder_did, macaroon_id)`
    /// pair. The wallet must `seed` before draining.
    #[error("no balance record for holder/macaroon; wallet must seed before deduct")]
    UnknownHolder,
    /// Balance < proposed cost. Carries both numbers for caller
    /// diagnostics.
    #[error("insufficient balance: balance={balance}, cost={cost}")]
    InsufficientBalance {
        /// Current balance (MicroOCTO_W).
        balance: MicroOCTO_W,
        /// Proposed cost (MicroOCTO_W).
        cost: MicroOCTO_W,
    },
    /// Underlying storage failure (e.g. Stoolap transaction error).
    #[error("spend-ledger storage error: {0}")]
    Storage(String),
}

impl From<SpendLedgerError> for PaidQueryError {
    fn from(e: SpendLedgerError) -> Self {
        match e {
            SpendLedgerError::UnknownHolder => PaidQueryError::UnknownHolder,
            SpendLedgerError::InsufficientBalance { balance, cost } => {
                PaidQueryError::InsufficientBalance { balance, cost }
            }
            // Surface storage errors as `UnknownHolder` so the
            // handler's `Proceed` arm fails closed (no drain
            // observable to caller as `Reject`).
            SpendLedgerError::Storage(_) => PaidQueryError::UnknownHolder,
        }
    }
}

/// In-memory `SpendLedger` implementation. Default backend for tests
/// + single-process deployments.
#[derive(Clone, Debug, Default)]
pub struct InMemorySpendLedger {
    inner: Arc<Mutex<LedgerState>>,
}

#[derive(Debug, Default)]
struct LedgerState {
    per_holder: HashMap<(WireDid, MacaroonId), MicroOCTO_W>,
}

impl InMemorySpendLedger {
    /// Construct an empty in-memory ledger.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(LedgerState::default())),
        }
    }
}

impl SpendLedger for InMemorySpendLedger {
    fn seed(
        &self,
        holder_did: &WireDid,
        macaroon_id: &MacaroonId,
        budget: MicroOCTO_W,
    ) -> Result<(), SpendLedgerError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| SpendLedgerError::Storage("ledger mutex poisoned".to_owned()))?;
        state
            .per_holder
            .insert((holder_did.clone(), *macaroon_id), budget);
        Ok(())
    }

    fn try_deduct(
        &self,
        holder_did: &WireDid,
        macaroon_id: &MacaroonId,
        cost: MicroOCTO_W,
    ) -> Result<MicroOCTO_W, SpendLedgerError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| SpendLedgerError::Storage("ledger mutex poisoned".to_owned()))?;
        let key = (holder_did.clone(), *macaroon_id);
        let balance = state
            .per_holder
            .get_mut(&key)
            .ok_or(SpendLedgerError::UnknownHolder)?;
        if *balance < cost {
            return Err(SpendLedgerError::InsufficientBalance {
                balance: *balance,
                cost,
            });
        }
        *balance -= cost;
        Ok(*balance)
    }

    fn balance(
        &self,
        holder_did: &WireDid,
        macaroon_id: &MacaroonId,
    ) -> Result<Option<MicroOCTO_W>, SpendLedgerError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| SpendLedgerError::Storage("ledger mutex poisoned".to_owned()))?;
        Ok(state
            .per_holder
            .get(&(holder_did.clone(), *macaroon_id))
            .copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_macaroon_id() -> MacaroonId {
        let mut id = [0u8; 16];
        for (i, b) in id.iter_mut().enumerate() {
            *b = (i as u8).wrapping_add(1);
        }
        id
    }

    fn sample_holder() -> WireDid {
        WireDid::new("did:octo:zTestHolderLedger".to_string())
    }

    #[test]
    fn seed_and_deduct_round_trip() {
        let l = InMemorySpendLedger::new();
        let h = sample_holder();
        let m = sample_macaroon_id();
        l.seed(&h, &m, 1_000).unwrap();
        assert_eq!(l.try_deduct(&h, &m, 250).unwrap(), 750);
        assert_eq!(l.try_deduct(&h, &m, 250).unwrap(), 500);
    }

    #[test]
    fn deduct_unknown_holder_errors() {
        let l = InMemorySpendLedger::new();
        let h = sample_holder();
        let m = sample_macaroon_id();
        let err = l.try_deduct(&h, &m, 1).unwrap_err();
        assert_eq!(err, SpendLedgerError::UnknownHolder);
    }

    #[test]
    fn deduct_insufficient_balance_carries_amounts() {
        let l = InMemorySpendLedger::new();
        let h = sample_holder();
        let m = sample_macaroon_id();
        l.seed(&h, &m, 100).unwrap();
        let err = l.try_deduct(&h, &m, 600).unwrap_err();
        assert_eq!(
            err,
            SpendLedgerError::InsufficientBalance {
                balance: 100,
                cost: 600
            }
        );
    }

    #[test]
    fn balance_returns_none_for_unknown_holder() {
        let l = InMemorySpendLedger::new();
        let h = sample_holder();
        let m = sample_macaroon_id();
        assert_eq!(l.balance(&h, &m).unwrap(), None);
    }
}
