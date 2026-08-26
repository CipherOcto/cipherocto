---
rfc: 0105-v3.5
title: Per-Asset Scale Table + AssetRegistry Side-Table (Asset-Generic Asset Metadata)
status: Accepted
version: 3.5
date: 2026-08-26
amends: RFC-0105 v3.4
builds_on:
  - rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md
  - rfcs/accepted/economics/0105-v34-private-asset-namespace.md
---

# RFC-0105 v3.5 — Per-Asset Scale Table + AssetRegistry Side-Table

## 0. Status

**Accepted (v3.5, 2026-08-26).** Amendment to RFC-0105 v3.4 (Private Asset ID Namespace). Round 6 of multi-round adversarial review.

**Promotion trail:** R1-R9 multi-round adversarial review 2026-08-25 → DRY closure at R9 2026-08-26 → Accepted 2026-08-26 per BLUEPRINT.md RFC process. R1-R9 = 5-lens reviews (substrate-fidelity / cross-RFC consistency / security / naming / spec-completeness); loop-until-DRY pattern reached 2 consecutive zero-finding rounds (R8=2 LOW, R9=0) per closure audit docs/audits/asset-generic-payment-caveat-review-DRY-2026-08-26.md.

**Substrate anchor:** Frozen `AssetId(pub [u8; 32])` at `crates/octo-vault/src/lib.rs:136` is unchanged (Layer A substrate). NEW `AssetRegistry` side-table proposed at `crates/octo-vault/src/asset_registry.rs` (GREENFIELD — NEW file, not yet on disk; Layer B additive, semver-minor) holds per-asset metadata `(scale: u8, display_decimals: u8, denomination: String, kind: AssetKind, governance_pubkey: Option<[u8; 32]>, version: u64, prev_commitment: Option<[u8; 32]>, tombstone: bool)`. Apply the same GREENFIELD marker to §3.1 (AssetRegistry), §3.6 (BridgeIdentityRegistry), §3.5 (CachedAssetRegistry).

**Substrate-fidelity note (Round 2):** the substrate comment at `crates/octo-cap-macaroon/src/caveat/payment.rs:24` states "Amount-bearing fields use `octo_determin::Dqa` directly with `scale = 0` enforced at the substrate boundary." The substrate today hardcodes wire scale=0. This amendment is the migration that introduces per-asset wire scales.

## 1. Motivation

The substrate `AssetId(pub [u8; 32])` at `crates/octo-vault/src/lib.rs:136` is a 32-byte opaque digest with NO metadata about the asset's decimal scale, denomination, or kind. The substrate enforces wire scale=0 at every boundary (see `crates/octo-cap-macaroon/src/caveat/payment.rs:24` comment); `MICRO_PER_OCTOW` at `crates/quota-router-storage/src/ask.rs:41` is `Dqa { value: 1_000_000, scale: 0 }`.

This gap forces four downstream consequences:

1. `PaymentCaveat` (`crates/octo-cap-macaroon/src/caveat/payment.rs:55`) has no `asset_id` field — the budget is implicitly OCTO-W at wire scale 0.
2. `amount_dqa_micros: i64` at `crates/octo-policy/src/policy_kinds.rs:263` (on `SelectorContext`, not on a burn-event type) is an i64 carrier that assumes wire scale 0 universally.
3. Bridged assets (e.g., a BTC mirror at 8 decimals) cannot be expressed.
4. Naming sites still use "micro-OCTO" terminology (`MICRO_PER_OCTOW`, `amount_dqa_micros`) after the 2026-08-17 `MicroOctoW` alias retirement.

This amendment adds a **per-asset scale table** (sovereign role tokens + private asset class + bridged + wrapped) and an **`AssetRegistry` side-table** that holds the metadata. The frozen `AssetId(pub [u8; 32])` substrate shape is UNCHANGED.

## 2. Per-Asset Scale Table

### 2.1 Sovereign namespace (RFC-0105 §2.1, with display + wire-scale columns)

| Asset    | Derivation (BLAKE3-256 input, 32-byte output) | Display decimal places | Display denomination | Wire scale (current substrate) | Wire scale (post-migration) |
| -------- | --------------------------------------------- | ---------------------- | -------------------- | ------------------------------ | --------------------------- |
| `OCTO-A` | `b"cipherocto/asset/v1/" ‖ "OCTO-A"`          | 6                      | micro-OCTO-A         | 0                              | 0 (uniform with current)    |
| `OCTO-B` | `b"cipherocto/asset/v1/" ‖ "OCTO-B"`          | 6                      | micro-OCTO-B         | 0                              | 0                           |
| `OCTO-D` | `b"cipherocto/asset/v1/" ‖ "OCTO-D"`          | 6                      | micro-OCTO-D         | 0                              | 0                           |
| `OCTO-H` | `b"cipherocto/asset/v1/" ‖ "OCTO-H"`          | 6                      | micro-OCTO-H         | 0                              | 0                           |
| `OCTO-M` | `b"cipherocto/asset/v1/" ‖ "OCTO-M"`          | 6                      | micro-OCTO-M         | 0                              | 0                           |
| `OCTO-N` | `b"cipherocto/asset/v1/" ‖ "OCTO-N"`          | 6                      | micro-OCTO-N         | 0                              | 0                           |
| `OCTO-O` | `b"cipherocto/asset/v1/" ‖ "OCTO-O"`          | 6                      | micro-OCTO-O         | 0                              | 0                           |
| `OCTO-S` | `b"cipherocto/asset/v1/" ‖ "OCTO-S"`          | 6                      | micro-OCTO-S         | 0                              | 0                           |
| `OCTO-W` | `b"cipherocto/asset/v1/" ‖ "OCTO-W"`          | 6                      | micro-OCTO-W         | 0                              | 0                           |

**Rationale:** the substrate today (post-2026-08-17 `MicroOctoW` retirement) enforces wire scale=0 at every boundary. The display decimal places column is a UX concern, not a wire concern. Post-migration, sovereign role tokens STAY at wire scale 0 (matches existing `MICRO_PER_OCTOW = 1_000_000` at scale 0). This avoids off-by-10^6 settlement errors against existing wallet/quota-router code that reads scale 0 micro-OCTO-W.

**Per-asset denomination:** every sovereign role token uses an asset-qualified display denomination (`micro-OCTO-W`, `micro-OCTO-B`, …) matching the substrate convention at `crates/quota-router-storage/src/ask.rs:40` ("micro-OCTO-W") and `crates/octo-network/src/porelay/economics.rs:134,137` ("micro-OCTO-N", "micro-OCTO-B"). A wallet reading `AssetRegistry::metadata` MUST use the per-asset denomination to disambiguate balances across role tokens.

**Display-name collision note (Round 1 security review Threat #5, partial mitigation):** BLAKE3-256 outputs are uniform random (per-pair preimage collision probability ≈ 2^-256 — negligible). However, a corporate-chain operator can register `PRIVATE-<chain_id>-OCTO-W` whose human-readable form LOOKS like the sovereign OCTO-W. Wallet UI MUST display both the asset_id (truncated hex) AND the human-readable form, with a warning when the human-readable form matches a sovereign namespace pattern inside a non-sovereign namespace.

### 2.2 Private namespace (RFC-0105 v3.4 §2.2, with scale column)

| Asset pattern                             | Derivation (BLAKE3-256 input, 32-byte output)                         | Display decimal places | Display denomination | Wire scale (post-migration)   |
| ----------------------------------------- | --------------------------------------------------------------------- | ---------------------- | -------------------- | ----------------------------- |
| `PRIVATE-{chain_id_32B-hex}-{asset_name}` | `b"cipherocto/asset/v1/" ‖ "PRIVATE-{chain_id_32B-hex}-{asset_name}"` | per-asset              | per-asset            | per-asset (registry-resolved) |

Examples (illustrative): `PRIVATE-<chain_id_hex>-USDC-MIRROR` (scale 6, denomination "micro-USDC"); `PRIVATE-<chain_id_hex>-BTC-MIRROR` (scale 8, denomination "satoshi"); `PRIVATE-<chain_id_hex>-ETH-MIRROR` (scale 18, denomination "wei").

### 2.3 Bridged external asset namespace (NEW in v3.5)

| Asset pattern                              | Derivation (BLAKE3-256 input, 32-byte output)                          | Display decimal places | Display denomination | Wire scale (post-migration)   |
| ------------------------------------------ | ---------------------------------------------------------------------- | ---------------------- | -------------------- | ----------------------------- |
| `BRIDGED-{bridge_id_32B-hex}-{asset_name}` | `b"cipherocto/asset/v1/" ‖ "BRIDGED-{bridge_id_32B-hex}-{asset_name}"` | per-asset              | per-asset            | per-asset (registry-resolved) |

Examples: `BRIDGED-<valid_bridge_id_hex>-WBTC` (scale 8, denomination "satoshi").

**Rationale:** bridged assets (e.g., a wBTC mirror issued by an external bridge contract) need a separate namespace to distinguish from sovereign role tokens and corporate-chain private assets. The `BRIDGED-{bridge_id_32B-hex}-{asset_name}` pattern makes the bridge binding explicit in the asset_id derivation input. **Bridge identity is NOT trusted at the asset_id derivation step** — it must be verified by the Bridge Identity Registry (§3.6 below).

### 2.4 Wrapped cross-chain asset namespace (NEW in v3.5)

| Asset pattern                                                | Derivation (BLAKE3-256 input, 32-byte output)                                            | Display decimal places | Display denomination | Wire scale (post-migration)   |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------- | ---------------------- | -------------------- | ----------------------------- |
| `WRAPPED-{bridge_id_32B-hex}-{origin_chain_id}-{asset_name}` | `b"cipherocto/asset/v1/" ‖ "WRAPPED-{bridge_id_32B-hex}-{origin_chain_id}-{asset_name}"` | per-asset              | per-asset            | per-asset (registry-resolved) |

**`origin_chain_id` typed value (NEW in v3.5):** the free-form string is replaced by an enumerated `BridgeChainNamespace` value (rename from earlier draft `ChainNamespace` — the substrate already has `pub enum ChainNamespace { Rfc, User }` at `crates/octo-policy/src/policy_kinds.rs:54-70` with `as_byte()` 0x01/0x02, so the new type MUST be named differently to satisfy the "no parallel abstractions" principle):

```rust
// crates/octo-vault/src/bridge_chain_namespace.rs (NEW)
pub enum BridgeChainNamespace {
    Mainnet,
    Testnet,
    Devnet,
    Sidechain,    // L2 / app-chain
    Other,        // free-form, requires explicit registration
}
```

Examples: `WRAPPED-<bridge_id_hex>-Mainnet-WBTC` (scale 8, denomination "satoshi").

**Rationale:** replacing the free-form origin_chain_id with an enumerated value closes the wrapped-asset-forgery vector identified in the Round 1 security review (Round 1 CRITICAL #3). `Other` requires explicit chain-namespace governance registration; mainnet/testnet/devnet are canonical entries.

## 3. AssetRegistry Side-Table

**AssetRegistry GREENFIELD path (Round 3 anchor):** `crates/octo-vault/src/asset_registry.rs` (NEW — not yet on disk). All consumer RFCs (RFC-0965, RFC-0960, RFC-0959) cascade-break if this path changes. The GREENFIELD marker applies to §3.1 (AssetRegistry), §3.5 (CachedAssetRegistry), and §3.6 (BridgeIdentityRegistry).

### 3.1 Substrate definition (NEW, Layer B additive)

```rust
// crates/octo-vault/src/asset_registry.rs (NEW file)

use crate::{AssetId, BridgeChainNamespace, ChainId};

fn hex_lower(bytes: &[u8]) -> String { /* lower-case hex; substrate-defined */ bytes.iter().map(|b| format!("{:02x}", b)).collect() }

pub const MAX_SCALE: u8 = 18;     // ETH upper bound (industry convention)

pub enum AssetKind {
    SovereignRoleToken,        // OCTO-A/B/D/M/N/O/S/H/W
    PrivateCorporateAsset,     // PRIVATE-{chain_id}-{asset_name}
    BridgedExternalAsset,      // BRIDGED-{bridge_id}-{asset_name}
    WrappedCrossChainAsset,    // WRAPPED-{bridge_id}-{origin_chain_id}-{asset_name}
}

pub struct AssetMetadata {
    pub wire_scale: u8,                      // 0..=MAX_SCALE; on-wire magnitude scale (RFC-0862 §Substrate types)
    pub display_decimals: u8,                // human display denominator (10^display_decimals == 1 display unit)
    pub denomination: String,                // asset-qualified human-readable unit ("micro-OCTO-W", "satoshi", "wei", ...) — DISPLAY form, NOT used in derivation
    pub symbol: String,                      // CANONICAL derivation tag ("OCTO-W", "BRIDGED-{bridge}-{name}", "PRIVATE-{chain}-{name}", "WRAPPED-{bridge}-{origin}-{name}")
    pub kind: AssetKind,
    pub governance_pubkey: Option<[u8; 32]>, // optional; REQUIRED for non-sovereign assets (see register())
    pub chain_id: ChainId,                   // RFC-0967-A1 ChainId; used in namespace_tag() for PrivateCorporateAsset and WrappedCrossChainAsset
    pub asset_name: String,                  // human-readable asset label ("USDC-MIRROR", "WBTC", ...); used in namespace_tag()
    pub version: u64,                        // monotonic per-asset; bumped on each registration event
    pub prev_commitment: Option<[u8; 32]>,   // previous version's commitment hash (chain-of-trust)
    pub tombstone: bool,                     // revoked; historical events still resolve, new events REJECT
}

impl AssetMetadata {
    /// Canonical namespace-tag string used as the BLAKE3-256 derivation input
    /// for `AssetId::derive(namespace_tag) -> AssetId`.
    /// Format: `b"cipherocto/asset/v1/" || kind_specific_tag`.
    /// The derivation input uses `symbol` (CANONICAL) and `chain_id`/`asset_name` (per-kind),
    /// NOT `denomination` (which is the DISPLAY form only).
    pub fn namespace_tag(&self) -> Vec<u8> {
        let kind_specific = match &self.kind {
            AssetKind::SovereignRoleToken => self.symbol.clone(),                                  // e.g. "OCTO-W"
            AssetKind::PrivateCorporateAsset => format!("PRIVATE-{}", hex_lower(&self.chain_id.0)), // "PRIVATE-<chain_hex>"
            AssetKind::BridgedExternalAsset => format!("BRIDGED-{}", self.symbol),                 // symbol carries "BRIDGED-{bridge}-{name}"
            AssetKind::WrappedCrossChainAsset => format!("WRAPPED-{}", self.symbol),               // symbol carries "WRAPPED-{bridge}-{origin}-{name}"
        };
        // Append `-{asset_name}` for PrivateCorporateAsset only (sovereign uses symbol directly,
        // bridged/wrapped carry the name inside `symbol`).
        let kind_specific = match &self.kind {
            AssetKind::PrivateCorporateAsset => format!("{}-{}", kind_specific, self.asset_name),
            _ => kind_specific,
        };
        let mut out = Vec::with_capacity(20 + kind_specific.len());
        out.extend_from_slice(b"cipherocto/asset/v1/");
        out.extend_from_slice(kind_specific.as_bytes());
        out
    }

    /// Returns the canonical byte tag for this asset's kind (used in body_hash and namespace_tag derivation).
    /// fires when `self.kind` is read for body_hash commitment or wire-form encoding.
    pub fn kind_tag(&self) -> u8 {
        match &self.kind {
            AssetKind::SovereignRoleToken => 0x01,
            AssetKind::PrivateCorporateAsset => 0x02,
            AssetKind::BridgedExternalAsset => 0x03,
            AssetKind::WrappedCrossChainAsset => 0x04,
        }
    }
}

pub trait AssetRegistry {
    fn metadata(&self, asset_id: &AssetId) -> Result<AssetMetadata, AssetError>;
    fn register(&mut self, asset_id: AssetId, metadata: AssetMetadata, bridge_registry: &dyn BridgeIdentityRegistry) -> Result<(), AssetError>;
    fn revoke(&mut self, asset_id: &AssetId, governance_sig: &[u8; 64]) -> Result<(), AssetError>;
    fn rotate_governance(&mut self, asset_id: &AssetId, old_sig: &[u8; 64], new_pubkey: [u8; 32], new_sig: &[u8; 64]) -> Result<(), AssetError>;
}

pub enum AssetError {
    /// fires when `asset_id` is not present in the registry table (lookup miss).
    Unknown,
    /// fires when `metadata.wire_scale > MAX_SCALE` (defense-in-depth; MAX_SCALE = 18).
    ScaleOutOfRange { scale: u8 },
    /// fires when re-registering an existing asset_id whose `wire_scale` differs from the prior value.
    ScaleImmutable { existing: u8, proposed: u8 },
    /// fires when `AssetId::derive(metadata.namespace_tag()) != asset_id` (BLAKE3 derivation mismatch).
    DerivationMismatch,
    /// fires when `metadata.kind` does not match the expected kind for the namespace-prefix in `namespace_tag()`.
    KindNamespaceMismatch { expected: AssetKind, actual: AssetKind },
    /// fires when registering a non-sovereign asset with `governance_pubkey = None` (Round 1 CRITICAL #3 mitigation).
    GovernanceMissing,
    /// fires when `governance_sig` does not verify against `metadata.governance_pubkey` (revoke/rotate path).
    GovernanceSignatureInvalid,
    /// fires when `register()` is called on an asset whose `kind == SovereignRoleToken` (sovereign role tokens are hardcoded at startup, never re-registered).
    SovereignRoleToken,
    /// fires when `revoke()` is called on an entry whose `tombstone` is already `true`.
    AlreadyRevoked,
    /// fires when caller asked about revoke but entry is live.
    NotRevoked,
    /// fires when the bounded LRU cache misses (caller should retry against live registry).
    BoundedCacheMiss,
    /// NEW in v3.5-r3 — bridge-forgery mitigation: BRIDGED/WRAPPED `register()` called with a
    /// `bridge_id` not present in the BridgeIdentityRegistry. Mirrors `BridgeError::Unknown`.
    BridgeUnknown { bridge_id: [u8; 32] },
    /// NEW in v3.5-r3 — bridge-forgery mitigation: the claimed `governance_pubkey` is not in
    /// the bridge's curator set. Mirrors `BridgeError::CuratorNotInBridgeSet`.
    CuratorNotInBridgeSet { bridge_id: [u8; 32], claimed_pubkey: Option<[u8; 32]> },
}
```

### 3.2 Scale-immutability rule (NEW, defends bridge-hijack vector)

`register()` REJECTS re-registration of an existing `asset_id` with a different `scale`. Rationale: the Round 1 security review (CRITICAL #1) identified bridge-hijack as a top-tier threat where an attacker re-registers an asset with a different scale, corrupting the historical audit trail.

```rust
pub fn register(
    &mut self,
    asset_id: AssetId,
    metadata: AssetMetadata,
    bridge_registry: &dyn BridgeIdentityRegistry, // NEW in v3.5-r3 — bridge-forgery mitigation
) -> Result<(), AssetError> {
    // Sovereign role tokens are hardcoded at startup from §2.1; runtime register() is rejected outright.
    if matches!(metadata.kind, AssetKind::SovereignRoleToken) {
        return Err(AssetError::SovereignRoleToken);
    }
    // Non-sovereign assets REQUIRE a governance_pubkey (Round 1 CRITICAL #3 mitigation).
    if metadata.governance_pubkey.is_none() {
        return Err(AssetError::GovernanceMissing);
    }
    if metadata.wire_scale > MAX_SCALE {
        return Err(AssetError::ScaleOutOfRange { scale: metadata.wire_scale });
    }
    // Derivation invariant: the asset_id MUST match the BLAKE3 derivation of the metadata's namespace tag.
    let derived = AssetId::derive(&metadata.namespace_tag());
    if derived != asset_id {
        return Err(AssetError::DerivationMismatch);
    }
    // Namespace-prefix / kind cross-check (Round 1 security review Threat #6 mitigation).
    let tag = metadata.namespace_tag();
    let expected_kind = match &tag {
        t if t.starts_with(b"cipherocto/asset/v1/PRIVATE-") => AssetKind::PrivateCorporateAsset,
        t if t.starts_with(b"cipherocto/asset/v1/BRIDGED-") => AssetKind::BridgedExternalAsset,
        t if t.starts_with(b"cipherocto/asset/v1/WRAPPED-") => AssetKind::WrappedCrossChainAsset,
        _ => AssetKind::SovereignRoleToken,
    };
    if metadata.kind != expected_kind {
        return Err(AssetError::KindNamespaceMismatch { expected: expected_kind.clone(), actual: metadata.kind.clone() });
    }
    // NEW in v3.5-r3 — bridge-forgery mitigation: for BRIDGED/WRAPPED kinds, parse bridge_id from
    // the namespace_tag() prefix section ("BRIDGED-{bridge_id_hex}-..." or "WRAPPED-{bridge_id_hex}-..."),
    // resolve via BridgeIdentityRegistry, AND verify the bridge's curator set contains the claimed
    // `metadata.governance_pubkey`. Bridge identity forgery is the top threat identified in
    // Round 3 R3-security lens — without this check, an attacker fabricates a bridge_id and
    // registers a forged BRIDGED asset that audit consumers treat as legitimate.
    if matches!(metadata.kind, AssetKind::BridgedExternalAsset | AssetKind::WrappedCrossChainAsset) {
        let bridge_id = parse_bridge_id_from_tag(&tag)?;   // helper, returns [u8; 32] or AssetError::DerivationMismatch
        let bridge = bridge_registry.resolve(&bridge_id).map_err(|_| AssetError::BridgeUnknown { bridge_id })?;
        if !bridge.curator_set_contains(&metadata.governance_pubkey) {
            return Err(AssetError::CuratorNotInBridgeSet {
                bridge_id,
                claimed_pubkey: metadata.governance_pubkey,
            });
        }
    }
    if let Ok(existing) = self.metadata(&asset_id) {
        if existing.tombstone { return Err(AssetError::AlreadyRevoked); }
        if existing.wire_scale != metadata.wire_scale {
            return Err(AssetError::ScaleImmutable { existing: existing.wire_scale, proposed: metadata.wire_scale });
        }
        // Sovereign path is rejected above; existing.wire_scale match implies same kind.
        // Scale unchanged; allow version bump.
    }
    // ... commit to table ...
    Ok(())
}
```

**Bridge-forgery cross-checks (NEW in v3.5-r3):** for `BridgedExternalAsset` / `WrappedCrossChainAsset`, `register()` now requires:

1. `parse_bridge_id_from_tag(&tag)` — extracts the 32-byte `bridge_id` from the `BRIDGED-{bridge_id_hex}-` / `WRAPPED-{bridge_id_hex}-` prefix section of `namespace_tag()`. Returns `AssetError::DerivationMismatch` on parse failure.
2. `bridge_registry.resolve(&bridge_id)` — verifies the bridge_id is registered. On miss, returns `AssetError::BridgeUnknown { bridge_id }` (NEW variant; mirrors `BridgeError::Unknown`).
3. `bridge.curator_set_contains(&metadata.governance_pubkey)` — cross-checks the claimed governance_pubkey is in the bridge's curator set. On miss, returns `AssetError::CuratorNotInBridgeSet { bridge_id, claimed_pubkey }` (NEW variant; corresponds to `BridgeError::CuratorNotInBridgeSet` on the bridge side).

These three checks together close the bridge-forgery vector: a forged `bridge_id` fails check #2, a real bridge_id with a non-curator governance key fails check #3, and a malformed namespace_tag fails check #1.

### 3.3 Revocation + governance rotation (NEW, Round 1 mitigation)

`revoke()` flips `tombstone = true`. Historical events still resolve (asset_id is in the table). NEW events (PaymentCaveat::verify, BurnEventRef::new, SettlementEvent::new) REJECT against tombstoned asset_id with `AssetError::Unknown`.

`rotate_governance()` requires the OLD governance_pubkey to co-sign the rotation. This prevents unilateral takeover when a single key is compromised (Round 1 security review CRITICAL #1 mitigation). The rotation bumps `version` and stores the previous commitment in `prev_commitment`.

### 3.4 Chain-of-trust commitment (NEW)

Each AssetMetadata carries `version: u64` + `prev_commitment: Option<[u8; 32]>`. The commitment uses **length-prefixed encoding** to prevent the concatenation-ambiguity attack where two distinct metadata tuples collide:

```text
prev_commitment = BLAKE3(
    u8_be(wire_scale)
  || u8_be(display_decimals)
  || u32_be(denomination.len()) || denomination.as_bytes()
  || u32_be(symbol.len()) || symbol.as_bytes()    // NEW in v3.5-r5 (R1 LOW-09) — canonical derivation tag
  || u8_be(kind_tag)          // 0x01=Sovereign, 0x02=Private, 0x03=Bridged, 0x04=Wrapped
  || [u8; 32]                  // governance_pubkey
  || [u8; 32]                  // chain_id (RFC-0967-A1 ChainId) — NEW in v3.5-r5 (R1 LOW-09)
  || u32_be(asset_name.len()) || asset_name.as_bytes()  // NEW in v3.5-r5 (R1 LOW-09)
  || u64_be(version - 1)
)
```

This creates an immutable per-asset history that can be reconstructed even if the live registry is wiped (Round 1 IMPORTANT #9). The length-prefix encoding guarantees that no two distinct metadata tuples produce the same commitment (no `denom` substring can be reinterpreted as another field's payload).

**Version-bump semantics (NEW in v3.5-r5):** any of `wire_scale`, `display_decimals`, `denomination`, `symbol`, `kind_tag`, `governance_pubkey`, `chain_id`, or `asset_name` changing on a re-registration MUST bump `version` and produce a new `prev_commitment`. The chain-of-trust guarantees that the previous-version commitment is fully reconstructible from the new-version commitment + the prior field values (a single field change is detectable via the commitment delta; an attacker tampering with a historical commitment is detectable because the next commitment cannot chain from a tampered prior).

### 3.5 Bounded LRU cache + registry-snapshot epoch (NEW, Round 1 DoS mitigation)

`AssetRegistry` implementation MUST cache `metadata(asset_id)` lookups in a bounded LRU keyed by asset_id, TTL = `current_epoch + N` where N = governance snapshot propagation window. Rate-limit `register()` per governance_pubkey.

```rust
use lru::LruCache;   // Cargo.toml: `lru = { version = "0.12", features = ["std"] }` — Round 1 DoS mitigation: bounded cache for AssetRegistry lookups

pub struct CachedAssetRegistry {
    inner: Box<dyn AssetRegistry>,
    cache: LruCache<AssetId, (AssetMetadata, u64)>,   // (metadata, snapshot_epoch)
    ttl_epochs: u64,
}

// Convention (Round 4 doc): field name on audit-trio structs = `registry_snapshot_epoch`;
// local variable / parameter = `snapshot_epoch` (prefix dropped for readability when
// context is unambiguous, e.g. inside `CachedAssetRegistry::metadata`).

impl CachedAssetRegistry {
    pub fn metadata(&mut self, asset_id: &AssetId) -> Result<AssetMetadata, AssetError> {
        let (meta, snapshot_epoch) = match self.cache.get(asset_id) {
            Some(entry) => entry.clone(),
            None => return Err(AssetError::BoundedCacheMiss),   // NEW in v3.5-r3 — explicit Err path; caller retries against live registry
        };
        if snapshot_epoch.saturating_add(self.ttl_epochs) < current_snapshot_epoch() {
            // Stale entry — fall through to live registry lookup.
            let meta = self.inner.metadata(asset_id)?;
            // Round 4 fix: populate the cache after a successful live lookup so subsequent
            // calls within the TTL window avoid the live-registry round trip.
            self.cache.put(*asset_id, (meta.clone(), current_snapshot_epoch()));
            return Ok(meta);
        }
        Ok(meta)
    }
}
```

When the cache misses or the snapshot epoch is stale, the implementation calls `metadata()` and updates the cache. `AssetError::BoundedCacheMiss` indicates a transient miss (caller should retry against the live registry) — the variant is REACHABLE in v3.5-r3 (previously listed but unreachable; the explicit `Err` return on cache miss makes the variant usable by callers detecting transient registry pressure).

### 3.6 Bridge Identity Registry (NEW, Round 1 CRITICAL #3 mitigation)

The asset_id derivation takes `bridge_id` as a free-form 32-byte hex string. Without a bridge identity registry, an attacker fabricates `bridge_id = 0xFAB...FAB` and registers a forged BRIDGED asset. Mitigation: a separate `BridgeIdentityRegistry` (also Layer B additive, semver-minor) maps `bridge_id -> (external_chain, contract_address, governance_hash, attestation_quorum_signature)`. Registration of a BRIDGED or WRAPPED asset REQUIRES the bridge_id to resolve to a known entry with quorum attestation.

**Quorum + signature scheme (Round 1 security review CRITICAL #2 mitigation):**

- **Signature algorithm:** BLS12-381 aggregated signatures (binding chain-of-trust to a fixed public key set).
- **Quorum size:** 3-of-5 (m-of-n; 3-of-5 is the substrate default; higher m requires RFC amendment).
- **Slashing:** `BridgeCurator` stakers (`slashing_stake: Dqa`) lose their stake on `BridgeCuratorSlashingCondition` enum:
  - `FalseAttestation { bridge_id, claim }` — quorum signed a claim that failed external verification.
  - `DoubleSign { bridge_id, epoch, sig_a, sig_b }` — same curator signed two conflicting attestations.
  - `KeyCompromise { bridge_id, rotated_at }` — pre-rotation attestations are re-attributed to the compromised key.

```rust
// crates/octo-vault/src/bridge_identity_registry.rs (NEW)

use crate::{BridgeChainNamespace, Dqa};

pub struct BridgeIdentity {
    pub bridge_id: [u8; 32],
    pub external_chain: BridgeChainNamespace,
    pub contract_address: Vec<u8>,
    pub governance_hash: [u8; 32],
    pub attestation_quorum_sig: Vec<u8>,    // BLS12-381 aggregated signature from 3-of-5 quorum
    pub slashing_stake: Dqa,                 // curator stake subject to BridgeCuratorSlashingCondition
    pub curator_set: Vec<[u8; 48]>,         // BLS12-381 public keys of the active curator quorum (NEW in v3.5-r3 — for register() cross-check)
}

pub enum BridgeCuratorSlashingCondition {
    /// fires when the 3-of-5 BLS12-381 quorum signed an attestation that external verification rejects
    /// (merkle-inclusion proof of failed external claim required).
    FalseAttestation { bridge_id: [u8; 32], claim: Vec<u8> },
    /// fires when the same curator signature appears in two attestations for the same
    /// `bridge_id + epoch` with conflicting claims.
    DoubleSign { bridge_id: [u8; 32], epoch: u64, sig_a: Vec<u8>, sig_b: Vec<u8> },
    /// fires when governance rotation post-dates a contested attestation, re-attributing
    /// pre-rotation signatures to the compromised key.
    KeyCompromise { bridge_id: [u8; 32], rotated_at: u64 },
}

pub enum BridgeError {
    /// fires when `bridge_id` is not present in the BridgeIdentityRegistry.
    Unknown,
    /// fires when the aggregated BLS12-381 quorum signature fails to verify.
    QuorumAttestationInvalid,
    /// fires when `bridge.external_chain` does not match the claimed asset's namespace-prefix.
    /// Construction site (NEW in v3.5-r5, R5 L5 HIGH): `AssetRegistry::register()` for a
    /// `BridgedExternalAsset` / `WrappedCrossChainAsset` cross-checks the asset's
    /// `bridge.external_chain` against the namespace-prefix kind BEFORE the curator-set check;
    /// on mismatch, returns `AssetError::ExternalChainMismatch` (mirrors this variant) and the
    /// bridge-side log records this variant for audit traceability.
    ExternalChainMismatch { expected: BridgeChainNamespace, actual: BridgeChainNamespace },
    /// fires when the bridge entry's `tombstone` is `true`. Construction site
    /// (NEW in v3.5-r5, R5 L5 HIGH): `AssetRegistry::register()` pre-check for BRIDGED/WRAPPED
    /// kinds — calls `bridge_registry.resolve(&bridge_id)` and inspects the resulting
    /// `BridgeIdentity`'s revoked flag; if revoked, returns `AssetError::Revoked` (mirrors
    /// this variant). A revoked bridge MUST NOT accept new BRIDGED/WRAPPED asset registrations;
    /// historical entries still resolve (audit-trail invariant).
    Revoked,
    /// NEW in v3.5-r3 — fires when a claimed `governance_pubkey` is not present in the bridge's
    /// active curator set (mirrors `AssetError::CuratorNotInBridgeSet`).
    CuratorNotInBridgeSet { bridge_id: [u8; 32], claimed_pubkey: Option<[u8; 32]> },
    /// NEW in v3.5-r5 (R5 L3 CRIT-2 mitigation) — fires when first-time `register()` of a
    /// NEW `bridge_id` is called WITHOUT a governance co-signature from cipherocto chain
    /// governance (the Layer A root key, separate from the BridgeCurator role). The co-signature
    /// attests to the curator_set chosen by the bridge operators; without it, `register()`
    /// rejects with this variant. Subsequent `rotate_curators()` calls do NOT require this
    /// co-signature (the curator-quorum threshold per R4 is sufficient).
    GovernanceAttestationMissing { bridge_id: [u8; 32] },
    /// NEW in v3.5-r6 — fires when a bridge was registered within an epoch window later flagged
    /// as root-key compromised. Retrospective auditor signal, NOT a runtime reject path. fires
    /// via root_key_rotation_epoch + retroactive_review.
    RegisteredUnderCompromisedKey { bridge_id: [u8; 32], suspected_epoch: u64 },
    /// NEW in v3.5-r6 — fires when `rotate_curators()` is called WITH an optional governance
    /// co-signature (`governance_co_signature = Some(...)`) for audit attribution. When
    /// `with_co_signature = true`, the rotation event records the Layer A root-key blessing;
    /// when `false` (i.e. rotation succeeded without co-signature), the audit log records the
    /// variant with `with_co_signature: false`. Auditor signal, NOT a runtime reject path.
    CuratorRotationGovernanceBlessed { bridge_id: [u8; 32], with_co_signature: bool },
}

pub trait BridgeIdentityRegistry {
    fn resolve(&self, bridge_id: &[u8; 32]) -> Result<BridgeIdentity, BridgeError>;
    /// NEW in v3.5-r3 — `register()` now requires an aggregated BLS12-381 attestation over the
    /// `(bridge_id ‖ external_chain ‖ contract_address ‖ governance_hash)` payload. The verifier
    /// MUST check the aggregate signature against the curator quorum public keys before commit.
    fn register(
        &mut self,
        identity: BridgeIdentity,
        quorum_pubkeys: &[[u8; 48]],
        quorum_sigs: &[Vec<u8>],
    ) -> Result<(), BridgeError>;
    /// NEW in v3.5-r6 — `governance_co_signature: Option<GovernanceSignature>` is an OPTIONAL
    /// Layer A root-key co-signature for audit attribution. First-time `register()` REQUIRES
    /// the Layer A root key (separate from the BridgeCurator role); rotation only requires
    /// old-quorum + optional co-signature. When `Some`, the rotation event records
    /// `BridgeError::CuratorRotationGovernanceBlessed { bridge_id, with_co_signature: true }`
    /// for audit attribution; when `None`, the audit log records the variant with
    /// `with_co_signature: false`.
    fn rotate_curators(
        &mut self,
        bridge_id: &[u8; 32],
        old_quorum_sigs: &[Vec<u8>],
        new_pubkeys: &[[u8; 48]],
        new_attestation_sig: &Vec<u8>,
        governance_co_signature: Option<GovernanceSignature>,
    ) -> Result<(), BridgeError>;   // NEW in v3.5-r3 — curator rotation ceremony
    fn slash(&mut self, bridge_id: &[u8; 32], condition: BridgeCuratorSlashingCondition) -> Result<(), BridgeError>;
}

/// NEW in v3.5-r6 — Layer A root-key signature carried in `rotate_curators()` for audit
/// attribution. Distinct from `BridgeCurator` BLS12-381 quorum signatures; uses the
/// cipherocto chain governance ed25519 root key per §3.6.2 Layer A Root Key Governance.
pub struct GovernanceSignature {
    pub root_key_fingerprint: [u8; 32],
    pub epoch: u64,
    pub sig: [u8; 64],
}
```

**`register()` verification (NEW in v3.5-r3, distinct-pubkey enforcement NEW in v3.5-r4, co-signature + canonicalization NEW in v3.5-r5):** before committing a `BridgeIdentity`, the registry verifies the aggregated BLS12-381 signature against the supplied `quorum_pubkeys` (the curator set in `BridgeIdentity::curator_set`) and `quorum_sigs`. The signed payload is:

```text
attestation_payload =
    bridge_id                                          // [u8; 32]
  || external_chain.encode()                            // 1 byte (BridgeChainNamespace discriminant)
  || u32_be(contract_address.len()) || contract_address // length-prefixed to prevent concat ambiguity
  || governance_hash                                    // [u8; 32]
  || blake3_hash(canonical_sort(curator_set))           // NEW in v3.5-r5 (R5 L3 CRIT-2) — curator_set commitment
```

**Canonical sort rule (NEW in v3.5-r5):** `canonical_sort(curator_set)` sorts the BLS12-381 public keys BYTE-LEXICOGRAPHICALLY (raw byte comparison, NOT group-theoretic order). The sort is stable (preserves no metadata beyond byte order). The hash is `blake3(canonical_sort(curator_set))` where the input is the concatenated canonical-form pubkeys (each `[u8; 48]` emitted in sorted order). The length-prefix rule from `prev_commitment` (§3.4) applies to the curator_set as a whole: `u32_be(curator_set.len() * 48) || sorted_pubkeys` before hashing, to prevent concatenation ambiguity.

**Verification sequence:**

1. **BLS pubkey canonicalization (NEW in v3.5-r5, R5 L3 LOW-1, normative):** each `quorum_pubkeys[i]` MUST be canonicalized BEFORE the distinct-pubkey check AND before the aggregate pairing check. Canonical form: G1 compressed with the sign bit flag forced to `0x80` (positive Y); the infinity point is encoded as `0xC0`. Use the substrate-chosen BLS library's `PublicKey::from_bytes` + `to_canonical` (or equivalent `bls12_381::G1Projective::normalize` round-trip) — the canonicalization must be IDEMPOTENT (a canonical key passes through unchanged). Reference: substrate-bls12-381 `bls12_381::G1Affine::serialize` with `CompressedFlag::YPositive`. A non-canonical input REJECTS with `BridgeError::QuorumAttestationInvalid`.
2. **Distinct-pubkey check (NEW in v3.5-r4, normative):** `register()` MUST reject if `quorum_pubkeys` contains duplicate `[u8; 48]` values (after canonicalization per step 1). The check is implemented as a set-membership scan (HashSet or sorted-scan): for each index `i` in `0..quorum_pubkeys.len()`, compare against all `j > i`; on any match, return `BridgeError::QuorumAttestationInvalid` IMMEDIATELY (before the aggregate pairing check). This is a hard requirement — duplicate pubkeys in the BLS aggregate would otherwise inflate the apparent quorum weight without contributing distinct signers, weakening the security guarantee. **Proof-of-possession per pubkey is OPTIONAL but RECOMMENDED for production deployments** (out of scope for this RFC; tracked in `docs/audits/`).
3. **Pairwise signature check:** each `quorum_sigs[i]` MUST verify under the canonicalized `quorum_pubkeys[i]` over the attestation_payload. Failure produces `BridgeError::QuorumAttestationInvalid`.
4. **Aggregate pairing check:** the aggregated signature (BLS aggregate over the canonicalized `quorum_pubkeys` / `quorum_sigs`) MUST verify against the aggregate public key over the same payload. Failure produces `BridgeError::QuorumAttestationInvalid`.

Failure of any of steps 1, 2, 3, or 4 REJECTS the registration. This closes the "fabricated bridge" vector — an attacker cannot register a `BridgeIdentity` without producing valid attestations from a DISTINCT, CANONICALIZED curator quorum.

**First-time `register()` co-signature requirement (NEW in v3.5-r5, R5 L3 CRIT-2 mitigation):** first-time `register()` of a NEW `bridge_id` MUST include a co-signature from cipherocto chain governance (the Layer A root key, separate from the BridgeCurator role) attesting to the curator_set. The co-signature payload is `b"cipherocto/bridge-register/v1/" || bridge_id || blake3_hash(canonical_sort(curator_set))` signed by ed25519 (canonical sig scheme per RFC-0105 §3.12 Cryptographic Primitives). Without this co-signature, `register()` rejects with `BridgeError::GovernanceAttestationMissing { bridge_id }` (NEW variant per §3.6 enum). Subsequent `rotate_curators()` calls do NOT require this co-signature — the curator-quorum 3-of-5 threshold (per R4) is sufficient because the bridge is already on-record at the Layer A root. The Layer A root key is held by cipherocto chain governance multisig (separate substrate role tier; NOT the same key set as any individual bridge curator set). This requirement closes the vector where an attacker fabricates a `bridge_id` AND a self-curated `curator_set` without the Layer A root ever seeing the curator_set; the co-signature forces the Layer A root to commit to the curator_set chosen.

**`rotate_curators()` (NEW in v3.5-r3):** bumps the `version`, replaces `curator_set` with `new_pubkeys`, and emits a `CuratorRotation { bridge_id, old_set_hash, new_set_hash, rotated_at: current_epoch }` event. The rotation REQUIRES signatures from the OLD quorum (threshold 3-of-5) to prevent unilateral rotation. The new set is anchored by a fresh `new_attestation_sig` over the new set hash. Used after `KeyCompromise` slashing to recover the bridge identity without invalidating historical BRIDGED asset entries.

`BridgeCurator` governance role (separate from `AssetRegistry` governance) manages bridge identity entries. Bridge identity and asset metadata have independent trust roots. The quorum verifier is substrate-mandated and MUST be invoked from every node that accepts a BRIDGED or WRAPPED asset — split-chain equivalence requires the same verifier across all nodes.

### 3.6.1 Bridge Slashing Governance (NEW in v3.5-r3)

**Caller permission:** `slash()` MUST be invoked by a governance-blessed multisig (separate from `AssetRegistry` governance; same substrate role tier). Single-key or threshold-mismatched callers REJECT with `BridgeError::Unknown` (caller not authorized).

**Evidence requirements per condition:**

- `FalseAttestation { bridge_id, claim }`: requires a merkle-inclusion proof of the failed external claim (i.e., a confirmed claim on the external chain that contradicts the signed claim). The proof MUST be verified by an external-chain light client bound to the same `BridgeChainNamespace`.
- `DoubleSign { bridge_id, epoch, sig_a, sig_b }`: requires both attestations to be on-record (resolve to known `attestation_quorum_sig` entries in the bridge's audit log) for the same `bridge_id + epoch` with conflicting claim content. The conflicting content MUST differ in a way that cannot be attributed to chain reorgs (canonical historical state required).
- `KeyCompromise { bridge_id, rotated_at }`: requires evidence that governance rotation `rotated_at` post-dates a contested attestation AND the contested attestation's signer set matches the pre-rotation key. This re-attributes the pre-rotation signatures to the compromised key (the rotation evidence proves the old key was leaked).

**Stake redistribution policy:** on `slash()`, the bridge's `slashing_stake: Dqa` is redistributed per `slashing_stake_redistribution_policy`:

- 50% burned (treasury sink);
- 30% returned to honest curators (pro-rata among the non-slashed quorum members);
- 20% to the slashing evidence provider (reporter reward).

Slashed curators are removed from `curator_set`; new curator elections are triggered via `rotate_curators()` after a 1-epoch cooldown.

### 3.6.2 Layer A Root Key Governance (NEW in v3.5-r6)

The Layer A root key is held by cipherocto chain governance multisig (separate substrate role tier; NOT the same key set as any individual bridge curator set). The root key is the apex authority that attests to first-time `register()` of a NEW `bridge_id` (per §3.6 first-time `register()` co-signature requirement). This subsection documents the root key's distinct governance lifecycle:

**(a) Rotation procedure (separate from curator rotation):** root key rotation is a governance-event distinct from `rotate_curators()`. The root key rotates under a separate ceremony owned by the cipherocto chain governance multisig; the rotation event emits a `RootKeyRotation { old_fingerprint, new_fingerprint, rotated_at: current_epoch, governance_attestation: Vec<u8> }` record on-chain. Curator rotation is unaffected — bridge curator sets continue to operate against the new root key once rotation completes.

**(b) BridgeIdentity registers root-key fingerprint:** `BridgeIdentity` stores `{registering_root_key_fingerprint, registering_epoch}` so verifiers cross-check against the historical root-key log. On `resolve()`, a verifier MUST confirm that `(registering_root_key_fingerprint, registering_epoch)` corresponds to a root key that was active at `registering_epoch`. If the root key was rotated prior to `registering_epoch`, the entry is flagged for retrospective review (see variant below).

**(c) Retrospective flagging:** `BridgeError::RegisteredUnderCompromisedKey { bridge_id, suspected_epoch }` fires when a bridge was registered within an epoch window later flagged as root-key compromised. Retrospective auditor signal, NOT a runtime reject path. fires via root_key_rotation_epoch + retroactive_review.

**(d) Cross-reference:** for root-key ownership and rotation ceremony details, see RFC-0009 §Identity and the cipherocto governance RFC. The Layer A root key is distinct from any `BridgeCurator` BLS12-381 key in any individual bridge's curator set — no shared key material between tiers.

**Cross-crate test vector (NEW in v3.5-r6):** `BridgeIdentityRegistry::resolve(bridge_id)` for a bridge registered at `registering_epoch = 100` whose root key was rotated at `root_key_rotation_epoch = 150` (post-registration) returns `Ok(bridge_identity)` (resolve succeeds); a subsequent `retroactive_review(bridge_id, suspected_epoch: 100)` call flags `BridgeError::RegisteredUnderCompromisedKey { bridge_id, suspected_epoch: 100 }` for auditor review (does NOT reject the bridge — historical entries still resolve per audit-trail invariant).

### 3.7 Population semantics

- **Sovereign role tokens**: populated at startup from the table in §2.1 (hardcoded). version = 1, no `prev_commitment`, no `governance_pubkey`. `register()` rejects sovereign role tokens outright via `AssetError::SovereignRoleToken` (the table is hardcoded; runtime registration is never the entry path).
- **Private / bridged / wrapped assets**: populated at runtime by governance-blessed registration. Non-sovereign assets REQUIRE `governance_pubkey` (enforced by `register()` guard). BRIDGED and WRAPPED assets additionally REQUIRE the bridge_id to resolve via `BridgeIdentityRegistry` (3-of-5 BLS12-381 quorum attestation; see §3.6).

### 3.8 Compatibility with frozen substrate

`AssetId(pub [u8; 32])` is UNCHANGED. The side-table is a Layer B additive extension. No consumer crate is required to migrate to a new `AssetId` shape. `AssetRegistry` is owned by `octo-vault` (Layer B) and consumed by `octo-cap-macaroon`, `octo-policy`, `quota-router-storage`, and other Layer B/C crates via trait injection.

**Single-source-of-truth rule:** `AssetKind`, `AssetRegistry`, `AssetMetadata`, `AssetError`, `MAX_SCALE`, and `AssetMetadata::namespace_tag` are defined ONCE in `octo-vault` (this RFC §3.1). Consumer crates (RFC-0965 §2, RFC-0960 §2, RFC-0959 §2) MUST import via `use octo_vault::asset_registry::{...};` and MUST NOT re-declare any of these types locally. Re-declaration creates parallel-abstraction drift and breaks the cross-RFC audit-invariant chain.

### 3.9 Wire Form (NEW, Round 1 fix)

The `AssetRegistry` side-table is a substrate persistence concern. The canonical on-wire form for `AssetMetadata` follows RFC-0862 §Substrate types conventions:

```text
AssetMetadata wire form (borsh-serialized, 100+ bytes):
  [u8; 32]   asset_id            // canonical BLAKE3 derivation
  u8         wire_scale          // 0..=MAX_SCALE
  u8         display_decimals    // 0..=18
  u32_be     denomination.len()  // length-prefix
  [u8; N]    denomination.as_bytes()
  u8         kind_tag            // 0x01=Sovereign, 0x02=Private, 0x03=Bridged, 0x04=Wrapped
  Option<[u8; 32]>  governance_pubkey  // 0x00 byte prefix + 32 bytes if Some
  u64_be     version
  Option<[u8; 32]>  prev_commitment    // 0x00 byte prefix + 32 bytes if Some
  bool       tombstone
```

`version` and `prev_commitment` are part of the wire form to enable audit-trail reconstruction from snapshot files. Cross-reference: RFC-0862 §Substrate types (`DqaEncoding` 16-byte BE form at `crates/octo-cap-macaroon/src/dqa_serde.rs:5`).

**Substrate wire form note (Round 3 R3-doc consistency):** the substrate-side `crates/octo-cap-macaroon/src/dqa_serde.rs` L5 doc-comment mentions the wire form abstractly, but the encoding/decoding functions at L24 (encode via `to_le_bytes`) and L46 (decode via `from_be_bytes` + `swap_bytes`) are the source of truth — the L5 doc-comment is NOT. Implementers MUST follow the L24 encode path / L46 decode path, not the L5 prose. This is a substrate-side inconsistency tracked in `docs/audits/` (no substrate edit per RFC process).

### 3.11 NonceRegistry substrate (GREENFIELD, NEW in v3.5-r4)

The `NonceRegistry` trait is the single-source-of-truth for `(governance_pubkey, nonce)` replay tracking across cipherocto. RFC-0960, RFC-0965, and RFC-0959 each import from this anchor; none re-declares the trait locally.

```rust
// crates/octo-vault/src/nonce_registry.rs (NEW — GREENFIELD)

/// Trait for tracking observed (governance_pubkey, nonce) pairs to prevent replay.
/// Single-source-of-truth: RFC-0105 §3.11; RFC-0960/0965/0959 import from this anchor.
pub trait NonceRegistry {
    /// Record an observation. Returns Err(NonceError::AlreadyObserved) if (pk, nonce) was previously seen.
    /// The canonical implementation is StoolapNonceRegistry (octo_vault::nonce_registry::StoolapNonceRegistry),
    /// persisted via cipherocto-fork stoolap with WAL-primary write semantics.
    /// InMemoryNonceRegistry is permitted ONLY for tests.
    fn observe(&mut self, pk: &[u8; 32], nonce: &[u8; 32]) -> Result<(), NonceError>;
    /// Check whether (pk, nonce) was previously observed (read-only).
    /// Renamed from the prior trait method name in v3.5-r5 (R5 R1 CRIT) for consistency with consumer RFC-0965 §2.3
    /// `observe_readonly` callsite — trait method name now matches the consuming RFC's vocabulary.
    fn observe_readonly(&self, pk: &[u8; 32], nonce: &[u8; 32]) -> bool;
}

/// Keying convention (LOCKED, do not vary across consumer RFCs):
/// - For event types with construction-time `new()` (BurnEventRef, SettlementEvent):
///   `meta.governance_pubkey.expect("guard at new()")` — non-sovereign event types carry the
///   governance_pubkey on construction; the `.expect()` enforces the §3.7 non-sovereign
///   governance_pubkey requirement at compile-of-event time.
/// - For caveat types without `new()` (PaymentCaveat — caveats are minted from a capability
///   without a per-event construction boundary): `registry.metadata(&asset_id).ok().and_then(|m| m.governance_pubkey)`.
///   The caveat-side looks up governance_pubkey from the live AssetRegistry at verification time.
/// - Sovereign fallback (both cases): when `governance_pubkey = None` (sovereign role tokens
///   per §2.1), use `sovereign_nonce_namespace(asset_id)` = `blake3_hash(b"octo:sovereign-nonce-ns:v1" || asset_id.0)`.
/// - NEVER key on `asset_id.0` directly for BRIDGED/WRAPPED/PRIVATE — that would defeat cross-authority replay isolation.

/// fires when the (pk, nonce) tuple was previously observed in NonceRegistry.
pub enum NonceError {
    AlreadyObserved { pk: [u8; 32], nonce: [u8; 32], prior_height: u64 },
    /// fires when the WAL write to cipherocto-fork stoolap fails (fsync error, disk-full,
    /// connection-lost). Construction site (NEW in v3.5-r5, R5 L5 HIGH): the canonical
    /// `StoolapNonceRegistry::observe()` MUST surface this variant on every WAL-write failure
    /// path (see `crates/stoolap/src/persistence/wal.rs`). Callers MUST treat
    /// `PersistenceFailure` as a hard failure (no silent retry) — a retry that also fails
    /// to persist leaves the (pk, nonce) unrecorded, opening the restart-window replay vector.
    /// In tests (InMemoryNonceRegistry), this variant is UNREACHABLE — it exists solely to
    /// bridge the production WAL failure mode into the trait's error vocabulary.
    PersistenceFailure { reason: String },
    /// NEW in v3.5-r6 — fires when WAL is recovering from outage. Caller SHOULD retry with backoff.
    /// distinct from PersistenceFailure which is permanent. fires via nonce_registry.observer.observe_wal_recovery_status().
    WalRecovering { backoff_ms: u64 },
}

/// Sliding-window observation-TTL applies: sovereign observations older than the latest
/// committed event (within the same namespace) may be evicted by a periodic GC pass. This
/// bounds WAL growth per asset at the cost of replay-protection window narrowing. NOT applied
/// to bridge-asset namespaces (bridge operator is presumed trusted). Document the asymmetry
/// as intentional.

/// BridgeChainNamespace is defined at §2.1 'Bridged external asset namespace' (L84-101).
/// Consumer RFCs should cite §2.1 directly, not §3.x.
```

**Capacity + eviction (Round 4 normative):** the canonical `StoolapNonceRegistry` is bounded LRU per `governance_pubkey` with capacity `~10^6 entries per pubkey`. TTL is tied to the asset revocation grace period — entries older than `(asset_revocation_grace_epochs × governance_pulse_interval)` are eligible for eviction during background compaction. This bounds the substrate footprint while preventing unbounded growth under sustained observation traffic.

**Persistence (Round 4 normative):** the canonical implementation persists via cipherocto-fork stoolap with WAL-primary write semantics. The substrate comment at `crates/octo-vault/src/nonce_registry.rs` (GREENFIELD — to be added) MUST declare `WALPrimary` and MUST reference `crates/stoolap/src/persistence/wal.rs` for the write path. `InMemoryNonceRegistry` is permitted ONLY inside the `octo-vault` test suite (gated by `#[cfg(test)]`); production binaries MUST NOT link `InMemoryNonceRegistry`.

**Ship milestone (NEW in v3.5-r5, R5 L3 CRIT-4):** the `StoolapNonceRegistry` implementation MUST land before v0 promotion to Accepted. Until that landing, v0 drafts are UNSAFE for production deployment — operators MUST attest (per `docs/audits/v0-nonce-registry-attest.md`) that the restart-window replay vector is acceptable for their deployment (a process restart re-bootstraps the in-memory registry from zero observations, opening the (pk, nonce) replay window until the WAL is restored). The acceptance-promotion checklist (§8) MUST include `StoolapNonceRegistry` landing as a blocking item. `InMemoryNonceRegistry` remains `#[cfg(test)]`-only and MUST NOT be promoted to a runtime feature flag.

**Keying convention rationale (Round 4):** keying on `governance_pubkey` (not on `asset_id`) isolates replay windows per authority. Two assets governed by the same pubkey share a nonce space (they trust the same signer); two assets governed by different pubkeys have independent nonce spaces (they do not). For sovereign assets (governance_pubkey = None), the per-asset namespace `blake3("octo:sovereign-nonce-ns:v1" || asset_id.0)` derives a synthetic authority key — this is the ONLY case where `asset_id` participates in the key derivation, and the derivation is locked to a versioned string so future revisions can rotate.

### 3.12 Cryptographic Primitives (NEW in v3.5-r5, canonical home)

This section is the **canonical home** for the cryptographic primitives referenced across RFC-0105, RFC-0965, RFC-0960, and RFC-0959. Prior drafts referenced these primitives under a prior section number; that section was renumbered to **§3.12** in v3.5-r5 (R5 L1 CRIT). Consumer RFC cite updates: `RFC-0965 §2.1 L54` → `RFC-0105 §3.12 Cryptographic Primitives`, plus all cross-RFC references to the prior section number → "§3.12 Cryptographic Primitives". Future references MUST cite §3.12, not the prior section number.

```rust
// crates/octo-cap-macaroon/src/lib.rs (substrate path — GREENFIELD amendment site; canonical home)

/// ed25519 signature scheme over BLAKE3-256(message).
/// Signature size: 64 bytes (R ‖ S). Public key size: 32 bytes.
/// GREENFIELD substrate path: `octo_cap_macaroon::verify_governance_signature` (RFC-0105 §3.12 canonical home).
/// Re-exported from octo_cap_macaroon in octo_vault::verify_governance_signature for consumer convenience.
pub fn verify_governance_signature(
    sig: &[u8; 64],
    msg: &[u8],
    pk: &[u8; 32],
) -> bool;

/// BLAKE3-256 hash function for body_hash and namespace_tag derivation.
/// GREENFIELD substrate path: `octo_cap_macaroon::blake3_hash` (RFC-0105 §3.12 canonical home).
/// Re-exported from octo_cap_macaroon in octo_vault::blake3_hash for consumer convenience.
pub fn blake3_hash(data: &[u8]) -> [u8; 32] {
    // Returns [u8; 32] canonical 32-byte form. Do NOT use blake3::hash(data).as_bytes() at call sites.
    blake3::hash(data).into()
}
```

**Single-source-of-truth rule (NEW in v3.5-r5, clarified in v3.5-r6):** `verify_governance_signature` and `blake3_hash` are defined ONCE in `octo-cap-macaroon` (Layer A substrate). Consumer crates MUST import via `use octo_cap_macaroon::{verify_governance_signature, blake3_hash};` OR via the re-export `use octo_vault::{verify_governance_signature, blake3_hash};` (octo_vault re-exports both). Both paths resolve to the canonical octo_cap_macaroon definition. Consumers MUST NOT re-declare these functions locally. Re-declaration creates parallel-abstraction drift (same principle as the §3.1 / §3.5 / §3.6 single-source rule for `AssetKind`, `AssetRegistry`, `AssetMetadata`, `BridgeIdentityRegistry`, and `BridgeIdentity`).

### 3.13 Tri-invariant declaration (NEW in v3.5-r3, renumbered from prior section number in v3.5-r5)

All amount-bearing events in the audit chain MUST satisfy `PaymentCaveat.asset_id == BurnEventRef.asset_id == SettlementEvent.cost_asset_id`. Violation at any pairwise REJECTS the event per the consuming crate's audit verifier. See RFC-0965 §2.3, RFC-0960 §4, RFC-0959 §4 for site-specific enforcement sites. This invariant is the canonical statement of the audit-chain consistency rule; consumer RFCs import this section by reference and MUST NOT weaken or restate it.

**Audit-batch replay enforcement (NEW in v3.5-r6):** audit-batch replay MUST re-check the tri-invariant pairwise for every `(PaymentCaveat, BurnEventRef, SettlementEvent)` tuple, not rely on per-event `validate()` cache. The per-event cache optimizes the steady-state hot path; the batch-replay path runs a fresh pairwise check on each tuple to defend against stale-cache false-positives in replay scenarios.

## 4. Cross-Reference Updates (Round 2: stripped version pins)

This amendment requires companion amendments to:

- RFC-0965 (Capability Extension Format)
- RFC-0960 (Vault Path Taxonomy)
- RFC-0959 (Ask Settlement Chain)

## 5. Backward Compatibility (NEW in v3.5)

- **Legacy substrate (pre-v3.5)**: nodes that have not yet migrated to `AssetRegistry` see no `metadata(asset_id)` calls; they continue enforcing wire scale=0 at the boundary. PaymentCaveat / BurnEventRef / SettlementEvent continue to operate with implicit OCTO-W at wire scale 0.
- **Mixed-fleet (post-v3.5 deprecation window)**: nodes that have migrated enforce the new invariants (AssetRegistry lookup + scale-binding). Nodes that have not migrated do not enforce. The substrate MUST continue to wire-scale=0 for the wire form to avoid breaking legacy nodes.
- **Deprecated sites** (carrying `amount_dqa_micros: i64` on SelectorContext): the migration in RFC-0960 §6 documents these as out-of-scope (responsibility of RFC-0967-A1 InteropSelector amendment). The deprecation window is one substrate release cycle (≈6 weeks per RFC-0965 §4.1). After that cycle, `amount_dqa_micros` is REJECTED by `AssetRegistry`-aware nodes.
- **Defaults for legacy wire forms**: legacy `amount_dqa_micros` reads as `Dqa { value: amount_dqa_micros, scale: 0 }` with `asset_id = OCTO_W_ASSET_ID`. This matches the substrate today (wire scale=0, OCTO-W default).
- **REJECTED after one cycle (asset-binding bypass close)**: any capability, burn event, or settlement event carrying legacy form with no `asset_id` field AND `cost_asset_id != OCTO_W_ASSET_ID` (per RFC-0959 v2.8 §0, `VaultId(pub [u8;32])` is a frozen Layer-A tuple struct — the asset binding lives on the separate `cost_asset_id: AssetId` field introduced by RFC-0959 v2.8) is REJECTED by migrated nodes. The cross-RFC audit chain `PaymentCaveat.asset_id == BurnEventRef.asset_id == SettlementEvent.cost_asset_id` MUST hold or the event is REJECTED.
- **Cross-version replay window (Round 1 security review Threat #1, partial mitigation):** legacy wire forms (`amount_micro_octo_w: i64`, `"paid-query/v1"` caveat, `cost: { amount_micro_octo_w }` envelope) default to `asset_id = OCTO_W_ASSET_ID` at scale 0, opening a 6-week asset-binding bypass window. For non-OCTO-W contexts (vaults carrying USDC-mirror or BTC-mirror), the legacy form MUST be REJECTED from day 1 of the deprecation window — the asset-binding bypass window is 0 days for non-OCTO-W contexts. Add `LegacyFormOnNonOctoWContext` to the rejection reason enums in RFC-0965 §2.3, RFC-0960 §2.3, and RFC-0959 §2.3.

**Cross-RFC error-enum ownership (NEW in v3.5-r3):** `LegacyFormOnNonOctoWContext` is a variant ADDED to the rejection-reason error enums in EACH of the consuming RFCs:

- RFC-0965 §2.3 (`CapabilityExtensionError` / capability-rejection enum — owns its definition)
- RFC-0960 §2.3 (`BurnEventError` — owns its definition; consumers of burn-event submissions)
- RFC-0959 §2.3 (`SettlementEventError` — owns its definition; consumers of settlement submissions)

RFC-0105 (this RFC) does NOT own these error enums — RFC-0965, RFC-0960, and RFC-0959 each declare `LegacyFormOnNonOctoWContext` in their own §2.3 error definitions. The pointer here is normative: this RFC mandates the variant's presence and semantics (reject any legacy `amount_micro_octo_w` form when `asset_id != OCTO_W_ASSET_ID`), but the source-of-truth declaration lives in the consuming RFC.

## 6. Naming Cleanup

| Old                                                               | New                                                                                                                                                                                                                          | Substrate site                                                                                                                                                                        |
| ----------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MICRO_PER_OCTOW` (constant `Dqa { value: 1_000_000, scale: 0 }`) | `UNITS_PER_OCTO_W: Dqa = Dqa { value: 1_000_000, scale: 0 }` (type-preserving rename; `SCALE_OF_OCTO_W = 0u8` is derived, not separate)                                                                                      | `crates/quota-router-storage/src/ask.rs:41` + 3 call-sites at lines 51, 59, 60                                                                                                        |
| `OCTO_WAmount(pub Dqa)` (newtype)                                 | `AssetAmount { amount: Dqa, asset_id: AssetId }` (asset-generic newtype)                                                                                                                                                     | `crates/quota-router-storage/src/ask.rs:32,49,57,70,77`                                                                                                                               |
| `to_micro()` / `from_micro()`                                     | `OCTO_WAmount::to_wire_scale_dqa()` / `OCTO_WAmount::from_wire_scale_dqa()` (NEW in v3.5-r4 — disambiguated to avoid clash with the existing `to_wire`/`from_wire` semantics at `crates/octo-network/src/dot/envelope.rs:493`, `crates/octo-ident/src/lib.rs:90`, `crates/quota-router-cli/src/commands.rs:1145`)                                                                                                                                                                                                  | `crates/quota-router-storage/src/ask.rs:49,57`                                                                                                                                        |
| `DqaNewtype`                                                      | folded into `AssetAmount`                                                                                                                                                                                                    | `crates/quota-router-storage/src/ask.rs:34`                                                                                                                                           |
| `amount_dqa_micros: i64` (on SelectorContext)                     | (no change — owned by RFC-0967-A1)                                                                                                                                                                                          | `crates/octo-policy/src/policy_kinds.rs:263` + `workflow_kind.rs:313,461,477`                                                                                                         |
| `amount_micro_octo_w: Dqa` (on Escrow/EscrowSnapshot)             | `amount: Dqa` + `asset_id: AssetId` (asset-qualified)                                                                                                                                                                        | `crates/quota-router-core/src/marketplace/escrow.rs:159,172` + `crates/quota-router-storage/src/slash_store.rs:95` + `quota-router-core/tests/task_market.rs:397,452,464,469,600,667` |
| `RELAY_RATE_B_MICRO_OCTO_PER_GB: u64 = 100_000`                   | **(GREENFIELD — AssetRate not yet on disk; §3.1 amendment landing blocks this row)** `RELAY_BANDWIDTH_RATE_PER_GB: AssetRate` where `AssetRate { amount: Dqa { value: 100_000, wire_scale: 0 }, asset_id: OCTO_B_ASSET_ID }` | `crates/octo-network/src/porelay/economics.rs:131`                                                                                                                                    |

**Constants kept, type widened u64 → AssetRate (NEW in v3.5-r3):**

| Constant (no identifier change)                  | Type widening                                                                                                                                                                                  | Substrate site                                     |
| ------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| `OCTO_S_ARCHIVAL_COST_PER_BYTE: u64 = 1`         | **(GREENFIELD — AssetRate not yet on disk; §3.1 amendment landing blocks this row)** `OCTO_S_ARCHIVAL_COST_PER_BYTE: AssetRate` (paired with `OCTO_S_ASSET_ID`) — **No identifier change**     | `crates/octo-network/src/porelay/economics.rs:39`  |
| `UPTIME_BONUS_MAX_OCTO_N: u64 = 50_000_000`      | **(GREENFIELD — AssetRate not yet on disk; §3.1 amendment landing blocks this row)** `UPTIME_BONUS_MAX_OCTO_N: AssetRate` (paired with `OCTO_N_ASSET_ID`) — **No identifier change**           | `crates/octo-network/src/porelay/economics.rs:134` |
| `DIVERSITY_PREMIUM_OCTO_B_PER_PEER: u64 = 5_000` | **(GREENFIELD — AssetRate not yet on disk; §3.1 amendment landing blocks this row)** `DIVERSITY_PREMIUM_OCTO_B_PER_PEER: AssetRate` (paired with `OCTO_B_ASSET_ID`) — **No identifier change** | `crates/octo-network/src/porelay/economics.rs:137` |

**Substrate-call-site edits required for `MICRO_PER_OCTOW` rename (Round 1 type-change note):** the rename is type-preserving (`Dqa → Dqa`), but the call sites at `ask.rs:51,59,60` consume `MICRO_PER_OCTOW` as a `Dqa` operand in `Dqa::multiply` / `Dqa::divide`. After rename they MUST consume `UNITS_PER_OCTO_W` directly (no shape change required). The `LEGACY_MICRO_PER_OCTOW` alias is declared as `pub const LEGACY_MICRO_PER_OCTOW: Dqa = UNITS_PER_OCTO_W;` so the value lives in one place (Round 3 R3 DRY fix — previously a parallel declaration of `Dqa { value: 1_000_000, scale: 0 }` duplicated the literal; now the legacy alias points at `UNITS_PER_OCTO_W`). The alias retains type `Dqa` for the deprecation window so both coexist.

**Display-layer-only note (Round 1 cleanup rationale):** `UNITS_PER_OCTO_W = 1_000_000` is **display-layer only** — it exists for the `display_decimals = 6` conversion (10^6 micro-units per display unit). It MUST NOT be used in wire arithmetic: the on-wire form is `Dqa { value, wire_scale: 0 }` and `wire_scale` is the canonical magnitude, not the display denominator. Wallet/quota-router code that reads the registry MUST use `display_decimals` from `AssetMetadata` for the display conversion.

**Asset-binding requirement (Round 1 cleanup completeness):** any earnings constant feeding a settlement or burn path MUST carry an explicit `asset_id` (either via `AssetRate` struct or via a paired `*_ASSET_ID` companion) to satisfy the RFC-0959 / RFC-0960 audit invariants. The `porelay/economics.rs` module-header comment block (`crates/octo-network/src/porelay/economics.rs:115-124`) MUST be updated to reflect the asset-qualified naming; the `apply_por_earnings_boost` doc at line 192 names the old constant.

**Migration status:** §6 covers the in-scope cleanup for this RFC. `OCTO_WAmount` → `AssetAmount` retirement requires a follow-up RFC (out of scope for v3.5; tracked in `docs/audits/` once promoted).

## 7. Version History

| Version | Date       | Author                   | Note                                                                                                                                                                                                                                                                                                                   |
| ------- | ---------- | ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 3.0     | 2026-08-22 | @mmacedoeu + @cipherocto | Initial v3.x (sovereign/private boundary).                                                                                                                                                                                                                                                                            |
| 3.1     | 2026-08-22 | @mmacedoeu + @cipherocto | R3 fix-all §1/§2.1/§2.2 rewrite.                                                                                                                                                                                                                                                                                        |
| 3.2     | 2026-08-23 | @mmacedoeu + @cipherocto | R5 filename + date + anchor.                                                                                                                                                                                                                                                                                           |
| 3.3     | 2026-08-23 | @mmacedoeu + @cipherocto | R7 cite defects + drifts.                                                                                                                                                                                                                                                                                              |
| 3.4     | 2026-08-23 | @mmacedoeu + @cipherocto | R9 cross-RFC propagation.                                                                                                                                                                                                                                                                                             |
| 3.5-r1  | 2026-08-26 | @mmacedoeu               | Initial v3.5 draft (Round 1).                                                                                                                                                                                                                                                                                         |
| 3.5-r2  | 2026-08-26 | @mmacedoeu               | scale-table + BridgeRegistry + ChainNamespace + cache.                                                                                                                                                                                                                                                                 |
| 3.5-r3  | 2026-08-26 | @mmacedoeu               | kind_tag + NonceRegistry + BLS distinct-pubkey.                                                                                                                                                                                                                                                                        |
| 3.5-r4  | 2026-08-26 | @mmacedoeu               | Bridge slasher governance ceremony.                                                                                                                                                                                                                                                                                    |
| 3.5-r5  | 2026-08-26 | @mmacedoeu               | §3.12 + curator_set + co-sig + BLS canonicalization + ship-milestone. |
| 3.5-r6  | 2026-08-26 | @mmacedoeu               | §3.6.2 + co-sig + NonceError variants + replay enforcement. |
| 3.5-r7  | 2026-08-26 | @mmacedoeu               | Round 7-9: VH trim + DRY closure + Accepted promotion. |

## 8. Pending

- [ ] R2 adversarial review (substrate-fidelity + cross-RFC + security/public-chain + naming + spec-completeness lenses) after fix.
- [ ] Cross-reference validation via Guard 2 cite validator (RFC cross-refs only).
- [ ] **Substrate anchor verification (NEW):** run `scripts/verify-substrate-anchors.sh <rfc-path>` to confirm all `path/to/file.rs:LINE` references resolve to a valid line in the current substrate. Re-run before acceptance promotion.
- [ ] Cross-crate test vector: `AssetRegistry::metadata(OCTO_W_ASSET_ID)` returns `wire_scale = 0, display_decimals = 6, denomination = "micro-OCTO-W", kind = SovereignRoleToken, version = 1, tombstone = false`.
- [ ] Cross-crate test vector: `AssetRegistry::register(USDC_MIRROR_ASSET_ID, AssetMetadata { wire_scale: 6, kind: PrivateCorporateAsset, governance_pubkey: None, .. })` returns `Err(AssetError::GovernanceMissing)`.
- [ ] Cross-crate test vector: `AssetRegistry::register(USDC_MIRROR_ASSET_ID, AssetMetadata { wire_scale: 6, kind: SovereignRoleToken, governance_pubkey: Some(...), .. })` returns `Err(AssetError::KindNamespaceMismatch { expected: PrivateCorporateAsset, actual: SovereignRoleToken })`.
- [ ] Cross-crate test vector: `AssetRegistry::register(OCTO_W_ASSET_ID, AssetMetadata { wire_scale: 6, kind: SovereignRoleToken, .. })` returns `Err(AssetError::SovereignRoleToken)` (sovereign role tokens are hardcoded; runtime register() is never the entry path).
- [ ] Cross-crate test vector: `AssetRegistry::register(USDC_MIRROR_ASSET_ID, AssetMetadata { wire_scale: 19, kind: PrivateCorporateAsset, governance_pubkey: Some(pk), .. })` returns `Err(AssetError::ScaleOutOfRange { scale: 19 })` (defense-in-depth; MAX_SCALE = 18).
- [ ] Cross-crate test vector: `AssetRegistry::register(USDC_MIRROR_ASSET_ID, AssetMetadata { wire_scale: 6, kind: PrivateCorporateAsset, governance_pubkey: Some(pk), asset_name: "WRONG", .. })` returns `Err(AssetError::DerivationMismatch)` (`AssetId::derive(namespace_tag())` does not match the supplied asset_id).
- [ ] Cross-crate test vector: `AssetRegistry::register(USDC_MIRROR_ASSET_ID, AssetMetadata { wire_scale: 8, kind: PrivateCorporateAsset, governance_pubkey: Some(pk), .. })` returns `Err(AssetError::ScaleImmutable { existing: 6, proposed: 8 })` on re-registration with a different scale (Round 1 CRITICAL #1 mitigation).
- [ ] Cross-crate test vector: `AssetRegistry::register(tombstoned_asset_id, AssetMetadata { .. })` returns `Err(AssetError::AlreadyRevoked)` (revoked entries cannot be re-registered).
- [ ] Cross-crate test vector: `AssetRegistry::revoke(live_asset_id, &invalid_sig)` returns `Err(AssetError::NotRevoked)` when the entry is live AND the signature does not verify; or `Err(AssetError::GovernanceSignatureInvalid)` if the entry is live AND the signature does not verify.
- [ ] Cross-crate test vector: `CachedAssetRegistry::metadata(OCTO_W_ASSET_ID)` on a cold cache returns `Err(AssetError::BoundedCacheMiss)` (caller retries against the live registry).
- [ ] Cross-crate test vector: `AssetRegistry::register(BRIDGED_ASSET_ID, AssetMetadata { kind: BridgedExternalAsset, governance_pubkey: Some(pk), .. })` where `bridge_id` is not in `BridgeIdentityRegistry` returns `Err(AssetError::BridgeUnknown { bridge_id })`.
- [ ] Cross-crate test vector: `AssetRegistry::register(BRIDGED_ASSET_ID, AssetMetadata { kind: BridgedExternalAsset, governance_pubkey: Some(non_curator_pk), .. })` returns `Err(AssetError::CuratorNotInBridgeSet { bridge_id, claimed_pubkey: Some(non_curator_pk) })`.
- [ ] Cross-crate test vector: `BridgeIdentityRegistry::slash(bridge_id, BridgeCuratorSlashingCondition::FalseAttestation { bridge_id, claim: vec![] })` with merkle-inclusion proof of failed external claim produces the slashing event and redistributes `slashing_stake` per policy.
- [ ] Cross-crate test vector: `BridgeIdentityRegistry::slash(bridge_id, BridgeCuratorSlashingCondition::DoubleSign { bridge_id, epoch: 42, sig_a: vec![0x01; 48], sig_b: vec![0x02; 48] })` requires both attestations to be on-record for the same `(bridge_id, epoch)` with conflicting claims; produces slashing event on success.
- [ ] Cross-crate test vector: `BridgeIdentityRegistry::slash(bridge_id, BridgeCuratorSlashingCondition::KeyCompromise { bridge_id, rotated_at: 1000 })` requires rotation evidence post-dating the contested attestation; produces slashing event on success.
- [ ] Cross-crate test vector: `NonceRegistry::observe(&pk, &nonce)` returns `Ok(())` on first call and `Err(NonceError::AlreadyObserved { pk, nonce, prior_height: H })` on second call with the same `(pk, nonce)` tuple (replay rejection).
- [ ] Acceptance promotion (7-day minimum review + 2 maintainer approvals per CLAUDE.md Branch Strategy).

---

**End of RFC-0105 v3.5 (Accepted 2026-08-26).**
