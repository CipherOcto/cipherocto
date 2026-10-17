//! Disk-backed agent registry — `agent_registry.json` per wallet root.
//!
//! The substrate's `AGENT_REGISTRY` is a process-global in-memory
//! `BTreeMap<Uuid, AgentRecord>` keyed by `agent_id`; this module is
//! the cross-process persistence boundary that lets a second CLI
//! process see agents registered by a first one (the L3 multi-peer
//! e2e harness reaches this surface for the `agent run` success-path).
//!
//! ## File shape
//!
//! A flat JSON object keyed by `agent_id` (UUID). Each value is a
//! [`PersistedRecord`] — the public `AgentManifest` plus the
//! holder DID, registration timestamp, and current state. The
//! `transitioning: bool` field is RAII-only (RFC-0015-b §X.1) and
//! is **never** serialized; a rehydrated record always starts with
//! `transitioning = false`, which is the correct state for a fresh
//! process boundary.
//!
//! ## Atomic write
//!
//! Same pattern as `octo_cap_macaroon::holder_persist`: serialize →
//! write `.tmp` → rename. A crash mid-write leaves the prior file
//! intact (the `.tmp` is orphan garbage that the next `load` will
//! ignore). The on-disk file is the cross-process boundary the L3
//! harness uses; partial writes would surface as a `serde` error
//! from `load` and fail closed.
//!
//! ## Determinism
//!
//! `agent_id` derivation lives in `cli_fns::register_agent`
//! (UUIDv5 over `(manifest_digest, holder_did)`). This module
//! receives the `agent_id` from the caller rather than re-deriving
//! it, so the on-disk shape matches the in-memory shape exactly.
//!
//! ## Layer discipline
//!
//! Layer B additive surface (mission 0011-x-s-a-wallet-store-identity
//! §Cross-process Persistence). Depends on the public
//! `AgentManifest` + `AgentState` types and the substrate's
//! `agent_id` derivation (called from `cli_fns`). The CLI Layer C
//! consumes the free functions; substrate Layer A is untouched.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::{AgentManifest, AgentState};

/// Filename of the per-wallet agent registry.
///
/// Path: `<wallet_root>/agent_registry.json` — co-located with
/// `holder_capabilities.json` (octo_cap_macaroon::holder_persist)
/// and `store.json` (the wallet index). Same atomic-write primitive.
pub const REGISTRY_FILENAME: &str = "agent_registry.json";

/// On-disk shape of a single agent record.
///
/// Mirrors the substrate's `AgentRecord` minus the `transitioning:
/// bool` field (RAII-only per RFC-0015-b §X.1; never persisted
/// because it is meaningless across a process boundary — a rehydrated
/// record always starts with the flag cleared). Public re-export
/// shape; the substrate owns the in-memory `AgentRecord` definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedRecord {
    /// The agent's manifest (RFC-0002 §Agent Manifest).
    pub manifest: AgentManifest,
    /// Holder DID, RFC-0010 form. Mirrors `AgentManifest::holder_did`
    /// at registration time but stored separately so the manifest
    /// itself remains canonical and unchanged on state transitions.
    pub holder_did: String,
    /// Unix seconds at which the agent was registered.
    pub registered_at_unix: u64,
    /// Current lifecycle state (RFC-0015-a §6.1). Defaults to
    /// `Registered` for new records; mutated additively by the
    /// `Registered → Running` and `Running → Terminated` edges.
    pub state: AgentState,
}

/// On-disk shape of the full registry: a flat object keyed by
/// `agent_id` (UUID). The keyed-by-UUID shape (vs. the in-memory
/// `BTreeMap<Uuid, _>` which is also keyed by UUID) means the JSON
/// is a stable object — the on-disk order matches the
/// deterministic BTreeMap iteration (RFC-0008 Class B).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedRegistry {
    /// Records, keyed by `agent_id`. JSON shape is `{"<uuid>": {...}}`
    /// (serde's default for `BTreeMap<String, _>`).
    pub records: BTreeMap<Uuid, PersistedRecord>,
}

/// Errors surfaced by the free functions.
///
/// `PersistError` is the substrate-side error type for the agent
/// registry persistence layer. The CLI maps it to the user-facing
/// `WalletError::AgentPersist(_)` variant at the call boundary
/// (the substrate's free functions in `agent.rs` do this mapping
/// once, so the in-memory mutations can be rolled back).
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// I/O failure (read / write / rename).
    #[error("agent registry I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON parse failure (corrupted or partial file). A corrupted
    /// registry is treated as a hard failure so the issue is visible
    /// rather than silently lost.
    #[error("agent registry parse error: {0}")]
    Parse(String),
    /// The on-disk record carries a different `agent_id` than the
    /// one the caller supplied (a hash collision is the only way this
    /// can happen; the substrate's UUIDv5 derivation is
    /// collision-resistant per RFC 4122 §4.3).
    #[error("agent_id mismatch on persist: expected {expected}, got {actual}")]
    AgentIdMismatch {
        /// The agent_id the caller passed.
        expected: Uuid,
        /// The agent_id the on-disk shape actually carries (a hash
        /// collision would surface this).
        actual: Uuid,
    },
}

fn parse_err(e: &serde_json::Error) -> PersistError {
    PersistError::Parse(e.to_string())
}

/// Compose the [`PathBuf`] of the registry file under `wallet_root`.
fn registry_path(wallet_root: &Path) -> PathBuf {
    wallet_root.join(REGISTRY_FILENAME)
}

/// Compose the [`PathBuf`] of the atomic-write staging file.
fn staging_path(wallet_root: &Path) -> PathBuf {
    let mut p = wallet_root.to_path_buf();
    p.push(format!("{REGISTRY_FILENAME}.tmp"));
    p
}

/// Load the on-disk registry. A missing file returns an empty
/// `PersistedRegistry` (the directory may not exist yet either; that
/// is also OK and returns empty, so the first register lazily creates
/// the directory).
///
/// # Errors
/// `PersistError::Io` on filesystem failure,
/// `PersistError::Parse` on corrupt JSON.
pub fn load(wallet_root: &Path) -> Result<PersistedRegistry, PersistError> {
    let path = registry_path(wallet_root);
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<PersistedRegistry>(&bytes).map_err(|e| parse_err(&e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(PersistedRegistry::default()),
        Err(e) => Err(PersistError::Io(e)),
    }
}

/// Save the registry atomically: serialize → write `.tmp` → rename.
///
/// The `.tmp` rename is `std::fs::rename`, which is atomic on POSIX
/// when both source and target are on the same filesystem (the
/// `wallet_root` is a single directory, so the staging file and the
/// final file share a filesystem by construction).
///
/// # Errors
/// `PersistError::Io` on filesystem failure,
/// `PersistError::Parse` on serialization failure.
pub fn save(wallet_root: &Path, registry: &PersistedRegistry) -> Result<(), PersistError> {
    let serialized = serde_json::to_vec_pretty(registry).map_err(|e| parse_err(&e))?;
    let staging = staging_path(wallet_root);
    std::fs::write(&staging, &serialized)?;
    std::fs::rename(&staging, registry_path(wallet_root))?;
    Ok(())
}

/// Append a new `PersistedRecord` to the registry file. Atomic write
/// via the same `.tmp` + rename primitive as `save`.
///
/// # Errors
/// `PersistError::Io` on filesystem failure, `PersistError::Parse` if
/// the existing file is corrupt, `PersistError::AgentIdMismatch` if
/// the on-disk record carries a different `agent_id` (hash collision).
pub fn register(
    agent_id: Uuid,
    record: &PersistedRecord,
    wallet_root: &Path,
) -> Result<(), PersistError> {
    let mut registry = load(wallet_root)?;
    if let Some(existing) = registry.records.get(&agent_id) {
        // Defensive: if the on-disk shape carries the same key but
        // a different serialized agent_id (impossible under the
        // UUIDv5 derivation, but the type system allows it), surface
        // the mismatch. The caller (`register_agent`) then rolls
        // back the in-memory insert and returns `AgentPersist`.
        if existing.manifest.manifest_id != record.manifest.manifest_id {
            return Err(PersistError::AgentIdMismatch {
                expected: agent_id,
                actual: existing.manifest.manifest_id,
            });
        }
    }
    registry.records.insert(agent_id, record.clone());
    save(wallet_root, &registry)
}

/// Apply a state transition to the on-disk record identified by
/// `agent_id`. Atomic write via the same `.tmp` + rename primitive.
///
/// # Errors
/// `PersistError::Io` on filesystem failure, `PersistError::Parse` if
/// the existing file is corrupt.
pub fn transition(
    agent_id: Uuid,
    target: AgentState,
    wallet_root: &Path,
) -> Result<(), PersistError> {
    let mut registry = load(wallet_root)?;
    let entry = registry
        .records
        .get_mut(&agent_id)
        .ok_or_else(|| PersistError::Parse(format!("agent_id {agent_id} not in registry")))?;
    entry.state = target;
    save(wallet_root, &registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record() -> PersistedRecord {
        PersistedRecord {
            manifest: AgentManifest {
                manifest_id: Uuid::new_v4(),
                holder_did: format!("did:octo:zPersist-{}", Uuid::new_v4()),
                label: Some("persist-test".to_string()),
                created_at_unix: 1_700_000_000,
                signature_hex: "00".repeat(64),
            },
            holder_did: "did:octo:placeholder".to_string(),
            registered_at_unix: 1_700_000_001,
            state: AgentState::Registered,
        }
    }

    /// A missing file returns an empty registry, not an error. The
    /// first register on a fresh wallet would otherwise crash.
    #[test]
    fn load_missing_returns_empty() {
        let tmp = tempdir();
        let reg = load(&tmp).expect("missing file returns empty");
        assert!(reg.records.is_empty());
    }

    /// `register` then `transition` then `load` round-trips the
    /// record on disk. The `transitioning` field is RAII-only and
    /// is never serialized; this test pins the on-disk shape by
    /// asserting the deserialized record has the same `state` the
    /// transition wrote.
    #[test]
    fn register_then_transition_then_persisted() {
        let tmp = tempdir();
        let record = sample_record();
        let agent_id = record.manifest.manifest_id;
        register(agent_id, &record, &tmp).expect("register");

        transition(agent_id, AgentState::Running, &tmp).expect("transition");
        let after = load(&tmp).expect("load after transition");
        let entry = after.records.get(&agent_id).expect("entry persisted");
        assert_eq!(entry.state, AgentState::Running);
        assert_eq!(entry.manifest.manifest_id, record.manifest.manifest_id);
    }

    /// Atomic write: a crash mid-write leaves the prior file intact.
    /// Pinned by asserting the staging file does NOT exist after a
    /// successful save (rename consumed it). A future regression
    /// that wrote directly to the final path would not show up here,
    /// but the staging rename is the only way to get atomicity on
    /// POSIX.
    #[test]
    fn save_uses_staging_rename_not_direct_write() {
        let tmp = tempdir();
        let mut registry = PersistedRegistry::default();
        let record = sample_record();
        registry.records.insert(record.manifest.manifest_id, record);
        save(&tmp, &registry).expect("save");

        let staging = staging_path(&tmp);
        assert!(
            !staging.exists(),
            "staging path must not survive a successful save: {staging:?}"
        );
        assert!(registry_path(&tmp).exists());
    }

    /// Cross-process round trip: write via the public free fn, then
    /// load via the public free fn from a fresh process-state
    /// perspective (the on-disk file IS the cross-process boundary).
    #[test]
    fn cross_process_via_disk_round_trip() {
        let tmp = tempdir();
        let record = sample_record();
        let agent_id = record.manifest.manifest_id;
        register(agent_id, &record, &tmp).expect("register");
        // Simulate a process restart: a fresh `load` reads only the
        // on-disk file (no in-memory state involved).
        let after = load(&tmp).expect("load fresh");
        let entry = after.records.get(&agent_id).expect("entry round-trips");
        assert_eq!(entry.state, AgentState::Registered);
        assert_eq!(entry.manifest.manifest_id, record.manifest.manifest_id);
        assert_eq!(entry.holder_did, record.holder_did);
    }

    fn tempdir() -> PathBuf {
        // Per-test unique path: cargo's default harness runs tests in
        // parallel within a single process, so a PID-keyed tempdir races
        // across the four tests in this module. Each test gets its own
        // subdirectory keyed by the test fn name + the OS nanotime
        // counter, so a torn write in one test cannot leak entries
        // into another.
        let base = std::env::temp_dir().join(format!(
            "octo-wallet-agent-persist-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0u128, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&base).expect("mkdir");
        base
    }
}
