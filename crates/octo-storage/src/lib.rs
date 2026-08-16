//! Layer B storage facade for the cipherocto workspace.
//!
//! Pure re-export of [`octo_storage_core`]. Owner crates that prefer a
//! non-`-core` import path can `use octo_storage::apply_pending` instead
//! of `use octo_storage_core::apply_pending`.
//!
//! ## Layer model
//!
//! Per `cipherocto-design-principles` Layer B, this crate is
//! **RFC-driven, additive only**. The re-export surface here mirrors
//! `octo_storage_core`'s public API byte-for-byte. New types land in
//! the substrate first; this facade pulls them in on the next semver
//! minor of the underlying crate.
//!
//! ## Why explicit (not `pub use *`)
//!
//! The substrate has a large public surface (`StorageError`,
//! `Migration`, `StaticMigration`, `ApplyConfig`, ...). A `pub use *`
//! glob would couple the facade's API to the substrate's private
//! `pub use`/re-export hygiene, plus accidentally re-export any
//! future internal-only types that the substrate adds. The explicit
//! list below pins the facade's API contract in code review — adding
//! a new symbol is a deliberate, visible change to this crate.
//!
//! This mirrors the RFC-0870 typed-discriminator discipline: typed
//! surfaces > glob re-exports for stable library APIs.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// Explicit curated re-export list. Adding a new symbol requires a
// deliberate edit here AND in the test below — a regression that
// drops a symbol from the substrate will fail the facade test
// instead of silently dropping the symbol from owner crates.
pub use octo_storage_core::apply_pending;
pub use octo_storage_core::open;
pub use octo_storage_core::open_in_memory;
pub use octo_storage_core::ApplyConfig;
pub use octo_storage_core::Migration;
pub use octo_storage_core::StaticMigration;
pub use octo_storage_core::StorageError;
pub use octo_storage_core::DEFAULT_TRACKER_TABLE;

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time guarantee that the substrate symbols this facade
    /// is meant to expose actually round-trip through the explicit
    /// `pub use` list above. If the substrate ever drops or renames
    /// any of these, this test fails to compile.
    ///
    /// `use super::*;` pulls the facade re-exports into scope so the
    /// test body references them via the facade path — a regression
    /// that accidentally bypasses the re-export would still fail to
    /// compile here.
    #[test]
    fn facade_round_trips_substrate_surface() {
        // === Functions ===
        let _ = apply_pending;
        let _ = open;
        let _ = open_in_memory;

        // === Constants ===
        let _ = DEFAULT_TRACKER_TABLE;

        // === Types ===
        let _cfg: ApplyConfig = ApplyConfig::default();
        let _err: StorageError = StorageError::UnknownMigration {
            version: 0,
            catalog_max: 0,
        };
        let _trait_marker: Option<&dyn Migration> = None;
        let _static: StaticMigration =
            StaticMigration::new(1_u32, "facade_round_trips_substrate_surface", "SELECT 1;");
    }
}
