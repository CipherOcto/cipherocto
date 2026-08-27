//! Layer B storage facade for the cipherocto workspace.
//!
//! Per RFC-0206 §Cargo.toml Templates Layer B, this facade exposes
//! exactly **4-item re-export** (Database + TypedStatement +
//! AdapterAllowlist + register helper). The legacy `apply_pending` /
//! `Migration` surface lives under `_legacy_*` aliases in
//! `octo_storage_core` per §Migration Order.
//!
//! ## Layer model
//!
//! Per `cipherocto-design-principles` Layer B, this crate is
//! **RFC-driven, additive only**. The re-export surface mirrors the
//! substrate's public API byte-for-byte. New types land in the substrate
//! first; this facade pulls them in on the next semver minor of the
//! underlying crate.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// Explicit curated re-export list per RFC v2.1 §Cargo.toml Templates
// Layer B. Adding a new symbol requires a deliberate edit here AND in
// the test below — a regression that drops a symbol from the substrate
// will fail the facade test instead of silently dropping the symbol
// from owner crates.
pub use octo_storage_core::AdapterAllowlist;
pub use octo_storage_core::Database;
pub use octo_storage_core::TypedStatement;

/// Adapter registration helper. Owner crates pass their
/// `VaultStore`/`ReputationStore`/etc. impl + an
/// [`AdapterAllowlist`](AdapterAllowlist); the facade wires them into
/// the substrate's typed execution surface.
///
/// The signature is intentionally minimal (returns the `Arc`-wrapped
/// impl back to the caller). Concrete adapter crates will specialize
/// the helper via per-trait impls in downstream missions
/// (`0206-009-adapter-crate-creation`).
pub fn register<A>(
    allowlist: std::sync::Arc<AdapterAllowlist>,
    adapter: std::sync::Arc<A>,
) -> std::sync::Arc<A>
where
    A: Send + Sync + 'static,
{
    // Phase 1.9 hook: full registration body (writes `allowlist` to a
    // process-global registry; consumes `adapter` into the same) lands
    // in `0206-009`. For now, this is the typed-surface witness:
    // callers MUST construct an `AdapterAllowlist` + typed adapter
    // before calling `register`, so the substrate redesign's load-bearing
    // type system invariants are enforced even before the runtime
    // registration logic exists.
    let _ = allowlist;
    adapter
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_storage_core::typed_statement::{DdlOperation, DdlTemplate, SqlInsert, SqlSelect};

    /// Compile-time guarantee that the substrate symbols this facade
    /// is meant to expose actually round-trip through the explicit
    /// `pub use` list above. If the substrate ever drops or renames
    /// any of these, this test fails to compile.
    #[test]
    fn facade_round_trips_substrate_surface() {
        // === Types ===
        let _db: std::sync::Arc<Database> =
            std::sync::Arc::new(Database::open_in_memory().unwrap());
        let _allowlist: AdapterAllowlist =
            AdapterAllowlist::new(octo_storage_core::AdapterId::new("facade_test"));
        let _stmt: TypedStatement = TypedStatement::Select(SqlSelect {
            tables: vec!["t".to_owned()],
        });

        // === register helper ===
        let allowlist = std::sync::Arc::new(AdapterAllowlist::with_registrations(
            octo_storage_core::AdapterId::new("facade_test"),
            ["t".to_owned()],
            [DdlTemplate {
                id: "create_t".to_owned(),
                operation: DdlOperation::CreateTable,
            }],
        ));
        let adapter = std::sync::Arc::new(42_i32);
        let _registered = register(allowlist, adapter);

        // === typed insert round-trip ===
        let _stmt2: TypedStatement = TypedStatement::Insert(SqlInsert {
            table: "t".to_owned(),
        });
    }
}
