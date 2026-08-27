# RFC-0965 (Economics): Capability Extension Format (Caveat DSL)

## Status

Accepted v2.0 — Status header synced with §Version History v2.0-CanonicalAlias row per M37 corpus-wide sync check (prior header read v1.1; VH row added 2026-08-17 with Canonical `MicroOctoW` alias cross-reference closure per mission 0862-c9 audit verdict Risk #1 CRITICAL).

> **Note:** Companion RFC to RFC-0960 §2.2 (Capability). Defines the caveat types added by RFC-0960 to the RFC-0957 macaroon substrate. Each new caveat is a typed wrapper around a `Constraint` (RFC-0964) with macaroon attenuation semantics. Attenuation invariant (add-only, monotonic restriction) is preserved by RFC-0957. Builds on RFC-0957 (macaroon v1), RFC-0126 (canonical_ser), RFC-0964 (constraint encoding), and Phase 3 research (`docs/research/2026-07-22-external-capability-based-spend-systems.md`).

## Version History

| Version | Date | Author | Note |
|---------|------|--------|------|
| v1.0 | 2026-07-23 | @cipherocto + @mmacedoeu | Initial draft. |
| v1.1 | 2026-07-23 | @cipherocto + @mmacedoeu | **Strategic reframe (R17+).** Added new caveat type `PolicyReference` (RFC-0967 Policy Object Graph). Capability now carries a `policy_id` reference instead of embedding all policy clauses. Backwards-compatible: existing caveat-only capabilities continue to work. Caveat total count: 21 → 22. Additive (non-breaking) bump. |
| v1.1-Accepted | 2026-07-23 | @cipherocto + @mmacedoeu | **Promoted Draft → Accepted.** R1-R28 multi-round adversarial review closed with R28 clean round (zero actionable defects). Companion RFCs (RFC-0960, RFC-0961, RFC-0962, RFC-0963, RFC-0964, RFC-0967) promoted in lockstep on 2026-07-23. |
| v1.2-Resolved | 2026-07-23 | @cipherocto + @mmacedoeu | **Risk-closure round.** All 6 Open Questions resolved: multi-`Permission` set union + RFC-0957 attenuation; `MaxUses` failsafe (`uses_consumed + 1 > max_uses` rejects even on partial sync); `Factory.Vet` per RFC-0126 + RFC-0964; `Sharded` caveat original-only; `PermissionKind` 2-byte expansion deferred v1.1 (schedule: post-v2.0); `AuditWindow` sync semantics (RFC-0862 delivers dispute; late = no effect). Additive (non-breaking) bump. |
| v1.3-Referenced | 2026-08-08 | @cipherocto + @mmacedoeu | **Cross-reference to RFC-0871.** The caveat discriminator byte pattern (1-byte `0x01-0xCF` for RFC-allocated + `0xD0-0xFF` for application-specific) established by this RFC is the precedent for the larger 16-byte `PayloadKindId` discriminator in RFC-0871 (specialized node protocol envelope). The same `Raw { discriminator, body }` escape hatch pattern in RFC-0871's `Authorization` enum follows the `RawCaveat` pattern defined here. Both RFCs share the same extension principle: new types are added via RFC + reserved-range allocation + Raw escape hatch; old code fail-closes on unknown discriminators. See `docs/research/2026-08-08-specialized-node-protocol-research.md` for the design lineage. |
| v1.4-CrateLayout | 2026-08-08 | @cipherocto + @mmacedoeu | **Per-extension crate layout.** Added §Per-Extension Crate Layout subsection mandating that caveat types AND capability types land in separate crates (`crates/octo-cap-{macaroon,zk,federation,time-lock,threshold-mpc}/` etc.), each registering a `CapabilitySpec` impl via plugin. Wallet core unchanged; capability attenuation invariant (RFC-0957 §3.5) preserved across the crate boundary. Migration path: today's single-crate `cipherocto-capability-ext` reference impl → `crates/octo-cap-macaroon/` per RFC-0957 §Per-Extension Crate Layout. Implementation missions: `missions/open/0957-ext-macaroon-crate.md`, `missions/open/0957-ext-zk-crate.md`. Cross-references: RFC-0957 §Per-Extension Crate Layout; RFC-0871 §Wallet Node Lifecycle. |
| v2.0-CanonicalAlias | 2026-08-17 | @cipherocto + @mmacedoeu | **Canonical `MicroOctoW` alias cross-reference.** §3 prologue note added: all amount-bearing caveat payload fields (`PaymentCaveat.budget`, `AmountMax`, `PerAxisMax.max_per_1k`, `PaymentCaveat.query_cost`, etc.) use the canonical alias `octo_determin::MicroOctoW` (= `Dqa` at `scale = 0`, integer micro-OCTO_W counts). Cross-ref RFC-0105 §substrate type coherence + RFC-0862 v2.0.3 §SpendLedger Substrate. Prior local `pub type MicroOctoW = u128` aliases in `octo-cap-macaroon` removed per mission `0862-c9` (audit verdict 2026-08-17 Risk #1 CRITICAL); construction is now `Dqa { value: <i64>, scale: 0 }` at the substrate boundary. TV-0862-17 + TV-0862-18 pin byte-exact round-trip. Additive (non-breaking) — subtype remains structurally identical to `Dqa` at `scale = 0`. |

## Authors

- Author: @cipherocto (Capability extension work)
- Contributor: @mmacedoeu (RFC-0965 protocol extraction)

## Maintainers

- Maintainer: @mmacedoeu
- Maintainer: @cipherocto

## Summary

A `Capability` (RFC-0957 macaroon) carries a list of `Caveat` objects. RFC-0965 defines the new caveat types that RFC-0960 adds on top of RFC-0957's existing caveat set.

Three artifacts:

1. **Caveat type enumeration** — RFC-0957's existing 12 caveat types + RFC-0965's 9 new types = 21 total caveat types.
2. **Caveat envelope** — typed wrapper: `(caveat_type_discriminator, constraint_payload)`. Discriminator 1 byte + constraint encoding per RFC-0964.
3. **Macaroon attenuation rules** — adding a caveat can only **restrict**, never **expand** the capability. RFC-0957 invariant preserved.

The new caveat types are the 9 RFC-0960-specific concepts that didn't fit RFC-0957's original surface: `Vault`, `Permission`, `ValidAfter`, `MaxUses`, `AuditWindow`, `RedemptionContext`, `WrappedOnly`, `Factory`, and `Sharded`. The remaining RFC-0960 fields (`valid_until`, `max_per_tx`, `rate_limit`, etc.) are conveyed via the RFC-0964 `Constraint` envelope — they reuse existing Constraint variants.

## Dependencies

### Required RFCs

| RFC | Status | Reason |
|-----|--------|--------|
| RFC-0960 | **Accepted v2.0 (2026-07-23; promoted in lockstep with this RFC)** | Defines §2.2 Capability extensions |
| RFC-0957 | Accepted | Macaroon substrate + attenuation invariant |
| RFC-0964 | **Accepted v1.1 (2026-07-23; promoted in lockstep with this RFC)** | Constraint canonical encoding (caveats wrap constraints) |
| RFC-0126 | Accepted (v2.5.1) | Canonical serialization for caveat envelopes |
| RFC-0853 | Draft | BLAKE3 primitive source |

### Companion RFCs

| RFC | Relationship | Reason |
|-----|--------------|--------|
| RFC-0961 | Builds on | CIPHERO_SQL procedures referenced by `AllowIf` constraint — Accepted v2.0 (2026-07-23; promoted in lockstep) |
| RFC-0963 | Builds on | `Sharded` caveat pins capability to one shard — Accepted v2.0 (2026-07-23; promoted in lockstep) |
| RFC-0967 | Refers to | Policy Object Graph (caveat type `PolicyReference` carries `policy_id`) — Accepted v1.0 (2026-07-23, NEW; promoted in lockstep) |

### Dependency Validation

| Dependency | Type | Current Status (2026-07-23) | Hard-block? |
|------------|------|------------------------------|-------------|
| RFC-0960 | Requires | **Accepted v2.0 (promoted in lockstep)** | **YES → resolved** |
| RFC-0957 | Requires | Accepted | No |
| RFC-0964 | Requires | **Accepted v1.1 (promoted in lockstep)** | **YES → resolved** |
| RFC-0126 | Requires | Accepted | No |
| RFC-0853 | Requires | Draft | YES |

**DAG check:** `0965 ← {0960, 0957, 0964, 0126, 0853}` — acyclic. RFC-0960 and RFC-0964 promoted to Accepted on 2026-07-23; their hard-blocks resolved.

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Attenuation invariant preserved | Adding a caveat can only restrict, never expand |
| G2 | Compact envelope | Caveat header ≤ 8 bytes; payload ≤ 64 bytes typical |
| G3 | Forward-compatible | New caveat type = new discriminator byte; old parsers fail-closed |
| G4 | Type-safe deserialization | Unknown caveat types rejected at parse time |
| G5 | Verifiable offline | All caveat checks are local; no external state lookups required at verify time |

## Motivation

### Why extend the macaroon caveat DSL?

RFC-0957's macaroon v1 carries:
- First-party caveats: predicates on the redeeming request (e.g., "request is POST", "amount < 100").
- Third-party caveats: discharge requirements (e.g., "Alice's bank must confirm").
- Discharge bag: collected third-party discharges.

For RFC-0960's economic primitive layer, RFC-0957's existing caveats are insufficient. We need:
- **Resource-bound caveats** — `Vault(vault_id)` to bind a capability to a specific vault.
- **Permission-kind caveats** — `Permission(NativeTokenTransfer)` to restrict the action type.
- **Audit-window caveats** — `AuditWindow(duration)` for the dispute period.
- **Composition caveats** — `WrappedOnly` for hierarchical capabilities.

These map cleanly to the macaroon caveat DSL: a caveat is a predicate on the redeeming operation. The macaroon verifier evaluates each caveat; if any returns false, the capability is invalid for that operation.

### Why fail-closed on unknown caveats?

A capability must be evaluated exactly the same on every node. If node A has caveat type X but node B doesn't, they diverge. Two solutions:
- **Fail-closed** (chosen): node B rejects the capability because it doesn't recognize X.
- **Fail-open with flag**: node B warns but accepts; eventually all nodes upgrade.

Fail-closed is safer: a malformed capability never gets past verification. The downside is that all nodes must upgrade in lockstep to accept a new caveat type. For our purposes (consensus + ZK-friendly verification), this is the right tradeoff.

## Specification

### 0. Wire-format envelope (outer prefix + inner envelope)

The outer-envelope model is defined in **RFC-0964 §0**. Tag values for the
RFC-0964/0965 stack:

| Tag | Meaning | Inner envelope spec |
|---|---|---|
| 0x00 | forbidden (fail-closed) | — |
| 0x01 | **Constraint** | RFC-0964 §1, §2 |
| 0x02 | **Caveat** | RFC-0965 §1, §2 below |
| 0x03 | **Capability** | RFC-0965 §6 |
| 0x04 | **ExecutionEnvelope** (RFC-0962; renamed from ConsensusSession) | RFC-0962 §4 |
| 0x05 | **Reservation** | RFC-0960 §2.3 |
| 0x06 | **SettlementReceipt** | RFC-0959 §Data Structures |
| 0x07 | `PolicyReference` (RFC-0967) | RFC-0967 §8.1 (added v1.1; `policy_id` ref) |
| 0x08-0x1F | reserved | TBD |
| 0x20-0xFF | application-specific | per app |

**PermissionKind** and **ReservationState** are NOT standalone envelopes —
they appear only as field values inside Caveat (§3.2) and Reservation
(RFC-0960 §2.3) envelopes respectively.

Receivers dispatch on the outer 1-byte tag first. Unknown tags fail-closed.
The inner Caveat envelope's `version_tag` (if any) is inside the inner
envelope and is independent of the outer tag.

### 1. Caveat type enumeration

RFC-0957 defines 12 existing caveat types. RFC-0965 adds 9 new types (0x10-0x18). RFC-0965 adds 1 more (`PolicyReference` at 0x19, RFC-0967). Total: 22 distinct bytes. Discriminator byte range: 0x01-0x0C (RFC-0957) + 0x10-0x19 (RFC-0965 + v1.1) = 22 distinct bytes. Range 0x0D-0x0F reserved per §0 (RFC-0964 namespace tag rules). 0x1A-0xCF reserved for future extensions. 0xD0-0xFF application-specific.

#### 1.1 RFC-0957 existing caveat types (unchanged)

| Discriminator | Name | Purpose |
|---|---|---|
| 0x01 | `IpAddress` | Restrict to source IP |
| 0x02 | `RequestPath` | Restrict URL path |
| 0x03 | `RequestMethod` | Restrict HTTP method |
| 0x04 | `Before` | Time-bound (deprecated; use `ValidAfter` instead) |
| 0x05 | `After` | Time-bound (deprecated; use `ValidRange` instead) |
| 0x06 | `Equals` | Equality check |
| 0x07 | `LT` | Less-than check |
| 0x08 | `GT` | Greater-than check |
| 0x09 | `IN` | Set membership |
| 0x0A | `ThirdPartyBind` | Third-party discharge binding |
| 0x0B | `Version` | Version pinning |
| 0x0C | `Custom` | Application-defined (must specify opaque verifier) |

#### 1.2 RFC-0965 new caveat types

| Discriminator | Name | Wraps (Constraint) | Purpose |
|---|---|---|---|
| 0x10 | `Vault` | — | Bind capability to a vault (32-byte vault_id) |
| 0x11 | `Permission` | — | Restrict action type (PermissionKind enum) |
| 0x12 | `ValidAfter` | — | Time bound (single timestamp, no end) |
| 0x13 | `MaxUses` | — | Use count limit |
| 0x14 | `AuditWindow` | — | Dispute period (Duration) |
| 0x15 | `RedemptionContext` | — | Domain separator for replay defense |
| 0x16 | `WrappedOnly` | — | Only usable through a parent capability |
| 0x17 | `Factory` | — | Pre-validated invocation (Vet, not bytes) |
| 0x18 | `Sharded` | — | Pin capability to one shard (RFC-0963) |

Discriminators `0xD0-0xFF` reserved for application-specific extensions. Discriminator `0x00` forbidden.

### 2. Caveat envelope encoding

```text
Caveat envelope (one caveat):
    discriminator_byte:   u8                       // 1 byte, ∈ [0x01, 0x18] ∪ [0xD0, 0xFF]
    length_prefix:       [u8; 4]                  // 4 bytes BE; length of payload
    payload:             [u8; length_prefix]      // variant-specific encoding
```

Total: 5 bytes overhead + payload bytes.

For caveats that wrap a `Constraint` (RFC-0964), the payload is a constraint envelope. For caveats that don't (e.g., `Vault`, `Permission`), the payload is the raw caveat-specific data.

### 3. New caveat payload encodings

**Amount-bearing payload type.** All caveat payload fields carrying an amount (e.g., `PaymentCaveat.budget`, `AmountMax`, `PerAxisMax.max_per_1k`, `PaymentCaveat.query_cost`) use the canonical alias `octo_determin::MicroOctoW` (= `Dqa` at `scale = 0`, integer micro-OCTO_W counts). The canonical alias lives in `determin/src/lib.rs` (Layer A frozen substrate per RFC-0105 §substrate); see RFC-0862 §SpendLedger Substrate for the workspace-wide type-system coherence invariant. Prior local `pub type MicroOctoW = u128` aliases in `crates/octo-cap-macaroon/src/caveat/mod.rs` + `crates/octo-cap-macaroon/src/caveat/payment.rs` were removed by mission `0862-c9` (audit verdict 2026-08-17 Risk #1); construction is now `Dqa { value: <i64>, scale: 0 }` at the substrate boundary. Mission `0862-c9` TV-0862-17 + TV-0862-18 pin byte-exact round-trip.

#### 3.1 Vault (0x10)

```text
Vault payload:
    vault_id:            [u8; 32]                // RFC-0960 VaultID
// 32 bytes payload
```

Verification: the redeeming operation's `vault_id` must equal this caveat's `vault_id`.

Attenuation: parent capability's `Vault` caveat MUST equal child's `Vault` caveat. A child cannot change the vault binding.

#### 3.2 Permission (0x11)

```text
Permission payload:
    permission_kind:     u8                       // 1 byte; PermissionKind enum value
// 1 byte payload
```

PermissionKind values:

| Value | Kind |
|---|---|
| 0x01 | `NativeTokenTransfer` |
| 0x02 | `ERC20TokenTransfer` |
| 0x03 | `ContractCall` |
| 0x04 | `Reservation` |
| 0x05 | `VaultMutation` |

Verification: the redeeming operation's `permission_kind` must be in this caveat's set. (Capability may carry multiple `Permission` caveats; the **set** is the union — order does not matter for verification.)

**Set semantics for `Permission`:** Even though `CaveatSet` is an ordered list (and the canonical encoding preserves order for `caveats_hash` determinism), `Permission` caveats are evaluated as a **set** during verification and attenuation. Two capabilities with the same `Permission` set in different CaveatSet positions have **different `caveats_hash` values** but **the same authorization surface**. This is intentional: `caveats_hash` is content-addressed; authorization is set-based.

Attenuation: child's `Permission` set MUST be a subset of parent's. Adding a `Permission` caveat restricts; removing is not allowed. The attenuation check is set-based, not sequence-based.

#### 3.3 ValidAfter (0x12)

```text
ValidAfter payload:
    not_before_unix:     u64 BE
// 8 bytes payload
```

Verification: `current_time >= not_before_unix`. This is a one-sided time bound; for two-sided use `Constraint::ValidRange` (RFC-0964).

Attenuation: child's `not_before_unix` MUST be ≥ parent's. Increasing the floor restricts.

#### 3.4 MaxUses (0x13)

```text
MaxUses payload:
    count:               u32 BE                   // 0 = unlimited
// 4 bytes payload
```

Verification: `uses_consumed < count`. The `uses_consumed` counter is a projection over the `capability_events` log (RFC-0960 §2.2).

Attenuation: child's `count` MUST be ≤ parent's. Lowering the cap restricts.

#### 3.5 AuditWindow (0x14)

```text
AuditWindow payload:
    duration_secs:       u64 BE                   // 0 = instant release (high trust)
// 8 bytes payload
```

Verification: after a `Settlement` lands, the reservation stays in `Auditable` state for at least `duration_secs` before transitioning to `Released` (RFC-0960 §6).

Attenuation: child's `duration_secs` MUST be ≥ parent's. Lengthening the window restricts (more time for disputes). **Zero-parent edge case:** if parent's `duration_secs` is `0` (instant release, high-trust), child can set any value ≥ 0 — including a non-zero value. This is the "upgrade from high-trust to auditable" path, which is a restriction, not an expansion. The attenuation rule `≥` already permits this; restated explicitly to avoid confusion.

#### 3.6 RedemptionContext (0x15)

```text
RedemptionContext payload:
    context_hash:        [u8; 32]                // BLAKE3(0xA2 || canonical_ser(context))
// 32 bytes payload
```

`context` is application-defined. Two canonical encodings are accepted:

| Encoding | Format | When to use |
|---|---|---|
| `CanonicalContext::Bytes(Vec<u8>)` | `0x01 || length_prefix || bytes` | Opaque blob; verifier does not interpret |
| `CanonicalContext::Structured(ContextFields)` | `0x02 || canonical_ser({request_id, chain_id, marketplace_id, ...})` | Typed fields; verifier validates structure |

Verification: `BLAKE3(0xA2 || canonical_ser(context)) == context_hash`. The same operation submitted with a different context fails. The `0xA2` prefix is the RedemptionContext-hash domain separator, distinct from the namespace tags (0x00-0x06) and version tags (0xA0-0xA1) per RFC-0964 §0+§4.

Cross-implementations MUST use the same `CanonicalContext` encoding. Mixed JSON vs protobuf encodings produce different hashes for the same logical context.

Attenuation: child's `context_hash` MUST equal parent's. Cannot change the bound context.

#### 3.7 WrappedOnly (0x16)

```text
WrappedOnly payload:
    parent_capability_id: [u8; 32]               // RFC-0957 CapabilityID of parent
// 32 bytes payload
```

Verification: the redeeming operation must present the parent capability in addition to this one. Implements hierarchical capability composition (RFC-0960 §11).

Attenuation: parent's `WrappedOnly` chain must be a prefix of child's. A child can extend the chain downward but cannot skip parents.

**Chain depth limit:** Maximum `WrappedOnly` chain depth = **16**. Verifiers reject any capability whose chain exceeds this. A 17-deep chain (or any circular reference like A→B→A) is malformed; verifiers MUST reject it with `E_CHAIN_DEPTH_EXCEEDED`. Cycle detection: each capability in the chain is identified by its 32-byte `capability_id`; a verifier walks the chain, recording seen IDs and rejecting any repeat or chain length > 16.

#### 3.8 Factory (0x17)

```text
Factory payload:
    vet:                 Vet                       // RFC-0960 §10.7; structured, NOT bytes
// variable bytes (typically 80-200 bytes)
```

`Vet` is a structured object:

```text
Vet {
    target_vault_id:     [u8; 32]                 // vault the vet creates/modifies
    action_template:     CanonicalAction,         // NOT opaque bytes
    required_caller:     Option<DID>,             // optional caller binding
    pre_conditions:      Vec<Constraint>,         // additional constraints
    expiry_for_deploy:   u64,                     // unix timestamp
}
```

Verification: the redeeming operation deploys the action_template against target_vault_id, optionally bound to required_caller, with pre_conditions evaluated, before expiry_for_deploy.

Attenuation: the vet's constraints MUST be a superset of (or equal to) the parent capability's relevant constraints. Adding pre_conditions restricts.

**Rejection of raw bytes:** the `Factory` caveat MUST NOT carry raw bytes (per RFC-0960 R1-F7 fix). The vet is structured so the verifier can run the same constraint pipeline against the deployed target. This blocks the phishing vector where a "factory" caveat carries arbitrary call data that looks benign but isn't.

#### 3.9 Sharded (0x18)

```text
Sharded payload:
    shard_id:            u32 BE                   // per RFC-0963 ShardID
// 4 bytes payload
```

Verification: `shard_id(capability.vault_id, num_shards) == shard_id`. The capability is valid only on this shard.

Attenuation: child's `shard_id` MUST equal parent's. Cannot move a capability across shards via attenuation.

#### 3.10 PolicyReference (0x19) — v1.1 NEW

```text
PolicyReference payload:
    policy_id:           [u8; 32]                // BLAKE3 of canonical_ser(PolicyObject); RFC-0967 §2
    policy_version_seq:  u64 BE                  // pin to specific PolicyObject version
    attenuation_proof:   AttenuationProof         // RFC-0967 §8.2 (32 bytes policy_id || 32 bytes policy_id || Vec<NodeID> || 64 bytes signature)
// 32 + 8 + variable payload (typical 100-300 bytes)
```

Verification: the referenced `PolicyObject` (RFC-0967) MUST exist on-chain with `policy_id` and `policy_version_seq` matching. The `attenuation_proof` attests that the referenced policy is consistent with the parent capability's policy lineage (subgraph relation per RFC-0967 §5). A capability without a `PolicyReference` caveat is interpreted as referencing the empty / fully-permissive policy (`policy_id = 0x00..00`).

Attenuation: a child capability MAY replace the parent's `PolicyReference` with a more restrictive one (subgraph relation per RFC-0967 §5). The child cannot reference a strictly more permissive policy. The `attenuation_proof.witness_signature` MUST be valid (signed by the parent capability issuer).

**Strategic rationale (v2.0 grand-design reframe):** A capability used to embed all policy clauses in caveats. With `PolicyReference`, the capability carries only a 32-byte hash reference; the policy graph is a separate, reusable, versionable, shareable artifact (RFC-0967). One `PolicyObject` may be referenced by N capabilities. Policy updates create new versions, not new capabilities. This separation mirrors how page tables work: the page-table root register (capability) is independent of the page table entries (policy).

### 4. CaveatSet encoding

```text
CaveatSet encoding:
    version_tag:         u8                       // 0x01 (current)
    caveat_count:        u32 BE                   // number of caveats
    caveats:             [Caveat; count]          // concatenated caveat envelopes
```

Caveat ordering is preserved exactly. Two `CaveatSet`s with the same caveats in different orders have **different encodings** and thus different `capability_hash`es.

### 5. Attenuation rules summary

RFC-0957's attenuation invariant: a child macaroon can only **restrict** the parent. RFC-0965 defines per-caveat-type attenuation rules:

| Caveat | Parent → Child |
|---|---|
| `Vault` | Equal (vault_id unchanged) |
| `Permission` | Subset (child ⊆ parent) |
| `ValidAfter` | `child.not_before ≥ parent.not_before` |
| `ValidRange` | `child.valid_after ≥ parent.valid_after ∧ child.valid_until ≤ parent.valid_until` |
| `MaxUses` | `child.count ≤ parent.count` |
| `AuditWindow` | `child.duration ≥ parent.duration` |
| `RedemptionContext` | Equal |
| `WrappedOnly` | Parent chain is prefix of child's |
| `Factory` | Vet constraints are superset |
| `Sharded` | Equal |
| `MaxPerTx` | `child.amount ≤ parent.amount` |
| `RateLimit` | Both `max_per_window ≤ parent` AND `window_duration ≥ parent` (tighter cap, longer window) |
| `LinearRelease` | `child.start ≥ parent.start ∧ child.end ≤ parent.end ∧ child.cliff ≥ parent.cliff` |
| `ComplianceHold` | `child.threshold ≥ parent.threshold ∧ child.delay ≥ parent.delay` |

Verification: when a capability is attenuated (a parent issues a child), the parent signer MUST check that each caveat's attenuation rule holds. Failure to satisfy = invalid child macaroon.

### 6. Capability envelope (RFC-0957 + RFC-0965)

```text
Capability (RFC-0957 macaroon + RFC-0965 extensions) {
    version_tag:           u8                    // 0x01
    root_key_id:           [u8; 32]              // RFC-0957
    issuer_did:            DID
    holder_did:            DID
    caveats:               CaveatSet             // RFC-0965 (extending RFC-0957)
    discharges:            DischargesBag         // RFC-0957 (unchanged)
    holder_signature:      Ed25519Signature
}

capability_id = BLAKE3(0x05 || canonical_ser(capability_unsigned))
```

Discriminator `0x05` is distinct from RFC-0957's prior discriminator (if any) so old + new capabilities can co-exist.

### 7. Worked example: rate-limited AI spend capability

A user wants: "I can spend up to 50 OCTO_W per day on GPT-4, with a 24h audit window."

```text
Capability {
    root_key_id:           <random-32-bytes>,
    issuer_did:            "did:cipherocto:user_alice",
    holder_did:            "did:cipherocto:gpt4_client_app",
    caveats: [
        Vault("vault-uuid-alice-mission-gpt4"),      // binds to Alice's mission vault
        Permission(Reservation),                    // action kind
        ValidAfter(2026-07-23 00:00 UTC),          // start today
        Constraint::ValidRange(2026-07-23, 2026-12-31),  // end of year
        Constraint::MaxPerTx(50_000_000),           // 50 OCTO_W
        Constraint::RateLimit(100_000_000, 86400),  // 100 OCTO_W per day
        MaxUses(1000),                              // up to 1000 redemptions
        AuditWindow(86400),                         // 24h dispute period
        Sharded(0),                                 // pinned to shard 0
    ],
    holder_signature:      <ed25519 over canonical_ser(capability_unsigned)>,
}
```

Encoded bytes: ~250 bytes total.

Verification when the holder redeems:
1. `vault_id` matches caveat's Vault → OK.
2. `permission_kind` matches Permission(Reservation) → OK.
3. `now >= ValidAfter` → OK.
4. `now in ValidRange` → OK.
5. `amount ≤ 50_000_000` → OK.
6. `RateLimit` projection (look up past 24h spend from `capability_events` log) → OK.
7. `uses_consumed < 1000` → OK.
8. After settlement, hold for 86400s before finalizing → OK.
9. `shard_id(vault_id, num_shards) == 0` → OK.

If all 9 pass: capability is valid; reservation can be created.

### 8. Worked example: child capability (attenuation)

Alice wants to delegate to Bob a subset of her capability: "I can spend up to 5 OCTO_W per day on GPT-4, only on weekdays, with a 7-day audit window."

```text
// Parent capability: Alice's 50 OCTO_W/day on GPT-4 (from §7)

// Child capability:
Capability {
    root_key_id:           <new-random-32-bytes>,         // new key (per-audience unlinkable)
    issuer_did:            "did:cipherocto:user_alice",  // same issuer
    holder_did:            "did:cipherocto:bob_app",     // new holder
    caveats: [
        Vault("vault-uuid-alice-mission-gpt4"),           // EQUAL to parent's Vault
        Permission(Reservation),                          // subset of parent's permissions (only one)
        ValidAfter(2026-07-23 00:00 UTC),                // ≥ parent's
        Constraint::ValidRange(2026-07-23, 2026-12-31),   // subset of parent's range
        Constraint::MaxPerTx(5_000_000),                  // ≤ parent's 50_000_000
        Constraint::RateLimit(10_000_000, 86400),         // ≤ parent's 100_000_000 (10 OCTO_W per day)
        MaxUses(100),                                     // ≤ parent's 1000
        AuditWindow(604800),                              // ≥ parent's 86400 (7 days)
        Sharded(0),                                       // EQUAL to parent's
        WrappedOnly(parent_capability_id),                // BOB must present parent's capability too
    ],
    holder_signature:      <ed25519 by bob_app's key>,
}
```

Alice signs the attenuation. Bob can now use the child capability, but only when also presenting Alice's parent. This is hierarchical capability composition.

### 9. Catalog schema

```sql
CREATE TABLE capabilities (
    capability_id          BLOB PRIMARY KEY,         -- BLAKE3(0x05 || canonical_ser(cap_unsigned))
    root_key_id            BLOB NOT NULL,
    issuer_did             TEXT NOT NULL,
    holder_did             TEXT NOT NULL,
    caveats_set            BLOB NOT NULL,            -- canonical_ser CaveatSet
    parent_capability_id   BLOB NULL,                -- WrappedOnly reference
    holder_signature       BLOB NOT NULL,
    revoked                BOOLEAN NOT NULL DEFAULT 0,
    expires_at_unix        BIGINT NOT NULL,
    created_at_unix        BIGINT NOT NULL,
    capability_hash         BLOB NOT NULL,            -- BLAKE3 over canonical_ser (independent of id)
    FOREIGN KEY (parent_capability_id) REFERENCES capabilities(capability_id),
    CHECK (parent_capability_id IS NULL OR parent_capability_id <> capability_id)
);

CREATE INDEX ix_capabilities_holder ON capabilities (holder_did, revoked, expires_at_unix);
CREATE INDEX ix_capabilities_parent ON capabilities (parent_capability_id);
```

## Open Questions

| # | Question | Resolution Target |
|---|----------|-------------------|
| 1 | Can a capability carry more than one `Permission` caveat? | Yes; set union. RFC-0957 attenuation: child's set is subset. |
| 2 | What happens to a `MaxUses` projection during long sync? | `uses_consumed` is computed from the event log; partial sync means partial count. RFC-0962 covers sync semantics. |
| 3 | How is `Factory`'s `Vet` canonically encoded? | Per RFC-0126 + RFC-0964 (Constraint subset); see RFC-0960 §10.7 |
| 4 | Can a `Sharded` caveat be added in attenuation? | No — must be in the original capability; cannot move across shards. |
| 5 | What is the encoding of `PermissionKind` if more kinds are added? | Single byte (current); could grow to 2 bytes if >255 kinds. v1.1 decision. |
| 6 | How does `AuditWindow` interaction with offline dispute filing? | Dispute period counts from settlement landing; offline dispute = late filing = no effect. |

## Resolved Decisions (v1.2-Resolved)

All Open Questions resolved with concrete answers (R28+ risk-closure round):

| # | Question | Resolution | Status |
|---|----------|------------|--------|
| 1 | Multiple `Permission` caveats per capability | **Yes — set union semantics.** Child capability's effective `Permission` set = subset of parent's (RFC-0957 §Attenuation invariant: child only restricts). Multi-`Permission` union is monotone restriction only; no expansion possible. | Resolved |
| 2 | `MaxUses` during long sync | **Conservative failsafe.** `uses_consumed` = event-log count of `Redemption` events referencing this capability. During partial sync (RFC-0862), verifier computes `uses_consumed` from locally-known events. **Verifier rule: reject if `uses_consumed + 1 > max_uses` even when event log is incomplete** — partial sync under-counts consumed uses, so conservative check prevents double-spend. Catch-up sync may later observe the rejection and reconcile. | Resolved |
| 3 | `Factory.Vet` canonical encoding | **RFC-0126 + RFC-0964 (Constraint subset).** `Vet` is a constraint payload (RFC-0964 §3) wrapped in a `Factory` caveat (RFC-0965 §3.4). `predicate_hash` references a CIPHERO_SQL procedure (RFC-0961 §9). Cross-reference: RFC-0960 §10.7 (`Factory` specification). | Resolved |
| 4 | `Sharded` caveat added during attenuation | **No.** `Sharded` binds a capability to a specific shard (RFC-0963). Moving across shards mid-capability would invalidate ZK proofs that depend on per-shard MMR roots. Must be set in original capability. Attenuation chain inherits shard binding (immutable). | Resolved |
| 5 | `PermissionKind` encoding (1 vs 2 bytes) | **Defer to v1.1.** Currently 1 byte = 255 kinds (0x00 reserved, 0x01-0xFF available). v1.1 bumps to 2 bytes (LE) if any RFC adds the 255th kind. Migration path: 2-byte form is length-prefixed (`kind_hi == 0xFF` → 16-bit kind); 1-byte form remains wire-compatible. Schedule: post-v2.0 lockstep promotion. | Deferred v1.1 |
| 6 | `AuditWindow` + offline dispute filing | **Dispute counts from settlement landing at the producer node.** RFC-0862 sync delivers dispute events to all nodes; nodes that miss the dispute window (offline > `audit_window`) reject late-filed disputes with `E_DISPUTE_WINDOW_EXPIRED`. The settlement itself is NOT reversed — late disputes have no effect. Producer node publishes settlement landing timestamp to enable clients to compute remaining window. | Resolved |

### §10 AuditWindow + Sync semantics (v1.2 NEW)

AuditWindow interaction with offline nodes is precise:

```text
Settlement lands at node N0 at unix_time T0.
AuditWindow = duration D.
Window closes at T0 + D.
Offline node N1 reconnects at T1 > T0 + D.
N1 receives dispute event via RFC-0862 sync.
N1 evaluates: T1 > T0 + D → E_DISPUTE_WINDOW_EXPIRED → reject.
```

The producer publishes `(T0, D)` in the settlement receipt metadata (RFC-0959). Offline nodes can compute remaining window from receipt + current clock, even before sync delivers dispute events.

## Cross-reference (RFC-0960 chain-aware bump, 2026-08-17)

RFC-0960 amended §2.1 root-vault example (removed; replaced with §20.3 lattice note) + added §Vault Substrate subsection. Caveat encoding (§6 envelope shape, §7/§8 worked examples) is unaffected — `WrappedOnly` (the cross-asset-policy-attachment mechanism per RFC-0960-Resolved) is now the sole path for hierarchical capability composition; vault hierarchy is explicitly REMOVED. §10 AuditWindow semantics unchanged. Per-extension crate layout (§1.4 amendment) unchanged.

## Out of Scope

- **Caveat verification algorithms.** This RFC defines encoding only. Verifier lives in the constraint pipeline + macaroon verifier.
- **Discharge protocol details.** RFC-0957 §Third-Party Discharge covers the discharge bag semantics; unchanged.
- **Capability delegation chains beyond `WrappedOnly`.** Hierarchical composition via `WrappedOnly` is the only mechanism. Other patterns (e.g., role-based) are out of scope.
- **Capability marketplace.** Trading capabilities is a separate primitive (RFC-0970+ candidates).

### Per-Extension Crate Layout (v1.4 amendment, 2026-08-08)

The caveat discriminator byte pattern (1-byte `0x01-0xCF` for RFC-allocated + `0xD0-0xFF` for application-specific) established by this RFC generalizes to the **per-extension crate model** for the broader capability substrate. Per RFC-0957 §Per-Extension Crate Layout, new caveat types AND new capability types live in separate crates:

```
crates/
├── octo-cap-macaroon/              # 22 caveat types (RFC-0965) + Macaroon v1 substrate
├── octo-cap-zk/                    # ZK-verified capabilities (RFC-0958)
├── octo-cap-federation/            # cross-domain delegation caveats
├── octo-cap-time-lock/             # time-bounded capabilities
├── octo-cap-threshold-mpc/         # threshold-signed capabilities
└── octo-cap-<user-extension>/      # user-defined extensions register via plugin
```

Each crate provides:
- `Caveat` enum variants (RFC-0965 discriminator bytes)
- `CapabilitySpec` impl registering via `octo-wallet::capability::CapabilityRegistry`
- Mint + verify impls (RFC-0957 attenuation invariant preserved across crate boundary)

Adding a new caveat type = new discriminator byte allocation per this RFC's pattern. Adding a new capability family = new crate. Wallet core (`octo-wallet`) unchanged.

**Migration path from `cipherocto-capability-ext`:**
- v1.0-v1.3: `cipherocto-capability-ext` is the single-crate reference impl
- v1.4 (this amendment): extract to `crates/octo-cap-macaroon/` per RFC-0957 §Per-Extension Crate Layout
- Future: `crates/octo-cap-{zk,federation,time-lock,threshold-mpc,...}/` land as their own RFCs

Implementation missions: `missions/open/0957-ext-macaroon-crate.md`, `missions/open/0957-ext-zk-crate.md`.

**Cross-references:** RFC-0957 §Per-Extension Crate Layout; RFC-0871 §Wallet Node Lifecycle; `docs/research/2026-08-08-specialized-node-protocol-research.md` §Layer 4 stability layering.

## Status

This RFC = Capability extension format. Status: **Accepted v1.1** (promoted from Draft on 2026-07-23 in lockstep with RFC-0960, RFC-0961, RFC-0962, RFC-0963, RFC-0964, and RFC-0967).

All companion RFCs reached Accepted in lockstep on 2026-07-23.

The `cipherocto-capability-ext` crate implements:
- `Caveat` enum (22 variants incl. RFC-0957 existing + new `PolicyReference`)
- `CaveatSet` ordered list with canonical encoding
- Attenuation rule verification (per-caveat-type)
- Capability envelope codec (encode / decode / verify)
- Cross-validator interface (offline verification)
