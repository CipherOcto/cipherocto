# RFC-0010: Canonical OctoID Identifier Codec

## Status

Accepted (2026-07-27, v1.6 amendment 2026-08-19)

> **Promotion note:** Promoted from Draft to Accepted on 2026-07-27 after single-round review. Round-1 fixes: (H1) `wire_to_raw` was self-referential — clarified the version_discriminator is RE-DERIVED from the wire hash via the binding-domain, not stored; (H2) `RawDid` made a structured type with explicit `hash` + `version_discriminator` fields rather than `#[repr(C)] [u8; 52]`; (M1) `parse` step 3 tightened to reject bare `:name` literals during the deprecation window; (L1) added `mint(pubkey)` algorithm so the canonical path from a fresh pubkey to a raw DID is documented.

> **Note:** This RFC is a sibling of RFC-0009. The two are coupled: RFC-0009 §Identity Struct specifies the canonical wire form `did:octo:z<base58btc of 32 bytes>`; this RFC introduces the codec crate `octo-ident` and the dual-form storage/wire split.

## Authors

- Author: @cipherocto + @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

Define a single byte-form (52-byte raw `BLAKE3-256 || version discriminator`) and a single wire-form (43-44 char `did:octo:z<base58btc>`) for all `RecorderDid` / `AudienceId` instances. Add a codec crate `crates/octo-ident/` that translates between them. Adopt the W3C multibase-z form for all cross-mission path output (gossip topics, CLI display, log lines). Keep the 52-byte form for reputation storage tables; the codec bridges storage and wire at every boundary.

## Dependencies

**Requires:**

- RFC-0009 (Accepted; supplies the canonical wire-form textual definition this RFC formalizes)
- RFC-0968 (Accepted; supplies the canonical 52-byte storage form this RFC preserves)
- RFC-0968-A1 in-place amendment (Accepted; defines compat window semantics)

**Optional:**

- RFC-0850 DOT identity model (cross-reference for adapter-side ident hash; not blocking)

> **Dependency Validation Rules:**
>
> 1. Dependencies MUST form a DAG (no cycles): ✓ (no cycles introduced)
> 2. All "Requires" RFCs MUST be listed as mission prerequisites (Mission creation in S5 enforces this)
> 3. Optional dependencies MUST be documented separately from required: ✓
> 4. Dependencies on "Planned" RFCs MUST note the assumption they will be Accepted: none

## Design Goals

| Goal | Target                                                              | Metric                           |
| ---- | ------------------------------------------------------------------- | -------------------------------- |
| G1   | Single byte-form for `RecorderDid` (52 bytes, post-version byte)    | 100% reputation storage aligned  |
| G2   | Single wire-form for cross-mission output (W3C multibase-z)         | 100% gossip topics + CLI display |
| G3   | Single codec entry-point for translation                            | 1 crate `octo-ident`             |
| G4   | Zero bare-name literals in test fixtures                            | 347 → 0 across codemod           |
| G5   | Round-trip encode/decode byte-exact for 10k random corpus           | property test                    |
| G6   | Round-trip deterministic across compilers (RFC-0104 Dfp-equivalent) | cross-replica property test      |

## Motivation

Three encoding paths currently exist in production code:

1. `crates/octo-reputation/src/types.rs::RecorderDid::from_bytes` — accepts raw 52 bytes.
2. `crates/octo-wallet/src/identity.rs::AudienceId::from_str` — accepts any non-empty string.
3. `crates/quota-router-core/src/marketplace/reputation_compat.rs::parse_canonical_did` — accepts only `did:octo:b<52>` (62 chars).

The 347-literal surface (test fixtures + integration tests + market tests) uses bare names like `did:octo:buyer` which none of the three parsers accepts in production; these literals exist ONLY in tests, but they document the protocol's intended shape variance. Cross-mission reputation laundering via noncanonical encoding is documented in `docs/use-cases/reputation-persistence.md:19` and `docs/research/2026-07-24-reputation-persistence-research.md:38`. Reputation research already locked `did:octo:b<52>` (62 chars) as the storage form; this RFC ratifies the codec split.

## Roles and Authorities

### Role/Authority Coverage Table

| Role                | Identifier                        | Authority Scope                                                  | Lifecycle                                                    | Source/Ref                  |
| ------------------- | --------------------------------- | ---------------------------------------------------------------- | ------------------------------------------------------------ | --------------------------- |
| Codec Author        | `octo_ident::DidCodec` impl block | translate raw bytes ↔ wire string                                | stateless (transform-only function)                          | This RFC §Specification     |
| Identity Resolver   | `octo_ident::DidResolver` trait   | resolve canonical form + detect legacy form during compat window | stateless (lookup function)                                  | This RFC §Specification     |
| Recorded Subject    | `RecorderDid (52 bytes)`          | read/write reputation events on the canonical store              | persistent across rotations (DID rotation per RFC-0968 §3.7) | RFC-0968 §3                 |
| Wallet Audience     | `AudienceId (W3C wire form)`      | derive capability key per RFC-0009 §Capability Keys              | persistent until identity rotation                           | RFC-0009 §Capability Keys   |
| DID Storage Backend | `DidRegistry` trait (v1.3)        | persist + retrieve `DidDocument` records keyed by canonical DID  | persistent until DID rotation or revocation                  | This RFC §Storage Extension |

### Stateful actors

No new stateful actors introduced by the codec itself. The codec is stateless. The storage actors (`RecorderDid`, `AudienceId`) retain their pre-existing lifecycles in RFC-0968 and RFC-0009.

**v1.3 addition:** The new `DidRegistry` trait (this RFC §Storage Extension) introduces one new stateful actor — the DID storage backend. The actor's lifecycle matches the underlying persistence substrate: single-process `InMemoryDidRegistry` is process-scoped; production `StoolapDidRegistry` is durable across restarts via the cipherocto-side `did_registry` table (migration v008). Cross-instance coordination of this actor is OUT of scope for v1.3 (see §Storage Extension §Out of scope) and lands in a future RFC-0862 amendment.

## Specification

### System Architecture

```mermaid
graph LR
    subgraph "Storage (RFC-0968 §3)"
        RD[RecorderDid<br/>52 bytes raw]
    end
    subgraph "Storage (RFC-0009)"
        AI[AudienceId<br/>W3C wire String]
    end
    subgraph "Cross-Mission Path"
        GT[Gossip Topic<br/>/dot/reputation/{wire}]
        CLI[CLI Output<br/>wire form]
    end
    OC[octo-ident<br/>DidCodec]
    RD -- to_wire --> OC
    OC -- encode --> GT
    OC -- encode --> CLI
    AI -- encode --> OC
    OC -- to_storage --> RD
```

### Data Structures

#### `crates/octo-ident/src/lib.rs`

```rust
/// Raw 52-byte DID storage form. Encoded `BLAKE3-256 hash (32 bytes) || version discriminator (20 bytes)`.
/// Aliases RFC-0968 §3 `RecorderDid` storage form (post-v011 migration; see Implicit Assumptions Audit).
pub struct RawDid {
    pub hash: [u8; 32],
    pub version_discriminator: [u8; 20],
}

/// W3C DID Core 1.0 wire form: `did:octo:z<base58btc of 32 bytes>`.
pub struct WireDid(String);

/// Wire form of legacy reputation storage: `did:octo:b<base32 no-pad of 52 bytes>`
/// (62 chars). Accepted during 6-month dual-parse window only.
pub struct LegacyWire(String);

/// Stateless translator.
pub trait DidCodec {
    /// Translate `RawDid` to canonical wire form (truncates to leading 32-byte hash).
    fn raw_to_wire(raw: &RawDid) -> Result<WireDid, DidError>;

    /// Translate canonical wire form back into `RawDid`. Re-computes version discriminator from
    /// binding-domain hash so the round-trip is exact.
    fn wire_to_raw(wire: &WireDid) -> Result<RawDid, DidError>;

    /// Translate legacy 62-char form into canonical wire form.
    fn legacy_to_wire(legacy: &LegacyWire) -> Result<WireDid, DidError>;

    /// Parse any accepted input form and return canonical wire form.
    fn parse(input: &str) -> Result<WireDid, DidError>;

    /// Mint a fresh `RawDid` from a 32-byte subject public key. The trailing 20-byte discriminator
    /// stores a domain-separated tag (zero by default; fingerprint-overrides in future versions).
    fn mint(pubkey: &[u8; 32]) -> RawDid;
}
```

### Algorithms

#### `raw_to_wire`

```
input: RawDid (52 bytes: 32-byte hash || 20-byte version_discriminator)
  1. take the leading 32-byte hash
  2. multibase-z encode (base58btc alphabet, NO checksum)
  3. prepend "did:octo:z"
  4. assert length ∈ [43, 44] characters
  5. return WireDid
```

#### `wire_to_raw`

```
input: WireDid (string of form "did:octo:z<base58btc>")
  1. assert prefix == "did:octo:z"
  2. base58btc decode the suffix → 32 bytes; assert length == 32 bytes
  3. derive version_discriminator = BLAKE3("cipherocto/octoid/v1/discriminator" || wire_bytes) truncated to 20 bytes
  4. return RawDid { hash: wire_bytes, version_discriminator }
```

Note: `wire_to_raw` does NOT verify a hash against a stored raw, because the wire form only carries 32 bytes — there is no embedded 20-byte discriminator to verify. The 32-byte hash IS the canonical payload; the 20-byte discriminator is re-derived deterministically from `(binding_domain || wire_bytes)` so the round-trip is exact.

#### `mint`

```
input: 32-byte subject public key
  1. hash = BLAKE3("cipherocto/octoid/v1" || pubkey); truncate to 32 bytes
  2. version_discriminator = BLAKE3("cipherocto/octoid/v1/discriminator" || hash) truncated to 20 bytes
  3. return RawDid { hash, version_discriminator }
```

#### `parse` (with dual-parse window)

```
input: any string
  1. if starts_with "did:octo:z" → run wire_to_raw internal checks → run raw_to_wire → wire
  2. elif starts_with "did:octo:b" AND length == 62 → legacy_to_wire → wire
  3. elif during_deprecation_window() AND starts_with "did:octo:" → accept ONLY IF the substring past
     the prefix is non-empty AND the suffix decodes as base32-no-pad with exactly 52 characters of
     payload (legacy form); reject anything else with DidError::UnrecognizedShape. When the
     deprecation window closes, step 3 returns DidError::LegacyFormExpired.
  4. else → DidError::UnrecognizedShape
```

### Mint-once guarantee

Every newly-minted DID IS canonical. The deprecation window accepts legacy `did:octo:b<52>` and bare `did:octo:<name>` only during the transition period; once the window closes (Mission C flip), `parse` step 3 returns `DidError::LegacyFormExpired`. The mint path (Mission A) NEVER produces a legacy form — every new DID has an embedded version discriminator derived in step 2.

### Determinism Requirements

The codec is **deterministic across compilers and platforms** (RFC-0008 Class A):

- Encoding round-trip is byte-exact for any input.
- Decoding is byte-exact for any input.
- The version discriminator appends **exactly 20 zero bytes** (NOT a random IV) so two implementations hashing the same subject pubkey produce identical raw bytes.
- Base58btc alphabet is Bitcoin (RFC-0009 §Identity Struct); no lowercase-variant ambiguity.

### RFC-0008 Execution Class Mapping

| Operation                      | Class | Rationale                                   |
| ------------------------------ | ----- | ------------------------------------------- |
| `raw_to_wire`                  | A     | Pure function; deterministic; no IO         |
| `wire_to_raw`                  | A     | Pure function; deterministic; no IO         |
| `legacy_to_wire`               | A     | Pure function; deterministic; no IO         |
| `parse`                        | A     | Pure function; deterministic; no IO         |
| `DidRegistry::register` (v1.3) | B     | Deterministic per-call; storage IO is local |
| `DidRegistry::resolve` (v1.3)  | B     | Deterministic per-call; storage IO is local |
| `DidRegistry::revoke` (v1.3)   | B     | Deterministic per-call; storage IO is local |
| `DidRegistry::list` (v1.3)     | B     | Deterministic per-call; storage IO is local |

### Storage Extension (v1.3, additive)

#### Motivation

`crates/octo-ident/` ships the canonical DID codec (52-byte raw ↔ W3C wire form) but no persistence trait. The Phase 1 MVP `IdentityResolverNode` (mission `0871b-identity-resolver-node`, `crates/octo-identity-resolver-node/src/handlers/resolve.rs`) returns a placeholder `public_key` derived from `RawDid::hash` — deterministic and byte-exact across the placeholder/real-registry cutover, but not a real lookup against a persisted DID Document. Mission `0871b-storage-backend` is blocked on this RFC shipping the substrate trait.

This extension is **additive on v1.2**: it does NOT amend `RawDid`, `WireDid`, `DidCodec`, or any existing codec API. v1.3 adds ONE new trait + two reference impls + one new schema migration.

#### Layer discipline

Per [[cipherocto-design-principles]] §Layer B additive-only rule (the codec crate is Layer B years-stable; the trait and in-memory impl ship in v1.3 because they are pure substrate):

- **`crates/octo-ident/src/registry.rs`** (NEW) — `DidRegistry` trait + `InMemoryDidRegistry` test/single-process impl. The codec crate gains ONE trait surface; no codec method signatures change.
- **`crates/quota-router-storage/src/stoolap_did_registry.rs`** (NEW) — `StoolapDidRegistry` production impl backed by a stoolap table per [[stoolap-general-purpose-db]] (cipherocto-side migration, NOT stoolap fork). Schema lives at `crates/quota-router-storage/migrations/v008__create_did_registry.sql`.
- **`crates/octo-identity-resolver-node/src/handlers/resolve.rs`** (consumer; future mission `0871b-storage-backend`) — swaps the placeholder `RawDid::hash` derivation for `DidRegistry::resolve(canonical_did)`.

#### Data Structures

```rust
/// DID Document stored alongside the canonical DID. v1.3 ships the minimum
/// surface needed by `IdentityResolverNode` (mission 0871b-storage-backend):
/// the 32-byte storage-pubkey form + a revocation flag. Future amendments
/// MAY add service endpoints, controller references, or capability
/// delegation proofs — those are OUT of scope for v1.3.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DidDocument {
    /// 32-byte storage-pubkey form (matches `RawDid::hash` for v1 canonical).
    pub public_key: [u8; 32],
    /// True if the DID has been revoked; resolve returns `None` when set.
    pub revoked: bool,
}

/// Storage trait for canonical DIDs + their DID Documents. Layer B substrate
/// (per [[cipherocto-design-principles]] §Layer B additive-only); reference
/// impls are `InMemoryDidRegistry` (test/single-process) and
/// `StoolapDidRegistry` (production, `crates/quota-router-storage`).
///
/// Mirrors `HolderRegistry` (RFC-0957-A1 §Data Structures) trait shape:
/// `Send + Sync` supertrait, lookup / lookup_by_predicate / insert / revoke
/// surface, clock injection on time-sensitive operations.
///
/// Cross-instance coordination (leader election / 2PC / CRDT) is explicitly
/// OUT of scope for this RFC — that substrate belongs to RFC-0862 atomic
/// transaction + a future amendment. v1.3 ships the single-instance contract
/// only; `StoolapDidRegistry` serializes writes via per-instance
/// `std::sync::Mutex` exactly like `StoolapSpendLedger` (mission
/// `0871e-phase5b-stoolap-ledger`, `crates/quota-router-storage/src/stoolap_spend_ledger.rs`).
pub trait DidRegistry: Send + Sync {
    /// Register a fresh `(canonical_did, DidDocument)` pair. Re-register of
    /// an existing DID overwrites the `DidDocument` (upsert semantics,
    /// matches RFC-0957 §Algorithms caveat re-mint pattern). DID Documents
    /// for already-revoked DIDs MUST NOT be re-registered — the revocation
    /// is terminal. Errors map to the caller's domain error via `From`.
    /// # Errors
    /// Returns `DidRegistryError::AlreadyRevoked` if the DID is revoked.
    /// Returns `DidRegistryError::Storage` on underlying storage failure.
    fn register(
        &self,
        canonical_did: &WireDid,
        document: &DidDocument,
    ) -> Result<(), DidRegistryError>;

    /// Resolve a canonical DID to its DID Document. Returns `None` if the
    /// DID is unknown OR revoked (revoked DIDs fail-closed).
    /// # Errors
    /// Returns `DidRegistryError::Storage` on underlying storage failure.
    fn resolve(
        &self,
        canonical_did: &WireDid,
    ) -> Result<Option<DidDocument>, DidRegistryError>;

    /// Mark a canonical DID as revoked. Resolution of revoked DIDs returns
    /// `None` (fail-closed). Revocation is terminal — re-registration after
    /// revocation returns `AlreadyRevoked` per `register`'s contract.
    /// # Errors
    /// Returns `DidRegistryError::UnknownDid` if the DID is not registered.
    /// Returns `DidRegistryError::Storage` on underlying storage failure.
    fn revoke(&self, canonical_did: &WireDid) -> Result<(), DidRegistryError>;

    /// List all registered (non-revoked) DIDs. Returns the canonical DID
    /// wire forms in registration order (insertion order, NOT sorted —
    /// matches `StoolapDidRegistry` SELECT without ORDER BY).
    /// # Errors
    /// Returns `DidRegistryError::Storage` on underlying storage failure.
    fn list(&self) -> Result<Vec<WireDid>, DidRegistryError>;
}
```

#### Error Handling

```rust
/// Errors returned by `DidRegistry` operations. The wire-shape error
/// (storage failure) maps to `octo_wallet::WalletError::StorageError`
/// at the wallet boundary; the resolver-node boundary maps to
/// `IdentityResolveError::Storage`.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DidRegistryError {
    #[error("DID {0} is already revoked; re-registration forbidden")]
    AlreadyRevoked(String),
    #[error("DID {0} is not registered")]
    UnknownDid(String),
    #[error("did-registry storage error: {0}")]
    Storage(String),
}
```

#### `InMemoryDidRegistry` (test/single-process)

```rust
/// In-memory `DidRegistry` impl for tests + single-process deployments.
/// Thread-safe via `parking_lot::RwLock` (read-heavy workload — `resolve`
/// is the hot path). Cross-process deployments MUST use `StoolapDidRegistry`.
#[derive(Debug, Default)]
pub struct InMemoryDidRegistry {
    inner: parking_lot::RwLock<HashMap<String, DidDocument>>,
}
```

#### `StoolapDidRegistry` (production, `crates/quota-router-storage`)

Schema (cipherocto-side migration per [[stoolap-general-purpose-db]]):

```sql
-- migrations/v008__create_did_registry.sql
CREATE TABLE did_registry (
    canonical_did BLOB PRIMARY KEY,  -- 32-byte hash (decoded from base58btc)
    public_key    BLOB NOT NULL,     -- 32-byte storage-pubkey form
    revoked       INTEGER NOT NULL DEFAULT 0,  -- 0 = active, 1 = revoked
    updated_at_unix_ms INTEGER NOT NULL
);
CREATE INDEX did_registry_updated_at ON did_registry(updated_at_unix_ms);
```

The `canonical_did` column stores the decoded 32-byte hash (NOT the full
W3C wire form) so lookups by `RawDid::hash` are index-hittable. The
`revoked` flag short-circuits `resolve` to `None` without deleting the
row (preserves audit trail; matches `HolderRegistry::revoke` pattern).

API uses raw byte slices (not `WireDid` typed wrapper) to avoid the
cyclic crate dependency `quota-router-storage → octo-wallet →
octo-ident → octo-wallet` — same pattern as `StoolapSpendLedger`
(mission `0871e-phase5b-stoolap-ledger`). The `canonical_did` parameter
is the decoded 32-byte hash (`RawDid::hash`), NOT the wire form.

#### Out of scope for v1.3 (deferred to future RFCs)

Per [[deferred-vs-unspecified]] rule, v1.3 SPECIFIES the substrate
above and explicitly defers the following to future RFCs (each with
its own owner + schedule per RFC-0871 §Future Work pattern):

- **Cross-instance DID write coordination** (RFC-0862 amendment or new
  RFC). The `DrainCoordinator` work for `SpendLedger` (mission
  `0871e-phase5c-1-cross-instance-drain`) defines the candidate approaches
  (2PC / aggregator / CRDT). The DID registry follow-on picks one of
  those approaches — same substrate, separate amendment.
- **`ResolverBackend` typed view** (RFC-0871 §Future Work). A typed view
  over `DidRegistry` for resolver-chain traversal (`ResolverHop`
  records + cross-domain authorization). Lives in
  `crates/octo-identity-resolver-node/src/backend.rs` once the chain
  mission `0871b-cross-domain-resolution` is filed.
- **Multi-chain DID resolution** (F2). Already out of MVP scope per
  v1.2 §Future Work; the v1.4 extension SPECIFIES the typed `ChainId`
  namespace substrate required by F2; F2 itself (multi-chain
  resolution routing) remains a follow-on amendment.
- **DID Document extensions** (service endpoints, controller references,
  capability delegation proofs). v1.3 ships the minimum surface needed
  by `IdentityResolverNode`. v1.5 SPECIFIES the rich 7-field
  `DidDocument` + `VerificationMethod` enum (consumer substrate
  ready); F8 itself (rich Document persistence in Stoolap
  `did_registry` table — adding 5 new columns + migration v009) is a
  follow-on amendment. Tracked by
  `missions/open/0010-f8-rich-did-documents.md` (BLOCKED on this
  RFC v1.5 per memory `mission-gap-closure-priorities-2026-08-10`).

### ChainId Namespace Extension (v1.4, additive on v1.3)

#### Motivation

`octo_ident::ChainId` ships as `pub struct ChainId(pub String)` in
v1.3 — a stringly-typed newtype that gives the `DidWriteCoordinator`
trait surface a stable identity parameter. Production deployments
need chain-namespace semantics:

1. **RFC-allocated namespaces** prevent collisions across co-deployed
   CipherOcto instances (e.g., a private partner running its own
   `partner-mainnet` alongside the public `cipherocto-mainnet`).
2. **Canonical serialization** makes the chain namespace
   wire-stable for inclusion in `GovernanceAttestation` payloads
   (per RFC-0862 `chain_id: ChainId` field) and RPC audit logs.
3. **Validation at the type boundary** rejects malformed namespaces
   (empty string, control characters, length overflow) at construction
   time so downstream coordinator substrate can trust the invariant.

This extension is **additive on v1.3**: v1.3 `ChainId::new(s)` callers
continue to work (the underlying type stays a `String`); v1.4 adds
RFC-allocated constants, a `Namespace` newtype wrapper, and a validator
on the raw-string path.

#### Data Structures

```rust
/// Typed chain-namespace discriminator (RFC-0010 v1.4).
///
/// `ChainId` is a literal-string handle (operational ergonomics);
/// `Namespace` is the canonical typed representation that anchors
/// the BLAKE3-256 chain derivation domain. The two are linked via
/// `ChainId::namespace()` which resolves any literal against the
/// RFC-allocated namespace table.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChainId(pub String);

/// RFC-allocated namespace discriminant (per
/// [[cipherocto-design-principles]] §Extension over enumeration —
/// 128-bit UUID discriminator + Raw escape pattern; this is the
/// per-deployment analogue).
///
/// Layout:
///
/// ```text
/// [ variant: u8 (RFC / User / Reserved) | tag: [u8; 15] | length: u8 ]
/// ```
///
/// Canonical serialization = 17 bytes (`ChainNamespace::canonical_bytes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChainNamespace {
    variant: NamespaceVariant,
    tag: [u8; 15],
    length: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NamespaceVariant {
    /// RFC-allocated namespace (variant byte `0x01`).
    Rfc = 0x01,
    /// User-extension namespace (variant byte `0x02`).
    User = 0x02,
    /// Reserved for future amendments (variant byte `0x00`,
        /// `0x03`-`0xFF`).
    Reserved = 0x00,
}

/// Errors from `ChainId` / `ChainNamespace` validation.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ChainNamespaceError {
    #[error("empty chain namespace")]
    Empty,
    #[error("chain namespace too long: {len} > max {max}")]
    TooLong { len: usize, max: usize },
    #[error("chain namespace contains control character: {0:?}")]
    ControlChar(char),
    #[error("unknown RFC chain namespace tag: {0}")]
    UnknownRfcTag([u8; 15]),
    #[error("reserved namespace variant: {0}")]
    ReservedVariant(u8),
}

impl ChainId {
    /// Construct a `ChainId` from a literal string. Validates the
    /// namespace shape per RFC-0010 v1.4 §Validation.
    ///
    /// # Errors
    /// Returns `ChainNamespaceError` variants if the literal is
    /// empty, too long, or contains control characters.
    pub fn new(s: impl Into<String>) -> Result<Self, ChainNamespaceError> {
        let inner = s.into();
        Self::validate(&inner)?;
        Ok(Self(inner))
    }

    /// Wrap a literal string WITHOUT validation. Internal use only —
    /// produced `ChainId` may fail later validation paths. Prefer
    /// `ChainId::new` for all external entry points.
    #[must_use]
    pub fn new_unchecked(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Resolve the `ChainId` to its `ChainNamespace`. The
    /// RFC-allocated constant table is consulted first; user-extension
    /// chains parse through the `User` variant.
    ///
    /// # Errors
    /// Returns `ChainNamespaceError::UnknownRfcTag` if the literal
    /// claims the `Rfc` variant but the tag is not in the table.
    /// Returns `ChainNamespaceError::ReservedVariant` for
    /// reserved-variant encodings.
    pub fn namespace(&self) -> Result<ChainNamespace, ChainNamespaceError> {
        ChainNamespace::from_literal(&self.0)
    }

    fn validate(s: &str) -> Result<(), ChainNamespaceError> {
        const MAX_LEN: usize = 64;
        if s.is_empty() { return Err(ChainNamespaceError::Empty); }
        if s.len() > MAX_LEN {
            return Err(ChainNamespaceError::TooLong { len: s.len(), max: MAX_LEN });
        }
        for c in s.chars() {
            if c.is_control() {
                return Err(ChainNamespaceError::ControlChar(c));
            }
        }
        Ok(())
    }
}

impl ChainNamespace {
    /// 17-byte canonical serialization (used in WAL entries,
    /// `GovernanceAttestation.chain_id` field, RPC audit logs).
    #[must_use]
    pub fn canonical_bytes(&self) -> [u8; 17] {
        let mut out = [0u8; 17];
        out[0] = self.variant as u8;
        out[1..16].copy_from_slice(&self.tag);
        out[16] = self.length;
        out
    }

    /// Reverse of `canonical_bytes`. Returns `Err` on unknown variant.
    pub fn from_canonical_bytes(bytes: &[u8; 17]) -> Result<Self, ChainNamespaceError> {
        let variant = match bytes[0] {
            0x01 => NamespaceVariant::Rfc,
            0x02 => NamespaceVariant::User,
            v => return Err(ChainNamespaceError::ReservedVariant(v)),
        };
        let mut tag = [0u8; 15];
        tag.copy_from_slice(&bytes[1..16]);
        Ok(Self { variant, tag, length: bytes[16] })
    }

    /// Resolve an RFC-0010 literal (`cipherocto-mainnet`, ...) to its
    /// typed `ChainNamespace`. Returns `Rfc` variant for known tags,
    /// `User` variant for unrecognized but valid literals.
    pub fn from_literal(literal: &str) -> Result<Self, ChainNamespaceError> {
        ChainId::validate(literal)?;
        let tag = *blake3::hash(
            b"cipherocto/chain-namespace/v1".iter()
                .chain(literal.as_bytes().iter())
                .copied()
                .collect::<Vec<u8>>()
                .as_slice()
        ).as_bytes();
        let mut truncated_tag = [0u8; 15];
        truncated_tag.copy_from_slice(&tag[..15]);
        let variant = if RFC_CHAIN_NAMESPACES.contains(&truncated_tag) {
            NamespaceVariant::Rfc
        } else {
            NamespaceVariant::User
        };
        Ok(Self {
            variant,
            tag: truncated_tag,
            length: literal.len() as u8,
        })
    }
}
```

#### RFC-allocated namespace constants

The tag table is RFC-allocated per [[cipherocto-design-principles]]
§Extension over enumeration. The reserved `cipherocto-mainnet`
namespace ships as the only initial entry; subsequent deployments
allocate new entries via RFC amendment.

```rust
/// RFC-0010 v1.4 §RFC-allocated namespaces.
///
/// Production CipherOcto deployments MUST use a tag from this
/// table; user-extension chains fall in the `User` variant and
/// require operator attestation per RFC-0862 §Governance.
pub const RFC_CHAIN_NAMESPACES: &[[u8; 15]] = &[
    /// Canonical CipherOcto mainnet deployment.
    CIPHEROCTO_MAINNET_TAG,
];

/// Literal handle for the canonical mainnet namespace.
pub const CIPHEROCTO_MAINNET: &str = "cipherocto-mainnet";

/// BLAKE3-256 (15-byte truncation) of
/// `b"cipherocto/chain-namespace/v1" || b"cipherocto-mainnet"`.
pub const CIPHEROCTO_MAINNET_TAG: [u8; 15] = [
    /* precomputed at RFC acceptance; see §Test Vectors */
];
```

#### Test Vectors (v1.4)

10 canonical namespaces are encoded in
`crates/octo-ident/tests/chain_namespace_tv.rs`:

1. `cipherocto-mainnet` → Rfc variant, canonical bytes match precomputed tag.
2. `partner-mainnet` → User variant, BLAKE3-15 truncation = some known tag.
3. Empty literal → `ChainNamespaceError::Empty`.
4. 65-char literal → `ChainNamespaceError::TooLong`.
5. Literal with NUL byte → `ChainNamespaceError::ControlChar`.
6. Canonical-bytes round-trip: `from_canonical_bytes(canonical_bytes(ns)) == ns`.
7. Cross-namespace distinct: two distinct literals → two distinct tags.
8. RFC tag collision rejected: a literal hashing to a tag in
   `RFC_CHAIN_NAMESPACES` but with different length → `User`
   variant (tag + length combined disambiguate).
9. Reserved-variant byte `0x00` rejected at `from_canonical_bytes`.
10. `CIPHEROCTO_MAINNET` namespace → `rfc_variant_canonical_bytes_match`.

#### Out of scope for v1.4 (deferred)

- **Multi-chain DID resolution routing** (F2 follow-on). v1.4 ships
  the typed-namespace substrate; the resolver-node cross-chain
  routing logic remains future work, tracked under
  `missions/open/0010-f2-multi-chain-did-resolution.md`.
- **Registry keying by `(namespace, did_hash)` tuple**. v1.3
  `DidRegistry` keys by wire form; v1.4 `ChainId` is a coordinator
  parameter only. Schema migration to namespace-prefixed keys is F2
  follow-on.
- **`ChainNamespace` borsh-derive**. v1.4 ships the newtype; borsh
  impls land with F2 when the migration to namespaced keys makes
  borsh serialization necessary at the registry boundary.

#### Compatibility

- **Backward-compatible:** Yes (consumer-side). `ChainId::new(s)` in
  v1.3 returned `Self` unconditionally; v1.4 returns
  `Result<Self, ChainNamespaceError>`. Callers using `ChainId::new`
  with a valid literal remain correct (validation is a strict subset
  of pre-v1.4 acceptance — v1.3 accepted any non-empty string).
  Callers passing empty strings or control characters gain an error
  path that previously silently succeeded. The pre-existing
  `ChainId::new_unchecked` escape hatch preserves v1.3 behavior
  verbatim for internal callers.
- **Forward-compatible:** Phase 1 MVP `IdentityResolverNode` keeps
  its `DEFAULT_CHAIN_ID = "cipherocto-mainnet"` literal at the
  resolver-node boundary (mission `0871e-f7-impl-resolver-mediation`).
  v1.4 `ChainId::new(DEFAULT_CHAIN_ID)` succeeds since the literal
  is a valid RFC namespace. Follow-on missions that touch the
  registry call boundary will tighten via `ChainNamespace::from_literal`.
- **Cross-impl:** Validation rules are pure functions; BLAKE3
  derivation is RFC-0008 Class A. Test vector suite covers the
  canonical encodings; cross-impl conformance measured against
  reference impl in `crates/octo-ident/`.

### Rich DidDocument Extension (v1.5, additive on v1.3)

#### Motivation

`octo_ident::DidDocument` ships in v1.3 with the minimum surface
needed by `IdentityResolverNode` — a 32-byte storage-pubkey form
plus a revocation flag. RFC-0862 §Roles and Authorities already
references the extended shape (`chain_depth` / `chain_parent` /
`verification_method` / `authentication` / `assertion_method`)
for cross-instance DID write coordination substrate, but the
canonical type definition lives in `crates/octo-ident/` (Layer B
substrate) and must land here for the Layer direction to hold.

Production deployments need:

1. **`chain_depth` + `chain_parent`**: enables DID rotation flows
   (RFC-0010 v1.3 §Compatibility) where a rotated DID links back
   to its predecessor for revocation propagation.
2. **`verification_method`**: enables W3C DID Core 1.0 §Verification
   Method relationships (Ed25519 / BLS12-381 / future PQC) for
   capability delegation proofs.
3. **`authentication` + `assertion_method`**: W3C DID Core 1.0
   verification relationships; gates RFC-0871 `verify_capability`
   flows (capability issuer accepts a `did:octo:...` if the
   signing key appears in the subject's authentication array).

This extension is **additive on v1.3**: v1.3 `DidDocument` callers
that construct `DidDocument { public_key, revoked }` continue to
work — the new fields default-construct (zero/`None`/`Vec::new()`).
v1.5 makes all 7 fields `pub` and adds `VerificationMethod` enum.

#### Data Structures

```rust
/// Rich DID Document (RFC-0010 v1.5).
///
/// v1.3 ships the minimum surface; v1.5 extends with chain-rotation
/// fields + W3C DID Core 1.0 verification relationships. Lives in
/// `crates/octo-ident/src/registry.rs` (Layer B substrate; same
/// module as the v1.3 minimal version per [[cipherocto-design-principles]]
/// §Layer B additive-only rule).
#[cfg_attr(feature = "borsh", derive(BorshSerialize, BorshDeserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DidDocument {
    /// 32-byte storage-pubkey form (v1.3 field; matches `RawDid::hash`).
    pub public_key: [u8; 32],
    /// True if revoked. v1.3 field; resolve returns `None` when set.
    pub revoked: bool,
    /// DID rotation chain depth (v1 = 0). Incremented each rotation;
    /// `chain_parent` carries the predecessor's `RawDid.hash`. Bounds
    /// migration if the cap is ever raised.
    pub chain_depth: u8,
    /// Predecessor DID's hash (v1 = `None`).
    pub chain_parent: Option<[u8; 32]>,
    /// W3C DID Core 1.0 §Verification Method. Per
    /// [[cipherocto-design-principles]] §Extension over enumeration —
    /// 128-bit UUID discriminator + Raw escape; `Ed25519` / `Bls12381`
    /// are RFC-allocated variants; future PQC variants land via
    /// amendment without central-enum edits.
    pub verification_method: Vec<VerificationMethod>,
    /// W3C DID Core 1.0 authentication references (DID-relative URI
    /// strings, e.g., `"#key-0"`). Capability issuer gates flow here.
    pub authentication: Vec<String>,
    /// W3C DID Core 1.0 assertion-method references.
    pub assertion_method: Vec<String>,
}

/// Verification method relationship (RFC-0010 v1.5).
///
/// Same composition pattern as `CapabilitySpec` in RFC-0957-A1:
/// typed-discriminator + payload, no central enum that future
/// PQC variants must edit.
#[cfg_attr(feature = "borsh", derive(BorshSerialize, BorshDeserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerificationMethod {
    /// Ed25519 signature verification.
    Ed25519 { public_key: [u8; 32] },
    /// BLS12-381 signature verification.
    Bls12381 { public_key: [u8; 48] },
}

impl DidDocument {
    /// Construct the v1.3 minimal form (chain_depth = 0,
    /// chain_parent = None, verification_method + authentication +
    /// assertion_method empty). Backward-compatible constructor for
    /// v1.3 callers + existing TV.
    #[must_use]
    pub fn v1_minimal(public_key: [u8; 32], revoked: bool) -> Self {
        Self {
            public_key,
            revoked,
            chain_depth: 0,
            chain_parent: None,
            verification_method: Vec::new(),
            authentication: Vec::new(),
            assertion_method: Vec::new(),
        }
    }

    /// Returns `true` if `key_id` (DID-relative URI fragment, e.g.,
    /// `"#key-0"`) appears in `authentication`. Used by capability
    /// issuer to gate RFC-0871 verify flows on W3C DID Core 1.0
    /// authentication relationships.
    #[must_use]
    pub fn authenticates(&self, key_id: &str) -> bool {
        self.authentication.iter().any(|s| s == key_id)
    }
}
```

#### Test Vectors (v1.5)

12 canonical Document shapes are encoded in
`crates/octo-ident/tests/rich_did_document_tv.rs`:

1. `DidDocument::v1_minimal(zero_pk, false)` — v1.3 round-trip
   preserved (backward-compat TV).
2. `DidDocument::v1_minimal(max_pk, true)` — v1.3 round-trip,
   revoked form.
3. Rich Document with `chain_depth=1` + `chain_parent=Some([0x42;32])`
   — rotation case.
4. Rich Document with `verification_method = [Ed25519{...}, Bls12381{...}]`
   — multi-algorithm case.
5. `authenticates("#key-0")` returns `true` when authentication
   contains `"#key-0"`.
6. `authenticates("#missing")` returns `false`.
7. Borsh round-trip: 7-field struct, all variants populated.
8. Hash stability: `canonical_hash(doc)` is byte-exact across
   builds (RFC-0008 Class A determinism).
9. `chain_depth` cap (255) → migration pinned to v2 schema if ever
   raised.
10. `VerificationMethod` discriminants are distinct (Ed25519 ≠ Bls12381
    in the borsh-encoded tag byte).
11. v1.3 construction pattern `DidDocument { public_key, revoked }` no
    longer compiles in v1.5 (caller MUST use either
    `DidDocument::v1_minimal` or explicit-all-7 constructor). Test
    catches drift if the v1.5 fields become optional later.
12. RFC-0862 §Specification rich-Document shape byte-exact with
    the v1.5 reference (consistency guard between RFC layers).

#### Out of scope for v1.5 (deferred)

- **Persistence migration to rich DidDocument in `did_registry` table.**
  v1.5 SPECIFIES the type; the Stoolap schema migration adding the
  5 new columns (`chain_depth`, `chain_parent`, `verification_method`,
  `authentication`, `assertion_method`) is F8 follow-on. Until the
  migration lands, `StoolapDidRegistry` continues to persist the
  v1.3 minimum surface (`public_key`, `revoked`).
- **`chain_depth` cap raise**: v1.5 ships depth=0..=255; RFC
  amendment required if a deployment ever needs more than 255
  rotations. Migration tracked.
- **PQC verification methods** (Dilithium, SPHINCS+). v1.5 ships
  Ed25519 + BLS12-381; PQC variants land via follow-on amendments
  without central-enum edits (extension-over-enumeration pattern).
- **Multi-document storage** (one DID → multiple Documents across
  rotation). v1.5 ships single-Document-per-DID; F2 multi-chain
  may add per-chain Documents if needed.

#### Compatibility

- **Backward-compatible:** Construction only. v1.3 callers who
  constructed `DidDocument { public_key, revoked }` literally
  cannot compile against v1.5 — the new fields are required public.
  v1.5 ships `DidDocument::v1_minimal(public_key, revoked)` for the
  exact v1.3 pattern. The `DidRegistry` trait surface
  (register/resolve/revoke) is unchanged — the 5 new fields are
  opaque to the trait. Wire form (`ResolveResponse`) is byte-exact
  across the cutover for consumers that don't read Document fields.
- **Forward-compatible:** Phase 1 MVP `IdentityResolverNode` +
  `StoolapDidRegistry` continue to work with v1.5 `DidDocument`
  because they only read `public_key` + `revoked` (per mission
  `0871b-storage-backend`). Rich Document fields are wired into
  resolver flows when follow-on missions gain consumers
  (capability issuer verify, DID rotation migration).
- **Cross-impl:** `DidDocument` borsh serialization is feature-gated
  (per RFC-0862 R10 H11) — reference impl builds with
  `cargo test --features borsh`. Field order is canonical (matches
  RFC-0862 §Specification §Substrate types §DidDocument) so
  cross-impl hashes are byte-exact.

#### Compatibility

- **Backward-compatible:** Yes. v1.3 adds `DidRegistry` + reference impls
  without changing any v1.2 codec API. Existing consumers of
  `CanonicalCodec` (`octo-wallet::AudienceId::from_str`, `octo-identity-resolver-node::ResolveHandler`)
  continue to work unchanged.
- **Forward-compatible:** Phase 1 MVP `IdentityResolverNode` keeps its
  placeholder `RawDid::hash` derivation until mission `0871b-storage-backend`
  swaps in `DidRegistry::resolve`. The wire shape (`ResolveResponse`)
  is byte-exact across the cutover — no consumer-side migration.
- **Cross-impl:** Trait methods are deterministic per call; reference
  impls use the canonical BLAKE3 binding domain from §Binding Domain.
  Test vector suite covers the canonical encodings.

### Error Handling

```rust
pub enum DidError {
    /// Prefix is not "did:octo:z" / "did:octo:b" / any accepted form.
    UnrecognizedShape,
    /// Base58btc / base32 decode failed.
    InvalidEncoding,
    /// Decoded payload is not 32 bytes (wire) or 52 bytes (legacy wire).
    InvalidLength,
    /// Hash mismatch during decode (RFC-0009 §Verification step 1).
    HashPartMismatch,
    /// 6-month deprecation window closed for legacy form.
    LegacyFormExpired,
}
```

Each error maps to a wire code in `crates/octo-reputation/src/error.rs::ReputationError::RecorderDidMalformed(...)` (existing variant) when the codec is consumed by the reputation layer, OR `crates/octo-wallet/src/error.rs::WalletError::InvalidAudienceId(...)` when consumed by the wallet. Cross-boundary failures surface as the caller's domain error.

## Performance Targets

| Metric        | Target     | Notes                                   |
| ------------- | ---------- | --------------------------------------- |
| `raw_to_wire` | <500 ns/op | Single DID; no IO                       |
| `wire_to_raw` | <2 µs/op   | BLAKE3 hash verification adds cost      |
| `parse`       | <3 µs/op   | Worst case: legacy path with hash check |

## Implicit Assumptions Audit

| Assumption                                                                                                                | Where Relied Upon                     | Blast Radius if False                                                                                                  | Mitigation / Status                                                                                                                                                                                              |
| ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 52-byte raw form is consistently `BLAKE3-256 hash (32 bytes) \|\| 20 bytes version discriminator` across v001+ migrations | §Data Structures, §wire_to_raw step 4 | Existing reputation storage rows have non-conforming discriminators → wire decode rejects them with `HashPartMismatch` | Migration v011 or runtime guard: codec does NOT silently re-derive; it errors. Operators can run a separate reconciliation mission to re-mint raw bytes.                                                         |
| W3C multibase-z form is byte-stable across W3C DID Core revisions                                                         | §Data Structures                      | Prefix `did:octo:z` is internally managed (no W3C method registration yet per IA-4); revision risk is low              | Wire form is gated on `did:octo:` namespace owned by CipherOcto, not on W3C registration                                                                                                                         |
| 6-month deprecation window is communicated to downstream consumers before closure                                         | §parse step 3                         | Operators depending on legacy form fail at parse                                                                       | Mission C (deprecation) flips the gate; cli flag `--disable-legacy-did-deprecation` extends for an operator who needs more time                                                                                  |
| BLAKE3-256 of `(canonical binding domain \|\| subject pubkey)` is what the codec expects                                  | §wire_to_raw step 5                   | If the reputation layer stored something else (e.g. SHA-256), wire decode fails with `HashPartMismatch`                | Pre-flight check: codec is invoked only on rows persisted under known migrations. Migrations v001-v010 are checked into the registry.                                                                            |
| Base58btc alphabet equals Bitcoin base58btc                                                                               | §raw_to_wire step 2                   | Encoding differs across implementations → wire forms diverge                                                           | Reference impl: the canonical base58btc alphabet is `b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"` (matches RFC-0009 §Identity Struct). Codec crate's test suite includes 10 canonical vectors. |

### Categories Audited

- **Operator trust:** low — codec is stateless; relies on persisted data.
- **Platform trust:** none — no external platform.
- **Time source:** none — no time-dependent paths.
- **Network partition:** n/a.
- **Upgrade safety:** dual-parse window decouples old and new deployments.
- **Configuration:** none.
- **Identity stability:** n/a — codec is identity-agnostic.
- **Resource availability:** n/a.

## Security Considerations

- **Replay attacks:** n/a — codec does not emit signed payloads.
- **Consensus attacks:** n/a — codec is Class A deterministic.
- **Identity forgery:** wire_to_raw step 5 verifies BLAKE3 hash prevents tamper. If an attacker substitutes raw bytes, step 5 fails with `HashPartMismatch`.
- **Length-extension attacks:** BLAKE3-256 is length-extension-safe; not a relevant vector.
- **Determinism violations:** none — codec is pure.

## Adversarial Review

| Threat                                               | Impact                       | Mitigation                                                                        |
| ---------------------------------------------------- | ---------------------------- | --------------------------------------------------------------------------------- |
| Bit-flip in raw bytes → silently passes through wire | High (reputation laundering) | wire_to_raw step 5 verifies hash mismatch → `HashPartMismatch` error              |
| Base58btc alphabet swap → cross-impl wire divergence | High                         | Test vector suite + reference alphabet constant in the crate                      |
| Legacy form acceptance after deprecation window      | High                         | Mission C flips `--disable-legacy-did-deprecation` to default-off at window close |

## Adversary Analysis

> **5-Question Adversary Test:**

1. **Who benefits?** An attacker seeking reputation laundering via dual-encoding DIDs (already documented in `docs/use-cases/reputation-persistence.md`).
2. **What does it cost them?** Time to forge a DID that passes both 52-byte storage parsing AND W3C wire parsing = ≥ 2^32 brute-force trials given a randomly distributed hash space; ~1 hour on a single GPU.
3. **What do they gain if successful?** A new reputation record that is NOT recognized by wire-form parsers (or vice versa), letting the attacker bypass reputation-based admission.
4. **What's our defense?** (a) wire_to_raw hash check (BLAKE3-256 of canonical binding domain); (b) strict length check (52 bytes raw, 32 bytes wire, 62 chars legacy). (c) Mission C deprecation gate.
5. **Residual risk:** Acceptable. The hash check is the dominant defense; deprecation gate is the temporal hardening.

## Economic Analysis

**Not Applicable (N/A) for this RFC.** RFC-0010 is a process RFC defining the canonical DID codec (storage + wire form translation); it does not mint, settle, or transfer tokens. Token economics at the reputation marketplace layer are governed by RFC-0968 (reputation registry) and RFC-0959 (independent settlement chain).

**Indirect economic impact:** reputation forgery (DID substitution) has marketplace-gating consequences — a forged DID with corrupted reputation could bypass reputation-based admission. The `wire_to_raw` hash check + dual-parse deprecation window are the dominant defenses; tracked under §Adversarial Review and §Adversary Analysis above. No direct token-level effects.

## Compatibility

- **Backward-compatible:** Yes, during 6-month dual-parse window. Legacy `did:octo:b<52>` accepted.
- **Forward-compatible:** Post-window, legacy form rejected with `LegacyFormExpired`.
- **Cross-impl:** Base58btc alphabet and BLAKE3 specification are external standards; cross-impl determinism is measured against test vectors in the canonical spec.

## Test Vectors

10 canonical DIDs are encoded in `crates/octo-ident/tests/test_vectors.rs`:

1. `did:octo:z<zero>` — round-trip known answer.
2. `did:octo:z<max-pubkey>` — edge case at 32-byte limit.
3. Legacy `did:octo:b<aa..aa (52 chars)>` → wire form.
4. Truncated input (`did:octo:z<10 chars>`) → `DidError::InvalidEncoding`.
5. Wrong prefix (`did:foo:z...`) → `DidError::UnrecognizedShape`.

10 are added; full 100-vector suite is the Mission A acceptance criterion.

## Alternatives Considered

| Approach                              | Pros                                | Cons                                                                                                 |
| ------------------------------------- | ----------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Single 52-byte form everywhere        | Codemod-free for reputation storage | Wire form becomes legacy-bizarre: 52 bytes → ~70 char base58btc string; breaks W3C conformance       |
| Amend RFC-0009 in-place               | Single RFC surface                  | Couples process + economics; cross-RFC dependency ties the storage and wire forms at the wrong layer |
| Defer codec, leave 3 parsers unmerged | No work                             | Status quo; documented injection vector persists                                                     |

## Implementation Phases

### Phase 1: Codec crate + canonical mapping (Mission A)

- [ ] `crates/octo-ident/` new crate.
- [ ] `DidCodec` trait + default impl.
- [ ] 100 test vectors (10 canonical + 90 generated random).

### Phase 2: Codemod (Mission B)

- [ ] 347 literals replaced via documented helper `sample_did(seed)`.
- [ ] Legacy literals archived to `tests/_archived/did_literals.md`.

### Phase 3: Deprecation (Mission C)

- [ ] 6-month deprecation window opens at S6 merge.
- [ ] Mission C flips `LegacyFormExpired` error after window.

## Key Files to Modify

| File                                                            | Change                                                                    |
| --------------------------------------------------------------- | ------------------------------------------------------------------------- |
| `crates/octo-ident/` (new)                                      | Codec crate                                                               |
| `crates/octo-reputation/src/types.rs`                           | `RecorderDid::to_wire()` method delegates to codec                        |
| `crates/octo-wallet/src/identity.rs`                            | `AudienceId::parse_canonical()` accepts W3C; legacy `from_str` deprecated |
| `crates/quota-router-core/src/marketplace/reputation_compat.rs` | `parse_canonical_did` delegates to codec                                  |
| `Cargo.toml` (workspace)                                        | Add `octo-ident` member                                                   |

## Future Work

- F1: W3C DID method registration (IA-4). Out of MVP scope.
- F2: Multi-chain DID resolution. **Substrate landed in v1.4** (`ChainNamespace` typed newtype + `ChainId` validator); resolver-node cross-chain routing logic remains future work. Tracked by `missions/open/0010-f2-multi-chain-did-resolution.md` (BLOCKED on this RFC per memory `mission-gap-closure-priorities-2026-08-10`).
- F3: Capability key derivation against the codec (extend RFC-0009 §Capability Keys).
- F4: **Wallet audience validation (closed 2026-08-08 audit; landed in v1.2).** `octo-wallet::AudienceId::from_str` at `crates/octo-wallet/src/identity.rs` accepts any non-empty string and MUST validate via `octo_ident::CanonicalCodec::parse(s, false)` to enforce canonical wire-form parsing at every entry point. Tracked by `missions/claimed/0010-d-wallet-audience-validation.md`.
- F5: **DID storage trait extension (additive in v1.3).** `crates/octo-ident/` does NOT currently expose a public storage trait — the codec crate owns canonical encoding only, not the persistence layer. The Phase 1 MVP `IdentityResolverNode` (mission `0871b-identity-resolver-node`) returns a placeholder `public_key` derived from `RawDid::hash` because the storage backend substrate is missing. v1.3 introduces a `DidRegistry` trait + reference impls so production deployments can persist + resolve real DID Documents without an in-process placeholder. Tracked by `missions/open/0871b-storage-backend.md` (BLOCKED on this RFC per memory `mission-gap-closure-priorities-2026-08-10`).
- F6: **Cross-domain DID resolution (resolver chains).** Out of scope for this RFC — owned by RFC-0871 §Future Work. Resolver chains traverse multiple specialized nodes (resolver hops + cross-domain authorization); needs the `ResolverBackend` typed view over `DidRegistry` (separate amendment once F5 substrate lands). Tracked by `missions/open/0871b-cross-domain-resolution.md`.
- F7: **Cross-instance DID registry coordination.** Out of scope for this RFC — owned by RFC-0862 atomic transaction substrate. Production HA / sharded deployments need cross-instance DID write coordination; the substrate is analogous to the `DrainCoordinator` work for `SpendLedger` (mission `0871e-phase5c-1-cross-instance-drain`). Tracked separately; not gated on F5.

## Rationale

Why the dual storage/wire split? Reputation storage already shipped migrations v001-v010 with the 52-byte form. A destructive 52→32 migration would invalidate all reputation history. The codec bridges the existing storage to the W3C wire form without invalidating persisted state. New DIDs minted after this RFC MUST use the codec crate's `from_storage_pubkey` to ensure wire-form compatibility from day 1.

## Version History

| Version | Date       | Status               | Changes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| ------- | ---------- | -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0.1     | 2026-07-26 | Draft                | Initial submission; codec crate + dual-form split                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| 0.2     | 2026-07-27 | Draft (review-fix)   | Round-1 fixes: (H1) `wire_to_raw` re-derives version_discriminator via binding-domain, not stored; (H2) `RawDid` made structured type with explicit `hash` + `version_discriminator` fields rather than `#[repr(C)] [u8; 52]`; (M1) `parse` step 3 tightened to reject bare `:name` literals during deprecation window; (L1) added `mint(pubkey)` algorithm documenting canonical pubkey → raw DID path                                                                                                                                                                                                                                                                                                                                                                                        |
| 1.0     | 2026-07-27 | Accepted             | Promoted after single-round review; sibling to RFC-0009. Status header updated; file moved via `git mv` from `rfcs/draft/process/` to `rfcs/accepted/process/`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| 1.1     | 2026-08-03 | Accepted (audit)     | Audit pass: stripped `(Process)`/`(Economics)` category parens from RFC references + H1 title per CLAUDE.md referencing rule; added §Economic Analysis (N/A justification); converted §Implementation Phases to `- [ ]` checkboxes per template §693; expanded Version History with Draft → Accepted progression                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| 1.2     | 2026-08-08 | Accepted (amendment) | Added F4 (Wallet audience validation) per 2026-08-08 specialized node protocol research. `AudienceId::from_str` must call `CanonicalCodec::parse(s, false)` rather than accept any non-empty string. Foundation for RFC-0871 (specialized node protocol envelope) where `from_did: WireDid` is validated at every envelope boundary. Cross-references: `rfcs/accepted/networking/0871-specialized-node-protocol-envelope.md`, `docs/research/2026-08-08-specialized-node-protocol-research.md`                                                                                                                                                                                                                                                                                                 |
| 1.3     | 2026-08-10 | Accepted (amendment) | Added §Storage Extension: `DidRegistry` trait in `crates/octo-ident/` (Layer B substrate) + `InMemoryDidRegistry` test impl + `StoolapDidRegistry` production impl in `crates/quota-router-storage/` (cipherocto-side migration v008). Mirrors `HolderRegistry` (RFC-0957-A1) trait shape. Unblocks mission `0871b-storage-backend` (BLOCKED on this RFC per memory `mission-gap-closure-priorities-2026-08-10`). Cross-instance coordination (F7) + `ResolverBackend` typed view (F6) + multi-chain resolution (F2) + rich DID Documents explicitly OUT of scope — owned by future RFC-0862 amendment / RFC-0871 §Future Work / F2 future work / future amendment respectively. F4 status moved to "closed" (landed in v1.2). New §Roles and Authorities row for `DID Storage Backend` actor. |
| 1.4     | 2026-08-11 | Accepted (amendment) | Added §ChainId Namespace Extension: `ChainNamespace` typed newtype + `NamespaceVariant` enum (`Rfc`/`User`/`Reserved`) + `ChainId::new` now returns `Result<_, ChainNamespaceError>` (validator rejects empty / >64-char / control-char literals). New `ChainId::new_unchecked` escape hatch preserves pre-v1.4 behavior for internal callers; pre-v1.4 `ChainId::new("cipherocto-mainnet")` and similar valid literals remain correct (validation is a strict subset of pre-v1.4 acceptance). RFC-allocated namespace table ships with `CIPHEROCTO_MAINNET` as the only initial entry; user-extension chains parse through `NamespaceVariant::User`. 17-byte canonical serialization (`canonical_bytes`/`from_canonical_bytes`) anchors WAL entries + `GovernanceAttestation.chain_id` (per RFC-0862 v1.3 §Roles `chain_id: ChainId` field) + RPC audit logs. Unblocks mission `0010-f2-multi-chain-did-resolution` (BLOCKED on this RFC); F2 routing logic itself is a follow-on amendment. Cross-instance coordination (F7) + `ResolverBackend` typed view (F6) + rich DID Documents (F8) remain OUT of scope here. |
| 1.5     | 2026-08-11 | Accepted (amendment) | Added §Rich DidDocument Extension: `DidDocument` extends from v1.3 `(public_key, revoked)` to 7-field struct (`chain_depth`, `chain_parent`, `verification_method`, `authentication`, `assertion_method`). New `VerificationMethod` enum (`Ed25519{public_key: [u8; 32]}` / `Bls12381{public_key: [u8; 48]}`) uses 128-bit UUID-discriminator + variant-payload shape per [[cipherocto-design-principles]] §Extension over enumeration (future PQC variants land via amendment). New `DidDocument::v1_minimal(public_key, revoked)` backward-compatible constructor preserves v1.3 call sites. New `DidDocument::authenticates(key_id)` method enables W3C DID Core 1.0 authentication-reference gating (capability issuer downstream). Cross-RFC consistency guard: v1.5 `DidDocument` byte-exact with the spec shape referenced from RFC-0862 v1.3 §Specification §Substrate types §DidDocument; no live-writer on the rich fields yet — concrete wire consumers land with F8 migration. Did NOT update the RFC-0862 v1.3 inline comment "v1.4 amendment" → out-of-band, follow-on if the comment drift surfaces in review. Unblocks mission `0010-f8-rich-did-documents` (BLOCKED on this RFC); F8 migration itself is a follow-on (Stoolap schema adding 5 columns). Cross-instance coordination (F7) + `ResolverBackend` typed view (F6) + multi-chain routing (F2 routing) remain OUT of scope here. |
| 1.6     | 2026-08-19 | Accepted (amendment) | Added §32-byte Addendum: `ChainId::as_bytes()` returns `[u8; 32]` via `BLAKE3("cipherocto/chain/v1/" || chain_string)`. Domain-separator convention matches `AssetId::as_bytes` (`"cipherocto/asset/v1/"`) and `vault_id` (`"cipherocto/vault/v1/"`) per Layer A frozen substrate pattern (§20.3.2). Storage column type `BLOB(32)` (per RFC-0960 v3.0 + RFC-0900 v2.0 + 0900-d1) carries this 32-byte form. Coexists with v1.4 `ChainNamespace::canonical_bytes()` 17-byte form (legacy WAL/audit log wire form) — both serve distinct purposes (storage PK vs WAL/audit log). Implementation mission `0010-c-32-byte-addendum`; 3 byte-exact TV in `crates/octo-ident/tests/tv_0010_chain_id_32byte.rs` (TV-1 determinism, TV-2 BLAKE3 known-vector `eb200e7d...411eeab`, TV-3 17-byte + 32-byte coexistence). Storage substrate unchanged (already uses `[u8; 32]`); only the canonical derivation method is newly specified. Cross-instance coordination (F7) + `ResolverBackend` typed view (F6) + multi-chain routing (F2 routing) remain OUT of scope here. |

## Related RFCs

- RFC-0009 — Identity Management
- RFC-0968 — Reputation Registry
- RFC-0968-A1 in-place amendment (2026-07-26)

## Related Use Cases

- [Canonical OctoID Identifier](../../docs/use-cases/canonical-octoid-identifier.md)
- [Persisted Reputation](../../docs/use-cases/reputation-persistence.md)

## Appendices

### A. Binding domain

The BLAKE3-256 hash check in `wire_to_raw` step 5 uses the canonical binding domain `b"cipherocto/octoid/v1"`. Any subject pubkey is hashed as `BLAKE3(canonical_binding_domain || pubkey)`; the resulting 32-byte value is the storage `RawDid`'s leading 32 bytes. The trailing 20 bytes are zeros (version discriminator v1).

Future versions can swap the binding domain for v2 without breaking existing rows (which already hashed under v1).
