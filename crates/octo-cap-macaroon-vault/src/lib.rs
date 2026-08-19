//! Vault glue crate for [`octo_cap_macaroon`].
//!
//! Owns the production-wired [`OctoVaultLookup`] that drives
//! [`octo_cap_macaroon::VaultLookup`] via the canonical RFC-0960
//! substrate's `vaults_vault_id_idx` UNIQUE INDEX lookup primitive.
//!
//! ## Why this crate exists (mission 0957-g1, S5.1 follow-on)
//!
//! `octo-cap-macaroon` is a Layer B extension crate (per-extension
//! crates + registry per `cipherocto-design-principles.md`). The
//! [`VaultLookup`] trait landed there in S5 as a primitive-typed
//! lookup contract (mission `0957-g-verify-time-invariant` LANDED
//! 2026-08-17); the substrate's data enum ([`octo_vault::VaultState`])
//! lives behind the trait boundary. Without this glue crate, the
//! production wiring would either force an `octo-vault` dep on
//! `octo-cap-macaroon` (forbidden — layer direction) or expose
//! `stoolap::Database` from `octo-vault` (forbidden — fork-persistence
//! red line per `feedback_stoolap_persistence`).
//!
//! This glue crate sits between, owning the [`VaultState`] →
//! `is_active: bool` mapping at lookup time. `octo-cap-macaroon`
//! remains free of vault-substrate types; `octo-vault` exposes a typed
//! [`octo_vault::VaultSubstrate`] handle (not the raw `Database`).
//!
//! ## Migration
//!
//! Callers that previously hand-rolled a [`VaultLookup`] impl (e.g.,
//! the `TestVaultLookup` stand-in in
//! `crates/octo-cap-macaroon/tests/tv_0957_verify_time.rs`) can now
//! wire the production [`OctoVaultLookup`] instead. The trait contract
//! is unchanged — only the impl source changes.
//!
//! ## Layer direction
//!
//! ```text
//! octo-cap-macaroon     (Layer B extension — consumer of trait)
//!        │
//!        ▼
//! octo-cap-macaroon-vault  (THIS crate — Layer B glue, owns mapping)
//!        │
//!        ▼
//! octo-vault            (Layer B substrate — typed VaultSubstrate handle)
//!        │
//!        ▼
//! stoolap fork          (Layer A — Database handle, NEVER re-exported)
//! ```
//!
//! Mirrors the [`TransportDeliveryCatalog`] topology
//! (`crates/octo-cap-macaroon-transport/`) exactly.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

mod octo_vault_lookup;

pub use octo_cap_macaroon::{VaultLookup, VaultLookupExt, VaultRowSnapshot};
pub use octo_vault_lookup::OctoVaultLookup;
