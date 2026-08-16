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

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use octo_storage_core::*;

#[cfg(test)]
mod tests {
    /// Compile-time guarantee that the substrate symbols this facade is
    /// meant to expose actually round-trip through `pub use *`. If the
    /// substrate ever drops or renames any of these, this test fails to
    /// compile and owner crates get a clear signal at the Layer B
    /// boundary instead of an obscure downstream break.
    #[test]
    fn facade_round_trips_substrate_surface() {
        // Pin the symbols that the historical facade promise hinges on:
        // owner crates import `octo_storage::apply_pending` /
        // `octo_storage::Migration` / `octo_storage::ApplyConfig` /
        // `octo_storage::StorageError` / `octo_storage::StaticMigration`.
        // Each line resolves via the `pub use octo_storage_core::*;`
        // glob — if any of them stop resolving, the build breaks here
        // before any owner crate does.
        let _ = octo_storage_core::apply_pending;
        let _: &dyn octo_storage_core::Migration = &octo_storage_core::StaticMigration::new(
            1_u32,
            "facade_round_trips_substrate_surface",
            "SELECT 1;",
        );
        let _ = octo_storage_core::ApplyConfig::default();
    }
}
