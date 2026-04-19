# RFC-0910 (Economics): Pricing Table Registry

## Status

Draft (v2 — aligns with RFC-0903 Final v29 + RFC-0903-B1 v22 + RFC-0903-C1 v3 + RFC-0909 v52)

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

Define a **versioned pricing table registry** that enables deterministic cost calculation across multiple router instances. Each pricing table is identified by a content-addressed hash, ensuring all routers use identical pricing definitions for reproducible billing and audit.

This RFC provides the tokenizer registry referenced by RFC-0909's `get_canonical_tokenizer()` function, resolving the MUST-implementation requirement for canonical tokenizer assignment.

## Dependencies

**Requires:**

- RFC-0903: Virtual API Key System (Final v29 + RFC-0903-B1 amendment v22 + RFC-0903-C1 amendment v3)
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

> **Note on `effective_from`:** This field is a registration-time **immutability constraint** — a new version with `effective_from` earlier than the current latest would retroactively change historical pricing. It is NOT a time-based query parameter. Runtime pricing selection uses `pricing_hash` as the anchor (see §Determinism Requirements). Historical spend events reference their `pricing_hash` and are verified via `get_by_hash()`, not via `effective_from`.

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
    /// Additional metadata (reserved for future use)
    pub metadata: BTreeMap<String, String>,
}

impl PricingTable {
    /// Compute deterministic SHA256 hash of the pricing table.
    /// ⚠️  This requires a canonical JSON serializer (RFC 8785, e.g., serde_json_raw crate).
    ///
    /// BTreeMap determinism scope: The `metadata: BTreeMap` field guarantees sorted iteration
    /// for that field's key-value pairs. The struct's other fields (`table_id`, `version`,
    /// `provider`, `model`, `prompt_cost_per_1k`, `completion_cost_per_1k`, `effective_from`)
    /// are serialized in **declaration order** by serde_json — this order is NOT specified by Rust
    /// and may vary across compiler versions. A canonical JSON serializer (RFC 8785) MUST be used
    /// to ensure identical output across implementations. The test vector below is computed
    /// with an RFC 8785-compliant implementation and MUST be matched exactly.
    ///
    /// ⚠️  This requires a canonical JSON serializer (RFC 8785, e.g., serde_json_raw crate).
    /// serde_json field ordering is NOT guaranteed across compiler versions.
    /// All router implementations MUST use the same canonical JSON library.
    pub fn compute_pricing_hash(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};

        // ⚠️  Example only — NOT for production. See comment above.
        let serialized = serde_json::to_string(&self)
            .expect("PricingTable serialization must succeed");
        let mut hasher = Sha256::new();
        hasher.update(serialized.as_bytes());
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
}

/// Global pricing registry using BTreeMap for deterministic iteration.
/// Maps (provider, model) → Vec<PricingTable> (all versions, sorted desc by version).
/// Secondary index: pricing_hash → Arc<PricingTable> for O(1) historical lookup.
/// Both indices are populated at registration time; superseded versions are
/// retained so get_by_hash() can resolve any historical pricing_hash.
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
    pub fn register(&mut self, table: PricingTable) -> Result<[u8; 32], RegistryError> {
        let key = (table.provider.clone(), table.model.clone());
        let hash = table.compute_pricing_hash();

        let entries = self.tables.entry(key).or_insert_with(Vec::new);

        // Check version constraints against the latest (last in vec, since sorted desc)
        if let Some(latest) = entries.last() {
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
            // table.version > latest.version: index ALL superseded entries by their hashes
            for superseded in entries.iter() {
                let h = superseded.compute_pricing_hash();
                self.by_hash.insert(h, Arc::new(superseded.clone()));
            }
            entries.clear();
        }

        entries.push(table);
        // Keep entries sorted desc by version (newest first)
        entries.sort_by(|a, b| b.version.cmp(&a.version));

        // Index new entry by hash
        self.by_hash.insert(hash, Arc::new(entries[0].clone()));

        Ok(hash)
    }

    /// Get the active (latest version) pricing for a provider/model.
    /// Returns the newest registered version, or None if no table exists.
    pub fn get_pricing(&self, provider: &str, model: &str) -> Option<&PricingTable> {
        self.tables
            .get(&(provider.to_string(), model.to_string()))
            .and_then(|v| v.first())
    }

    /// Get pricing by exact pricing_hash for verification.
    /// O(1) lookup — can resolve any historical pricing_hash, including superseded versions.
    pub fn get_by_hash(&self, hash: &[u8; 32]) -> Option<&PricingTable> {
        self.by_hash.get(hash).map(|arc| arc.as_ref())
    }

    /// Returns all registered versions for a (provider, model) pair, newest first.
    pub fn get_versions(&self, provider: &str, model: &str) -> Vec<&PricingTable> {
        self.tables
            .get(&(provider.to_string(), model.to_string()))
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// List all registered (provider, model) pairs (from latest version only).
    pub fn list_models(&self) -> impl Iterator<Item = (&str, &str)> {
        self.tables.keys().map(|(p, m)| (p.as_str(), m.as_str()))
    }
}
```

### Cost Calculation with Pricing Hash

```rust
/// Compute cost deterministically using integer arithmetic.
/// Name and semantics align with RFC-0909 §compute_cost.
///
/// # Parameters
/// - `pricing`: the PricingTable for the model being charged
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
pub fn compute_cost(
    pricing: &PricingTable,
    input_tokens: u32,
    output_tokens: u32,
) -> u64 {
    let prompt_cost = (input_tokens as u64 * pricing.prompt_cost_per_1k) / 1000;
    let completion_cost = (output_tokens as u64 * pricing.completion_cost_per_1k) / 1000;
    prompt_cost.saturating_add(completion_cost)
}
```

### SpendReceipt Structure

```rust
use serde::{Deserialize, Serialize};

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
| `o1-mini`, `o1-preview` | *(see notes)* | — | Verify with provider |
| `claude-*` | `tiktoken-cl100k_base-v1.2.3` | cl100k_base | Anthropic models |
| `gemini-*` | *(see notes)* | — | May use SentencePiece; requires verification |
| All other models | `tiktoken-cl100k_base-v1.2.3` | cl100k_base | Default fallback |

> **Note:** `gemini-*` models may use SentencePiece encoding rather than BPE. The assignment above is uncertain. Routers SHOULD verify tokenizer compatibility before production use. Unknown model families fall through to the default fallback.

### Tokenizer Identifier Derivation

Tokenizer versions are converted to 16-byte identifiers via BLAKE3 (per RFC-0909):

```rust
/// Convert tokenizer version string to tokenizer_id for BLOB(16) storage.
/// Uses BLAKE3 truncated to 16 bytes (per RFC-0909 §tokenizer_id).
///
/// # Truncation Note
/// BLAKE3 produces 32 bytes; this function truncates to the first 16 bytes.
/// Collision probability becomes non-negligible after ~2^32 versions — acceptable
/// for tokenizer versioning.
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
    bytes[..16].try_into().unwrap()
}
```

### Tokenizer Lookup Function

```rust
/// Get canonical tokenizer version for a model family.
/// Returns static str reference — zero allocation.
///
/// # Determinism Requirement
/// This function's output MUST be bit-for-bit identical across all router
/// implementations. If two routers return different tokenizer versions for the
/// same model, event_id determinism breaks (different token_source values
/// produce different event_id hashes for identical requests).
///
/// # Implementation Notes
/// - This function is the single source of truth for canonical tokenizer assignment
/// - Routers MUST NOT use local estimation or provider-reported tokenizer names
/// - The prefix-match dispatch is O(1) per call
/// - Unknown model families fall through to the default fallback
pub fn get_canonical_tokenizer(model: &str) -> &'static str {
    const DEFAULT_TOKENIZER: &str = "tiktoken-cl100k_base-v1.2.3";

    match model.chars().next() {
        'g' => {
            // ⚠ 'g' prefix matches BOTH gpt-* (GPT) and gemini-* (uncertain).
            // This arm uses cl100k_base as an approximation for GPT models.
            // gemini-* may use SentencePiece (not cl100k_base) — assignment is UNCERTAIN.
            // For gemini-* production use, verify tokenizer compatibility before deployment.
            // See Tokenizer Assignment Table §gemini-* note.
            "tiktoken-cl100k_base-v1.2.3"  // version aligned with Tokenizer Assignment Table
        },
        'o' => {
            // o1, o3 — OpenAI o-series with o200k_base vocab (VERIFIED)
            // o1-mini, o1-preview — DIFFERENT vocab from o200k_base; assignment UNCERTAIN.
            // See Tokenizer Assignment Table §o1-mini/o1-preview note.
            "tiktoken-o200k_base"
        },
        'c' => {
            // claude-* family — uses cl100k_base (Anthropic BPE)
            "tiktoken-cl100k_base-v1.2.3"
        },
        _ => DEFAULT_TOKENIZER, // Unknown: fall through to default
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
    vocab_size INTEGER,                      -- Vocabulary size (optional)
    encoding_type TEXT,                      -- Encoding type (e.g., "bpe", "sentencepiece")
    provider TEXT,                           -- Provider name (e.g., "openai", "anthropic")
    PRIMARY KEY (tokenizer_id)
);

CREATE UNIQUE INDEX idx_tokenizers_version ON tokenizers(version);

-- Canonical tokenizer assignment table
-- Maps model patterns to tokenizer versions
CREATE TABLE tokenizer_assignments (
    assignment_id BLOB(16) NOT NULL,
    model_pattern TEXT NOT NULL,             -- e.g., "gpt-4*", "claude-3*"
    tokenizer_id BLOB(16) NOT NULL,         -- FK to tokenizers(tokenizer_id)
    effective_from INTEGER NOT NULL,        -- Unix epoch
    PRIMARY KEY (assignment_id),
    UNIQUE(model_pattern)                   -- prevent ambiguous multi-row matches
);

CREATE INDEX idx_tokenizer_assignments_pattern ON tokenizer_assignments(model_pattern);
```

> **Phase 1 vs Phase 2 note:** The `tokenizer_assignments` table above defines the schema for DB-backed
> lookups. Phase 1 (`get_canonical_tokenizer` in §Tokenizer Lookup Function) uses in-memory first-character
> prefix dispatch only — it does NOT query this table. Phase 2 populates the table with rows corresponding
> to the Tokenizer Assignment Table and replaces the in-memory dispatch with a DB-backed lookup. See
> Implementation Phases §Phase 2.

## Determinism Requirements

### Pricing Hash Determinism

1. **Canonical JSON serialization**: All routers MUST use RFC 8785-compliant canonical JSON. `serde_json` field ordering is NOT guaranteed.
2. **Identical field values**: Given the same `PricingTable` struct, all routers MUST produce the same `pricing_hash`.
3. **Version pinning**: Pricing tables are immutable after registration. Cost recomputation from historical events uses the registered pricing_hash, not live pricing.

### Tokenizer Determinism

1. **Canonical assignments**: All routers MUST use the same tokenizer version for the same model family.
2. **Identical token counts**: When provider-reported tokens are unavailable, routers compute token counts using the canonical tokenizer — producing identical counts across all router instances.
3. **Cross-router event_id**: Since `event_id` includes `token_source`, identical token counts ensure identical `event_id` values across routers.

## Error Handling

| Error | Response | Recovery |
|-------|----------|----------|
| Unknown model, no fallback | Use default tokenizer | Log warning; proceed |
| Pricing table not found | Return `None` / `KeyError::NotFound` | Caller must handle; do not fall back |
| Canonical tokenizer unknown | Use default fallback | Log warning; proceed |
| Serialization failure | Panic | Fatal; indicates implementation bug |

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Pricing lookup | <1µs | In-memory BTreeMap |
| Hash computation | <10µs | SHA256 of canonical JSON |
| Tokenizer lookup | <1µs | O(1) prefix dispatch |
| Cost calculation | <1µs | Integer arithmetic only |

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
| Different pricing_hash across routers | Verify against registered registry | Use canonical JSON serializer |
| Different token counts | event_id mismatch on replay | Use canonical tokenizer assignment |
| Floating point in cost calc | Test vectors fail | Integer-only arithmetic enforced |

## Adversarial Review

### Failure Mode Analysis

| Mode | Cause | Detection | Impact |
|------|-------|-----------|--------|
| Cross-router cost divergence | Non-canonical JSON serializer | Test vectors | Billing disputes |
| Token count mismatch | Wrong tokenizer version | event_id replay | Incorrect billing |
| Price drift | Live pricing used instead of registered | pricing_hash verification | Non-deterministic replay |
| Double-charge | request_id collision | UNIQUE constraint | User overcharged |

### Mitigation Effectiveness

- **Canonical JSON**: Eliminates serializer-level non-determinism
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

Expected `compute_pricing_hash()` output: `a127db97a3695861f7a34ab2abe821ed0b8d7ec47e3dc579d7a5ca8cfb7a0641`

> **Canonical JSON input:** `{"table_id":"openai-gpt4-v1","version":1,"provider":"openai","model":"gpt-4","prompt_cost_per_1k":30000,"completion_cost_per_1k":60000,"effective_from":1704067200,"metadata":{}}` (RFC 8785 canonical form — definition-order fields, compact separators, minimal number representation)

### Cost Calculation Test Vector

| Input | Value |
|-------|-------|
| prompt_cost_per_1k | `30_000` |
| completion_cost_per_1k | `60_000` |
| input_tokens | `100` |
| output_tokens | `50` |

Expected `compute_cost()` output: `3000 + 3000 = 6000` micro-units

### Tokenizer ID Test Vector

| Input | Expected Output |
|-------|---------------|
| `"tiktoken-cl100k_base-v1.2.3"` | `e3c8e8ff724411c6416dd4fb135368e3` (16 bytes hex) |
| `"tiktoken-o200k_base"` | `be1b3be0a2698c863b31edc1b7809a9c` (16 bytes hex) |

### Tokenizer Assignment End-to-End Test Vector

The following test vectors verify the complete path from model family to `tokenizer_id`
for use in `event_id` computation (RFC-0909 §compute_event_id).

| Model | Canonical Tokenizer Version | tokenizer_id (BLAKE3-16) | token_source |
|-------|---------------------------|--------------------------|-------------|
| `"gpt-4"` | `"tiktoken-cl100k_base-v1.2.3"` | `e3c8e8ff724411c6416dd4fb135368e3` | CanonicalTokenizer |
| `"o3"` | `"tiktoken-o200k_base"` | `be1b3be0a2698c863b31edc1b7809a9c` | CanonicalTokenizer |
| `"claude-3-opus"` | `"tiktoken-cl100k_base-v1.2.3"` | `e3c8e8ff724411c6416dd4fb135368e3` | CanonicalTokenizer |
| `"gemini-2.0-flash"` | `"tiktoken-cl100k_base-v1.2.3"` (fallback) | `e3c8e8ff724411c6416dd4fb135368e3` | CanonicalTokenizer |
| `"unknown-model"` | `"tiktoken-cl100k_base-v1.2.3"` (default) | `e3c8e8ff724411c6416dd4fb135368e3` | CanonicalTokenizer |

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

`BTreeMap<(String, String), PricingTable>` ensures deterministic iteration order (sorted by provider, then model). This is required for consistent `pricing_hash` computation when the registry itself is hashed. `HashMap` iteration order is implementation-defined.

### Why BLAKE3 for tokenizer_id?

BLAKE3 provides:
- 32-byte output, easily truncated to 16 bytes
- SIMD-accelerated, fast computation
- Well-tested security properties
- Truncation to 16 bytes provides 2^64 collision resistance (acceptable for tokenizer versioning)

### Why integer-only arithmetic?

Floating point produces non-deterministic results across architectures (x87 vs SSE, compiler optimizations). Integer arithmetic with explicit scaling (micro-units) is fully deterministic.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v2 | 2026-04-19 | Round 48 fixes (ext review R38): fix 910-C1 (PricingRegistry: store all versions via Vec values; add Arc-indexed by_hash for O(1) historical get_by_hash; add RegistryError enum); fix 910-C3 (remove RFC-0909 from Requires list — RFC-0910 is a provider not a consumer of RFC-0909; clarify Required By note); fix 910-H1 (register returns Result<[u8; 32], RegistryError> instead of panicking; add DuplicateVersion/VersionNotIncrement variants); fix 910-H2 (get_by_hash now O(1) via by_hash HashMap); fix 910-H3 (compute pricing_hash test vector: a127db97a3695861f7a34ab2abe821ed0b8d7ec47e3dc579d7a5ca8cfb7a0641); fix 910-M1 (effective_from: add note clarifying it is registration-time immutability constraint, not a time-based query parameter); fix 910-M2 (add UNIQUE(model_pattern) to tokenizer_assignments); fix 910-M3 (add event_id to SpendReceipt; clarify receipt_id is locally-generated, not reproducible); fix 910-M4 (compute_pricing_hash comment: clarify BTreeMap only ensures sorted iteration for metadata field, not entire struct) / Round 47 fixes: fix C1 ('g' arm: add gemini-* uncertainty note; 'o' arm: add o1-mini/o1-preview uncertainty note); fix C2 (add Phase 1 vs Phase 2 note clarifying tokenizer_assignments table is DB-backed Phase 2, Phase 1 uses in-memory dispatch) / Round 46 fixes: fix C1 (add BLAKE3-16 expected output for tiktoken-o200k_base: be1b3be0a2698c863b31edc1b7809a9c); fix C2 (add Tokenizer Assignment End-to-End Test Vector table) / Round 43 fixes: align tokenizer assignments with RFC-0909 get_canonical_tokenizer (o200k_base unversioned); tokenizers schema RFC-0903-B1 reference; SpendReceipt.token_source→TokenSource; request_id encoding clarification; RFC-0909 v50 cross-reference updates; add RFC-0126 to Dependencies; RFC-0903 references include B1/C1 amendments; tokenizer_assignments "(future extension)" removed; add test vectors / Round 44 fixes: fix C2 (footer "Version: 2" → "Version: v2"); update circular RFC-0909 reference from v50 to v52 / Round 45 fixes: fix C2 ('g' arm get_canonical_tokenizer: version suffix added to align with Tokenizer Assignment Table) |
| v1 | 2026-04-19 | Initial Draft: expand from Planned v2 to full Blueprint template; add canonical tokenizer registry; add test vectors; add Security Considerations and Adversarial Review |

## Related RFCs

- RFC-0903: Virtual API Key System (Final v29 + RFC-0903-B1 amendment v22 + RFC-0903-C1 amendment v3)
- RFC-0909: Deterministic Quota Accounting (Draft v52)
- RFC-0126: Deterministic Serialization (Accepted v2.5.1)
- RFC-0201: Binary BLOB Type for Deterministic Hash Storage (Accepted v5.24)

## Related Use Cases

- `docs/use-cases/enhanced-quota-router-gateway.md`

---

**Version:** v2
**Draft Date:** 2026-04-19
**Last Updated:** 2026-04-19
