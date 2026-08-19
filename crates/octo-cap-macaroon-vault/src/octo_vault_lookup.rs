//! [`OctoVaultLookup`] — production [`VaultLookup`] impl wired to the
//! [`octo_vault::VaultSubstrate`] handle.
//!
//! Mirrors the [`TransportDeliveryCatalog`] glue-crate pattern
//! (`crates/octo-cap-macaroon-transport/src/lib.rs`): the consumer trait
//! stays in `octo-cap-macaroon` (Layer B extension); the typed substrate
//! handle lives in `octo-vault` (Layer B substrate); this glue crate
//! sits between, owning the `VaultState → bool` mapping at lookup time.
//!
//! ## Why this crate exists (S5.1 follow-on)
//!
//! `octo-cap-macaroon` is a Layer B extension crate (per-extension
//! crates + registry per `cipherocto-design-principles.md`). The
//! [`VaultLookup`] trait was added there in S5
//! (mission `0957-g-verify-time-invariant` LANDED 2026-08-17) as a
//! primitive-typed lookup contract (`VaultRowSnapshot.is_active: bool`)
//! — the substrate enum (`octo_vault::VaultState`) lives behind the
//! trait so the consumer crate never imports the substrate's data types.
//!
//! Without this glue crate, the production wiring would either:
//! (a) add a direct `octo-vault` dep to `octo-cap-macaroon` (forbidden —
//!     layer direction A → B → C → D/E; consumer extensions don't depend
//!     on Layer B substrate owners), or
//! (b) `pub use stoolap::Database` in `octo-vault` (forbidden — violates
//!     `feedback_stoolap_persistence` red line; owner crates depend on
//!     the fork directly, never re-export the fork handle).
//!
//! This glue crate resolves both: `octo-vault` exports a typed
//! [`octo_vault::VaultSubstrate`] handle (not the raw `Database`); the
//! glue crate owns the `VaultState → bool` mapping; `octo-cap-macaroon`
//! remains substrate-free.
//!
//! ## §20.6.1 algorithm step 2 (canonical)
//!
//! ```text
//! vault_row = lookup.lookup_vault(vault_id)?  // 1 UNIQUE INDEX lookup against vaults_vault_id_idx
//! ```
//!
//! [`OctoVaultLookup::lookup_vault`] delegates to
//! [`VaultSubstrate::lookup_by_vault_id`], which executes the canonical
//! `SELECT chain_id, state FROM vaults WHERE vault_id = ?` against the
//! `vaults_vault_id_idx` UNIQUE INDEX (~1-3ms SSD on cold cache; warmed
//! by substrate page cache thereafter).

use octo_cap_macaroon::{VaultLookup, VaultRowSnapshot};
use octo_vault::{ChainId, VaultId, VaultState, VaultSubstrate};

/// Production-wired [`VaultLookup`] that reads through the canonical
/// RFC-0960 substrate's `vaults_vault_id_idx` UNIQUE INDEX lookup
/// primitive (review §20.6.1 algorithm step 2).
///
/// The struct is `Send + Sync` (the substrate handle is `Clone` of an
/// `Arc`-wrapped `Database`, both `Send + Sync` per Stoolap's API
/// contract) and is intended to be installed in the wallet's
/// `VaultLookupRegistry` once per `VaultSubstrate` lifecycle (typically
/// per node startup).
///
/// # Identity context
///
/// `chain_id` in [`VaultRowSnapshot`] is the substrate row's `chain_id`
/// (32-byte BLAKE3-derived per §20.3.2) — NOT a network-level chain
/// identifier. The verify-time invariant (review §20.6.1 algorithm
/// step 3) compares this against the macaroon-attested chain_id to
/// gate cross-chain replay.
///
/// # State mapping
///
/// - [`VaultState::Active`] → `is_active: true`
/// - [`VaultState::Frozen`] → `is_active: false`
/// - Any future substrate variant (the enum is `#[non_exhaustive]`) →
///   `is_active: false` (fail-closed: only Active state is operational).
pub struct OctoVaultLookup {
    substrate: VaultSubstrate,
}

impl OctoVaultLookup {
    /// Construct a new `OctoVaultLookup` from a substrate handle.
    ///
    /// The substrate handle should already have [`octo_vault::apply`]
    /// (and any other migrations) run on its underlying database;
    /// `OctoVaultLookup` does not auto-migrate.
    pub fn new(substrate: VaultSubstrate) -> Self {
        Self { substrate }
    }

    /// Helper for callers that need the substrate's
    /// `VaultSubstrate` shape (e.g. composite-catalog adapters that
    /// expose multiple lookup primitives through a single handle).
    pub fn substrate(&self) -> &VaultSubstrate {
        &self.substrate
    }
}

impl std::fmt::Debug for OctoVaultLookup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Per RFC-0957-A1 §Security: do not leak substrate identity in
        // Debug (operator-facing diagnostic only; the substrate's `Debug`
        // impl already redacts its `Database` handle). Same
        // defense-in-depth as `TransportDeliveryCatalog`.
        f.debug_struct("OctoVaultLookup")
            .field("substrate", &"<VaultSubstrate>")
            .finish()
    }
}

impl VaultLookup for OctoVaultLookup {
    fn lookup_vault(&self, vault_id: &[u8; 32]) -> Option<VaultRowSnapshot> {
        // Wrap into the typed `VaultId` so the substrate can apply its
        // trust-posture invariants (no length checks needed — `[u8; 32]`
        // is exactly the substrate's `vault_id` BLOB(32) wire form).
        let vid = VaultId::from_bytes(*vault_id);
        match self.substrate.lookup_by_vault_id(&vid) {
            Ok(Some((chain_id, state))) => Some(VaultRowSnapshot {
                chain_id: chain_bytes(chain_id),
                is_active: matches!(state, VaultState::Active),
            }),
            Ok(None) => None,
            Err(_e) => None,
        }
    }
}

/// Extract the underlying `[u8; 32]` from a `ChainId`. The `ChainId`
/// newtype is `#[non_exhaustive]` but the inner field is `pub`; this
/// adapter centralises the unwrap so any future inner-field rename is
/// a single-site fix.
fn chain_bytes(chain_id: ChainId) -> [u8; 32] {
    *chain_id.as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_bytes_round_trips() {
        let original = [0x42u8; 32];
        let cid = ChainId::from_bytes(original);
        assert_eq!(chain_bytes(cid), original);
    }
}
