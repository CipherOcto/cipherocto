# 0105-v35-asset-registry-nonce-registry-substrate — RFC-0105 v3.5 canonical Layer B substrate

**Status:** Open
**Substrate:** RFC-0105 v3.5 §3.1 + §3.4 + §3.5 + §3.6 + §3.11 + §3.12 (canonical Layer B additive surface for all consumer RFCs)
**Parent:** RFC-0105 v3.5 §3 + RFC-0965 v2.1 §2.1 + RFC-0960 v3.6 §2.1 + RFC-0959 v2.8 §2.1 (canonical homes cited)
**Depends on:** RFC-0105 v3.5 (Accepted 2026-08-26); RFC-0862 §Substrate types for Dqa wire form

## Scope

Land the **canonical Layer B additive substrate** that RFC-0105 v3.5 §3.x
allocates as the single-source-of-truth for asset registry, bridge identity
registry, nonce registry, typed newtypes, cryptographic primitives, and the
sovereign-nonce namespace helper. Every consumer RFC (RFC-0965 v2.1,
RFC-0960 v3.6, RFC-0959 v2.8, RFC-0960 v3.7) imports from these anchors;
none re-declares the types locally (RFC-0105 v3.5 §3.8 single-source-of-truth
rule at L545 + §3.12 L663).

### Mission D sub-steps

1. **`AssetMetadata` struct + `AssetKind` enum + `MAX_SCALE` const** —
   `crates/octo-vault/src/asset_registry.rs` (NEW). Per RFC-0105 v3.5 §3.1
   L114-135. `MAX_SCALE: u8 = 18` is **re-exported** here (canonical home
   per §3.1 L114; substrate DFA value lives at `determin/src/dqa.rs:13` —
   this const is the asset-side mirror, NOT a re-binding of DFA
   MAX_SCALE). `AssetKind` has 4 variants:
   `SovereignRoleToken` / `PrivateCorporateAsset` / `BridgedExternalAsset`
   / `WrappedCrossChainAsset` (L116-121).
   `AssetMetadata` fields per L123-135. `namespace_tag()` impl per L143-160
   (canonical derivation input for `AssetId::derive`). `kind_tag()` impl
   per L164-171 (discriminant byte 0x01..0x04 for body_hash commitment).

2. **`AssetRegistry` trait + `AssetError` enum** — same file.
   Per L174-210. 4 methods (`metadata` / `register` / `revoke` /
   `rotate_governance`). `AssetError` has 14 variants covering lookup miss,
   scale validation, derivation mismatch, namespace-prefix cross-check,
   governance key requirements, revocation state, LRU cache miss, and the
   two NEW bridge-forgery variants (`BridgeUnknown`, `CuratorNotInBridgeSet`).

3. **`register()` scale-immutability + bridge-forgery triple check** —
   same file. Per §3.2 L217-277. REJECTS re-registration with different
   `wire_scale`. REJECTS sovereign role token runtime registration.
   REJECTS missing `governance_pubkey` on non-sovereign assets. Validates
   `AssetId::derive(metadata.namespace_tag()) == asset_id`. Cross-checks
   namespace-prefix / kind. For BRIDGED/WRAPPED kinds: parses bridge_id
   from namespace_tag prefix, resolves via `BridgeIdentityRegistry`,
   verifies curator set contains `metadata.governance_pubkey` (3 checks
   close bridge-forgery vector per §3.2 L280-286).

4. **Revocation + governance rotation** — same file. Per §3.3 L288-292.
   `revoke()` flips `tombstone = true` (historical events still resolve,
   NEW events REJECT). `rotate_governance()` requires OLD governance
   pubkey co-signature (prevents unilateral takeover; bumps `version`,
   stores `prev_commitment`).

5. **Chain-of-trust commitment** — same file. Per §3.4 L294-314. Length-
   prefixed BLAKE3-256 commitment over `wire_scale || display_decimals ||
len-prefixed(denomination) || len-prefixed(symbol) || kind_tag ||
governance_pubkey || chain_id || len-prefixed(asset_name) ||
(version - 1)`. Version-bump semantics: any of the listed fields
   changing MUST bump `version` and produce new `prev_commitment`.

6. **`CachedAssetRegistry` + bounded LRU + registry-snapshot epoch** —
   same file or NEW `crates/octo-vault/src/cached_asset_registry.rs`.
   Per §3.5 L316-352. `LruCache<AssetId, (AssetMetadata, u64)>`. TTL =
   `current_snapshot_epoch() - snapshot_epoch < ttl_epochs`. `Err(
AssetError::BoundedCacheMiss)` on cache miss (Round 1 DoS mitigation;
   variant is REACHABLE in v3.5-r3 per L354).

7. **`BridgeIdentityRegistry` trait + `BridgeIdentity` struct +
   `BridgeError` enum + `BridgeCuratorSlashingCondition` enum** —
   `crates/octo-vault/src/bridge_identity_registry.rs` (NEW). Per §3.6
   L356-474. `BridgeIdentity` fields per L374-382. `BridgeError` has 8
   variants (L396-435) covering bridge lookup miss, quorum attestation
   failure, external-chain mismatch, revocation state, curator-set check,
   missing governance co-signature, retroactive compromised-key flag, and
   co-signature audit attribution. `register()` performs 4-step
   verification (L488-496): BLS pubkey canonicalization (sign-bit → 0x80,
   idempotent; L491), distinct-pubkey check (rejects duplicates; L492),
   pairwise signature verification (L493), aggregate pairing check (L494).
   First-time `register()` of NEW `bridge_id` REQUIRES Layer A root-key
   co-signature (L498); subsequent `rotate_curators()` does NOT.
   `slash()` redistributes `slashing_stake` 50/30/20 (burn/curators/reporter
   per L514-519). `BridgeCuratorSlashingCondition` has 3 variants
   (`FalseAttestation` / `DoubleSign` / `KeyCompromise`).

8. **`BridgeChainNamespace` enum** — `crates/octo-vault/src/bridge_chain_namespace.rs`
   (NEW). Per §2.4 L84-95. 5 variants:
   `Mainnet` / `Testnet` / `Devnet` / `Sidechain` / `Other`. NO parallel
   abstraction with existing `pub enum ChainNamespace { Rfc, User }` at
   `crates/octo-policy/src/policy_kinds.rs:54-70` (no-parallel-abstractions
   principle per RFC-0105 v3.5 §2.4 L84).

9. **`NonceRegistry` trait + `NonceError` enum** — `crates/octo-vault/src/nonce_registry.rs`
   (NEW). Per §3.11 L569-633. 2 methods (`observe` / `observe_readonly` —
   latter renamed in v3.5-r5 per L585-587). `NonceError` has 3 variants:
   `AlreadyObserved { pk, nonce, prior_height }` / `PersistenceFailure`
   (NEW in v3.5-r5 for WAL failure surfacing; L603-613) / `WalRecovering`
   (NEW in v3.5-r6 for outage recovery; L615).

10. **`StoolapNonceRegistry` impl (WALPrimary)** — same file. Per §3.11
    L629-633. Persists via cipherocto-fork stoolap with WAL-primary write
    semantics. `crates/octo-vault/src/nonce_registry.rs` header comment
    MUST declare `WALPrimary` and reference `crates/stoolap/src/persistence/wal.rs`.
    Bounded LRU per `governance_pubkey` with capacity `~10^6 entries per
pubkey` (L629). TTL tied to asset revocation grace period.

11. **`InMemoryNonceRegistry` impl (test-only)** — same file. Per §3.11
    L631. Gated by `#[cfg(test)]`. Production binaries MUST NOT link.
    Persists zero observations across process restart — this is the
    documented restart-window replay vector (L633 acceptance-promotion
    checklist item).

12. **`newtypes` module** — `crates/octo-vault/src/newtypes.rs` (NEW).
    Per RFC-0965 v2.1 §0 + RFC-0960 v3.6 §2.1 L54 + RFC-0959 v2.8 §2.1 L50.
    3 types:

    ```rust
    pub struct Nonce(pub [u8; 32]);
    pub struct Epoch(pub u64);
    pub struct GovernanceSignature {  // RFC-0105 §3.6 L469-473
        pub root_key_fingerprint: [u8; 32],
        pub epoch: u64,
        pub sig: [u8; 64],
    }
    ```

13. **`verify_governance_signature` + `blake3_hash`** — `crates/octo-cap-macaroon/src/crypto_primitives.rs`
    (NEW) OR amend `crates/octo-cap-macaroon/src/lib.rs`. Per RFC-0105
    v3.5 §3.12 L641-660. Canonical home is **octo-cap-macaroon** (Layer A
    substrate); `octo_vault` re-exports for consumer convenience per
    §3.12 L647 + L656. `blake3_hash(data: &[u8]) -> [u8; 32]` uses
    `blake3::hash(data).into()` — NOT `blake3::hash(data).as_bytes()` at
    call sites (L658). Single-source-of-truth rule per L663 — consumers
    MUST NOT re-declare.

14. **`sovereign_nonce_namespace` helper** — `crates/octo-vault/src/bridge_chain_namespace.rs`
    OR NEW `crates/octo-vault/src/sovereign_nonce.rs`. Per RFC-0965 v2.1
    §2.4 L383-393. Domain-separated hash:
    `blake3_hash(b"octo:sovereign-nonce-ns:v1" || asset_id.0) -> [u8; 32]`.
    Domain string `"octo:sovereign-nonce-ns:v1"` MUST be globally unique
    across all substrate uses of blake3 (L386-387).

15. **Sovereign role token hardcoding at startup** — `crates/octo-vault/src/sovereign_role_tokens.rs`
    (NEW). Per §3.7 L536-539 + §2.1 (sovereign namespace table). The
    table of OCTO-A/B/D/M/N/O/S/H/W is loaded at process init from §2.1.
    `AssetRegistry::register()` REJECTS `kind == SovereignRoleToken` via
    `AssetError::SovereignRoleToken`. version = 1, no `prev_commitment`,
    no `governance_pubkey` for these entries.

### Cargo deps (add to `crates/octo-vault/Cargo.toml`)

- `blake3 = "<version>"` (BLAKE3-256 hash function for namespace_tag +
  blake3_hash + sovereign_nonce_namespace)
- `lru = { version = "0.12", features = ["std"] }` (bounded LRU per
  §3.5 L323)
- `borsh = { version = "<version>", features = ["derive"] }` (wire form
  per §3.9 L547-565)
- `serde = { version = "<version>", features = ["derive"] }`
- `bls12_381 = "<version>"` (BLS12-381 aggregated signatures for bridge
  quorum per §3.6 L362-364; verify exact version pin at landing)
- `thiserror = "<version>"` (error enum derives)

### Cargo deps (add to `crates/octo-cap-macaroon/Cargo.toml`)

- `blake3 = "<version>"` (canonical home for `blake3_hash` per §3.12 L657)
- `ed25519-dalek = { version = "<version>", features = ["serde"] }`
  (governance signature scheme per §3.12 L644-652)

## Test Vectors

Per RFC-0105 v3.5 §3.1+§3.4+§3.5+§3.6+§3.11+§3.12 spec blocks. Selectors:

- TV-AS1: `AssetMetadata::namespace_tag()` for `SovereignRoleToken`
  symbol=`"OCTO-W"` returns `b"cipherocto/asset/v1/OCTO-W"` verbatim
- TV-AS2: `kind_tag()` returns 0x01 for sovereign, 0x02 for private,
  0x03 for bridged, 0x04 for wrapped
- TV-AS3: `register()` with `wire_scale = 19` returns
  `AssetError::ScaleOutOfRange { scale: 19 }` (defense-in-depth; MAX_SCALE = 18)
- TV-AS4: `register()` re-registration with different `wire_scale`
  returns `AssetError::ScaleImmutable { existing, proposed }`
- TV-AS5: `register()` with `AssetId::derive(metadata.namespace_tag()) != asset_id`
  returns `AssetError::DerivationMismatch`
- TV-AS6: `register()` for BRIDGED kind with `bridge_id` not in
  `BridgeIdentityRegistry` returns `AssetError::BridgeUnknown { bridge_id }`
- TV-AS7: `register()` for BRIDGED kind with `governance_pubkey` not in
  bridge's curator set returns
  `AssetError::CuratorNotInBridgeSet { bridge_id, claimed_pubkey }`
- TV-AS8: `register()` for sovereign role token returns
  `AssetError::SovereignRoleToken`
- TV-AS9: chain-of-trust commitment length-prefix encoding — two distinct
  metadata tuples produce different commitments (no concat-ambiguity
  collision)
- TV-AS10: `CachedAssetRegistry::metadata()` cache miss returns
  `AssetError::BoundedCacheMiss` (variant REACHABLE per v3.5-r3)
- TV-AS11: `CachedAssetRegistry` TTL expiry triggers live-registry lookup
  - cache repopulation
- TV-BR1: `BridgeIdentityRegistry::register()` with duplicate BLS pubkeys
  returns `BridgeError::QuorumAttestationInvalid` (distinct-pubkey check
  per §3.6 L492)
- TV-BR2: `BridgeIdentityRegistry::register()` with non-canonical BLS
  pubkey (sign bit set) returns `BridgeError::QuorumAttestationInvalid`
  (canonicalization step per L491)
- TV-BR3: First-time `register()` of NEW bridge without Layer A root-key
  co-signature returns `BridgeError::GovernanceAttestationMissing`
- TV-BR4: `rotate_curators()` succeeds WITHOUT co-signature (only
  curator-quorum threshold required for rotation)
- TV-BR5: `BridgeIdentity` registered at `registering_epoch = 100` whose
  root key rotated at `epoch = 150` resolves `Ok(bridge_identity)` +
  `retroactive_review(bridge_id, suspected_epoch: 100)` flags
  `BridgeError::RegisteredUnderCompromisedKey`
- TV-NR1: `NonceRegistry::observe(pk, nonce)` first call returns `Ok(())`
- TV-NR2: `NonceRegistry::observe(pk, nonce)` second call with same
  (pk, nonce) returns `Err(NonceError::AlreadyObserved)`
- TV-NR3: `observe_readonly(pk, nonce)` does NOT mutate the registry
  (verified by subsequent `observe` succeeding)
- TV-NR4: WAL failure during `observe` returns `NonceError::PersistenceFailure`
  (canonical StoolapNonceRegistry impl per §3.11 L607-613)
- TV-NR5: Sovereign-asset namespace fallback —
  `sovereign_nonce_namespace(asset_id)` returns `blake3_hash(b"octo:sovereign-nonce-ns:v1" || asset_id.0)`,
  distinct from any ed25519 pubkey derivation
- TV-NR6: `InMemoryNonceRegistry` is `#[cfg(test)]`-gated — production
  builds MUST NOT link (verified via `cargo build --release` symbol scan)
- TV-CP1: `verify_governance_signature(sig, msg, pk)` returns `true` for
  valid ed25519 signature; `false` for malformed sig or wrong pubkey
- TV-CP2: `blake3_hash(data)` returns canonical 32-byte form
  (NOT `blake3::hash(data).as_bytes()`)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-vault` (Layer B) — `asset_registry` + `bridge_identity_registry` +
  `nonce_registry` + `bridge_chain_namespace` + `newtypes` + sovereign
  role tokens = **all Layer B additive, semver-minor**
- `octo-cap-macaroon` (Layer B frozen substrate) — `crypto_primitives` =
  **Layer B-additive cryptographic primitive surface**; consumers MUST
  import via `octo_cap_macaroon::{verify_governance_signature, blake3_hash}`
  OR via `octo_vault` re-export (both paths resolve to the same canonical
  octo_cap_macaroon definition per §3.12 L663)
- BLS12-381 quorum verifier is substrate-mandated across all nodes
  (split-chain equivalence per §3.6 L502)
- No cross-layer inversion

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features full -- -D warnings
cargo test --workspace --lib
cargo test --workspace  # StoolapNonceRegistry WAL tests; bridge quorum tests
cargo test -p octo-vault --lib nonce_registry  # InMemoryNonceRegistry (test-only)
```

**Cargo dep audit:** verify `bls12_381` is sourced from a pinned substrate
version per [[feedback_stoolap_persistence]] (CipherOcto fork pin). If
non-substrate: BLOCK landing, surface to user per `feedback_initiation_user_only`.

## Backward compat

- All new types are additive (no existing crate modification)
- `AssetId(pub [u8; 32])` UNCHANGED per §3.8 L541
- `VaultId(pub [u8; 32])` UNCHANGED
- `Dqa` UNCHANGED
- `MAX_SCALE: u8 = 18` re-export at `octo_vault::asset_registry::MAX_SCALE`
  is a NEW re-export; the DFA substrate const at `determin/src/dqa.rs:13`
  is the source — Mission D adds a `pub use` to surface it for §3.1 spec
  compliance, NOT a parallel declaration
- `InMemoryNonceRegistry` is `#[cfg(test)]`-only — production builds
  MUST NOT link (acceptance-promotion checklist item per §3.11 L633)

## Cross-references

- RFC-0105 v3.5 §2.1 — sovereign namespace table (canonical hardcoded entries)
- RFC-0105 v3.5 §2.4 — BridgeChainNamespace definition (L84-95)
- RFC-0105 v3.5 §3.1 — AssetRegistry trait + AssetMetadata + AssetKind + AssetError + MAX_SCALE
- RFC-0105 v3.5 §3.2 — scale-immutability + bridge-forgery triple check (L217-286)
- RFC-0105 v3.5 §3.3 — revocation + governance rotation
- RFC-0105 v3.5 §3.4 — chain-of-trust commitment (length-prefix BLAKE3)
- RFC-0105 v3.5 §3.5 — CachedAssetRegistry + LRU + TTL
- RFC-0105 v3.5 §3.6 — BridgeIdentityRegistry + BLS12-381 quorum + co-signature
- RFC-0105 v3.5 §3.6.1 — bridge slashing governance (50/30/20 redistribution)
- RFC-0105 v3.5 §3.6.2 — Layer A root-key governance (rotation ceremony)
- RFC-0105 v3.5 §3.7 — population semantics (sovereign hardcoded at startup)
- RFC-0105 v3.5 §3.8 — single-source-of-truth rule (L545; consumers MUST import)
- RFC-0105 v3.5 §3.9 — wire form (borsh 100+ bytes)
- RFC-0105 v3.5 §3.11 — NonceRegistry + StoolapNonceRegistry + WALPrimary (L569-633)
- RFC-0105 v3.5 §3.12 — Cryptographic Primitives canonical home (L641-663)
- RFC-0105 v3.5 §3.13 — tri-invariant declaration
- RFC-0862 §Substrate types — DqaEncoding wire form (16-byte BE)
- RFC-0965 v2.1 §2.1 — PaymentCaveat asset_id + newtypes imports (L53-57)
- RFC-0965 v2.1 §2.3 — verify() scale-binding + NonceRegistry key (L283-285)
- RFC-0965 v2.1 §2.4 — NonceRegistry key + legacy-form rejection (L383-393)
- RFC-0960 v3.6 §2.1 — BurnEventRef imports from Mission D substrate (L53-60)
- RFC-0959 v2.8 §2.1 — SettlementEvent imports from Mission D substrate (L50-56)
- RFC-0960 v3.7 — VaultBalanceProjection consumes AssetRegistry + newtypes
- [[cipherocto-design-principles]] — Layer B additive-only rule
- [[feedback_stoolap_persistence]] — CipherOcto fork pin
- Mission A (`0960-v37-a-vault-balance-projection-substrate.md`) — depends on Mission D for AssetRegistry/MAX_SCALE/newtypes imports
- Mission B (`0960-v37-b-event-log-producer-wiring.md`) — depends on Mission D for producer boundary types
- Mission E (`0965-v21-payment-caveat-asset-binding-substrate.md`) — depends on Mission D
- Mission F (`0960-v36-burn-event-dqa-migration-substrate.md`) — depends on Mission D
- Mission G (`0959-v28-settlement-cost-dqa-migration-substrate.md`) — depends on Mission D

## Ship milestone (RFC-0105 v3.5 §3.11 L633)

`StoolapNonceRegistry` MUST land before v0 promotion to Accepted.
Until landing, v0 drafts UNSAFE for production deployment — operators
MUST attest (per `docs/audits/v0-nonce-registry-attest.md`) that the
restart-window replay vector is acceptable. Acceptance-promotion
checklist item.

## Claimant

@unassigned

## Pull Request

#
