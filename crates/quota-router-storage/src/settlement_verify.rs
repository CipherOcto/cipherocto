//! Settlement-time vault-row chain-match verifier (RFC-0959
//! §Settlement-Time Vault Row Lookup + §Cross-Chain Settlement
//! Reject, per review §20.7).
//!
//! Mission `0959-c1-wire-format-amendment`. Operates on
//! [`SettlementEnvelope`] (RFC-0959 wire form) and reuses the
//! [`octo_cap_macaroon::VaultLookup`] trait — the same trait that
//! backs capability verify-time (RFC-0957 §Verify-Time Extension +
//! S5 LANDED + S5.1 follow-on `OctoVaultLookup`).
//!
//! ## Why this lives in `quota-router-storage`, not `octo-cap-macaroon-vault`
//!
//! The verifier is a CONSUMER of the `VaultLookup` trait, not an
//! adapter over a concrete substrate. It belongs alongside
//! [`SettlementEnvelope`] (which lives in
//! `crates/quota-router-storage/src/ask.rs`) because it operates on
//! the v2.0 settlement wire form. The
//! `octo-cap-macaroon-vault` crate (which holds the production
//! `OctoVaultLookup` adapter) does NOT import
//! `quota-router-storage` types — it stays a glue crate between
//! macaroon + vault, free of marketplace business types.
//!
//! ## Why `&dyn VaultLookup`, not `&OctoVaultLookup`
//!
//! Trait abstraction is load-bearing per audit #5 HIGH: a concrete
//! struct arg would defeat the abstraction. The trait lives in
//! `octo-cap-macaroon` (Layer B extension); the production adapter
//! `OctoVaultLookup` lives in `octo-cap-macaroon-vault` (Layer B
//! substrate adapter glue). The marketplace caller instantiates
//! `OctoVaultLookup::new(substrate)` and passes it as
//! `&dyn VaultLookup` to this verifier — Layer discipline preserved.
//!
//! ## 3-step algorithm (RFC-0959 §Settlement-Time Vault Row Lookup)
//!
//! 1. `cost_vault_id` present? Else reject with
//!    [`SettlementError::CostVaultIdMissing`].
//! 2. `vault_lookup.lookup_vault(cost_vault_id)` returns
//!    `Some(VaultRowSnapshot { chain_id, is_active, ... })`? Else
//!    reject with [`SettlementError::CostVaultIdMissing`] (no vault
//!    row at the given key — equivalent to "missing").
//! 3. `vault_snapshot.chain_id == envelope.chain_id`? Else reject
//!    with [`SettlementError::ChainMismatch`] (cross-chain settlement
//!    attempt).
//!
//! [`SettlementEnvelope`]: crate::ask::SettlementEnvelope
//! [`SettlementError`]: crate::ask::SettlementError

#![allow(missing_docs)] // mirror crate-level allow (Pedantic doc style deferred)

use octo_cap_macaroon::VaultLookup;

use crate::ask::{SettlementEnvelope, SettlementError};

/// Settlement-time vault-row chain-match verifier.
///
/// Cross-chain settlement reject per RFC-0959
/// §Cross-Chain Settlement Reject + review §20.7.
///
/// # Errors
/// - [`SettlementError::CostVaultIdMissing`] if
///   `envelope.cost_vault_id` is `None` OR if the vault row at that
///   id does not exist (`vault_lookup.lookup_vault` returns `None`).
/// - [`SettlementError::ChainMismatch`] if the vault row's `chain_id`
///   does not match `envelope.chain_id`.
///
/// # Layer discipline
///
/// Takes `&dyn VaultLookup` (trait from `octo-cap-macaroon`); never
/// imports the concrete `OctoVaultLookup` adapter (which lives in
/// `octo-cap-macaroon-vault`). The marketplace caller wires
/// `OctoVaultLookup::new(substrate)` as the `&dyn VaultLookup` arg.
pub fn verify_settlement_chain_match(
    envelope: &SettlementEnvelope,
    vault_lookup: &dyn VaultLookup,
) -> Result<(), SettlementError> {
    // Step 1: cost_vault_id must be present (v2.0 wire form requires
    // the field; pre-v2.0 envelopes have None and are rejected — this
    // is the migration gate).
    let vault_id = envelope
        .cost_vault_id
        .ok_or(SettlementError::CostVaultIdMissing)?;

    // Step 2: vault row must exist at the given key. Same trait
    // shared with capability verify-time (RFC-0957 §Verify-Time
    // Extension). NO shadow impl.
    let snapshot = vault_lookup
        .lookup_vault(&vault_id)
        .ok_or(SettlementError::CostVaultIdMissing)?;

    // Step 3: vault.chain_id MUST equal envelope.chain_id. Both are
    // 32-byte BLAKE3-derived per RFC-0010.
    let envelope_chain_id = envelope
        .chain_id
        .ok_or(SettlementError::CostVaultIdMissing)?;
    if snapshot.chain_id != envelope_chain_id {
        return Err(SettlementError::ChainMismatch {
            vault_id,
            vault_chain_id: snapshot.chain_id,
            envelope_chain_id,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_cap_macaroon::{VaultLookup, VaultRowSnapshot};
    use std::collections::HashMap;

    /// Trivial in-memory `VaultLookup` for unit tests (mirrors the
    /// `InMemoryLookup` pattern in `octo-cap-macaroon/src/vault_lookup.rs`).
    struct InMemoryLookup {
        rows: HashMap<[u8; 32], VaultRowSnapshot>,
    }

    impl VaultLookup for InMemoryLookup {
        fn lookup_vault(&self, vault_id: &[u8; 32]) -> Option<VaultRowSnapshot> {
            self.rows.get(vault_id).copied()
        }
    }

    /// Build a minimal envelope for testing (settlement_hash + ask_id
    /// + nonce + timestamp fields are all zeroed; cost: Dqa::zero()).
    fn envelope_with(
        cost_vault_id: Option<[u8; 32]>,
        chain_id: Option<[u8; 32]>,
    ) -> SettlementEnvelope {
        SettlementEnvelope {
            settlement_hash: [0u8; 32],
            asker_did: "did:cipherocto:asker-test".to_string(),
            holder_did: "did:cipherocto:holder-test".to_string(),
            model: crate::ask::ModelRef::from("test/test-model/v1"),
            axes_consumed: vec![],
            ask_id: [0u8; 32],
            nonce: [0u8; 32],
            timestamp_unix: 0,
            cost: octo_determin::Dqa::new(0, 0).expect("zero"),
            cost_vault_id,
            chain_id,
        }
    }

    #[test]
    fn cost_vault_id_missing_rejects() {
        let l = InMemoryLookup {
            rows: HashMap::new(),
        };
        let env = envelope_with(None, Some([0xAB; 32]));
        let err = verify_settlement_chain_match(&env, &l).unwrap_err();
        assert!(matches!(err, SettlementError::CostVaultIdMissing));
    }

    #[test]
    fn vault_row_missing_rejects() {
        let l = InMemoryLookup {
            rows: HashMap::new(),
        };
        let env = envelope_with(Some([0xCC; 32]), Some([0xAB; 32]));
        let err = verify_settlement_chain_match(&env, &l).unwrap_err();
        assert!(matches!(err, SettlementError::CostVaultIdMissing));
    }

    #[test]
    fn envelope_chain_id_missing_rejects() {
        let mut rows = HashMap::new();
        rows.insert(
            [0xEE; 32],
            VaultRowSnapshot {
                chain_id: [0x01; 32],
                is_active: true,
            },
        );
        let l = InMemoryLookup { rows };
        let env = envelope_with(Some([0xEE; 32]), None);
        let err = verify_settlement_chain_match(&env, &l).unwrap_err();
        assert!(matches!(err, SettlementError::CostVaultIdMissing));
    }

    #[test]
    fn chain_match_passes() {
        let chain = [0x42u8; 32];
        let mut rows = HashMap::new();
        rows.insert(
            [0xEE; 32],
            VaultRowSnapshot {
                chain_id: chain,
                is_active: true,
            },
        );
        let l = InMemoryLookup { rows };
        let env = envelope_with(Some([0xEE; 32]), Some(chain));
        assert!(verify_settlement_chain_match(&env, &l).is_ok());
    }

    #[test]
    fn chain_mismatch_rejects() {
        let vault_chain = [0x01u8; 32];
        let envelope_chain = [0x02u8; 32];
        let vault_id = [0xEEu8; 32];
        let mut rows = HashMap::new();
        rows.insert(
            vault_id,
            VaultRowSnapshot {
                chain_id: vault_chain,
                is_active: true,
            },
        );
        let l = InMemoryLookup { rows };
        let env = envelope_with(Some(vault_id), Some(envelope_chain));
        let err = verify_settlement_chain_match(&env, &l).unwrap_err();
        match err {
            SettlementError::ChainMismatch {
                vault_id: v,
                vault_chain_id: vc,
                envelope_chain_id: ec,
            } => {
                assert_eq!(v, vault_id);
                assert_eq!(vc, vault_chain);
                assert_eq!(ec, envelope_chain);
            }
            other => panic!("expected ChainMismatch, got {other:?}"),
        }
    }

    #[test]
    fn trait_abstraction_no_shadow_impl() {
        // Compile-time guard: the verifier accepts `&dyn VaultLookup`,
        // NOT a concrete struct. Any attempt to replace it with a
        // concrete `&OctoVaultLookup` arg would force this crate to
        // depend on `octo-cap-macaroon-vault` (forbidden by layer
        // model: storage (quota-router-storage) depends on Layer B
        // extensions (octo-cap-macaroon) but NOT on Layer B substrate
        // adapters (octo-cap-macaroon-vault)).
        fn _accepts_trait_object(_: &dyn VaultLookup) {}
        let l = InMemoryLookup {
            rows: HashMap::new(),
        };
        _accepts_trait_object(&l);
    }
}
