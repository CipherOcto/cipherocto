//! [`VaultLookup`] trait — RFC-0957 (verify-time bump) per review §20.6.1.
//!
//! Injected into `Macaroon::verify_for_vault_op` at config time. Mirrors
//! the existing `CapabilityCatalog` pattern (`crates/octo-cap-macaroon/src/macaroon.rs:432`):
//! trait lives in this consumer crate, impl lives in the substrate owner
//! (`octo-vault`).
//!
//! ## Why no `octo_vault::VaultState` import
//!
//! `octo_vault::VaultState` is the substrate's enum (variants: `Active`,
//! `Frozen`). Importing it here would invert the Layer B → Layer B
//! dependency direction (octo-cap-macaroon as extension should NOT pull
//! the substrate's data types). Instead [`VaultRowSnapshot::is_active`]
//! is a primitive `bool` — the substrate adapter maps its own state
//! enum into this at lookup time.
//!
//! ## §20.6.1 algorithm step 2 (canonical)
//!
//! ```text
//! vault_row = lookup.lookup_vault(vault_id)?  // 1 UNIQUE INDEX lookup against vaults_vault_id_idx
//! ```
//!
//! Returns `None` iff no vault row exists. `verify_for_vault_op` maps
//! `None` to `VaultVerifyError::VaultRowMissing { vault_id }`.

#![allow(missing_docs)] // mirror crate-level allow (Pedantic doc style deferred)

use super::vault_verify_error::VaultVerifyError;

/// Snapshot of a vault row extracted from the substrate.
///
/// Contains the row data strictly required by the verify-time invariant
/// (review §20.6.1): `chain_id` for the chain-matching check
/// (algorithm step 3) and a primitive active flag for the state check
/// (step 4). Additional columns (balance, policy, metadata) are out of
/// scope here — verify-time invariant does not consult them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VaultRowSnapshot {
    /// Composite-PK leading column — the canonical chain identifier.
    /// BLAKE3("cipherocto/chain/v1/" || chain_string) per review §20.3.2.
    pub chain_id: [u8; 32],
    /// True iff the substrate row's state is `VaultState::Active`.
    /// `Frozen` and any future non-Active state map to `false`.
    pub is_active: bool,
}

/// Vault row lookup for the capability verify-time path.
///
/// Backed in production by the Stoolap-fork substrate's
/// `vaults_vault_id_idx` UNIQUE INDEX lookup per review §20.6.1
/// algorithm step 2 (~1-3ms SSD; option (b) — adopted per §20.6.1 last
/// row).
///
/// **Implementor contract:**
/// - MUST return `None` if no row with the given `vault_id` exists.
/// - MUST return `Some(snapshot)` with accurate `chain_id` + `is_active`
///   fields if the row exists.
/// - MUST be side-effect-free (cache-friendly; verification is hot path).
/// - MUST be deterministic across nodes for a given substrate state
///   (consensus invariant — the verify-time invariant must agree
///   across all verifiers).
pub trait VaultLookup: Send + Sync {
    /// Look up a vault row by its derived `vault_id` (32 bytes). Returns
    /// `None` if no row exists with that key.
    fn lookup_vault(&self, vault_id: &[u8; 32]) -> Option<VaultRowSnapshot>;
}

/// Convert a `VaultLookup` lookup failure into a structured error.
///
/// Helper trait for callers that already have a `vault_id` in scope.
/// Centralizes the `None -> VaultRowMissing` mapping so verify paths
/// stay one-liners.
pub trait VaultLookupExt: VaultLookup {
    /// Wraps `lookup_vault` with the canonical error mapping.
    ///
    /// # Errors
    /// Returns `VaultVerifyError::VaultRowMissing { vault_id }` iff
    /// `lookup_vault(vault_id)` returns `None`.
    fn require_vault(&self, vault_id: &[u8; 32]) -> Result<VaultRowSnapshot, VaultVerifyError> {
        self.lookup_vault(vault_id)
            .ok_or(VaultVerifyError::VaultRowMissing {
                vault_id: *vault_id,
            })
    }
}

impl<T: VaultLookup + ?Sized> VaultLookupExt for T {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trivial in-memory `VaultLookup` for unit tests. Production
    /// consumers wire `OctoVaultLookup` (Layer B substrate adapter).
    #[derive(Default)]
    struct InMemoryLookup {
        rows: std::collections::HashMap<[u8; 32], VaultRowSnapshot>,
    }

    impl VaultLookup for InMemoryLookup {
        fn lookup_vault(&self, vault_id: &[u8; 32]) -> Option<VaultRowSnapshot> {
            self.rows.get(vault_id).copied()
        }
    }

    #[test]
    fn in_memory_lookup_hit() {
        let mut rows = std::collections::HashMap::new();
        let vid = [0xAB; 32];
        let chain = [0x01; 32];
        rows.insert(
            vid,
            VaultRowSnapshot {
                chain_id: chain,
                is_active: true,
            },
        );
        let l = InMemoryLookup { rows };
        let snap = l.lookup_vault(&vid).expect("hit");
        assert_eq!(snap.chain_id, chain);
        assert!(snap.is_active);
    }

    #[test]
    fn in_memory_lookup_miss_returns_none() {
        let l = InMemoryLookup::default();
        assert_eq!(l.lookup_vault(&[0xCC; 32]), None);
    }

    #[test]
    fn require_vault_maps_none_to_error() {
        let l = InMemoryLookup::default();
        let err = l.require_vault(&[0xCD; 32]).unwrap_err();
        assert!(
            matches!(err, VaultVerifyError::VaultRowMissing { vault_id } if vault_id == [0xCD; 32])
        );
    }

    #[test]
    fn require_vault_returns_snapshot_for_hit() {
        let mut rows = std::collections::HashMap::new();
        let vid = [0xEE; 32];
        let chain = [0x02; 32];
        rows.insert(
            vid,
            VaultRowSnapshot {
                chain_id: chain,
                is_active: false,
            },
        );
        let l = InMemoryLookup { rows };
        let snap = l.require_vault(&vid).expect("hit");
        assert_eq!(snap.chain_id, chain);
        assert!(!snap.is_active);
    }
}
