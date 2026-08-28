//! Layer B `[ADD]` free functions per RFC-0011 §Subcommand Taxonomy entries #14-17.

#![allow(clippy::module_name_repetitions)]

use crate::cli_record::{NameHashIndex, PolicyFilter, PolicyListEntry, PolicyRecord};
use crate::policy_registry::PolicyRegistryError;

use std::sync::OnceLock;

/// Process-local `NameHashIndex`. Real impl is a `OnceCell<NameHashIndex>`
/// populated from `PolicyRegistry` at startup. Stub: always empty.
static NAME_HASH_INDEX: OnceLock<NameHashIndex> = OnceLock::new();

/// Stub `show` — returns `NotFound` for unknown names. Real impl walks
/// `PolicyRegistry::lookup_policy` keyed on `(name, version)` via the
/// NameHashIndex.
pub fn show(name: &str, version: u32) -> Result<PolicyRecord, PolicyRegistryError> {
    let _ = version;
    Err(PolicyRegistryError::NotFound(name.to_string()))
}

/// Stub `list` — returns empty Vec.
pub fn list(_filter: &PolicyFilter) -> Result<Vec<PolicyListEntry>, PolicyRegistryError> {
    Ok(Vec::new())
}

/// Stub `latest_version` — returns `NotFound`.
pub fn latest_version(name: &str) -> Result<u32, PolicyRegistryError> {
    Err(PolicyRegistryError::NotFound(name.to_string()))
}

/// Get the (process-local) NameHashIndex. Real impl is a `OnceCell<NameHashIndex>`
/// populated from `PolicyRegistry` at startup. Stub: always empty.
pub fn name_hash_index() -> &'static NameHashIndex {
    NAME_HASH_INDEX.get_or_init(NameHashIndex::new)
}
