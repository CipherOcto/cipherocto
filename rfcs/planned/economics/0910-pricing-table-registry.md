# RFC-0910 (Economics): Pricing Table Registry

## Status

Planned (v2 - canonical tokenizer)

## Authors

- Author: @cipherocto

## Summary

Define a **versioned pricing table registry** that enables deterministic cost calculation across multiple router instances. Each pricing table is identified by a hash, ensuring all routers use identical pricing definitions for reproducible billing.

## Dependencies

**Requires:**

- RFC-0903: Virtual API Key System
- RFC-0909: Deterministic Quota Accounting

**Optional:**

- RFC-0900: AI Quota Marketplace Protocol

## Motivation

In a distributed router network, pricing inconsistency causes:

- Different routers calculate different costs for the same request
- Billing disputes with users
- Non-deterministic accounting (violates RFC-0909)

Example problem:

```
Router A: gpt-4 input = $0.01
Router B: gpt-4 input = $0.0101
```

### The Provider Price Drift Problem

Most AI gateways calculate cost using **live provider price tables**. Providers change prices frequently:

```
Jan 01: gpt-4 input = $0.01 per 1K tokens
Feb 01: gpt-4 input = $0.008 per 1K tokens
```

A request on Jan 15 with 2000 tokens:

- Correct cost on Jan 15: 2000 × $0.01 = $0.02
- Recomputed with new prices: 2000 × $0.008 = $0.016

This breaks **deterministic accounting** — the same request produces different costs.

**Distributed router drift:**

- Router A updates price config
- Router B has not
- Same request: Router A = $0.016, Router B = $0.020
- Determinism is destroyed

### Solution: Immutable Versioned Pricing

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

### Design Goals

1. **Immutable tables**: Once registered, pricing cannot change
2. **Versioned**: New prices = new table version
3. **Hash-verified**: `pricing_hash` in spend receipts proves which table was used
4. **Deterministic**: Same tokens + same pricing = same cost everywhere

### PricingTable Structure

```rust
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingTable {
    pub table_id: String,
    pub version: u32,
    pub provider: String,
    pub model: String,
    /// Price per 1K input tokens (in deterministic micro-units)
    pub input_price_per_1k: u64,
    /// Price per 1K output tokens (in deterministic micro-units)
    pub output_price_per_1k: u64,
    /// Timestamp when this pricing becomes effective
    pub effective_from: i64,
    /// Metadata for the table
    pub metadata: BTreeMap<String, String>,
}

impl PricingTable {
    /// Compute deterministic hash of the pricing table
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        // Serialize deterministically - field order matters
        hasher.update(self.table_id.as_bytes());
        hasher.update(self.version.to_le_bytes());
        hasher.update(self.provider.as_bytes());
        hasher.update(self.model.as_bytes());
        hasher.update(self.input_price_per_1k.to_le_bytes());
        hasher.update(self.output_price_per_1k.to_le_bytes());
        hasher.update(self.effective_from.to_le_bytes());
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }
}
```

### PricingTable Registry

```rust
pub struct PricingRegistry {
    tables: HashMap<String, PricingTable>,
    active_table_hash: [u8; 32],
}

impl PricingRegistry {
    /// Register a new pricing table
    pub fn register(&mut self, table: PricingTable) -> [u8; 32] {
        let hash = table.hash();
        let key = format!("{}:{}:{}", table.provider, table.model, table.version);
        self.tables.insert(key, table);
        hash
    }

    /// Get active pricing for a provider/model
    pub fn get_pricing(&self, provider: &str, model: &str) -> Option<&PricingTable> {
        self.tables.get(&format!("{}:{}", provider, model))
    }

    /// Verify a pricing hash matches active table
    pub fn verify_hash(&self, hash: &[u8; 32]) -> bool {
        &self.active_table_hash == hash
    }
}
```

### Cost Calculation with Pricing Hash

```rust
/// Calculate cost using a specific pricing table
pub fn calculate_cost(
    table: &PricingTable,
    input_tokens: u32,
    output_tokens: u32,
) -> u64 {
    // Deterministic integer math - no floating point
    let input_cost = (input_tokens as u64 * table.input_price_per_1k) / 1000;
    let output_cost = (output_tokens as u64 * table.output_price_per_1k) / 1000;
    input_cost + output_cost
}

/// Spend receipt includes pricing hash for audit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendReceipt {
    pub receipt_id: Uuid,
    pub key_id: Uuid,
    pub request_id: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub pricing_hash: [u8; 32],  // Tie cost to specific pricing version
    pub total_cost: u64,
    pub timestamp: i64,
}
```

## Canonical Tokenizer Registry

RFC-0903 and RFC-0909 require a **canonical tokenizer** for deterministic token counting when providers do not return usage metadata.

```rust
/// Canonical tokenizer specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalTokenizer {
    /// Unique identifier
    pub tokenizer_id: String,
    /// Implementation name (e.g., "tiktoken", "huggingface")
    pub implementation: String,
    /// Specific model/encoding (e.g., "cl100k_base")
    pub encoding: String,
    /// Version string
    pub version: String,
    /// SHA256 hash of the tokenizer binary for verification
    pub binary_hash: [u8; 32],
    /// Effective from timestamp
    pub effective_from: i64,
}

impl CanonicalTokenizer {
    /// Current recommended tokenizer
    pub fn current() -> Self {
        Self {
            tokenizer_id: "tiktoken-cl100k_base-v1.2.3".to_string(),
            implementation: "tiktoken".to_string(),
            encoding: "cl100k_base".to_string(),
            version: "1.2.3".to_string(),
            binary_hash: [
                // SHA256 of tiktoken v0.5.1 binary - to be verified at runtime
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ],
            effective_from: 1704067200, // 2024-01-01
        }
    }
}
```

**Determinism rule:**

```
All routers MUST use the canonical tokenizer version specified above
when provider-reported tokens are unavailable.
```

**Tokenizer version pinning:**

```rust
const CANONICAL_TOKENIZER_VERSION: &str = "tiktoken-cl100k_base-v1.2.3";
```

This ensures identical token counts across all router instances.

## Why Needed

- **Deterministic billing**: All routers use same prices
- **Audit trail**: Spend receipts reference specific pricing versions
- **Dispute resolution**: Users can verify costs against published tables
- **Multi-router networks**: Critical for horizontal scaling
- **Price drift prevention**: Prevents cost recalculation errors when providers change prices

### Why Price Drift Breaks Determinism

Without versioned pricing, recomputing costs from historical events produces different results:

```
Original calculation (Jan 15): 2000 tokens × $0.01 = $0.02
Recalculated (Feb 01):         2000 tokens × $0.008 = $0.016
```

This violates **deterministic replay** — the core requirement for verifiable accounting.

### Integration with RFC-0903 Spend Events

The pricing hash from RFC-0903's spend event recording should reference this registry:

```rust
pub struct SpendEvent {
    // ... existing fields ...
    pub pricing_hash: [u8; 32],  // References immutable PricingTable
}
```

This creates a complete audit chain:

1. Request → 2. Token usage → 3. Pricing table selected → 4. Cost computed → 5. Event recorded with pricing_hash

## Out of Scope

- Real-time price updates (handled by registry operator)
- Provider negotiation (future marketplace feature)
- Dynamic pricing based on volume (future)

## Approval Criteria

- [ ] PricingTable structure defined with deterministic hash
- [ ] Registry supports version lookup and hash verification
- [ ] SpendReceipt includes pricing_hash field
- [ ] Cost calculation uses integer arithmetic only
