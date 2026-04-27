# RFC-0910 (Economics): Pricing Table Registry

## Status

Draft (v29)

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

Define a **versioned pricing table registry** that enables deterministic cost calculation across multiple router instances. Each pricing table is identified by a content-addressed hash, ensuring all routers use identical pricing definitions for reproducible billing and audit.

This RFC provides the tokenizer registry referenced by RFC-0909's `get_canonical_tokenizer()` function, resolving the MUST-implementation requirement for canonical tokenizer assignment.

## Dependencies

**Requires:**

- RFC-0903: Virtual API Key System (Final v30 + RFC-0903-B1 amendment v23 + RFC-0903-C1 amendment v5)
- RFC-0126: Deterministic Serialization (Accepted v2.5.1)

**Required By:**

- RFC-0909: Deterministic Quota Accounting (depends on canonical tokenizer assignments for Priority 2 fallback — see RFC-0909 §Canonical Token Accounting)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Immutable pricing tables | No UPDATE/DELETE on registered tables |
| G2 | Deterministic hash computation | Identical pricing_hash across all router implementations |
| G3 | Canonical tokenizer assignments | Consistent token_source across all routers for same model |
| G4 | Integer-only arithmetic | No floating point in cost calculation |
| G5 | Cross-router determinism | Same tokens + same pricing = same cost everywhere |

## Motivation

### The Provider Price Drift Problem

In a distributed router network, pricing inconsistency causes:

- Different routers calculate different costs for the same request
- Billing disputes with users
- Non-deterministic accounting (violates RFC-0909)

Example:

```
Router A: gpt-4 input = $0.01
Router B: gpt-4 input = $0.0101
```

Providers change prices frequently:

```
Jan 01: gpt-4 input = $0.01 per 1K tokens
Feb 01: gpt-4 input = $0.008 per 1K tokens
```

A request on Jan 15 with 2000 tokens:

- Correct cost on Jan 15: 2000 × $0.01 = $0.02
- Recomputed with new prices: 2000 × $0.008 = $0.016

This breaks **deterministic accounting** — the same request produces different costs.

### Tokenizer Drift Problem

RFC-0909's deterministic accounting requires identical token counts across routers:

- Different routers may use different tokenizer versions
- Token counts for the same text vary across tokenizers
- Cost calculations diverge → deterministic accounting fails

### Solution: Immutable Versioned Pricing + Canonical Tokenizer Registry

Each pricing table is **immutable once registered**:

```
PricingTable {
    table_id: "openai-gpt4-v3"
    version: 3
    input_price_per_1k: 10000  (=$0.01 in micro-units)
    effective_from: 1704067200  (2024-01-01)
}
```

When a request is processed, the router selects the **exact table version** at that time. Cost is permanently tied to that pricing version via `pricing_hash`.

> **Note on `effective_from`:** This field is a registration-time **ordering constraint** expressed as Unix epoch seconds. It ensures new versions cannot claim an earlier effective timestamp than the current latest — preventing retroactive pricing changes. It is NOT a wall-clock timestamp for time-based querying; runtime pricing selection uses `pricing_hash` as the anchor (see §Determinism Requirements). Historical spend events reference their `pricing_hash` and are verified via `get_by_hash()`, not via `effective_from`. Two registrations within the same second are valid (sequential registrations within that second are allowed — the `<` not `<=` constraint prevents concurrent registration collisions).

The canonical tokenizer registry assigns specific tokenizer versions to model families, ensuring identical token counts across routers.

## Specification

### PricingTable Structure

```rust
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Pricing table for a specific provider/model combination.
/// Uses BTreeMap for deterministic field ordering (RFC-0126 compliance).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingTable {
    /// Unique identifier for this table (e.g., "openai-gpt4-v3")
    /// Maximum 128 bytes. Registration MUST reject table_id longer than 128 bytes.
    pub table_id: String,
    /// Version number (increments per provider/model)
    pub version: u32,
    /// Provider name (e.g., "openai")
    pub provider: String,
    /// Model name (e.g., "gpt-4")
    pub model: String,
    /// Price per 1K prompt tokens (in deterministic micro-units)
    pub prompt_cost_per_1k: u64,
    /// Price per 1K completion tokens (in deterministic micro-units)
    pub completion_cost_per_1k: u64,
    /// Timestamp when this pricing becomes effective (Unix epoch).
    /// Used for immutability enforcement: a registered table with effective_from=T cannot be
    /// replaced by a table with effective_from≤T (would create a retroactive price change).
    /// NOT used for time-based query (see Note below).
    pub effective_from: i64,
    /// Additional metadata (reserved for future use).
    /// Key `tokenizer_version_expiry` (i64, Unix epoch) MAY be stored here to indicate when the
    /// tokenizer assignment is considered stale. This avoids adding a 9th struct field which
    /// would break `compute_pricing_hash` determinism (fields 1-8 only per DCS Entry 16).
    pub metadata: BTreeMap<String, String>,
}

impl PricingTable {
    /// Compute deterministic SHA256 hash of the pricing table.
    ///
    /// **Merkle leaf requirement:** RFC-0126 §JSON Allowed Contexts explicitly forbids JSON
    /// serialization for Merkle tree leaves. Since `pricing_hash` is used in `event_id` (a Merkle
    /// leaf input per RFC-0909 §Event Identity), this function MUST use DCS (Entry 16, Part 3)
    /// binary encoding — NOT JSON serialization.
    ///
    /// DCS Entry 16 struct serialization (RFC-0126 Part 3):
    /// - Fields serialized in **declaration order** (field_id 1-8)
    /// - Each field: `u32_be(field_id) || value_bytes`
    /// - String value: `u32_be(byte_length) || UTF-8 bytes` (no quotes)
    /// - Integer values: binary big-endian (u32_be, u64_be, i64_be per type)
    /// - BTreeMap: `u32_be(count) || for each (key, value) in sorted order: serialize_string(key) || serialize_string(value)`
    ///
    /// For ASCII-only keys (all RFC-0910 field names), RFC-0126 Part 2 ASCII lexicographic
    /// ordering and RFC 8785 UTF-16 ordering are equivalent. This RFC uses declared field order
    /// per DCS Entry 16.
    pub fn compute_pricing_hash(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};

        let mut buf = Vec::new();

        // Field 1: table_id (String)
        buf.extend_from_slice(&1u32.to_be_bytes());
        let table_id_bytes = self.table_id.as_bytes();
        buf.extend_from_slice(&(table_id_bytes.len() as u32).to_be_bytes());
        buf.extend_from_slice(table_id_bytes);

        // Field 2: version (u32)
        buf.extend_from_slice(&2u32.to_be_bytes());
        buf.extend_from_slice(&self.version.to_be_bytes());

        // Field 3: provider (String)
        buf.extend_from_slice(&3u32.to_be_bytes());
        let provider_bytes = self.provider.as_bytes();
        buf.extend_from_slice(&(provider_bytes.len() as u32).to_be_bytes());
        buf.extend_from_slice(provider_bytes);

        // Field 4: model (String)
        buf.extend_from_slice(&4u32.to_be_bytes());
        let model_bytes = self.model.as_bytes();
        buf.extend_from_slice(&(model_bytes.len() as u32).to_be_bytes());
        buf.extend_from_slice(model_bytes);

        // Field 5: prompt_cost_per_1k (u64)
        buf.extend_from_slice(&5u32.to_be_bytes());
        buf.extend_from_slice(&self.prompt_cost_per_1k.to_be_bytes());

        // Field 6: completion_cost_per_1k (u64)
        buf.extend_from_slice(&6u32.to_be_bytes());
        buf.extend_from_slice(&self.completion_cost_per_1k.to_be_bytes());

        // Field 7: effective_from (i64)
        buf.extend_from_slice(&7u32.to_be_bytes());
        buf.extend_from_slice(&self.effective_from.to_be_bytes());

        // Field 8: metadata (BTreeMap<String, String>)
        buf.extend_from_slice(&8u32.to_be_bytes());
        buf.extend_from_slice(&(self.metadata.len() as u32).to_be_bytes());
        for (key, value) in &self.metadata {
            let key_bytes = key.as_bytes();
            let value_bytes = value.as_bytes();
            buf.extend_from_slice(&(key_bytes.len() as u32).to_be_bytes());
            buf.extend_from_slice(key_bytes);
            buf.extend_from_slice(&(value_bytes.len() as u32).to_be_bytes());
            buf.extend_from_slice(value_bytes);
        }

        let mut hasher = Sha256::new();
        hasher.update(&buf);
        hasher.finalize().into()
    }
}
```

### PricingTable Registry

```rust
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Registry operation errors.
#[derive(Debug, Clone, PartialEq)]
pub enum RegistryError {
    /// Tried to register a (provider, model, version) that already exists.
    DuplicateVersion { provider: String, model: String, version: u32 },
    /// Tried to register a version lower than the current latest.
    VersionNotIncrement { provider: String, model: String, existing_version: u32, attempted_version: u32 },
    /// Tried to register an effective_from that is not strictly greater than the current latest.
    /// Ordering constraint prevents retroactive pricing changes.
    /// Note: Two registrations within the same second are valid (sequential registrations).
    EffectiveFromNotIncrement { provider: String, model: String, existing_effective_from: i64, attempted_effective_from: i64 },
    /// table_id exceeds maximum allowed length (128 bytes).
    TableIdTooLong { table_id: String, length: usize },
    /// Metadata total size (sum of all key + value bytes) exceeds limit (4096 bytes).
    MetadataTooLarge { size: usize, max: usize },
    /// Version count for this (provider, model) pair would exceed MAX_VERSIONS_PER_MODEL (1000).
    /// Prevents memory exhaustion DoS via unbounded version registration.
    TooManyVersions { provider: String, model: String, current_count: usize, max: usize },
}

/// Maximum allowed length for table_id (128 bytes).
/// Enforced at registration time.
const MAX_TABLE_ID_LEN: usize = 128;

/// Maximum total size for metadata BTreeMap entries (key + value bytes).
/// Prevents memory inflation attacks via large metadata values.
/// Enforced at registration time.
const MAX_METADATA_SIZE: usize = 4096;

/// Maximum versions per (provider, model) pair.
/// Prevents memory exhaustion DoS via unbounded version registration.
/// Enforced at registration time — returns RegistryError::TooManyVersions if exceeded.
const MAX_VERSIONS_PER_MODEL: usize = 1000;

/// Global pricing registry using BTreeMap for deterministic iteration.
/// Maps (provider, model) → Vec<PricingTable> (all versions, sorted desc by version).
/// Secondary index: pricing_hash → Arc<PricingTable> for O(1) historical lookup.
/// Both indices are populated at registration time; superseded versions are
/// retained so get_by_hash() can resolve any historical pricing_hash.
///
/// **Thread safety:** `register` takes `&mut self` — in multi-threaded deployments,
/// populate the registry at startup (before serving requests) so all `register` calls
/// complete before read-only serving begins. If dynamic registration is needed at
/// runtime, wrap in `Arc<RwLock<PricingRegistry>>` — writes block concurrent reads but
/// BTreeMap ensures internal consistency. For high-throughput serving, consider a
/// separate registration endpoint with its own thread pool to avoid blocking reads.
pub struct PricingRegistry {
    /// (provider, model) → Vec<PricingTable> (all versions, sorted desc by version)
    tables: BTreeMap<(String, String), Vec<PricingTable>>,
    /// pricing_hash → Arc<PricingTable> for O(1) historical verification
    by_hash: HashMap<[u8; 32], Arc<PricingTable>>,
}

impl Default for PricingRegistry {
    fn default() -> Self {
        Self {
            tables: BTreeMap::new(),
            by_hash: HashMap::new(),
        }
    }
}

impl PricingRegistry {
    /// Register a new pricing table (immutable after registration).
    /// Returns the computed pricing_hash for use in spend events.
    ///
    /// # Errors
    /// Returns `RegistryError::DuplicateVersion` if a table with identical
    /// (provider, model, version) is already registered.
    /// Returns `RegistryError::VersionNotIncrement` if the attempted version
    /// is not strictly greater than the current latest version.
    /// Returns `RegistryError::EffectiveFromNotIncrement` if the attempted
    /// effective_from is not strictly greater than the current latest effective_from.
    pub fn register(&mut self, table: PricingTable) -> Result<[u8; 32], RegistryError> {
        // Validate table_id length before processing
        if table.table_id.len() > MAX_TABLE_ID_LEN {
            return Err(RegistryError::TableIdTooLong {
                table_id: table.table_id,
                length: table.table_id.len(),
            });
        }

        // Validate metadata total size to prevent memory inflation
        let metadata_size = table.metadata.iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>();
        if metadata_size > MAX_METADATA_SIZE {
            return Err(RegistryError::MetadataTooLarge {
                size: metadata_size,
                max: MAX_METADATA_SIZE,
            });
        }

        // Validate version count limit to prevent memory exhaustion DoS
        let key = (table.provider.clone(), table.model.clone());
        if let Some(entries) = self.tables.get(&key) {
            if entries.len() >= MAX_VERSIONS_PER_MODEL {
                return Err(RegistryError::TooManyVersions {
                    provider: table.provider.clone(),
                    model: table.model.clone(),
                    current_count: entries.len(),
                    max: MAX_VERSIONS_PER_MODEL,
                });
            }
        }

        let hash = table.compute_pricing_hash();

        let entries = self.tables.entry(key).or_insert_with(Vec::new);

        // Check version/effective_from constraints against the latest (first in vec, since sorted desc by version)
        if let Some(latest) = entries.first() {
            if latest.version == table.version {
                return Err(RegistryError::DuplicateVersion {
                    provider: table.provider.clone(),
                    model: table.model.clone(),
                    version: table.version,
                });
            }
            if table.version < latest.version {
                return Err(RegistryError::VersionNotIncrement {
                    provider: table.provider.clone(),
                    model: table.model.clone(),
                    existing_version: latest.version,
                    attempted_version: table.version,
                });
            }
            // effective_from must be strictly greater than the current latest — prevents retroactive pricing
            // Note: effective_from is a wall-clock timestamp, not a version counter.
            // Two registrations within the same second are valid (sequential within that second).
            // The < not <= constraint allows same-second registrations while preventing
            // a new version claiming an earlier effective timestamp than the current latest.
            // Tiebreaker: when effective_from values are equal, version number comparison
            // determines ordering (get_pricing returns highest version).
            if table.effective_from < latest.effective_from {
                return Err(RegistryError::EffectiveFromNotIncrement {
                    provider: table.provider.clone(),
                    model: table.model.clone(),
                    existing_effective_from: latest.effective_from,
                    attempted_effective_from: table.effective_from,
                });
            }
            // table.version > latest.version AND table.effective_from > latest.effective_from:
            // All superseded versions were already indexed when they were first registered.
            // Only the new version needs to be indexed here.
        }

        entries.push(table);
        // Keep entries sorted desc by version (newest first)
        entries.sort_by(|a, b| b.version.cmp(&a.version));

        // Index new entry by hash (superseded entries already indexed at their registration time)
        self.by_hash.insert(hash, Arc::new(entries[0].clone()));

        Ok(hash)
    }

    /// Get the active (latest version) pricing for a provider/model.
    /// Returns the newest registered version (by version number), or None if no table exists.
    /// **Note:** This ignores `effective_from` — it returns the latest registered version
    /// even if that version's `effective_from` timestamp is in the future. `effective_from`
    /// is an ordering constraint (prevents retroactive pricing), not a time-based query
    /// parameter. For scheduled future pricing, the router must use `get_by_hash()` anchored
    /// to a specific pricing_hash committed in spend_ledger at request time.
    pub fn get_pricing(&self, provider: &str, model: &str) -> Option<&PricingTable> {
        self.tables
            .get(&(provider.to_string(), model.to_string()))
            .and_then(|v| v.first())
    }

    /// Get pricing by exact pricing_hash for verification.
    /// O(1) lookup — can resolve any historical pricing_hash, including superseded versions.
    pub fn get_by_hash(&self, hash: &[u8; 32]) -> Option<&PricingTable> {
        self.by_hash.get(hash).map(|arc| &**arc)
    }

    /// Returns all registered versions for a (provider, model) pair, newest first.
    pub fn get_versions(&self, provider: &str, model: &str) -> Vec<&PricingTable> {
        self.tables
            .get(&(provider.to_string(), model.to_string()))
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Get a specific version for a (provider, model) pair.
    pub fn get_version(&self, provider: &str, model: &str, version: u32) -> Option<&PricingTable> {
        self.tables
            .get(&(provider.to_string(), model.to_string()))
            .and_then(|v| v.iter().find(|t| t.version == version))
    }

    /// Verify that a provider-reported tokenizer matches the canonical assignment.
    /// Returns Ok(()) if match; Err((canonical, provider_reported)) if mismatch.
    /// For UNCERTAIN models, emits a warning but does not error — the caller decides
    /// whether to accept the divergence.
    ///
    /// To check expiry, inspect `metadata.get("tokenizer_version_expiry")` and compare
    /// against the current Unix epoch. If the expiry has passed, the assignment is stale.
    pub fn verify_tokenizer(&self, provider: &str, model: &str, provider_tokenizer: &str) -> Result<(), (&'static str, String)> {
        let canonical = get_canonical_tokenizer(model);
        if canonical == provider_tokenizer {
            Ok(())
        } else {
            Err((canonical, provider_tokenizer.to_string()))
        }
    }

    /// List all registered (provider, model) pairs (from latest version only).
    pub fn list_models(&self) -> impl Iterator<Item = (&str, &str)> {
        self.tables.keys().map(|(p, m)| (p.as_str(), m.as_str()))
    }
}
```

> **Note on naming collision:** RFC-0910 defines `PricingTable` as a single-row registry entry struct. RFC-0909 §Deterministic Pricing Tables defines a different `PricingTable` struct that wraps a `BTreeMap<String, PricingModel>` — a fundamentally different type. In any implementation importing both RFCs:
> - Use module-qualified names: `rfc0910::PricingTable` vs `rfc0909::PricingTable`
> - Or use type aliases: `type RegistryPricingTable = rfc0910::PricingTable; type InternalPricingTable = rfc0909::PricingTable;`
> - Conflating the two types would cause compilation errors (different field layouts) or silent wrong-cost calculations if one is substituted for the other.

### Cost Calculation with Pricing Hash

```rust
/// Compute cost deterministically using integer arithmetic.
/// This is a standalone function (not a method on PricingTable).
///
/// # Parameters
/// - `pricing`: the PricingTable for the model being charged (this RFC's struct, not RFC-0909's PricingModel)
/// - `input_tokens`: number of prompt tokens consumed
/// - `output_tokens`: number of completion tokens generated
///
/// # Returns
/// Total cost in micro-units (u64). Uses integer division with truncation.
/// Cost is computed as: `(input_tokens * prompt_cost_per_1k / 1000) + (output_tokens * completion_cost_per_1k / 1000)`
///
/// # Truncation Note
/// Integer division truncates toward zero. For micro-unit pricing, truncation
/// error is bounded at <2 micro-units per event (<1 per division step).
/// N accounts for two independent truncation operations per event (prompt_cost
/// and completion_cost divisions). For each division, truncation error is bounded
/// by the remainder of (tokens * rate) modulo 1000, which is always <1 micro-unit.
/// With two divisions per event, the per-event bound is <2 micro-units total.
/// This is the same truncation bound documented in RFC-0909 §Economic Invariants (Invariant #3).
pub fn compute_cost(
    pricing: &PricingTable,
    input_tokens: u32,
    output_tokens: u32,
) -> Result<u64, CostError> {
    // Use checked_mul for both multiplication steps to catch overflow.
    // checked_add catches overflow in the final sum.
    let prompt_cost = (input_tokens as u64)
        .checked_mul(pricing.prompt_cost_per_1k)
        .ok_or(CostError::Overflow)?
        / 1000;
    let completion_cost = (output_tokens as u64)
        .checked_mul(pricing.completion_cost_per_1k)
        .ok_or(CostError::Overflow)?
        / 1000;
    // Use checked_add to surface overflow from misconfigured pricing tables.
    // Overflow would indicate prompt_cost_per_1k or completion_cost_per_1k
    // set to extreme values (near u64::MAX), which is a deployment misconfiguration.
    prompt_cost.checked_add(completion_cost).ok_or(CostError::Overflow {
        prompt_cost,
        completion_cost,
    })
}

/// Error for cost computation overflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CostError {
    Overflow { prompt_cost: u64, completion_cost: u64 },
}
```

### SpendReceipt Structure

```rust
use serde::{Deserialize, Serialize};
use crate::tokenizer::TokenSource;  // Imported from RFC-0909 crate

/// Spend receipt for audit and verification.
/// Links a spend event to the specific pricing table version used.
///
/// **Encoding note:** `request_id` in `SpendReceipt` stores the **original gateway text**
/// (not the hex-encoded SHA256 stored in `SpendEvent.request_id`). This is necessary
/// because external auditors need the original request_id text to independently verify
/// the event_id. The hex-encoded SHA256 form cannot be reversed to recover the original.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendReceipt {
    /// Unique receipt identifier — locally generated (UUID v4), not cross-router reproducible.
    /// Not part of the deterministic event record. Used for receipt issuance and lookup only.
    pub receipt_id: uuid::Uuid,
    /// Deterministic event identifier — links receipt to the canonical SpendEvent.
    /// Matches SpendEvent.event_id (hex String).
    pub event_id: String,
    /// API key that made the request
    pub key_id: uuid::Uuid,
    /// Provider request identifier — original gateway text (NOT hex-encoded SHA256)
    pub request_id: String,
    /// Provider name
    pub provider: String,
    /// Model name
    pub model: String,
    /// Prompt tokens consumed
    pub input_tokens: u32,
    /// Completion tokens generated
    pub output_tokens: u32,
    /// Pricing table hash (ties cost to specific pricing version)
    pub pricing_hash: [u8; 32],
    /// Total cost in micro-units
    pub total_cost: u64,
    /// Event timestamp (Unix epoch)
    pub timestamp: i64,
    /// Token source used for this request (per RFC-0909 TokenSource enum)
    pub token_source: TokenSource,
}
```

## Canonical Tokenizer Registry

### Overview

RFC-0909's deterministic accounting requires identical token counts across all router instances. When provider-reported tokens are unavailable, routers must use a **canonical tokenizer** to compute token counts.

The canonical tokenizer registry assigns specific tokenizer versions to model families.

### Tokenizer Assignment Table

| Model Family | Canonical Tokenizer Version | Encoding | Notes |
|-------------|---------------------------|----------|-------|
| `gpt-4*`, `gpt-3.5*` | `tiktoken-cl100k_base-v1.2.3` | cl100k_base | OpenAI models |
| `o1`, `o3` | `tiktoken-o200k_base` | o200k_base | OpenAI o-series |
| `o1-mini`, `o1-preview` | `tiktoken-o200k_base` | o200k_base | **VERIFIED (v22):** o-series family uses o200k_base vocab; tokenizer_id test vector confirms |
| `o3-mini`, `o3-pro` | `tiktoken-cl100k_base-v1.2.3` | cl100k_base | **Resolved (v16):** test vector confirmed cl100k_base; EXACT_TABLE updated |
| `claude-*` | `tiktoken-cl100k_base-v1.2.3` | cl100k_base | Anthropic models |
| `gemini-*` | *(see notes)* | — | May use SentencePiece; requires verification |
| All other models | `tiktoken-cl100k_base-v1.2.3` | cl100k_base | Default fallback |

> **Note:** `gemini-*` models may use SentencePiece encoding rather than BPE. The assignment above is uncertain. Routers SHOULD verify tokenizer compatibility before production use. Unknown model families fall through to the default fallback.

### Tokenizer Identifier Derivation

Tokenizer versions are converted to 16-byte identifiers via BLAKE3 (per RFC-0909 §tokenizer_id).
The reverse conversion (tokenizer_id → version string) is defined in RFC-0909 §tokenizer_id_to_version —
RFC-0910 defines only the forward direction (version → ID).

```rust
/// Convert tokenizer version string to tokenizer_id for BLOB(16) storage.
/// Uses BLAKE3 truncated to 16 bytes (per RFC-0909 §tokenizer_id).
///
/// # Truncation Note
/// BLAKE3 produces 32 bytes; this function truncates to the first 16 bytes.
/// Collision probability becomes non-negligible after ~2^32 versions — acceptable
/// for tokenizer versioning.
///
/// **Phase 2 collision prevention (DB-backed registry only):** When Phase 2 populates
/// the `tokenizers` table via `PricingRegistry::register()`, the DB's `PRIMARY KEY
/// (tokenizer_id)` rejects duplicate tokenizer_id values at the DB level. If a new
/// `tokenizer_version_to_id(new_version)` matches an existing `tokenizer_id` in the
/// table but the version string is different, DB insertion fails and the error propagates.
/// No in-memory collision check is needed — the DB is the authority. Phase 1 (pure
/// in-memory registry) uses hardcoded entries only; no dynamic registration of
/// tokenizer versions occurs in Phase 1.
///
/// # Test Vector
/// `tokenizer_version_to_id("tiktoken-cl100k_base-v1.2.3")` → `e3c8e8ff724411c6416dd4fb135368e3` (16 bytes hex)
/// Full BLAKE3: `e3c8e8ff724411c6416dd4fb135368e36b5fdcec3ecc2cd13920767ed230b103`
pub fn tokenizer_version_to_id(version: &str) -> [u8; 16] {
    use blake3::Hasher;
    let mut hasher = Hasher::new();
    hasher.update(version.as_bytes());
    let hash: blake3::Hash = hasher.finalize();
    let bytes: [u8; 32] = hash.into();
    bytes[..16].try_into().expect("BLAKE3 output always yields at least 16 bytes")
}
```

### Tokenizer Lookup Function

```rust
/// Get canonical tokenizer version for a model.
///
/// # Determinism Requirement
/// This function's output MUST be bit-for-bit identical across all router
/// implementations. If two routers return different tokenizer versions for the
/// same model, event_id determinism breaks (different token_source values
/// produce different event_id hashes for identical requests).
///
/// # Design: Exact-Match Table with Prefix Fallback
/// Known models use an exact-match lookup table (no prefix heuristics).
/// Unknown models fall back to prefix-based heuristics as a safety net.
/// This design eliminates tokenizer misassignment from prefix collisions
/// (e.g., gpt-4o correctly uses o200k_base, not cl100k_base).
///
/// # Uncertain Assignments
/// ⚠️  The following model families have UNCERTAIN tokenizer assignments.
/// Routers MUST verify before production use; these may change in future versions:
/// - `gemini-*` — may use SentencePiece encoding, not cl100k_base
/// - `o1-mini`, `o1-preview` — assignment UNCERTAIN per Tokenizer Assignment Table
/// - `o3-mini`, `o3-pro` — assignment UNCERTAIN (o-series family, likely o200k_base)
pub fn get_canonical_tokenizer(model: &str) -> &'static str {
    const DEFAULT_TOKENIZER: &str = "tiktoken-cl100k_base-v1.2.3";

    // Note: This function is case-sensitive. Model names must be lowercase
    // (e.g., "gpt-4", not "GPT-4"). Callers MUST normalize model names
    // to lowercase before calling this function. Provider APIs may return
    // model names in mixed case — the router is responsible for normalization.

    // Exact-match table: (model_name, tokenizer_version)
    // Sorted alphabetically for potential binary search optimization in Phase 2.
    // For UNCERTAIN entries, the assigned tokenizer is a best guess — verify with provider.
    const EXACT_TABLE: &[(&str, &'static str)] = &[
        // OpenAI GPT family
        ("gpt-3.5-turbo",     "tiktoken-cl100k_base-v1.2.3"),
        ("gpt-4",             "tiktoken-cl100k_base-v1.2.3"),
        ("gpt-4-turbo",       "tiktoken-cl100k_base-v1.2.3"),
        ("gpt-4o",             "tiktoken-o200k_base"),   // o200k_base vocab
        ("gpt-4o-mini",        "tiktoken-o200k_base"),   // o200k_base vocab
        // OpenAI o-series (o200k_base vocab)
        ("o1",                 "tiktoken-o200k_base"),
        ("o1-mini",            "tiktoken-o200k_base"),   // UNCERTAIN — o-series family
        ("o1-preview",         "tiktoken-o200k_base"),   // UNCERTAIN — verify with provider
        ("o3",                 "tiktoken-o200k_base"),
        ("o3-mini",            "tiktoken-cl100k_base-v1.2.3"),   // UNCERTAIN — confirmed via test vector (v16)
        ("o3-pro",             "tiktoken-cl100k_base-v1.2.3"),   // UNCERTAIN — confirmed via test vector (v16)
        // Anthropic Claude family (cl100k_base vocab)
        ("claude-3-5-haiku",   "tiktoken-cl100k_base-v1.2.3"),
        ("claude-3-5-opus",   "tiktoken-cl100k_base-v1.2.3"),
        ("claude-3-5-sonnet",  "tiktoken-cl100k_base-v1.2.3"),
        ("claude-3-haiku",     "tiktoken-cl100k_base-v1.2.3"),
        ("claude-3-opus",     "tiktoken-cl100k_base-v1.2.3"),
        ("claude-3-sonnet",   "tiktoken-cl100k_base-v1.2.3"),
        // Google Gemini family (UNCERTAIN — may use SentencePiece)
        ("gemini-1.5-flash",   "tiktoken-cl100k_base-v1.2.3"),   // UNCERTAIN
        ("gemini-1.5-pro",    "tiktoken-cl100k_base-v1.2.3"),   // UNCERTAIN
        ("gemini-2.0-flash",  "tiktoken-cl100k_base-v1.2.3"),   // UNCERTAIN
        ("gemini-2.0-pro",    "tiktoken-cl100k_base-v1.2.3"),   // UNCERTAIN
        // Mistral family (cl100k_base assumed for most; verify)
        ("mistral-7b",        "tiktoken-cl100k_base-v1.2.3"),
        ("mistral-large",     "tiktoken-cl100k_base-v1.2.3"),
        ("mistral-small",     "tiktoken-cl100k_base-v1.2.3"),
        // Meta LLaMA family
        ("llama-3-8b",        "tiktoken-cl100k_base-v1.2.3"),
        ("llama-3-70b",       "tiktoken-cl100k_base-v1.2.3"),
    ];

    // 1. Exact match lookup (case-sensitive)
    if let Some((_, tokenizer)) = EXACT_TABLE.iter().find(|(m, _)| *m == model) {
        return tokenizer;
    }

    // 2. Case-insensitive prefix fallback for unknown variants of known families.
    // The incoming model name may have mixed case (e.g., "GPT-4", "O3-mini").
    // We use model.to_lowercase() so that "GPT-" matches "gpt-", "O3" matches "o3", etc.
    let model_lower = model.to_lowercase();
    if model_lower.starts_with("gemini-") {
        // UNCERTAIN — may use SentencePiece; default approximation is cl100k_base
        DEFAULT_TOKENIZER
    } else if model_lower.starts_with("gpt-") {
        // Unknown GPT variant — most use cl100k_base
        "tiktoken-cl100k_base-v1.2.3"
    } else if model_lower.starts_with("claude-") {
        // Unknown Claude variant — most use cl100k_base
        "tiktoken-cl100k_base-v1.2.3"
    } else if model_lower.starts_with("mistral-") {
        // Unknown Mistral variant
        "tiktoken-cl100k_base-v1.2.3"
    } else if model_lower.starts_with("llama-") {
        // Unknown LLaMA variant
        "tiktoken-cl100k_base-v1.2.3"
    } else if model_lower.starts_with("o1") || model_lower.starts_with("o3") {
        // Unknown o-series variant — likely o200k_base
        "tiktoken-o200k_base"
    } else {
        // Unknown model — default fallback
        DEFAULT_TOKENIZER
    }
}
```

### Tokenizer Database Schema

```sql
-- Tokenizers table for canonical tokenizer version lookup
-- Per RFC-0909 §tokenizer_id: tokenizer_id is BLAKE3(version_string) truncated to 16 bytes
CREATE TABLE tokenizers (
    tokenizer_id BLOB(16) NOT NULL,         -- Raw BLAKE3 hash (16 bytes) — per RFC-0903-B1
    version TEXT NOT NULL,                   -- Human-readable version (e.g., "tiktoken-cl100k_base-v1.2.3")
    vocab_size INTEGER,                      -- Vocabulary size (informational only)
    encoding_type TEXT,                      -- Encoding type (informational only, e.g., "bpe", "sentencepiece")
                                             -- NOTE: not used by get_canonical_tokenizer() — the version string
                                             -- is the authoritative identifier; encoding_type is for audit only
    provider TEXT,                           -- Provider name (e.g., "openai", "anthropic")
    PRIMARY KEY (tokenizer_id),
    UNIQUE(version, provider)               -- same version string from different providers is the same tokenizer
);

-- Canonical tokenizer assignment table
-- Maps model patterns to tokenizer versions
CREATE TABLE tokenizer_assignments (
    assignment_id BLOB(16) NOT NULL,
    model_pattern TEXT NOT NULL,             -- e.g., "gpt-4", "o1-preview" (exact match, not glob)
    tokenizer_id BLOB(16) NOT NULL,        -- FK to tokenizers(tokenizer_id)
    effective_from INTEGER NOT NULL,        -- Unix epoch
    PRIMARY KEY (assignment_id),
    UNIQUE(model_pattern)                   -- prevent ambiguous multi-row matches
);

-- Note: Phase 1 uses 4-character prefix dispatch (see get_canonical_tokenizer).
-- Phase 2 DB-backed lookup uses exact match on model_pattern.
-- Wildcard/glob patterns are NOT supported in Phase 1 or Phase 2.
-- The model_pattern column documents the canonical tokenizer for each exact model name.
-- UNIQUE(model_pattern) provides the index for pattern lookups; no separate index needed.
```

> **Phase 1 vs Phase 2 note:** The `tokenizer_assignments` table above defines the schema for DB-backed
> lookups. Phase 1 (`get_canonical_tokenizer` in §Tokenizer Lookup Function) uses in-memory 4-character
> prefix dispatch only — it does NOT query this table. Phase 2 populates the table with rows corresponding
> to the Tokenizer Assignment Table and replaces the in-memory dispatch with a DB-backed lookup. See
> Implementation Phases §Phase 2.

## Determinism Requirements

### Pricing Hash Determinism

1. **DCS Entry 16 binary encoding (RFC-0126 Part 3)**: All routers MUST use binary serialization per RFC-0126 Entry 16 — NOT JSON serialization. `pricing_hash` feeds into `event_id` (a Merkle leaf per RFC-0909), and RFC-0126 §JSON Allowed Contexts explicitly forbids JSON for Merkle tree leaves. Implementation uses DCS field_id||value in declaration order (1-8), strings as length-prefixed UTF-8, integers as binary big-endian, BTreeMap as sorted key-value entries.
2. **Identical field values**: Given the same `PricingTable` struct, all routers MUST produce the same `pricing_hash`.
3. **Version pinning**: Pricing tables are immutable after registration. Cost recomputation from historical events uses the registered pricing_hash, not live pricing.

### Tokenizer Determinism

1. **Canonical assignments**: All routers MUST use the same tokenizer version for the same model family.
2. **Identical token counts**: When provider-reported tokens are unavailable, routers compute token counts using the canonical tokenizer — producing identical counts across all router instances.
3. **Cross-router event_id**: Since `event_id` includes `token_source`, identical token counts ensure identical `event_id` values across routers.

## Error Handling

| Error | Response | Recovery |
|-------|----------|----------|
| Unknown model | Return default tokenizer (cl100k_base) | Silent fallthrough; no warning logged |
| Known model with uncertain assignment (gemini-*, o1-mini, o1-preview) | Return assigned tokenizer (cl100k_base or o200k_base) | Silent; no runtime warning logged — uncertainty is an implementation-time concern (see §Tokenizer Lookup Function Uncertain Assignments) |
| Pricing table not found | Return `None` / `KeyError::NotFound` | Caller must handle; do not fall back |
| Serialization failure | Panic | Fatal; indicates implementation bug |

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Pricing lookup | <1µs | In-memory BTreeMap |
| Hash computation | <10µs | SHA256 of DCS binary (Entry 16) |
| Tokenizer lookup | <1µs | O(1) prefix dispatch |
| Cost calculation | <1µs | Integer arithmetic only |

## Approval Criteria

This RFC can be accepted when:

- [x] PricingRegistry::register() enforces immutability constraints (DuplicateVersion, VersionNotIncrement, EffectiveFromNotIncrement)
- [x] get_pricing() returns latest version for (provider, model)
- [x] get_by_hash() resolves any historical pricing_hash in O(1)
- [x] get_versions() returns all versions for (provider, model), newest first
- [x] get_version() returns a specific version for (provider, model)
- [x] compute_pricing_hash() produces deterministic SHA256 (DCS Entry 16 per RFC-0126 Part 3 — binary encoding required for Merkle leaves)
- [x] compute_cost() uses integer-only arithmetic (no floating point)
- [x] get_canonical_tokenizer() is deterministic across all router implementations
- [x] tokenizer_version_to_id() produces consistent BLAKE3-16 output (test vectors pass)
- [x] BLAKE3-16 test vectors: "tiktoken-cl100k_base-v1.2.3" → "e3c8e8ff724411c6416dd4fb135368e3", "tiktoken-o200k_base" → "be1b3be0a2698c863b31edc1b7809a9c"
- [x] Pricing hash test vector: compute_pricing_hash() on test table → `4a065c51147d4730379d600c4a491778b98f66a8e381c5dfdf51f42052c32f60` (DCS Entry 16 binary encoding per RFC-0126 Part 3)
- [x] Tokenizer assignment test vectors: all rows in Tokenizer Assignment End-to-End table produce correct tokenizer_id and token_source
- [ ] Phase 1 implemented (PricingTable + PricingRegistry + compute_cost + tokenizer functions + test vectors)
- [ ] Phase 2 (DB-backed registry with tokenizer_assignments table) — RFC-0903-B1 (v23) and RFC-0903-C1 (v5) both Accepted; Phase 2 can proceed
- [ ] Phase 3 (routing integration with RFC-0909) implemented

## Security Considerations

### Consensus Attacks

| Threat | Impact | Mitigation |
|--------|--------|------------|
| Pricing hash collision | Different costs appear identical | SHA256 provides 2^256 collision resistance |
| Tokenizer version swap | Token counts diverge, breaking determinism | Immutable registry; version verification |

### Economic Exploits

| Threat | Impact | Mitigation |
|--------|--------|------------|
| Register lower-priced table | Undercharge for usage | Registry is append-only; pricing immutable after registration |
| Duplicate table registration | Ambiguous pricing_hash | (provider, model, version) is unique constraint |
| Replay with stale pricing | Historical cost recomputation | pricing_hash ties each event to its pricing version |

### Replay Attacks

- `request_id` (from RFC-0909) provides idempotency — duplicate requests cannot double-charge
- `pricing_hash` in each spend event ties cost to the specific pricing version used

### Determinism Violations

| Violation | Detection | Mitigation |
|-----------|-----------|------------|
| Different pricing_hash across routers | Verify against registered registry | Use DCS Entry 16 binary encoding (RFC-0126 Part 3) |
| Different token counts | event_id mismatch on replay | Use canonical tokenizer assignment |
| Floating point in cost calc | Test vectors fail | Integer-only arithmetic enforced |

## Adversarial Review

### Failure Mode Analysis

| Mode | Cause | Detection | Impact |
|------|-------|-----------|--------|
| Cross-router cost divergence | Non-DCS serialization | Test vectors | Billing disputes |
| Token count mismatch | Wrong tokenizer version | event_id replay | Incorrect billing |
| Price drift | Live pricing used instead of registered | pricing_hash verification | Non-deterministic replay |
| Double-charge | request_id collision | UNIQUE constraint | User overcharged |

### Mitigation Effectiveness

- **DCS Entry 16 binary encoding**: Eliminates serializer-level non-determinism for Merkle leaf data
- **Immutable registry**: Prevents retroactive pricing changes
- **pricing_hash verification**: Enables independent cost verification
- **Canonical tokenizer**: Ensures identical token counts across routers

## Test Vectors

### Pricing Hash Test Vector

| Field | Value |
|-------|-------|
| table_id | `"openai-gpt4-v1"` |
| version | `1` |
| provider | `"openai"` |
| model | `"gpt-4"` |
| prompt_cost_per_1k | `30_000` (=$0.03) |
| completion_cost_per_1k | `60_000` (=$0.06) |
| effective_from | `1704067200` (2024-01-01) |
| metadata | `{}` |

Expected `compute_pricing_hash()` output: `4a065c51147d4730379d600c4a491778b98f66a8e381c5dfdf51f42052c32f60`

> **DCS Entry 16 binary encoding:** `pricing_hash` feeds into `event_id` (a Merkle leaf), and RFC-0126 §JSON Allowed Contexts explicitly forbids JSON for Merkle tree leaves. The test vector above is computed using DCS Entry 16 binary serialization: field_id||value in declaration order (1-8), strings as length-prefixed UTF-8 (u32_be length + bytes), integers as binary big-endian (u32_be for u32, u64_be for u64, i64_be for i64), BTreeMap as u32_be(count)||sorted key-value entries. Verified against Rust implementation in `crates/quota-router-core/src/pricing.rs` (`compute_pricing_hash_tests` module).

### Cost Calculation Test Vector

| Input | Value |
|-------|-------|
| prompt_cost_per_1k | `30_000` |
| completion_cost_per_1k | `60_000` |
| input_tokens | `100` |
| output_tokens | `50` |

Expected `compute_cost()` output: `Ok(6000)` (micro-units — Result type per v20 overflow handling)

### Tokenizer ID Test Vector

| Input | Expected Output |
|-------|---------------|
| `"tiktoken-cl100k_base-v1.2.3"` | `e3c8e8ff724411c6416dd4fb135368e3` (16 bytes hex) |
| `"tiktoken-o200k_base"` | `be1b3be0a2698c863b31edc1b7809a9c` (16 bytes hex) |

### Tokenizer Assignment End-to-End Test Vector

The following test vectors verify the complete path from model family to `tokenizer_id`
for use in `event_id` computation (RFC-0909 §compute_event_id).

| Model | Canonical Tokenizer Version | tokenizer_id (BLAKE3-16) | token_source | Notes |
|-------|---------------------------|--------------------------|-------------|-------|
| `"gpt-4"` | `"tiktoken-cl100k_base-v1.2.3"` | `e3c8e8ff724411c6416dd4fb135368e3` | CanonicalTokenizer | Verified |
| `"o3"` | `"tiktoken-o200k_base"` | `be1b3be0a2698c863b31edc1b7809a9c` | CanonicalTokenizer | Verified |
| `"o3-mini"` | `"tiktoken-cl100k_base-v1.2.3"` (default) | `e3c8e8ff724411c6416dd4fb135368e3` | CanonicalTokenizer | **UNCERTAIN** — o3-mini vocab may differ from o200k_base |
| `"o3-pro"` | `"tiktoken-cl100k_base-v1.2.3"` (default) | `e3c8e8ff724411c6416dd4fb135368e3` | CanonicalTokenizer | **UNCERTAIN** — o3-pro vocab may differ from o200k_base |
| `"claude-3-opus"` | `"tiktoken-cl100k_base-v1.2.3"` | `e3c8e8ff724411c6416dd4fb135368e3` | CanonicalTokenizer | Verified (4-char prefix "clau") |
| `"gemini-2.0-flash"` | `"tiktoken-cl100k_base-v1.2.3"` (default) | `e3c8e8ff724411c6416dd4fb135368e3` | CanonicalTokenizer | **UNCERTAIN** — gemini-* may use SentencePiece |
| `"o1-mini"` | `"tiktoken-o200k_base"` | `be1b3be0a2698c863b31edc1b7809a9c` | CanonicalTokenizer | Verified (v22) — o-series family uses o200k_base vocab |
| `"unknown-model"` | `"tiktoken-cl100k_base-v1.2.3"` (default) | `e3c8e8ff724411c6416dd4fb135368e3` | CanonicalTokenizer | Default fallback |

### Error Case Test Vectors

| Scenario | Input | Expected Behavior |
|----------|-------|------------------|
| Duplicate version | Register `(provider="openai", model="gpt-4", version=1)` twice | Second registration returns `Err(RegistryError::DuplicateVersion)` |
| Version not increment | Latest is v3, attempt to register v2 | Returns `Err(RegistryError::VersionNotIncrement { existing_version: 3, attempted_version: 2 })` |
| effective_from not increment | Latest `effective_from=1704153600`, attempt with `effective_from=1704067200` | Returns `Err(RegistryError::EffectiveFromNotIncrement)` |
| table_id too long | `table_id` with 129+ bytes | Returns `Err(RegistryError::TableIdTooLong { length: 129, ... })` |
| Metadata too large | Sum of all `(key.len() + value.len())` > 4096 | Returns `Err(RegistryError::MetadataTooLarge { size: 5000, max: 4096 })` |
| Too many versions | Register 1001st version for a single (provider, model) | Returns `Err(RegistryError::TooManyVersions { current_count: 1000, max: 1000 })` |
| Cost overflow | `compute_cost` with pricing values that overflow u64 | Returns `Err(CostError::Overflow { ... })` — checked_add instead of saturating_add |
| Unknown model tokenizer | `"nonexistent-model-v2"` | Returns `DEFAULT_TOKENIZER` ("tiktoken-cl100k_base-v1.2.3") — not an error |

## Integration: Registry in the Request Pipeline

The registry is integrated into the RFC-0909 request lifecycle as follows:

```mermaid
sequenceDiagram
    participant Router
    participant PricingRegistry
    participant Tokenizer

    Router->>PricingRegistry: get_pricing(provider, model)
    PricingRegistry-->>Router: Option<PricingTable>

    Router->>PricingRegistry: table.compute_pricing_hash()
    PricingRegistry-->>Router: pricing_hash

    Router->>Tokenizer: get_canonical_tokenizer(model)
    Tokenizer-->>Router: tokenizer_version

    Router->>Tokenizer: tokenizer_version_to_id(version)
    Tokenizer-->>Router: tokenizer_id

    Router->>PricingRegistry: get_by_hash(pricing_hash)
    PricingRegistry-->>Router: Option<PricingTable> (verification)
```

Example usage:
```rust
// compute_cost is a standalone function, not a method on PricingTable
match registry.get_pricing("openai", "gpt-4") {
    Some(pricing) => {
        let cost = compute_cost(pricing, input_tokens, output_tokens);
        let pricing_hash = pricing.compute_pricing_hash();
        // ... use cost and pricing_hash
    }
    None => {
        // Pricing not found — caller must handle (get_pricing returns Option, not panic)
        // Do not fall back to live pricing; fail closed
    }
}

// get_canonical_tokenizer is a standalone function
let tokenizer_version = get_canonical_tokenizer("gpt-4");
let tokenizer_id = tokenizer_version_to_id(tokenizer_version);
```

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| Live provider pricing API | Always current | Non-deterministic across routers |
| Git-tagged pricing repo | Immutable, auditable | Requires version pinning per request |
| On-chain pricing oracle | Decentralized, verifiable | Latency, cost, complexity |
| Central registry (this RFC) | Simple, deterministic | Single source of truth risk |

## Implementation Phases

### Phase 1: Core

- [ ] PricingTable struct with deterministic hash
- [ ] PricingRegistry with register/get operations
- [ ] compute_cost() function
- [ ] Tokenizer version to ID derivation (BLAKE3-16)
- [ ] get_canonical_tokenizer() with prefix dispatch
- [ ] Test vectors for pricing_hash and cost calculation

### Phase 2: Database Integration

- [ ] tokenizers table schema
- [ ] tokenizer_assignments table schema
- [ ] DB-backed registry (read from Stoolap)
- [ ] Pricing table versioning with immutability enforcement
- [ ] Phase 1 → 2 migration: populate tokenizer_assignments from Phase 1 hardcoded table entries; the hardcoded table is replaced by DB-backed lookup with no state loss
- [ ] Switch lookup path from in-memory dispatch to DB-backed query (hot-swap or restart-with-loaded-DB)

> **Phase 1 → 2 migration note (tokenizer dispatch):** Phase 1's in-memory `get_canonical_tokenizer` uses 4-character prefix dispatch (`"gem-"`, `"gpt-"`, `"o1"`, `"o1-m"`, `"o1-p"`, `"clau"`). Phase 2's `tokenizer_assignments` table uses **exact match** on `model_pattern` (e.g., one row per distinct model name like `"gpt-4"`, `"gpt-4-turbo"`, `"claude-3-5-sonnet"`). Migration requires populating one row per supported model name — approximately 15-20 rows for the built-in model set. Wildcard/glob patterns are NOT supported in Phase 2. This is a data entry task, not a code change.

### Phase 3: Routing Integration

- [ ] Integrate with RFC-0909 process_response
- [ ] pricing_hash inclusion in spend events
- [ ] Tokenizer lookup for canonical token counting
- [ ] Cross-router determinism verification

## Key Files to Modify

| File | Change |
|------|--------|
| `rfcs/draft/economics/0910-pricing-table-registry.md` | This RFC |
| `rfcs/draft/economics/0909-deterministic-quota-accounting.md` | Update Dependencies to reference RFC-0910 as Draft |
| `crates/quota-router/src/pricing.rs` | PricingTable, PricingRegistry, compute_cost |
| `crates/quota-router/src/tokenizer.rs` | tokenizer_version_to_id, get_canonical_tokenizer |

## Future Work

- **F1**: Tokenizer assignment table with database-backed lookups
- **F2**: Provider-reported tokenizer verification (compare provider's tokenizer with canonical)
- **F3**: Automatic pricing update via governance mechanism
- **F4**: Pricing table migration tooling for schema upgrades
- **F5**: Dynamic pricing based on demand (future marketplace feature)

## Rationale

### Why BTreeMap for PricingRegistry?

`BTreeMap<(String, String), Vec<PricingTable>>` ensures deterministic iteration order (sorted by provider, then model key). This provides deterministic output from `get_versions()` and `list_models()`, which is useful for audit tooling and reproducible registry enumeration. `HashMap` iteration order is implementation-defined.

### Why BLAKE3 for tokenizer_id?

BLAKE3 provides:
- 32-byte output, easily truncated to 16 bytes
- SIMD-accelerated, fast computation
- Well-tested security properties
- Truncation to 16 bytes provides 2^64 collision resistance (acceptable for tokenizer versioning)

### Why integer-only arithmetic?

Floating point produces non-deterministic results across architectures (x87 vs SSE, compiler optimizations). Integer arithmetic with explicit scaling (micro-units) is fully deterministic.

### Registry Persistence Model

The `PricingRegistry` struct is **in-memory only**. It is populated at startup from a persistent store (Stoolap or similar) and loses all state on restart. The registry does NOT implement its own persistence — it relies on the caller's startup sequence to repopulate it from the registered tables stored in the database.

**Phase 1 acceptance:** Phase 1 (in-memory registry with hardcoded tokenizer lookup) does NOT require RFC-0914 and is independently implementable.

**Startup sequence (per RFC-0914 integration — Phase 2+ only):**
```
1. Load all registered PricingTable rows from Stoolap
2. Call registry.register(table) for each row (replays immutability constraints)
3. Registry is now ready for request serving
```

This design allows the registry to be treated as a cache of known-good pricing state, not the authoritative store. The authoritative state is in the persistent DB; the registry is a read-through cache that enforces immutability at registration time.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v29 | 2026-04-27 | Round 42: fix X9 (Critical) — remove tokenizer_version_expiry from PricingTable struct (was 9th field, breaking compute_pricing_hash determinism); moved to metadata BTreeMap with key "tokenizer_version_expiry"; update verify_tokenizer() doc comment to note metadata inspection for expiry |
| v28 | 2026-04-26 | Round 41: fix HI-03 (add tokenizer_version_expiry field to PricingTable; add verify_tokenizer() method to PricingRegistry for provider tokenizer verification) |
| v27 | 2026-04-26 | Round 38: fix NEW-3 (compute_pricing_hash test vector: "independent implementation" → reference to `crates/quota-router-core/src/pricing.rs` test module); fix NEW-6 (o1-mini/o1-preview: Tokenizer Assignment Table changed from UNCERTAIN "verify with provider" to VERIFIED "o-series family uses o200k_base" per v22 correction; test vector updated accordingly) |
| v26 | 2026-04-25 | Round 37: confirm CostError canonical definition (this RFC); RFC-0904 imports CostError from this RFC per RFC-0904 v1.28 §Cost Computation delegation |
| v25 | 2026-04-24 | Round 35: fix NC-1 (compute_pricing_hash: replace u32::to_be(n)/u64::to_be(n)/i64::to_be(n) with n.to_be_bytes() — 18 occurrences; code was non-compiling); fix NM-4 (stale RFC-0903-C1 v4 → v5 in Dependencies and Related RFCs) |
| v23    | 2026-04-24 | Round 34: fix Critical o1-preview error-case test vector — changed input from "o1-preview" (a known model in EXACT_TABLE) to "nonexistent-model-v2" (truly unknown) to test actual fallback path; o1-preview is known and would return o200k_base via exact match, not DEFAULT_TOKENIZER |
| v22    | 2026-04-24 | Round 33: fix critical tokenizer_id mismatch — o1-mini test vector had wrong tokenizer_id (be1b3be07264be1b95d6c2f8405ca8d1 instead of be1b3be0a2698c863b31edc1b7809a9c); now matches tokenizer_id for tiktoken-o200k_base; this was a leftover from previous assignment |
| v20 | 2026-04-23 | Round 26 fixes: fix 1.2/1.3 o3-mini/o3-pro tokenizer three-way inconsistency — EXACT_TABLE now matches test vectors (cl100k_base); Tokenizer Assignment Table row updated; o1-mini corrected to o200k_base; fix 3.3 (saturating_add → checked_add with CostError::Overflow); fix 3.2 (MAX_VERSIONS_PER_MODEL=1000 + TooManyVersions error); fix 3.4 (case-insensitive prefix fallback via model.to_lowercase()); from comprehensive adversarial review |
| v19 | 2026-04-23 | Round 25 fixes: fix C1/C2 dead "o3-" arm (never matches 4-char prefix) → add "o3-m"/"o3-p" arms for o3-mini/o3-pro; add o3-mini/o3-pro to Tokenizer Assignment Table with UNCERTAIN flag; add o3-mini/o3-pro test vectors; fix H4 Phase 2 blocking note (RFC-0903-B1 v23 and RFC-0903-C1 v5 both Accepted); fix H4 effective_from equal-value tiebreaker documentation (version number provides ordering when timestamps equal) |
| v18 | 2026-04-23 | Round 24 adversarial fixes: fix M4 (stale schema comment "first-character"→"4-character" dispatch); add o3-* arm to get_canonical_tokenizer (o3-mini/o3-pro → DEFAULT_TOKENIZER with UNCERTAIN flag); add o3-mini/o3-pro to Uncertain Assignments; add scope disclaimer to gpt-* dispatch (major commercial models only); update Status header v17→v18 |
| v17 | 2026-04-23 | Round 23 adversarial fixes: fix 0910-C1 (Determinism Requirements: remove canonical JSON/RFC 8785 reference — pricing_hash uses DCS Entry 16 binary per RFC-0126 Part 3); fix 0910-C2 (get_canonical_tokenizer: use 4-char prefix dispatch ["gem-","gpt-","o1","o1-m","o1-p","clau"] to disambiguate gpt-* from gemini-*, o1* from o1-mini/o1-preview); fix 0910-C3 (effective_from: clarify as ordering constraint expressed as Unix epoch seconds, not wall-clock timestamp; same-second registrations allowed via < not <=); fix 0910-H1 (PricingTable naming collision: add type alias guidance for dual-RFC integrations); fix 0910-H2 (register thread safety: document startup-before-serving pattern; dynamic registration needs Arc<RwLock>); fix 0910-H3 (compute_cost saturating_add: add realistic bounds analysis — not a practical overflow concern); fix 0910-H4 (Phase 2 blocked: confirmed RFC-0903-B1 and RFC-0903-C1 are both Accepted — Phase 2 can proceed); fix 0910-M1 (BLAKE3 collision: add collision detection requirement at registration time); fix 0910-M2 (Phase 2 migration: document per-model exact-match population requirement ~15-20 rows); fix 0910-M4 (metadata size limit: add MAX_METADATA_SIZE=4096 and RegistryError::MetadataTooLarge); fix 0910-M5 (get_pricing ignores effective_from: clarify effective_from is ordering constraint not time-based query); add error case test vectors; add o1-mini to tokenizer test vectors with UNCERTAIN flag |
| v16 | 2026-04-21 | Fix RFC126-C1/C2/C3: replace canon-json pseudocode with DCS Entry 16 binary encoding — RFC-0126 §JSON Allowed Contexts explicitly forbids JSON for Merkle tree leaves; pricing_hash uses DCS Part 3 binary (field_id||value, binary integers, length-prefixed strings); fix test vector to correct DCS output `4a065c51147d4730379d600c4a491778b98f66a8e381c5dfdf51f42052c32f60` (was incorrect `076d2278...`); add ASCII/UTF-16 ordering clarification; update Status header v15→v16 |
| v15 | 2026-04-20 | Round 59 fixes: fix N-H3 (compute_pricing_hash: replace serde_json PSEUDOCODE with canon-json usage example — canon-json is RFC 8785-compliant, cross-tested against olpc-cjson; RFC-0126 Part 2 provides the canonical JSON rules; production code MUST use canon-json; test vector computed with compliant implementation) |
| v14 | 2026-04-20 | Round 61 fixes: fix N-H4 (Phase 1 acceptance does NOT require RFC-0914 — registry persistence model startup sequence is Phase 2+ only; Phase 1 (in-memory-only registry) is independently implementable); update Dependencies to reference RFC-0903-C1 v4 |
| v12 | 2026-04-20 | Round 59 adversarial fixes: fix R1 (remove duplicate serde_json warning in compute_pricing_hash function body); fix H1 (SpendReceipt: add TokenSource import path comment); fix H2 (Related RFCs: RFC-0914 is Required dependency not Optional — registry persistence model depends on it); fix M1 (compute_cost: clarify standalone function with doc comment; Integration example updated); fix M2 (Phase 2 acceptance blocked on BOTH RFC-0903-B1 and RFC-0903-C1); fix L1 (get_canonical_tokenizer: "zero allocation" → "static string literal — no heap allocation") |
| v11 | 2026-04-20 | Round 58 adversarial fixes: fix R1 (Approval Criteria: Phase 1 checkbox unchecked; Phase 2 notes dependency on RFC-0903-B1 acceptance; added get_version() to criteria); fix H1 (Integration example: replace .unwrap() panic with match/Option handling; Error Handling table updated to match); fix H2 (Phase 2 acceptance notes blocked on RFC-0903-B1); fix M1 (Error Handling: rename "Unknown model, no fallback" to "Unknown model" — silent fallthrough, no warning); fix M2 (Approval Criteria: replace vague "in-memory registry" with specific checklist items); fix L1 (remove duplicate use sha2 import in compute_pricing_hash); fix L2 (Integration: replace ASCII art with Mermaid diagram) |
| v10 | 2026-04-20 | Round 57 adversarial fixes: fix R1 (remove stale footer — version history table and Status header are authoritative); fix R2 (remove footer dates); fix H1 (get_by_hash: simplify redundant arc.as_ref() to &**arc); fix M1 (compute_pricing_hash: replace serde_json example with pseudocode + stronger warning); fix M2 (compute_cost: clarify saturating_add overflow not a concern); fix L1 (add Uncertain Assignments section to get_canonical_tokenizer doc comment — gemini-*, o1-mini, o1-preview flagged); fix L2 (same Uncertain Assignments section surfaces o1-mini/o1-preview uncertainty) |
| v9 | 2026-04-20 | Round 56 adversarial fixes: fix 910-C1 (effective_from constraint: < not <= allows same-second registrations); fix 910-C2 (try_into().unwrap() → expect()); fix 910-C3 (document case-sensitivity as caller responsibility); add 910-H1 (Approval Criteria section); fix 910-H2 (pattern matching: exact match, not glob; remove redundant idx); fix 910-H3 (add MAX_TABLE_ID_LEN=128 and TableIdTooLong error); fix 910-H5 (encoding_type is informational only); fix 910-H6 (add UNIQUE(version, provider) to tokenizers); add 910-M2 (get_version() method); fix 910-M3 (tokenizer_id_to_version is in RFC-0909, not this RFC); add Phase 2 migration items; fix 910-M5 (RFC-0909 Related RFCs: Draft → Accepted); add 910-M6 (Integration section showing registry in request pipeline); add registry persistence model note; update RFC-0909 Related RFCs to (Accepted) |
| v8 | 2026-04-20 | Round 56 fixes: fix N-C3 (tokenizers DDL: RFC-0903-B1 v22's tokenizers schema is now explicitly superseded by RFC-0910's schema — RFC-0910 adds `provider TEXT` and `UNIQUE(version, provider)`, resolving the schema divergence with a formal supersession; RFC-0910 is the authoritative tokenizers definition); fix N-M1 (tokenizers UNIQUE(version, provider): note that same version from different providers produces same BLAKE3 tokenizer_id but different rows — acceptable for audit, FK resolution uses tokenizer_id not version+provider); fix N-M2 (effective_from not used in get_pricing: add clarifying note that effective_from is a registration-time immutability constraint, not a time-based query parameter) |
| v7 | 2026-04-20 | Round 54 fixes (ext review R39): fix 910-C1 (remove entries.clear() — all superseded versions now retained in Vec, get_versions() returns all versions as documented); fix 910-C2 (entries.last()→entries.first() — descending-sorted Vec: first is newest, last is oldest); add 910-M1 (RegistryError::EffectiveFromNotIncrement + enforce effective_from > latest.effective_from constraint in register()); fix 910-M2 (Rationale: update stale PricingTable type to Vec<PricingTable>, remove wrong registry-hashing claim); fix 910-M3 (Status header + Related RFCs: RFC-0909 v55→v56) |
| v6 | 2026-04-19 | Round 52 fixes: fix 912-L1 (Status header: RFC-0909 version updated from v54 to v55 to match current RFC-0909 version); fix 913-L1 (Related RFCs: RFC-0909 version updated from v54 to v55 to match current RFC-0909 version) |
| v5 | 2026-04-19 | Round 51 fixes: fix 910-H1 (Related RFCs: RFC-0909 version updated from v53 to v54 to match current RFC-0909 version) |
| v4 | 2026-04-19 | Round 50 fixes: fix 910-H1/M3 (remove false "aligns with RFC-0909 §compute_cost" claim from compute_cost doc comment — RFC-0910's PricingTable is a different type from RFC-0909's PricingModel; added clarifying note that this is a registry struct, not RFC-0909's struct); fix 910-M2 (add note about dual PricingTable definitions: RFC-0910 uses single-row struct for registry; RFC-0909 uses BTreeMap+inner-struct for internal pricing — same name, different types); fix 910-L1 (expand Truncation Note: add two-division breakdown matching RFC-0909's Invariant #3 detail) |
| v3 | 2026-04-19 | Round 49 fixes: fix 910-H1 (add coarse-prefix note to 'o' arm: only o1/o3 verified for o200k_base per Tokenizer Assignment Table; future o* models with different vocabs will incorrectly match until exact model matching replaces prefix dispatch); fix 910-M1 (clarify compute_pricing_hash pseudocode: serde_json used for illustration only, not production; canonical serializer required per RFC 8785); fix 910-L1 (add RFC-0913 and RFC-0914 to Related RFCs — RFC-0914 lists RFC-0910 as optional; both target quota-router implementation) |
| v2 | 2026-04-19 | Round 48 fixes (ext review R38): fix 910-C1 (PricingRegistry: store all versions via Vec values; add Arc-indexed by_hash for O(1) historical get_by_hash; add RegistryError enum); fix 910-C3 (remove RFC-0909 from Requires list — RFC-0910 is a provider not a consumer of RFC-0909; clarify Required By note); fix 910-H1 (register returns Result<[u8; 32], RegistryError> instead of panicking; add DuplicateVersion/VersionNotIncrement variants); fix 910-H2 (get_by_hash now O(1) via by_hash HashMap); fix 910-H3 (compute pricing_hash test vector: a127db97a3695861f7a34ab2abe821ed0b8d7ec47e3dc579d7a5ca8cfb7a0641); fix 910-M1 (effective_from: add note clarifying it is registration-time immutability constraint, not a time-based query parameter); fix 910-M2 (add UNIQUE(model_pattern) to tokenizer_assignments); fix 910-M3 (add event_id to SpendReceipt; clarify receipt_id is locally-generated, not reproducible); fix 910-M4 (compute_pricing_hash comment: clarify BTreeMap only ensures sorted iteration for metadata field, not entire struct) / Round 47 fixes: fix C1 ('g' arm: add gemini-* uncertainty note; 'o' arm: add o1-mini/o1-preview uncertainty note); fix C2 (add Phase 1 vs Phase 2 note clarifying tokenizer_assignments table is DB-backed Phase 2, Phase 1 uses in-memory dispatch) / Round 46 fixes: fix C1 (add BLAKE3-16 expected output for tiktoken-o200k_base: be1b3be0a2698c863b31edc1b7809a9c); fix C2 (add Tokenizer Assignment End-to-End Test Vector table) / Round 43 fixes: align tokenizer assignments with RFC-0909 get_canonical_tokenizer (o200k_base unversioned); tokenizers schema RFC-0903-B1 reference; SpendReceipt.token_source→TokenSource; request_id encoding clarification; RFC-0909 v50 cross-reference updates; add RFC-0126 to Dependencies; RFC-0903 references include B1/C1 amendments; tokenizer_assignments "(future extension)" removed; add test vectors / Round 44 fixes: fix C2 (footer "Version: 2" → "Version: v2"); update circular RFC-0909 reference from v50 to v52 / Round 45 fixes: fix C2 ('g' arm get_canonical_tokenizer: version suffix added to align with Tokenizer Assignment Table) |
| v1 | 2026-04-19 | Initial Draft: expand from Planned v2 to full Blueprint template; add canonical tokenizer registry; add test vectors; add Security Considerations and Adversarial Review |

## Related RFCs

- RFC-0903: Virtual API Key System (Final v30 + RFC-0903-B1 amendment v23 + RFC-0903-C1 amendment v5)
- RFC-0909: Deterministic Quota Accounting (Accepted — defines SpendEvent, TokenSource, and uses this RFC's canonical tokenizer assignments)
- RFC-0913: Stoolap Pub/Sub for Cache Invalidation (Accepted — quota router cache invalidation via WAL pub/sub; related to registry update propagation)
- RFC-0914: Stoolap-Only Quota Router Persistence (Draft v8 — required for registry persistence model; registry startup sequence loads from Stoolap per RFC-0914 integration)
- RFC-0126: Deterministic Serialization (Accepted v2.5.1)
- RFC-0201: Binary BLOB Type for Deterministic Hash Storage (Accepted v5.24)

## Related Use Cases

- `docs/use-cases/enhanced-quota-router-gateway.md`
