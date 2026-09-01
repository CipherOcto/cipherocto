//! Mission `0011-e-vault-substrate-additions` substrate (public surface).
//!
//! Re-export hub for the focused sibling modules split out per the
//! Wave 1.5 hygiene callout (the original 767-line module crossed the
//! per-module size threshold per `[[cipherocto-design-principles]]`
//! §No god-objects).
//!
//! Substrate-truth layer model:
//!
//! - Layer B substrate (RFC-driven, additive only). Owns the canonical
//!   types [`VaultSummary`], [`TransferHandle`], [`TransferStatus`], the
//!   port trait [`VaultOwnerIndex`], and the substrate entry points
//!   [`list_owned`], [`project_vault_balance`], [`initiate_transfer`].
//! - Deps: Layer A frozen (`octo-cap-macaroon` for the 32-byte newtypes).
//! - Does NOT depend on `octo-sync`, `octo-protocol`, `octo-wallet`, or
//!   any transport-layer crate.
//!
//! ## Substrate-truth deviation note (RFC-0011-e §Substrate Additions)
//!
//! The mission YAML specifies `list_owned(owner_did: &Did)` and
//! `VaultSummary.owner_did: Did` where `Did` is `octo_wallet::Did`
//! (RFC-0010 canonical form). The substrate here uses the substrate's
//! own [`crate::OwnerDid`] (`String`) alias for `owner_did` because
//! adding `octo-wallet` as a `octo-vault` dep would create a workspace
//! cycle (`octo-vault → octo-wallet → octo-policy → octo-vault`). The
//! substrate's `OwnerDid` is RFC-0010-aligned at the wire level (TEXT
//! column carrying the canonical DID string); consumers that hold an
//! `octo_wallet::Did` pass `.as_str()` at the substrate boundary. A
//! future substrate amendment may lift `Did` into
//! `octo-cap-macaroon` (Layer A frozen substrate) per RFC-0105
//! canonical-home rule, at which point this module's signature is
//! updated additive-only.
//!
//! ## Sibling module map
//!
//! | Module | Owns |
//! |--------|------|
//! | [`crate::vault_summary`] | `VaultSummary` |
//! | [`crate::transfer_handle`] | `TransferHandle` + `TransferStatus` |
//! | [`crate::vault_owner`] | `VaultError` + `VaultOwnerIndex` port + `list_owned` |
//! | [`crate::vault_balance_proj`] | `project_vault_balance` (7-param SUM projection) + substrate cache |
//! | [`crate::vault_initiate`] | `initiate_transfer` (envelope builder) |
//! | [`crate::nonce`] | `handle_id` + `handle_nonce` BLAKE3 derivations |

pub use crate::nonce::{handle_id, handle_nonce};
pub use crate::transfer_handle::{TransferHandle, TransferStatus};
pub use crate::vault_balance_proj::project_vault_balance;
#[cfg(any(test, feature = "testing"))]
pub use crate::vault_balance_proj::reset_substrate_cache_for_test;
pub use crate::vault_initiate::initiate_transfer;
pub use crate::vault_owner::{list_owned, VaultError, VaultOwnerIndex};
pub use crate::vault_summary::VaultSummary;

// ============================================================================
// Tests (Layer B unit tests; per RFC-0011-e §Test Vectors TV-VLT9)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::StubVaultAssetResolver;
    use crate::vault_balance_projection::ProjectionSource;
    use crate::OwnerDid;
    use octo_cap_macaroon::{AssetMetadata, ChainId, InMemoryAssetRegistry, VaultId};

    fn sample_chain() -> ChainId {
        ChainId::derive("cipherocto/testnet/v1")
    }

    fn sample_asset() -> octo_cap_macaroon::AssetId {
        octo_cap_macaroon::AssetId::derive("OCTO-W")
    }

    fn sample_vault() -> VaultId {
        VaultId::from_bytes([0x42u8; 32])
    }

    fn sample_dest() -> VaultId {
        VaultId::from_bytes([0x99u8; 32])
    }

    fn sample_did() -> OwnerDid {
        "did:octo:test-alice".to_string()
    }

    fn sample_registry() -> InMemoryAssetRegistry {
        let mut r = InMemoryAssetRegistry::new();
        let asset = sample_asset();
        r.register(
            asset,
            AssetMetadata::new(
                12,
                12,
                "OCTO-W".to_string(),
                "OCTO-W".to_string(),
                octo_cap_macaroon::AssetKind::SovereignRoleToken,
            ),
        );
        r
    }

    fn sample_resolver() -> StubVaultAssetResolver {
        StubVaultAssetResolver::with_mapping(vec![(sample_chain(), sample_vault(), sample_asset())])
    }

    // ----- TV-VO-1: VaultSummary canonical shape -----

    /// TV-VO-1 (R10 test-coverage): `VaultSummary` carries the canonical
    /// substrate fields with the correct types (no Hex32 / Did mismatch
    /// per VH v1.6 R1 substrate-truth fix).
    #[test]
    fn tv_vo1_vault_summary_canonical_fields() {
        let s = VaultSummary {
            vault_id: sample_vault(),
            chain_id: sample_chain(),
            owner_did: sample_did(),
            asset_symbol: "OCTO-W".to_string(),
            balance_projected: "100.000000000000".to_string(),
            last_updated_unix: Some(1_700_000_000),
        };
        assert_eq!(s.vault_id, sample_vault());
        assert_eq!(s.chain_id, sample_chain());
        assert_eq!(s.owner_did, "did:octo:test-alice");
        assert_eq!(s.balance_projected, "100.000000000000");
    }

    // ----- TV-VO-2: TransferStatus unit variants -----

    /// TV-VO-2 (R10 test-coverage): `TransferStatus` carries the canonical
    /// unit variants (`DryRun | Pending | Confirmed | Failed`) per
    /// RFC-0011-e §Output Envelope.
    #[test]
    fn tv_vo2_transfer_status_unit_variants() {
        assert_eq!(TransferStatus::Pending as u8, 0);
        assert_eq!(TransferStatus::Confirmed as u8, 1);
        assert_eq!(TransferStatus::Failed as u8, 2);
        assert_eq!(TransferStatus::DryRun as u8, 3);
        // `#[non_exhaustive]` — wildcard arm is mandatory in downstream
        // consumers; pin the discipline here so a future variant addition
        // surfaces a compile-error at every consumer site.
        fn _pin_wildcard_arm(s: TransferStatus) -> u8 {
            match s {
                TransferStatus::Pending => 0,
                TransferStatus::Confirmed => 1,
                TransferStatus::Failed => 2,
                TransferStatus::DryRun => 3,
                #[allow(unreachable_patterns)]
                _ => u8::MAX, // substrate fails-closed on unknown variant
            }
        }
        assert_eq!(_pin_wildcard_arm(TransferStatus::Pending), 0);
        assert_eq!(_pin_wildcard_arm(TransferStatus::DryRun), 3);
    }

    /// `DryRun` is a Layer C envelope-build state: the substrate itself
    /// MUST always return `Pending` from [`initiate_transfer`] (per
    /// RFC-0011-e Appendix D — the substrate never suppresses its own
    /// broadcast).
    #[test]
    fn tv_vo2b_substrate_never_produces_dry_run() {
        let h = initiate_transfer(
            &sample_vault(),
            &sample_dest(),
            1_000,
            &sample_asset(),
            1_700_000_000,
        )
        .expect("transfer handle");
        assert_eq!(h.status, TransferStatus::Pending);
        assert_ne!(h.status, TransferStatus::DryRun);
    }

    // ----- TV-VO-3: list_owned substrate port binding -----

    /// Stub `VaultOwnerIndex` returning a fixture vector.
    struct StubOwnerIndex {
        rows: Vec<VaultSummary>,
    }

    impl VaultOwnerIndex for StubOwnerIndex {
        fn vaults_for_owner(&self, _: &str) -> Result<Vec<VaultSummary>, VaultError> {
            Ok(self.rows.clone())
        }
    }

    /// TV-VO-3 (R10 test-coverage): `list_owned` returns the rows
    /// supplied by the substrate port, verbatim. Confirms the port
    /// binding contract.
    #[test]
    fn tv_vo3_list_owned_returns_port_rows() {
        let summary = VaultSummary {
            vault_id: sample_vault(),
            chain_id: sample_chain(),
            owner_did: sample_did(),
            asset_symbol: "OCTO-W".to_string(),
            balance_projected: "0.000000000000".to_string(),
            last_updated_unix: None,
        };
        let index = StubOwnerIndex {
            rows: vec![summary.clone()],
        };
        let got = list_owned(&index, &sample_did()).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0], summary);
    }

    /// TV-VO-4: `list_owned` propagates `VaultError::OwnerNotFound` from
    /// the port.
    struct EmptyOwnerIndex;

    impl VaultOwnerIndex for EmptyOwnerIndex {
        fn vaults_for_owner(&self, _: &str) -> Result<Vec<VaultSummary>, VaultError> {
            Err(VaultError::OwnerNotFound)
        }
    }

    #[test]
    fn tv_vo4_list_owned_propagates_port_error() {
        let err = list_owned(&EmptyOwnerIndex, &sample_did()).unwrap_err();
        assert!(matches!(err, VaultError::OwnerNotFound));
    }

    // ----- TV-VO-5: project_vault_balance canonical 7-param signature -----

    /// TV-VO-5: `project_vault_balance` returns a `VaultBalanceProjection`
    /// with `source_kind = FreshLogScan` on cache miss.
    #[test]
    fn tv_vo5_project_vault_balance_cache_miss_fresh_log_scan() {
        // Reset the process-global substrate cache so prior test state
        // doesn't poison this assertion (Wave 1.5 fix 1; Wave 2.5 fix 4
        // upgraded the reset helper to return a Drop guard that holds
        // a process-global serialization mutex for the test's lifetime).
        let _guard = reset_substrate_cache_for_test();
        let log = crate::testing::StubTransferEventLog::default();
        let proj = project_vault_balance(
            &sample_chain(),
            &sample_vault(),
            &sample_registry(),
            &sample_resolver(),
            &log,
            1, // current_registry_epoch
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(proj.chain_id, sample_chain());
        assert_eq!(proj.vault_id, sample_vault());
        assert_eq!(proj.asset_id, sample_asset());
        assert_eq!(proj.source_kind, ProjectionSource::FreshLogScan);
        assert_eq!(proj.registry_snapshot_epoch, 1);
    }

    /// TV-VO-6: `project_vault_balance` lifts `VaultAssetResolverError`
    /// into `ProjectionError::VaultUnknown` for unknown vaults.
    #[test]
    fn tv_vo6_project_vault_balance_unknown_vault_lifts_error() {
        let _guard = reset_substrate_cache_for_test();
        let log = crate::testing::StubTransferEventLog::default();
        let empty_resolver = StubVaultAssetResolver::default();
        let err = project_vault_balance(
            &sample_chain(),
            &sample_vault(),
            &sample_registry(),
            &empty_resolver,
            &log,
            1,
            1_700_000_000,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            crate::vault_balance_projection::ProjectionError::VaultUnknown { .. }
        ));
    }

    // ----- TV-VO-7: initiate_transfer canonical signature -----

    /// TV-VO-7: `initiate_transfer` returns a `TransferHandle` with
    /// `status = Pending` and a deterministic `handle_id` per inputs.
    #[test]
    fn tv_vo7_initiate_transfer_builds_pending_handle() {
        let h1 = initiate_transfer(
            &sample_vault(),
            &sample_dest(),
            100_000_000,
            &sample_asset(),
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(h1.status, TransferStatus::Pending);
        assert_eq!(h1.vault_id, sample_vault());
        assert_eq!(h1.dest_vault_id, sample_dest());
        assert_eq!(h1.amount_dqa_micros, 100_000_000);
        assert_eq!(h1.asset_id, sample_asset());
        assert_ne!(h1.handle_id, [0u8; 32], "handle_id must not be all-zero");
        assert_ne!(h1.nonce, [0u8; 32], "nonce must not be all-zero");

        // Determinism: same inputs MUST yield same handle_id (idempotent
        // envelope identification).
        let h2 = initiate_transfer(
            &sample_vault(),
            &sample_dest(),
            100_000_000,
            &sample_asset(),
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(
            h1.handle_id, h2.handle_id,
            "initiate_transfer MUST be substrate-deterministic per inputs"
        );
    }

    /// TV-VO-8: `initiate_transfer` fails-CLOSED on non-positive amount.
    #[test]
    fn tv_vo8_initiate_transfer_rejects_non_positive_amount() {
        let err = initiate_transfer(
            &sample_vault(),
            &sample_dest(),
            0,
            &sample_asset(),
            1_700_000_000,
        )
        .unwrap_err();
        assert!(matches!(err, VaultError::Substrate));
        let err = initiate_transfer(
            &sample_vault(),
            &sample_dest(),
            -1,
            &sample_asset(),
            1_700_000_000,
        )
        .unwrap_err();
        assert!(matches!(err, VaultError::Substrate));
    }

    /// TV-VO-9: `initiate_transfer` produces distinct `handle_id` per
    /// distinct inputs (no input collision).
    #[test]
    fn tv_vo9_initiate_transfer_handle_id_collision_free() {
        let h1 = initiate_transfer(
            &sample_vault(),
            &sample_dest(),
            100,
            &sample_asset(),
            1_700_000_000,
        )
        .unwrap();
        let other_vault = VaultId::from_bytes([0x43u8; 32]);
        let h2 = initiate_transfer(
            &other_vault,
            &sample_dest(),
            100,
            &sample_asset(),
            1_700_000_000,
        )
        .unwrap();
        assert_ne!(
            h1.handle_id, h2.handle_id,
            "different vault_id MUST produce different handle_id"
        );
    }
}
