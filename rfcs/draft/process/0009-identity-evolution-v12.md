# RFC-0009 v1.2 — Identity Evolution: Hierarchical Attenuation + MPC Threshold

**Status:** Draft (2026-08-10)
**Author:** @cipherocto + @mmacedoeu
**Maintainers:** @cipherocto (primary), @mmacedoeu (review)
**Substrate:** RFC-0009 §Capability Keys + §HsmAdapter Integration
**Parent:** Mission `0957-f-v2-bundle` (V2 spec per R8 H1 rewrite)

> **Promotion note:** In-place additive amendment to RFC-0009. This
> RFC ratifies two capability-key extensions originally listed as
> §Future Work items: "Capability attenuation protocols beyond
> pairwise" (parent → child → grandchild chains) and "MPC threshold
> identity" (RFC-0853 §F3).

> **Breaking changes acknowledged (per R4-R11):** See §Breaking
> Changes + §Acceptance Criteria for the migration contract.

## Summary

Extend RFC-0009 §Capability Keys with:

1. **Hierarchical attenuation chains.**
2. **MPC threshold identity.**

## Review State

- **R1-R38 completed (2026-08-11); R39 in progress.**
- **Termination condition:** convergence when a new round returns
  zero NEW findings.

## Breaking Changes (10 main items + BC#9 ADDITIVE mirrored per
R28 M3 / R30 L3 / R31 L1 — corrected language per R11 M6 + R25 L4;
BC#1 expanded into 9 sub-items (1a-1i); section totals 20 numbered
items (9 sub-items BC#1a-1i + 11 main items BC#2-12 including BC#9
mirrored to §Additive Changes) per R18 M6 + R24 L3 + R25 L4 +
R27 L2 + R30 L3 + R31 L1)

1a. **NEW: `ThresholdSigner::threshold_sign` method** (replaces
    `sign_combined`; no prior equivalent exists).
1b. **NEW enum: `ThresholdShare`** (replaces `KeyShare`; no prior
    equivalent exists).
1c. **NEW type: `BoundedShareVec`** (replaces `Vec<KeyShare>`; no
    prior equivalent exists).
1d. **NEW error: `ThresholdError`** (replaces `MpcError`; no prior
    equivalent exists).
1e. **NEW error variant: `HsmError::ThresholdSignerRequired`** (per
    R13 M1 — fail-closed sentinel for `IdentityKey::sign` when
    `threshold_signer` configured).
1f. **RENAMED: `ThresholdSigner::threshold()` + `share_count()` →
    single `threshold_params() -> (M, N)`** (per R13 M3 — actual
    `crates/octo-wallet/src/mpc.rs` §ThresholdSigner has 4 methods
    including `threshold` + `share_count`; spec replaces with
    single tuple-returning method).
1g. **NEW trait: `ThresholdCoordinator`** (per R13 L4 — `async
    collect_shares` + `aggregate` methods; RFC-0853 §F3 substrate
    contract).
1h. **NEW method: `IdentityKey::sign_threshold`** (per R13 M5 —
    companion to fail-closed `sign` from BC#11; dispatches to
    `threshold_signer.threshold_sign`).
1i. **NEW method: `ThresholdSigner::group_public_key()`** (per
    R12 M7 — group PK exposed for `Authorization::ThresholdSignature`
    aggregated_pk_check).
2. **`Xor2Of3Signer` additive `threshold_signer` field.**
3. **`CapabilityBundleV2` NEW struct.**
4. **`IdentityKey` 9 existing fields UNCHANGED + 3 new fields
   ADDED** (per R9 H1 + R16 M4 + R23 H1 — `threshold_signer` +
   `coordinator` + `shareholders` fields added by R9 H1 / R15 M3 /
   R19 M2 respectively; `warned_threshold_misconfig` field removed
   per R22 M2 as write-only dead code).
5. **`Authorization::ThresholdSignature` struct variant EXTENDED
   in RFC-0871** (per R11 M3 + R12 H3 — ownership belongs to
   RFC-0871; this RFC documents the EXTENSION SCHEMA only;
   `aggregated_pk_check` + `dkg_proof` fields + verifier MUST
   semantics land in `crates/octo-cap-threshold-mpc/` per
   RFC-0871 §Future Work).
   - **Per R13 M2:** RFC-0871 §Future Work currently lists
     "Threshold-MPC for high-value transitions (BLS via
     `Authorization::ThresholdSignature`)" but does NOT carry the
     `aggregated_pk_check` + `dkg_proof` concrete fields. The
     forward-pointer is dangling. Resolution: RFC-0871 amendment
     (out of scope for RFC-0009 v1.2) MUST add the concrete
     fields to §Future Work OR §Specification before v1.2
     acceptance of this RFC. Until then, this RFC's cross-ref is
     ASPIRATIONAL.
   - **Per R25 H1:** softened — this is a SHOULD not a hard MUST;
     RFC-0009 v1.2 documents the extension schema; verifier MUST
     semantics gated by AC#9 (RFC-0871 amendment FILED before
     RFC-0009 v1.2 promotion to Accepted).
6. **`BoundedShareVec::new` enforces m == threshold.**
7. **`HsmAdapter::get_public_key` return type change** (per R18 L8 —
   from `[u8; 32]` to `Result<[u8; 32], HsmError>`; device transport
   can fail).
8. **`derive_capability_key` 4-param signature.**
9. **NEW: `OperatorId` struct + `pubkey()` method** (per R15 L4 —
   OperatorId is NEW in v1.2; not present in v1.0/v1.1 substrate;
   per R28 M3 — ADDITIVE change; mirrored to §Additive Changes below
   since no prior identifier or symbol is shadowed).
10. **`HsmAdapter::sign` NON-BREAKING** — current code uses
    `Result<[u8; 64], HsmError>` per `HsmAdapter::sign` symbol in
    `crates/octo-wallet/src/hsm.rs` + v1.1 baseline (per R11 C1).
    R10 H15 framing reversed; pseudocode was wrong.
11. **`IdentityKey::sign` MUST fail-closed when `threshold_signer`
    configured** (per R11 M4) — refactor: change from "doesn't
    branch" to "fails with `ThresholdSignerRequired`".
12. **NEW: `Authorization::FrostSignature` variant** (per R18 M5 +
    R20 L6 + R25 L4) — FROST Ed25519 signatures with `signers`,
    `sig`, `group_pk` + forward-pointers `nonce_proof` +
    `dkg_proof` per RFC-0871 §Future Work (parallel pattern to
    `Authorization::ThresholdSignature` BC#5). Closes coverage
    gap for FROST-signed envelopes.

## Additive Changes (per R28 M3 + R29 M1 + R33 M1)

Per R28 M3 + R29 M1 + R33 M1: BC#9 (`OperatorId` struct +
`pubkey()` method) is ADDITIVE, not BREAKING (no prior identifier
or symbol shadowed; OperatorId is fresh in v1.2 substrate).
**Mirrored to** this section (per R33 M1 — "Moved here" wording
contradicted §Breaking Changes intro "mirrored" semantic; corrected).
Other additive items in §Breaking Changes (BC#1a-1i NEW
types/methods/errors, BC#2 additive field, BC#3 NEW struct,
BC#4 3 new fields ADDED, BC#6 NEW fn on NEW struct, BC#12 NEW
variant) may also be listed here — not required, but documents
additive/non-breaking scope explicitly.

- **9-ADDITIVE. `OperatorId` struct + `pubkey()` method** (per
  R15 L4 + R28 M3) — fresh identifier in v1.2 substrate;
  mirrored from §Breaking Changes BC#9.

## Design Goals

| Goal | Target | Metric |
| ---- | ------ | ------ |
| G1 | Chain depth bounded | ≤ 8 levels (W3C VC-DID best practice) |
| G2 | Unlinkability | v1 root ↔ v2 child cryptographically independent |
| G3 | Threshold signing latency | ≤ 2x single-key for 2-of-3 |
| G4 | Key-share loss tolerance | (N − M) lost shares recoverable (per R14 L1 — off-by-one; M-of-N tolerates N−M losses, not N−M+1) |
| G5 | Cascading revocation | Cryptographic walk |
| G6a | Root mint back-compat | v1.1 callers via `derive_capability_key_v11` shim |
| G6b | Child mint is new | 4-param signature |
| G7 | Migration | 5 commits per R18 H1 (Commits 1-5 listed in §Implementation Phases) + atomic Phase 2 per RFC-0870 §NodeEnvelope Adoption |

## Acceptance Criteria

This RFC is ready for promotion to Accepted when:

1. **Phase 0+1 complete** (per R18 H1, R24 M2 — Phase 0+1 covers
   Commits 1-3; Commits 4-5 covered by AC#4 + AC#5 below):
   - Commit 1: `ThresholdSigner::threshold_sign` NEW +
     `BoundedShareVec::new` (enforces m == threshold) +
     `ThresholdCoordinator` trait interface + Cargo deps pinned
     in `crates/octo-wallet/Cargo.toml` + `HsmAdapter::get_public_key`
     signature update (per R18 M4 — phase boundary matches
     §Implementation Phases Commit 1) + `OperatorId` struct (per
     R33 L4) + `OperatorId::pubkey()` method (per R19 L3)
     + Clippy lint registration in
     `clippy.toml` (per R21 L6 + R25 M2) + `[[test]]` entry in
     Cargo.toml mapping `frost_nonce_determinism` to
     `tests/integration/frost_nonce_determinism.rs` (per R22 M3
     + R25 M2).
   - Commit 2: `BLS12381ThresholdSigner` +
     `SchnorrThresholdSigner` impls + `Xor2Of3Signer` additive
     field + `IdentityKey::sign` fail-closed when `threshold_signer`
     configured + `InMemorySigner` + `LedgerSigner` + `YubiHsmSigner`
     updated (per R20 L5 — §Implementation Phases Commit 2).
   - Commit 3: `derive_capability_key` 4-param + v11 shim +
     Phase 1 TV functions.
2. **Mission `0957-f-v2-bundle` V2 work complete** (per R25 L7 —
   scope: V2 spec authoring, `CapabilityBundleV2` + `CapabilityTokenV2`
   structs per §Forward compatibility; AC#8 covers mission §Migration
   field separately — NO overlap with AC#4-5 which cover V2 wire
   form + atomicity).
3. **`tests/fixtures/phase1_tv.json` exists** (RFC-0009 TV-1..3 per
   R10 H3 disambiguation).
4. **Phase 2 V2 wire form complete:** `CapabilityBundleV2` struct
   defined.
5. **Phase 3 atomic with Phase 2** per RFC-0870 §NodeEnvelope
   Adoption.
6. **`cargo test -p octo-wallet --lib -- --list phase1_tv_json | grep
   -qE "phase1_tv_json_(v11_round_trip_equivalence|child_unlinkability|hsm_boundary_no_seed_exfil)"`**
   passes (per R11 H2 + R22 L6 — enumerates all 3 TV functions;
   previous grep pattern was too loose).
7. **RFC-0870 §NodeEnvelope:PayloadKindId ordering:** all 7
   `PayloadKindId` UUIDs use V2 field ordering.
8. **Mission `0957-f-v2-bundle` §Migration complete** (per R25 L7 —
   scope: consumer migration to V2 wire form across wallet +
   capability issuer + `octo-cap-macaroon` + `octo-cap-zk`;
   distinct from AC#2 mission V2 authoring scope).
8b. **`tests/integration/frost_nonce_determinism.rs` exists** (per R20 M3)
    and asserts 100K ops produce deterministic nonces; gated by
    `cargo test -p octo-wallet --test frost_nonce_determinism`
    (per R21 L4 — file path `tests/integration/...` requires
    `[[test]]` entry in Cargo.toml mapping binary name
    `frost_nonce_determinism` to the file path; cargo `--test`
    flag matches binary name, NOT file basename).
9. **RFC-0871 amendment FILED** (per R25 H1) with concrete
    `aggregated_pk_check` + `dkg_proof` fields + verifier MUST
    semantics added to §Future Work OR §Specification, BEFORE
    RFC-0009 v1.2 promotion to Accepted. Until then, BC#5
    forward-pointer is ASPIRATIONAL (downgraded from MUST per
    R25 H1).
10. **RFC-0871 amendment covers BC#12 `nonce_proof` field**
    (per R28 M5) for FROST signatures; gates parallel to AC#9
    for BC#5 `aggregated_pk_check` + `dkg_proof`. FROST
    forward-pointer ASPIRATIONAL until AC#10 satisfied.

## Motivation

Mission `0957-f-v2-bundle` (V2 spec per R8 H1 rewrite) requires
the bundle struct encoding `CapabilityTokenV2 + HolderRecord +
DischargeMacaroon` triplet to embed the full attenuation chain.

## Dependencies

**Requires:**

- RFC-0009 (identity substrate)
- RFC-0853 §F3
- **RFC-0871 §Specification** (per R10 H5 — `pub enum Authorization`
  variant; this RFC cross-references the extension; per R11 M3 —
  ownership belongs to RFC-0871)

**Optional:** RFC-0958

## Roles and Authorities

| Role | Identifier | Authority Scope | Lifecycle | Source |
|------|------------|-----------------|-----------|--------|
| Identity Holder | `IdentityHolderId` | Owns IdentityKey; revokes | Persistent | This RFC v1.2 |
| HSM | `Arc<dyn HsmAdapter>` | Persists IdentityKey | Persistent per device | RFC-0009 §HsmAdapter Integration |
| IdentityKey (logical) | `IdentityKey` (base + additive fields) | Sign; derive capability keys | Persistent per device (per R18 M3 — wraps HSM via `signer: Arc<dyn HsmAdapter>`) | RFC-0009 §Capability Keys |
| Capability Issuer | `CapabilityTokenV2` (NOT CapabilityKey) carries `chain_depth` (per R11 H1) | Mint child keys | Stateless | RFC-0009 §Capability Keys |
| Capability Holder | `CapabilityTokenV2` (NOT CapabilityKey) | Redeem | Per-capability expiry | RFC-0009 §Capability Keys |
| Threshold Signer (object) | role ID `threshold-signer` (impl types: `BLS12381ThresholdSigner` / `SchnorrThresholdSigner` per R31 L4) | Sign via M-of-N | Persistent per IdentityKey | `octo-wallet` §threshold (impls) + This RFC (trait) |
| Key-Share Holder | `ShareHolderId` | Custody of one share | Persistent per device | This RFC |
| Threshold Coordinator | `ThresholdCoordinator` (RFC-0853 §F3) | Collect M shares; aggregate | Per signing request | RFC-0853 §F3 |
| Operator | `OperatorId` (per R15 L8 — NEW in v1.2) | Sign governance attestations (M-of-N) | Per ceremony | This RFC §MPC threshold identity |

### Threshold Coordinator interface

```rust
pub trait ThresholdCoordinator {
    async fn collect_shares(
        &self,
        msg: &[u8],
        shareholders: &[ShareHolderId],
        threshold: usize,
        timeout_ms: u64,
    ) -> Result<BoundedShareVec, ThresholdError>;

    fn aggregate(
        &self,
        msg: &[u8],
        shares: &BoundedShareVec,
    ) -> Result<ThresholdSigBytes, ThresholdError>;
}
```

### `OperatorId::pubkey()`

```rust
pub struct OperatorId(pub [u8; 32]);

impl OperatorId {
    pub fn pubkey(&self) -> [u8; 32] { self.0 }
}
```

### `threshold_params()` semantics (per R11 M5)

`threshold_params(&self) -> (M, N)` — **M = required shares
(threshold count)**, **N = total holders**. `BoundedShareVec::new`
enforces `m == M`.

**Authority matrix:**

| Action | Identity Holder | Key-Share Holders (collectively) | Threshold Coordinator |
|---|---|---|---|
| Initiate signing | YES | NO | NO |
| Provide share | NO | YES (M of N) | NO |
| Aggregate shares | NO | NO | YES |
| Revoke IdentityKey | YES (cascades) | NO | NO |
| Dispute resolution | YES | NO | NO |
| Re-key ceremony | YES (initiates) | YES (M of N) | NO |

**Out-of-scope roles:** key-share ceremony operator (governance);
DID method registrar (separate RFC); vault offline recovery operator.

## Specification

### Hierarchical attenuation chains

```rust
pub fn derive_capability_key(
    identity_key: &IdentityKey,
    audience_did: &DID,
    channel_id: &ChannelId,
    parent_cap_key: Option<&CapabilityKey>,  // NEW (BC#8)
) -> Result<CapabilityKey, WalletError>;
```

- **Root** (`None`): HKDF-BLAKE3 with `salt =
  identity_key.seed_bytes_for_hkdf()` (per R21 L5 — matches actual
  substrate API; hardware adapters refuse plain `seed_bytes()` by
  design per mission `0009-a`), `ikm = audience_did`,
  `info = "cipherocto/cap/v1/" + channel_id`.
- **Child** (`Some(key)`): HKDF-BLAKE3 with `salt =
  parent_cap_key.as_bytes()` (per R11 M1 — extraction), `ikm =
  audience_did`, `info = "cipherocto/cap/v2/child/" +
  parent_depth_be_bytes`.

**Invariant:** `parent_depth_be_bytes = V2 token chain_depth - 1`.
**Per R11 H1:** `chain_depth` lives ONLY on `CapabilityTokenV2`
(the wire form); `CapabilityKey` (key material) does NOT carry
depth.

**`parent_depth_be_bytes` fixed-width:** 4 bytes BE
(`to_be_bytes::<4>()`).

### MPC threshold identity

```rust
/// Per `crates/octo-wallet/src/identity.rs` — 9 existing fields
/// UNCHANGED + 3 new fields ADDED (per R16 M4).
pub struct IdentityKey {
    pub signer: Arc<dyn HsmAdapter>,
    pub public_key: [u8; 32],
    pub lifecycle: crate::lifecycle::LifecycleState,
    pub activated_at_unix_secs: Option<u64>,
    pub revoked_at_unix_secs: Option<u64>,
    pub revoked_proof: Option<[u8; 64]>,
    pub successor_key: Option<Box<IdentityKey>>,
    pub rotation_started_at_unix_secs: Option<u64>,
    pub deprecated: bool,

    pub threshold_signer: Option<Arc<dyn ThresholdSigner>>,
    /// Per R15 M3 + R16 M3: coordinator handle (Option; None = single-key only).
    pub coordinator: Option<Arc<dyn ThresholdCoordinator>>,
    /// Per R19 M2: shareholders registered at ceremony (carried
    /// alongside threshold_signer).
    pub shareholders: Vec<ShareHolderId>,
    /// Per R18 L10: REMOVED per R22 M2 — field was write-only
    /// (initialized + set + assigned; never read). Warning happens
    /// at construction; no once-guard needed.
    /// (Placeholder removed; struct field count = 9 existing + 3
    /// new = 12 per R22 H1.)
}

/// Per R11 C1: matches `HsmAdapter::sign` symbol in
/// `crates/octo-wallet/src/hsm.rs`.
pub trait HsmAdapter: Send + Sync {
    fn sign(&self, msg: &[u8]) -> Result<[u8; 64], HsmError>;
    /// Per R12 H4: device transport can fail; Result wrapper required.
    fn get_public_key(&self) -> Result<PublicKeyBytes, HsmError>;
}

/// Per R33 L2: `IdentityHolderId` definition (referenced in
/// §Roles and Authorities Source column).
pub struct IdentityHolderId(pub [u8; 32]);

/// Per R33 L3: `ShareHolderId` definition (referenced in
/// §Roles and Authorities Source column + used in
/// `ThresholdCoordinator::collect_shares` +
/// `IdentityKey.shareholders` + `IdentityKey::new`).
pub struct ShareHolderId(pub [u8; 32]);

impl ShareHolderId {
    /// Placeholder for ceremonial shareholder identification;
    /// production wiring uses registry from RFC-0853 §F3 key-share
    /// ceremony. Kept as separate constructor to make ceremony
    /// registration explicit at call sites (replaces R15 M3
    /// `from_index` placeholder which was undefined per R19 M2).
    pub fn from_registry_index(_registry: &KeyShareRegistry, _index: u16) -> Self {
        Self([0u8; 32])
    }
}

pub struct KeyShareRegistry;

pub type PublicKeyBytes = [u8; 32];

pub trait ThresholdSigner: Send + Sync {
    /// M = required shares, N = total holders.
    fn threshold_sign(
        &self,
        msg: &[u8],
        shares: &BoundedShareVec,
    ) -> Result<ThresholdSigBytes, ThresholdError>;

    fn threshold_params(&self) -> (usize, usize);

    /// Per R12 M7: group PK exposed for `Authorization::ThresholdSignature`
    /// aggregated_pk_check (forward-pointed per R12 H3).
    /// Per R18 H2: sum type — BLS12-381 PK is 48 bytes (NOT 32);
    /// FROST Ed25519 PK is 32 bytes.
    fn group_public_key(&self) -> GroupPublicKey;
}

/// Per R18 H2: sum type for group PKs (BLS12-381 = 48 bytes,
/// Ed25519 = 32 bytes).
#[derive(Clone, PartialEq, Eq)]
pub enum GroupPublicKey {
    Bls12381([u8; 48]),
    SchnorrEd25519([u8; 32]),
}

/// Per R11 M4: fail-closed when threshold_signer configured.
/// Per R12 H2: spec return type matches actual `IdentityKey::sign`
/// at `crates/octo-wallet/src/identity.rs` (returns
/// `Result<Signature, WalletError>` wrapping
/// `ed25519_dalek::Signature`); threshold fallback rewrapped via
/// `WalletError::Hsm(...)`.
/// Per R15 M3 + R16 M3: ThresholdCoordinator wired via `coordinator`
/// field on IdentityKey struct (not impl block; Rust forbids
/// field decls in impl).
/// Per R19 M2: `shareholders` registered at ceremony (carried on
/// IdentityKey struct, set via `new`); `sign_threshold` uses the
/// registry, not `from_index` placeholders.
impl IdentityKey {
    /// Per R18 L10 + R22 M2: warning happens at construction; no
    /// once-guard needed (AtomicBool field removed per R22 M2).
    /// Per R19 M2: `shareholders` registered at ceremony.
    pub fn new(
        signer: Arc<dyn HsmAdapter>,
        public_key: [u8; 32],
        threshold_signer: Option<Arc<dyn ThresholdSigner>>,
        coordinator: Option<Arc<dyn ThresholdCoordinator>>,
        shareholders: Vec<ShareHolderId>,
    ) -> Self {
        if threshold_signer.is_some() && coordinator.is_none() {
            tracing::warn!("IdentityKey constructed with threshold_signer but no coordinator; sign_threshold will fail");
        }
        Self {
            signer,
            public_key,
            lifecycle: crate::lifecycle::LifecycleState::Active,
            activated_at_unix_secs: None,
            revoked_at_unix_secs: None,
            revoked_proof: None,
            successor_key: None,
            rotation_started_at_unix_secs: None,
            deprecated: false,
            threshold_signer,
            coordinator,
            shareholders,
        }
    }

    pub fn sign(&self, msg: &[u8]) -> Result<ed25519_dalek::Signature, WalletError> {
        if self.threshold_signer.is_some() {
            return Err(WalletError::Hsm(HsmError::ThresholdSignerRequired));
        }
        let bytes = self.signer.sign(msg)?;
        Ok(ed25519_dalek::Signature::from_bytes(&bytes)
            .expect("HsmAdapter::sign returns 64-byte Ed25519 signature"))
    }

    pub async fn sign_threshold(&self, msg: &[u8])
        -> Result<ThresholdSigBytes, ThresholdError>
    {
        let thresh = self.threshold_signer.as_ref()
            .ok_or(ThresholdError::NoThresholdSigner)?;
        let coordinator = self.coordinator.as_ref()
            .ok_or(ThresholdError::NoThresholdCoordinator)?;
        // Per R20 H1: rename for clarity — `m` = threshold count
        // (required shares), `n` = total registered shareholders.
        let (m, n) = thresh.threshold_params();
        let shareholders = &self.shareholders;
        // Per R20 H1: check against `n` (total), NOT `m` (threshold).
        if shareholders.len() != n {
            return Err(ThresholdError::ShareholderCountMismatch { actual: shareholders.len(), expected: n });
        }
        let shares = coordinator.collect_shares(msg, shareholders, m, 30_000).await?;
        // Per R19 L4: caller (sign_threshold) runs validate_shares_scheme
        // before sort + aggregate. ThresholdSigner::threshold_sign impls
        // MAY re-check but MUST NOT be relied on for the property.
        validate_shares_scheme(shares.as_slice())?;
        let mut sortable = shares.as_slice().to_vec();
        sort_shares_for_aggregation(&mut sortable);
        // Reconstruct BoundedShareVec for threshold_sign.
        let bounded = BoundedShareVec::new(sortable, m)?;
        thresh.threshold_sign(msg, &bounded)
    }
}
```

**`verify_chain_parent` (per R11 C2 — corrected):**

```rust
/// Per R11 C2: actually binds child to parent via concatenation
/// hash (NOT just re-hashing parent — `chain_parent` must commit
/// to both the parent AND the child position in the chain).
pub fn verify_chain_parent(
    parent_cap_key: &CapabilityKey,
    child_cap_key: &CapabilityKey,
    chain_parent: &[u8; 32],
    child_depth: u8,
) -> bool {
    let binding_input = [
        parent_cap_key.as_bytes().as_slice(),
        child_cap_key.as_bytes().as_slice(),
        child_depth.to_be_bytes().as_slice(),
    ].concat();
    *chain_parent == *blake3::hash(&binding_input).as_bytes()
}
```

**`compute_chain_parent` (per R12 M5 — symmetric mint side):**

```rust
/// Per R12 M5: symmetric mint-side construction. Without this
/// function, an implementer cannot derive `chain_parent` correctly
/// (verify uses 1-byte `child_depth`; info string uses 4-byte BE
/// parent_depth — DO NOT confuse the two).
pub fn compute_chain_parent(
    parent_cap_key: &CapabilityKey,
    child_cap_key: &CapabilityKey,
    child_depth: u8,
) -> [u8; 32] {
    let binding_input = [
        parent_cap_key.as_bytes().as_slice(),
        child_cap_key.as_bytes().as_slice(),
        child_depth.to_be_bytes().as_slice(), // 1 byte, NOT 4
    ].concat();
    *blake3::hash(&binding_input).as_bytes()
}
```

**`check_wrapped_chain` (per R21 L7 — moved here from §A7):**

```rust
/// Multi-step chain walk for cascading revocation. Walks
/// `chain_parent` chain from leaf to root, verifying each step
/// via `verify_chain_parent`. Returns Ok if entire chain is
/// valid + un-revoked; Err if any ancestor is revoked.
pub fn check_wrapped_chain(
    leaf: &CapabilityKey,
    chain: &[CapabilityTokenV2],  // root..leaf order
    revoked_set: &HashSet<[u8; 32]>,  // chain_parent hashes of revoked ancestors
) -> Result<(), CapabilityError> {
    for window in chain.windows(2) {
        let parent = &window[0];
        let child = &window[1];
        if revoked_set.contains(&parent.chain_parent) {
            return Err(CapabilityError::AncestorRevoked);
        }
        if !verify_chain_parent(
            &parent.cap_key,
            &child.cap_key,
            &child.chain_parent,
            child.chain_depth,
        ) {
            return Err(CapabilityError::ChainLinkInvalid);
        }
    }
    Ok(())
}
```

**`Authorization::ThresholdSignature` extension (per R11 M3 + R12 H3):**

```rust
// Per R12 H3: actual `crates/octo-protocol/src/authorization.rs`
// variant has ONLY `signers` + `sig`; the `aggregated_pk_check` +
// `dkg_proof` fields are NOT in substrate. Concrete threshold-
// signature semantics (BLS aggregate, key registration, DKG proof)
// land in `crates/octo-cap-threshold-mpc/` per RFC-0871
// §Future Work. RFC-0009 v1.2 cross-references the extension
// schema; verifier MUST semantics are ASPIRATIONAL until the
// substrate lands.
// Per R18 M5: ThresholdSignature variant is BLS-only; FROST
// signatures emitted via `ThresholdSigBytes::SchnorrEd25519` use
// a separate `Authorization::FrostSignature` envelope variant
// (also defined in RFC-0871 §Future Work). Adding the FROST
// variant prevents coverage gap for FROST-signed envelopes.
pub enum Authorization {
    ThresholdSignature {
        signers: Vec<WireDid>,
        sig: BlsSignature,
        // Forward-pointer (RFC-0871 §Future Work):
        // aggregated_pk_check: bool,
        // dkg_proof: Vec<u8>,
    },
    FrostSignature {
        signers: Vec<WireDid>,
        sig: FrostEd25519Signature,
        group_pk: [u8; 32],
        // Forward-pointer (RFC-0871 §Future Work per R20 L6):
        // nonce_proof: Vec<u8>,  // FROST nonce-binding proof
        // dkg_proof: Vec<u8>,    // shared with ThresholdSignature
    },
    // ...
}
```

**Per R12 H3:** BC#5 revised — this RFC documents the EXTENSION
SCHEMA only. `aggregated_pk_check` + `dkg_proof` fields + verifier
MUST semantics land in `crates/octo-cap-threshold-mpc/` per
RFC-0871 §Future Work. RFC-0009 v1.2 does NOT commit to substrate
fields.

**`BoundedShareVec`:**

```rust
pub const MAX_M: usize = 7;  // per R20 L8 — bounds BoundedShareVec
                                  // allocation (7 shareholders); aligned with
                                  // `static_assertions` MAX_M >= 2 check below
                                  // (CONSTANT floor per R31 M3; MAX_M <= 7
                                  // ceiling enforced by struct literal cap).
/// Per R21 L3: chain depth cap (W3C VC-DID best practice per G1).
pub const MAX_CHAIN_DEPTH: u8 = 8;

pub struct BoundedShareVec {
    shares: Vec<ThresholdShare>,
    m: usize,
}

impl BoundedShareVec {
    pub fn new(shares: Vec<ThresholdShare>, threshold: usize) -> Result<Self, ThresholdError> {
        let m = shares.len();
        if m == 0 || m > MAX_M {
            return Err(ThresholdError::InvalidShareCount { actual: m, max: MAX_M });
        }
        if m != threshold {
            return Err(ThresholdError::ShareCountMismatch { actual: m, expected: threshold });
        }
        Ok(Self { shares, m })
    }

    pub fn len(&self) -> usize { self.m }

    /// Per R19 M1: accessor so callers can compose with
    /// `sort_shares_for_aggregation` + `validate_shares_scheme`.
    pub fn as_slice(&self) -> &[ThresholdShare] { &self.shares }
}

pub enum ThresholdShare {
    Bls12381([u8; 32]),
    SchnorrEd25519([u8; 32]),
}

pub enum ThresholdSigBytes {
    Bls12381([u8; 96]),
    SchnorrEd25519([u8; 64]),
}

/// Per R11 H5: sort by FULL 32-byte slice (NOT single byte `b[0]`).
/// Class A determinism requires total order.
/// Per R18 L7: mixed-scheme aggregation fails closed (returns
/// `Ordering::Greater` to push mixed schemes to end + signals
/// caller via separate `validate_shares_scheme` precondition
/// check).
pub fn sort_shares_for_aggregation(shares: &mut [ThresholdShare]) {
    shares.sort_by(|a, b| {
        let (a_bytes, b_bytes) = match (a, b) {
            (ThresholdShare::Bls12381(a), ThresholdShare::Bls12381(b)) => (a, b),
            (ThresholdShare::SchnorrEd25519(a), ThresholdShare::SchnorrEd25519(b)) => (a, b),
            _ => return std::cmp::Ordering::Greater,  // mixed-scheme: fail-closed sort
        };
        a_bytes.cmp(b_bytes)
    });
}

/// Per R18 L7: separate precondition check that rejects mixed
/// schemes before sort + aggregate.
pub fn validate_shares_scheme(shares: &[ThresholdShare]) -> Result<(), ThresholdError> {
    if shares.is_empty() { return Err(ThresholdError::InvalidShareCount { actual: 0, max: MAX_M }); }
    let first = match shares[0] {
        ThresholdShare::Bls12381(_) => Scheme::Bls12381,
        ThresholdShare::SchnorrEd25519(_) => Scheme::SchnorrEd25519,
    };
    for s in &shares[1..] {
        let s_scheme = match s {
            ThresholdShare::Bls12381(_) => Scheme::Bls12381,
            ThresholdShare::SchnorrEd25519(_) => Scheme::SchnorrEd25519,
        };
        if s_scheme != first { return Err(ThresholdError::MixedSchemes); }
    }
    Ok(())
}

enum Scheme { Bls12381, SchnorrEd25519 }
```

**Cargo deps pinned exact** in `crates/octo-wallet/Cargo.toml`
(per R12 M6 — actual file at commit `bf58559d` does NOT contain
these; AC#1 Commit 1 must ADD them):
```toml
[dependencies]
# Layer A — BLS12-381 threshold primitives.
# Per R17 L4 + R30 L4: pinned to 0.3.11 for cross-arch CI baseline
# ABI; bump requires re-running x86_64 + ARM64 determinism suite
# per A5 (BLS12-381 deterministic cross-arch) + §Implicit
# Assumptions Audit Platform row (BLS12-381 deterministic across
# x86_64 + ARM64).
blst = "=0.3.11"
# Layer A — Shamir secret sharing (RFC-0009 §MPC threshold identity).
# Per R17 H1: crate name is `vsss-rs` (NOT `vsss`); 0.5.2 not
# on crates.io — use latest stable 6.0.1.
vsss-rs = "=6.0.1"
# Layer A — RFC-9591 FROST Ed25519 threshold signing.
# Per R17 H2: 2.0.3 not on crates.io — use 2.2.0.
frost-ed25519 = "=2.2.0"
# Layer B-substrate — compile-time invariant assertions
# (per R20 L7 + R22 L5 + R31 M3): `const _: () = assert!(MAX_M >= 2)`
# asserts the CONSTANT floor (prevents MAX_M < 2 future regression);
# chain depth `const _: () = assert!(MAX_CHAIN_DEPTH >= 2)`
# asserts depth constant floor. (Per R31 M3 — assertion guards
# the constant, NOT runtime `m`; BoundedShareVec::new still
# accepts m = 1 + threshold = 1 (M=1 degenerate). Add explicit
# runtime `m >= 2` check below if M=1 must be forbidden at
# runtime; for now M=1 is allowed.)
static_assertions = "=1.1.0"
```

**`ThresholdShare` deserialization validation** — length check +
BLS subgroup check via `blst` + Schnorr scalar range via
`frost-ed25519`.

## Determinism Requirements

Per RFC-0008 Execution Class mapping:

| Operation | Class | Justification |
|---|---|---|
| HKDF-BLAKE3 derivation | **A** | Pure function |
| BLS12-381 signature aggregation | **A** | IETF BLS Signatures + `hash_to_curve` deterministic |
| FROST Ed25519 signing | **A** | RFC-9591 §5.3 deterministic nonce |
| Cascading revocation verification | **A** | Pure cryptographic walk |
| Share collection coordination | **B** | Coordinator-dependent |
| **Share aggregation (sorted)** | **A** | Per R11 H5 — sort by full 32-byte slice |
| IdentityKey dispatch routing | **B** | Init-time decision |

## Implicit Assumptions Audit

| Category | Assumption | Risk | Mitigation |
|---|---|---|---|
| Operator | Key-share ceremony with secure RNG | Ceremony compromise | Per-scheme ceremony + audit log |
| Platform | BLS12-381 deterministic across x86_64 + ARM64 | Cross-arch divergence | blst `DISABLE_PREFETCH` + cross-arch CI |
| Platform | FROST Ed25519 deterministic (RFC-9591 §5.3) | Same | frost-ed25519 deterministic nonce + cross-arch CI |
| Platform | Linux baseline | BSD/Windows divergence | x86_64 Ubuntu 22.04 + ARM64 macOS runners in CI per A6 + A9 cross-arch defenses (per R28 L4 — concrete CI matrix documented; was tautological "Linux baseline") |
| Platform | stoolap fork at pin | API drift | Pin commit hash |
| Time | Chain depth ≤ 8 | Migration if raised | Depth cap constant |
| Network | Threshold coordinator timeout | Coordinator failure stalls | Timeout (default 30s) + retry |
| Upgrade | v1.1 → v1.2 capability keys valid | v1.1 wallets reject `v2/child/` | Info-string discriminator check |
| Config | `parent_cap_key` default `None` | Forgetting = security regression | `derive_capability_key_v11` shim + Clippy lint |
| Identity | BLS threshold master key independent from Ed25519 | Cross-scheme confusion | Per-scheme key generation (RFC-0853 §F3 — key-separation ceremony contract per R18 L9) |
| Identity | FROST Ed25519 threshold master key independent | Same | Per-scheme key generation (RFC-0853 §F3 — key-separation ceremony contract per R18 L9) |
| Identity | `chain_parent` bound via `blake3(parent || child || depth)` | Spoofing | `verify_chain_parent` predicate (per R11 C2 — binds child too) |
| Identity | **M of N holders mutually distrusting AND evictable by Identity Holder** | Collusion attack | Governance + economic stake (RFC-0853 §F3 per R11 L2) |
| Storage | M shares persist on ShareHolder devices | Host memory compromise | HSM-internal share persistence |
| Resource | `BoundedShareVec::new` enforces m == threshold | Unbounded M | Runtime check |
| Hash Construction | HKDF-BLAKE3 | Implementer picks one | HKDF-BLAKE3 documented (HMAC-BLAKE3 has zero callsites per R15 M2 — dropped from audit) |

## Security Considerations

- **HKDF-BLAKE3 PRF property.**
- **BLS aggregate PK check + DKG-based PK-set derivation.**
- **FROST nonce reuse** defenses (per R11 H6 — quartet, not triplet):
  (a) exact crate pin, (b) `frost-ed25519` cross-arch determinism
  check (per R15 H1 — `blst` `DISABLE_PREFETCH` is BLS-specific,
  not FROST nonce defense), (c) integration test, (d) compile-time
  audit dep.
- **Cascading revocation** purely cryptographic.
- **HSM seed isolation.**
- **Share-loss DoS.**
- **Caller-supplied malicious shares.**
- **`chain_parent` forgery vs tampering** — `verify_chain_parent`
  (per R11 C2 binds child too).
- **HSM vs Identity Holder separation.**
- **`IdentityKey::sign` fail-closed** when `threshold_signer` configured
  (per R11 M4).

## Adversary Analysis (per R11 H3 — full body content)

### A1 — Child key leak via parent.
- **Threat:** attacker observes child capability key on the wire.
- **Attack:** derive parent capability key from child.
- **Defense:** HKDF-BLAKE3 PRF property (RFC-5869 analogue); child key
  is `HKDF(salt=parent, ikm=audience, info=".../v2/child/<depth>")`;
  inverting HKDF requires breaking BLAKE3.
- **Residual:** computationally infeasible (BLAKE3-256 security).
- **Test:** `root_vs_child_distinct_keys`.

### A2 — Chain depth > `MAX_CHAIN_DEPTH` DoS.
- **Threat:** attacker mints chain at depth > `MAX_CHAIN_DEPTH`.
- **Attack:** verifier does exponential walk over chain.
- **Defense:** `CapabilityError::ChainTooDeep` at mint time
  (`depth > MAX_CHAIN_DEPTH` returns error); no chain at depth
  > `MAX_CHAIN_DEPTH` exists. (Per R22 L4 — literal 8 replaced
  with constant.)
- **Residual:** soft cap (amendable).
- **Test:** `chain-depth-bounded-at-MAX_CHAIN_DEPTH`.

### A3 — Threshold signing race.
- **Threat:** attacker requests two concurrent sign ops.
- **Attack:** produce two signatures that look identical.
- **Defense (FROST):** nonce per request (RFC-9591 §5.3);
  different nonce → different signature.
- **Defense (BLS):** aggregation deterministic given shares + msg;
  no bypass.
- **Residual:** none (both schemes covered).
- **Test:** `threshold_race_distinct_nonces_produce_distinct_sigs`
  + `bls_aggregation_deterministic_given_shares_msg`.

### A4 — Threshold fallback bypass.
- **Threat:** operator misconfigures threshold as (1, 1) and falls
  back to single-key.
- **Attack:** produce single-key signatures when M-of-N was intended.
- **Defense (per R11 M4):** `IdentityKey::sign` fails-closed with
  `ThresholdSignerRequired` when `threshold_signer` configured.
- **Residual:** configuration error still possible (operator sets
  `threshold_signer = None` despite intended M-of-N); A4 covers the
  configured-with-wrong-params case.
- **Test:** `sign_fails_closed_when_threshold_signer_configured`.

### A5 — BLS aggregation malicious key.
- **Threat:** attacker submits a malicious BLS PK share.
- **Attack:** aggregate signature over unintended message.
- **Defense:** BLS aggregate verification includes PK check
  (RFC-0009 §Verification); DKG-based PK-set derivation (RFC-9591
  §5 for FROST; Pedersen DKG / Joint-Feldman variant for BLS12-381
  per R15 L5 — `BLS Shamir` is informal shorthand, not a real DKG
  protocol); shares outside the key set fail PK check.
- **Residual:** none.
- **Test:** `bls_threshold_2_of_3_with_malicious_pk_share_fails`.

### A6 — FROST nonce reuse.
- **Threat:** attacker exploits FROST nonce reuse.
- **Attack:** recover private key from two signatures with same nonce.
- **Defense (per R11 H6 — quartet):** (a) `frost-ed25519 = "=2.2.0"`
  exact pin (per R17 H2 — 2.0.3 not on crates.io); (b) `frost-ed25519`
  cross-arch determinism check
  (per R15 H1 — `blst` `DISABLE_PREFETCH` is BLS-specific, not
  FROST nonce defense; correct fix is x86_64 + ARM64 CI test
  suite + per-implementation cross-arch nonce audit); (c)
  integration test `frost_nonce_determinism.rs` asserting 100K ops
  produce deterministic nonces (per R20 L8 + R21 L2 — 100K is a
  CI budget choice; large enough to detect cross-arch divergence
  without excessive runtime; NOT an RFC-9591 §5.3 mandated count);
  (d) **CI-tool audit dep** (per R31 M2 — `cargo audit` invoked
  via §Validation exec command; NOT a `[dependencies]` entry; CI
  tooling only).
- **Residual:** library-level guarantee; trust boundary on
  `frost-ed25519` impl.
- **Test:** `frost_nonce_determinism_100k_iterations`.

### A7 — Cascading revocation false negative.
- **Threat:** holder of a child capability whose parent was revoked.
- **Attack:** continue using revoked capability.
- **Defense:** `check_wrapped_chain` (per R20 M2 — defined in
  §Specification > MPC threshold identity per R21 L7)
  cryptographically walks `chain_parent` chain; revocation of any
  ancestor invalidates all descendants.
- **Residual:** none (cryptographic guarantee).
- **Test:** `cascading_revocation_kills_descendants`.

### A8 — Share-loss DoS.
- **Threat:** single shareholder refuses participation.
- **Attack:** stall signing past coordinator timeout.
- **Defense:** shareholder liveness monitoring + replaceable share
  holder (governance).
- **Residual:** operator must manually replace non-responsive
  shareholder; out-of-scope for code.
- **Test:** `shareholder_unresponsive_triggers_coordinator_timeout`.

### A9 — BLS rogue-key attack.
- **Threat:** malicious BLS shareholder constructs PK that aggregates
  with honest shares.
- **Attack:** rogue-key attack on aggregate signature.
- **Defense:** DKG-based PK-set derivation (Pedersen DKG /
  Joint-Feldman variant for BLS12-381 per R15 L5 — per R26 M2,
  RFC-9591 §5 is FROST-specific, not BLS; A5 differentiates FROST
  vs BLS schemes consistently with this fix); PK-set fixed at
  ceremony; malicious shareholder cannot inject.
- **Residual:** none.
- **Test:** `rogue_pk_attack_mitigated_by_dkg`.

### A10 — `chain_parent` forgery.
- **Threat:** attacker constructs `chain_parent` claiming a parent
  that doesn't exist.
- **Attack:** child capability accepted without real parent chain.
- **Defense (per R11 C2):** `verify_chain_parent` checks
  `blake3(parent || child || depth) == chain_parent`; forgery
  requires inverting BLAKE3.
- **Residual:** none (BLAKE3-256 security).
- **Test:** `chain_parent_forgery_breaks_verification`.

### A11 — Caller-supplied malicious shares.
- **Threat:** caller passes malformed `ThresholdShare` bytes.
- **Attack:** aggregate signature over unintended message.
- **Defense:** `BoundedShareVec::new` runtime check (m ≤ MAX_M +
  m == threshold) + `ThresholdShare` deserialization validation
  (length + BLS subgroup + Schnorr scalar range).
- **Residual:** none (validation chain at type boundary).
- **Test:** `caller_supplied_malicious_shares_rejected`.

## Economic Analysis

N/A. Identity-substrate gating.

## Compatibility

### Backward compatibility

- `derive_capability_key` signature change (BC#8).
- `derive_capability_key_v11` shim (90-day deprecation).
- V1.1 callers migrate: 1 prod + 7 test sites.
- **`HsmAdapter::get_public_key` return type change** (BC#7).
- **`sign` NON-BREAKING** for current code (per R11 C1).
- **`IdentityKey::sign` fail-closed when `threshold_signer`
  configured** (BC#11).

### Forward compatibility

- V2 wire = separate `CapabilityBundleV2` struct.
- V1 consumers reject V2 via unknown struct.
- V2 consumers recognize V1 via `bundle_version == 1` field +
  use V1 path (NOT reject; per R25 M3 — v11 shim 90-day
  deprecation semantic; "reject" wording contradicted the
  backwards-compat shim).

**`CapabilityBundleV2` struct (per R15 L6 — schema sketch lives in
§Compatibility > §Forward compatibility so implementers don't need
cross-RFC jump):**

```rust
#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct CapabilityBundleV2 {
    pub bundle_version: u8, // = 2
    pub token: CapabilityTokenV2,
    pub holder_record: HolderRecord,
    pub discharge_macaroon: DischargeMacaroon,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct CapabilityTokenV2 {
    pub chain_depth: u8,            // per R11 H1 — lives on token, not key
    pub chain_parent: [u8; 32],     // per R11 C2 — binds parent || child || depth
    pub audience_did: String,
    pub channel_id: [u8; 16],
    pub expires_at_unix_secs: u64,
    pub issuer_did: String,
}
```

## Alternatives Considered

1. **HKDF-BLAKE3 salt chain** (chosen).
2. **HD-wallet-style hardened derivation (BIP32/SLIP-0010)**.
3. **BLS aggregate signature over the chain**.

## Implementation Phases

**Commit 1 — substrate trait + Cargo deps:**
- NEW `ThresholdSigner::threshold_sign`
- NEW `BoundedShareVec::new` (enforces m == threshold)
- NEW `ThresholdCoordinator` trait interface
- Cargo deps pinned in `crates/octo-wallet/Cargo.toml`
- `HsmAdapter::get_public_key` signature update
- `OperatorId::pubkey()` method + `OperatorId` struct definition
  `pub struct OperatorId(pub [u8; 32]);` (per R33 L4 — Commit 1
  owns struct definition, not just `pubkey()`)
- **Clippy lint registration in `clippy.toml`** (per R21 L6 — owns
  §Configuration Validation threshold misconfig lint)
- **`[[test]]` entry in `crates/octo-wallet/Cargo.toml`** (per R22 M3):
  ```toml
  [[test]]
  name = "frost_nonce_determinism"
  path = "tests/integration/frost_nonce_determinism.rs"
  ```
  Maps AC#8b subdirectory test path to binary name.

**Commit 2 — impls + Xor2Of3Signer additive field:**
- `BLS12381ThresholdSigner` impl
- `SchnorrThresholdSigner` impl
- `Xor2Of3Signer` additive `threshold_signer` field
- `InMemorySigner` + `LedgerSigner` + `YubiHsmSigner` updated
- `IdentityKey::sign` fail-closed when `threshold_signer` configured
  (per R11 M4)

**Commit 3 — derive + Phase 1 TV + fixture:**
- `derive_capability_key` 4-param signature (with `as_bytes()`
  extraction per R11 M1)
- `derive_capability_key_v11` shim
- `tests/fixtures/phase1_tv.json` (RFC-0009 TV-1..3)
- `phase1_tv_json_*` functions

**Commit 4 — V2 wire form** (atomic with Commit 5 per RFC-0870
§NodeEnvelope Adoption):
- `CapabilityBundleV2` struct (per R29 L5 — embeds existing
  `HolderRecord` + `DischargeMacaroon` types; these are
  pre-existing substrate types from RFC-0009 §Capability Keys
  + RFC-0957 §Capability Macaroon, NOT new in Commit 4)
- `CapabilityTokenV2` struct (carries `chain_depth` + `chain_parent`
  — per R11 H1; `CapabilityKey` does NOT)

**Commit 5 — V2 consumer migration** (atomic with Commit 4):
- Wallet + Capability issuer + `octo-cap-macaroon` + `octo-cap-zk`

### Configuration Validation

A4 enforced via:
- `IdentityKey::sign` fails-closed with `ThresholdSignerRequired`
  when `threshold_signer` configured (per R11 M4)
- `tracing::warn!` at construction when threshold_signer configured
  but coordinator is None (per R22 M2 — AtomicBool removed; no
  once-guard)
- Clippy lint registered in `clippy.toml` (per R15 L9 — actual file
  is `clippy.toml`, not `cargo-clippy.toml`); owned by Commit 1
  per R21 L6 (substrate additions)

## Future Work

(Dropped all phantom rows per R8 M2 + R9 M3 + R10 M4.)

## Rationale

- HKDF-BLAKE3 over HKDF-SHA256: faster.
- Depth ≤ 8: W3C VC-DID best practice.
- BLS12-381 over BN254: ZK-friendly.
- FROST Ed25519 over MuSig: RFC-9591 (Proposed Standard, June 2025 —
  per R21 L1; stale "(draft)" qualifier dropped).
- `parent_depth_be_bytes` as 4-byte BE.
- `vsss-rs` crate for Shamir (per R17 H1 — crate name is
  `vsss-rs`, not `vsss`; `vsss` does not exist on crates.io).
- `BoundedShareVec::new` enforces m == threshold.
- Separate `threshold_signer` field.
- V2 = separate struct.
- `sign` + `sign_threshold` separate methods.
- **`Authorization::ThresholdSignature` BREAKING extension** +
  verifier MUST check new fields (cross-ref to RFC-0871).
- `chain_depth` lives on `CapabilityTokenV2` only (per R11 H1).
- `verify_chain_parent` binds parent + child + depth (per R11 C2).
- `IdentityKey::sign` fail-closed when `threshold_signer` configured
  (per R11 M4).
- `sort_shares_for_aggregation` sorts by full 32-byte slice
  (per R11 H5).
- A6 FROST defenses are a quartet (per R11 H6).

## Test Vectors (preview)

External acceptance artifact: `tests/fixtures/phase1_tv.json`.

- **Phase 1 TV (RFC-0009 v1.2):**
  - TV-1: `phase1_tv_json_v11_round_trip_equivalence`
  - TV-2: `phase1_tv_json_child_unlinkability`
  - TV-3: `phase1_tv_json_hsm_boundary_no_seed_exfil`

## Layer direction

**Per R11 H4:** `octo-wallet` (Layer B) does NOT depend on
`octo-cap-macaroon` (Layer 4 — cleaned up per R25 L8; previous
Layer E label stale per R29 L2) directly. The registrar pattern
is L4 registers into B, not B → L4. The macaroon substrate uses
`Arc<dyn CapabilityToken>` interface injected at construction;
`octo-wallet` registers as the registrar.

- `octo-wallet` (Layer B) — registrar for capability extensions
- `octo-protocol` (Layer A) — `Authorization::ThresholdSignature`
  variant EXTENDED (ownership: RFC-0871)
- `octo-cap-zk` (Layer 4 — per R30 L5 relabel from Layer E to
  match octo-cap-macaroon post-Phase-2c convention) — sibling;
  registers into `octo-wallet` registrar
- **`octo-cap-macaroon` (Layer 4) — REMOVED from layer table per R25 L8**
  (post Phase 2c cleanup: zero cross-layer deps; previously
  registered into `octo-wallet` registrar at Phase 2b; current
  L4↔L-D coupling is via `crates/octo-cap-macaroon-transport/`
  glue crate to TransportDeliveryCatalog)

Dependency direction:
- `octo-wallet` → `octo-protocol` (B → A; OK)
- `octo-cap-zk` → `octo-wallet` (L4 → B registrar; OK)
- `octo-cap-macaroon-transport` → `octo-cap-macaroon` (L4↔D glue
  crate per R29 L3; mediates TransportDeliveryCatalog via
  Phase 2c-1; OK — L4↔D coupling is isolated to this glue crate,
  NOT a direct L4 → L-D dep)

**Per R20 M4 + R23 M3 + R24 L4:** post Phase 2c cleanup (commit
`a471843b` + `4cfe7165` on 2026-08-09), `octo-cap-macaroon` has
zero DIRECT cross-layer deps on L-B / L-C. L4↔L-D coupling is
isolated to `crates/octo-cap-macaroon-transport/` glue crate
(TransportDeliveryCatalog, per Phase 2c-1) — this is the only
cross-layer coupling, mediated by an L4↔D glue crate (not a
direct L4 → L-D dep). The previous `octo-cap-macaroon` →
`octo-wallet` (L4 → B registrar) line was true at Phase 2b but
is now obsolete; no L4↔C coupling exists.

No reverse dependencies. ✓

## Validation

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo clippy -p octo-wallet -p octo-cap-macaroon -p octo-cap-zk --all-targets -- -D warnings
cargo build -p octo-wallet  # per R11 L3 — validates Phase 0 compile
cargo test -p octo-wallet --lib -p octo-cap-macaroon --lib -p octo-cap-zk --lib
cargo test -p octo-wallet --lib -- --list phase1_tv_json | grep -qE "phase1_tv_json_(v11_round_trip_equivalence|child_unlinkability|hsm_boundary_no_seed_exfil)"  # per R11 H2 + R22 L6 + R23 M2
cargo test -p octo-wallet --lib phase1_tv_json_*  # per R11 L3 — actual test run
cargo test -p octo-wallet --test frost_nonce_determinism  # per R21 L4 + AC#8b (R26 L4)
cargo audit  # per R27 M1 — exercises A6 quartet (d) CI-tool audit dep (R31 M2)
cargo doc --workspace --no-deps
```

## Cross-references

- RFC-0009 §Capability Keys
- **RFC-0009 §Wallet Audience Validation** (per R10 H4; per R23 L4
  version pin dropped per RFC Reference Conventions)
- RFC-0853 §F3
- **RFC-0871 §Specification** (Authorization ownership — per R11 M3)
- **RFC-0871 §Future Work** (field-named threshold-signature
  semantics, `aggregated_pk_check` + `dkg_proof` — per R12 H3 +
  R29 L4; ASPIRATIONAL per AC#9 + AC#10 until RFC-0871 amendment
  FILED)
- **RFC-0870 §NodeEnvelope Adoption** (atomicity invariant per R10 H18)
- **RFC-0008 §Execution Class Mapping** (per R12 H1 — RFC-0104 has no
  Class A/B/C content; the taxonomy lives in RFC-0008)
- Mission `0957-f-v2-bundle` (V2 spec; per R11 H1 — `chain_depth`
  on `CapabilityTokenV2` only; per R11 M2 — fixture file count
  consistent with RFC: 1 file `tests/fixtures/phase1_tv.json` for
  RFC-0009 + 1 file `tests/fixtures/phase1_tv_0862.json` for
  RFC-0862 + 1 file `tests/fixtures/v2_bundle_tv.json` for V2
  wire form)
- Mission `0957-phase1-fixture-author` (RFC-0009 Phase 1 fixture
  scope; per R17 M3 — does NOT cover RFC-0862)
- Mission `0862-phase1-tv-fixture` (RFC-0862 Phase 1 fixture scope;
  FILED per R17 M3)

## Version History

| Version | Date       | Status               | Changes                                           |
| ------- | ---------- | -------------------- | ------------------------------------------------- |
| 1.0     | 2026-07-19 | Accepted             | Initial specification                             |
| 1.1     | 2026-08-08 | Accepted (amendment) | HSM routing + wallet audience validation          |
| 1.2     | 2026-08-10 | Draft                | Hierarchical attenuation + MPC threshold identity |

## Review Process

Multi-round adversarial review per BLUEPRINT §RFC Process. R1-R38
completed (2026-08-11). Convergence target: zero NEW findings per
R40+. Note: the "R-count" line in §Review State is a
mechanical bookkeeping pointer — its drift each round is
expected and is NOT classified as a substantive finding in
the per-round report (per R39 meta-decision: drift is
bookkeeping, not spec defect).
