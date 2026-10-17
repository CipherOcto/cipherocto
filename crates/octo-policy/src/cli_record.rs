//! Layer B `[ADD]` CLI record types per RFC-0011 §Subcommand Taxonomy.

#![allow(clippy::module_name_repetitions)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Policy record (CLI-visible form, field-aligned to substrate `RegisteredPolicy`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRecord {
    pub name: String,
    pub kind_uuid: [u8; 16],
    /// Canonical policy body (CBOR / trait-spec bytes).
    pub body: Vec<u8>,
    pub execution_class: String,
    pub registered_by_did: String,
    pub registered_at_unix: i64,
    pub revoked_at_unix: Option<i64>,
    pub revoked_by_did: Option<String>,
    pub revocation_reason: Option<String>,
    pub superseding_policy_hash: Option<[u8; 32]>,
}

/// Policy list entry (CLI-visible form for `policy list`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyListEntry {
    pub name: String,
    pub kind_uuid: [u8; 16],
    pub execution_class: String,
    pub version: u32,
    pub policy_hash: [u8; 32],
}

/// Filter for `policy list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyFilter {
    pub kind: Option<String>,
    pub execution_class: Option<String>,
}

/// Name → hash index (CLI-side; substrate uses content-hash registry,
/// CLI adds the (name, version) → policy_hash lookup layer).
#[derive(Debug, Default, Clone)]
pub struct NameHashIndex {
    pub by_name: BTreeMap<String, Vec<(u32, [u8; 32])>>,
}

impl NameHashIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve `(name, version)` → `policy_hash`.
    /// Returns `None` if name not registered or version not found.
    pub fn resolve(&self, name: &str, version: Option<u32>) -> Option<[u8; 32]> {
        let versions = self.by_name.get(name)?;
        match version {
            None => versions.iter().max_by_key(|(v, _)| *v).map(|(_, h)| *h),
            Some(v) => versions.iter().find(|(ver, _)| *ver == v).map(|(_, h)| *h),
        }
    }
}
