# RFC-0960 (Economics): Grand Design — Vaults, Capabilities, Reservations

## Status

Draft

> **Note:** This RFC synthesizes the grand-design research into a single canonical architecture. It supersedes the value-layer gap analysis from Phase 1 of `docs/research/2026-07-22-value-transfer-model-internal-landscape.md` and codifies the primitives, Constraint set, audit window, event-sourced ledger, Economic VM, and Consensus Sessions from the grand-design doc.

## Version History

| Version | Date | Author | Note |
|---------|------|--------|------|
| v1.0 | 2026-07-22 | @cipherocto + @mmacedoeu | Initial draft; synthesizes Phases 1-5 research. Round 1 self-review applied: 8 fixes (see §R1 Self-Review). |
| v1.1 | 2026-07-23 | @cipherocto + @mmacedoeu | Round 2 self-review applied: 3 fixes (see §R2 Self-Review). |
| v1.2 | 2026-07-23 | @cipherocto + @mmacedoeu | Cross-RFC review applied: 7 fixes (see §R3 Cross-RFC Self-Review). |
| v1.3 | 2026-07-23 | @cipherocto + @mmacedoeu | Deeper R4 review: 6 fixes (see §R4 Self-Review). |
| v1.4 | 2026-07-23 | @cipherocto + @mmacedoeu | R5 implementation/test gap review: 7 fixes (see §R5 Self-Review). |

## R1 Self-Review (multi-round adversarial)

Self-applied R1 fixes prior to circulation. Each fix maps to a defect surfaced during in-thread review.

### R1-F1 — RFC-0959 `SettlementReceipt` already defines the on-chain receipt

**Defect:** §2.4 defined a new `Settlement { settlement_id, reservation_id, proof, transfers, timestamp }` whose `proof: Proof` was unspecified, leaving a wire incompatibility with RFC-0959 v1.0's `SettlementReceipt { envelope, router_signature }` (Accepted 2026-07-20).

**Fix:** §2.4 now aliases `Settlement` to RFC-0959's `SettlementReceipt`. No new primitive; RFC-0960 only adds the **`reservation_id`** link that RFC-0959 does not have. The audit-window extension (§4) is layered onto RFC-0959's state machine without overriding it.

### R1-F2 — RFC-0957 caveats are the substrate; RFC-0960 extends, does not replace

**Defect:** §2.2 added `permission_kind`, `constraints`, `max_uses`, `audit_window`, `redemption_context`, `factory`, `parent_capability` etc. as fields on a `Capability` struct, but RFC-0957 already defines `Capability` as a macaroon with first-party + third-party caveats + discharge bag. Adding flat fields breaks macaroon attenuation invariant.

**Fix:** §2.2 now lists **all** extended fields as **`Constraint` variants** carried via the macaroon caveat DSL (RFC-0957 caveat types). Attenuation (add-only monotonic restriction) is preserved. Companion RFC-0965 enumerates the new caveat types exactly.

### R1-F3 — Settlement state machine layering, not override

**Defect:** §4 introduced "Reserved → Executing → Settled → Auditable → Released" as a state machine for `Settlement`, but RFC-0959 v1.0 already defines `Minted → Settled → Consumed` as the receipt state machine. The two machines operate on different objects: `Reservation` (RFC-0960) and `SettlementReceipt` (RFC-0959). Conflating them invited a contradiction.

**Fix:** §4 explicitly separates the two machines. `Reservation` has the 8-state machine. `SettlementReceipt` keeps RFC-0959's 3-state machine. The audit-window logic lives on `Reservation`, and the transition `Reservation: Auditable → Released` is what consumes the corresponding `SettlementReceipt` (via `settlement_ref`).

### R1-F4 — `Proof` shape references RFC-0959

**Defect:** `proof: Proof` was unspecified.

**Fix:** §2.4 now states `proof = SettlementReceipt` (RFC-0959 v1.0 type). No new `Proof` primitive.

### R1-F5 — `Transfer` projection semantics, not primitive

**Defect:** §2.5 defines `Transfer` as a struct with explicit fields, which reads as a primitive even though the prose says "consequence". Risk of downstream code treating it as the canonical schema.

**Fix:** §2.5 now names a single canonical SQL projection `transfer_events` and explicitly forbids a `transfers` table. The struct is the projection row shape, not a first-class object; the only first-class object is the event row.

### R1-F6 — `PermissionKind` enum is placeholder; concrete set required

**Defect:** `PermissionKind` enum was a sketch.

**Fix:** §2.2 lists the canonical five kinds plus the rule that `PermissionKind` is a *companion-RFC enumeration* (RFC-0965). Adding new kinds is a backwards-compatible variant add.

### R1-F7 — `factory` / `factoryData` analog requires a vet, not a byte string

**Defect:** `factory: Option<(VaultID, Bytes)>` left the `Bytes` semantics ambiguous.

**Fix:** §2.2 (and §10.7) now state that `factory` carries a vet (pre-validated invocation: target + selector + arg template), not raw bytes. The vet is canonicalised by RFC-0126 and the verifier runs the same constraint pipeline against the deployed target.

### R1-F8 — Event schema's `corrections` array needs ordering rule

**Defect:** `corrections: Vec<EventId>` allowed arbitrary order; different nodes would serialize corrections differently, breaking replay determinism.

**Fix:** §5 specifies ascending `event_id` ordering, enforced by the canonical_ser implementation. Tested in companion RFC-0964 test vectors.

### R1-F9 — Vet factory needs concrete definition

**Defect:** §10.7 was only about identity translation; it didn't define the `Vet` struct that R1-F7 referenced. Cross-reference dangling.

**Fix:** §10.7 now includes a `Vet` struct definition (target + action_template + required_caller + pre_conditions + expiry_for_deploy), explicit rejection of raw bytes (phishing vector), and the canonical use cases (hierarchical vault creation, cross-DAO delegation).

## R2 Self-Review (multi-round adversarial)

R2 cross-check after RFC-0961 and RFC-0962 landed (2026-07-22). Each finding tied to a specific defect surfaced during the R2 review pass.

### R2-F1 — Companion RFC status staleness

**Defect:** §Dependencies and §Dependency Validation listed RFC-0961 and RFC-0962 as `Planned`. As of 2026-07-22 both have Draft RFCs landed (`rfcs/draft/economics/0961-ciphero-sql-language-spec.md` and `rfcs/draft/economics/0962-consensus-session-protocol.md`). Stale status misleads reviewers about which hard-blocks apply.

**Fix:** Both RFCs now flagged `Draft (2026-07-22)` in the companions list and in the dependency table. Hard-block status promoted from `Best-effort / No` to `Yes (IA-1) / YES` and `Yes (IA-2) / YES` respectively. IA-1: RFC-0961 CIPHERO_SQL parser + registry. IA-2: RFC-0962 ConsensusSession signature aggregation + ZK circuit.

### R2-F2 — Step 6 prose staleness

**Defect:** §2.3 said "Step 6 of the 11-step exercise (currently `blake3::hash(b"escrow/v1")`) becomes a real `Reservation` row." The placeholder is now gone: `crates/quota-router-core/tests/eleven_step.rs::step6_escrow_preauth` now constructs a real `quota_router_sm_engine::Reservation` via `Reservation::mint(...)` (landed 2026-07-23).

**Fix:** §2.3 prose updated to reflect the closeout. The reference to the prior hash-of-string pattern is replaced with: "Step 6 now constructs a real `Reservation` row via `quota_router_sm_engine::Reservation::mint()` (landed 2026-07-23, R1-F1 closeout). The prior `blake3::hash(b"escrow/v1")` placeholder is removed."

### R2-F3 — Open Questions status staleness

**Defect:** §Open Questions listed all five companion RFCs as `planned`. RFC-0961 and RFC-0962 are now `Draft (2026-07-22)`.

**Fix:** §Open Questions updated to flag both RFCs as `Draft 2026-07-22` (bold). Reviewers reading the open questions should know which gaps are still TODO vs which have a landed Draft to review.

## R3 Cross-RFC Self-Review (multi-round adversarial)

R3 cross-checks the 5-RFC stack (0960 + 0961 + 0962 + 0963 + 0964 + 0965) for internal consistency. 7 fixes applied.

### R3-F1 — RFC-0960 §5 is conceptual; §3 missing normative pointer

**Defect:** §3 ("Constraint Set") shows a 14-variant conceptual list but never tells the reader to consult RFC-0964 for canonical encoding. Reviewer must discover RFC-0964 to know the discriminator bytes, length-prefix format, and wire shape.

**Fix:** §3 now opens with a "Normative reference" note pointing to RFC-0964 §0 (wire-format namespace tag) and §1 (variant enumeration), and to RFC-0965 for the caveat-to-constraint mapping.

### R3-F2 — `audit_window` type name drift between 0960 §531 and 0965 §3.5

**Defect:** RFC-0960 §531 said "the window length comes from `Reservation.audit_window`" (Duration-typed). RFC-0965 §3.5 says "AuditWindow payload: `duration_secs: u64 BE`". Same conceptual field, two type names (Duration vs u64 seconds).

**Fix:** RFC-0960 §531 now explicitly identifies `Reservation.audit_window` as `u64` seconds, identical to the `AuditWindow(duration_secs)` caveat payload in RFC-0965 §3.5. One canonical name (seconds as `u64`), one canonical semantic.

### R3-F3 — RFC-0962 `SessionReceipt` collides with RFC-0959 `SettlementReceipt`

**Defect:** RFC-0962 §32 named a new object `SessionReceipt`. The name collides with RFC-0959's `SettlementReceipt` (Accepted 2026-07-20). Cross-RFC ambiguity: a reader sees "Receipt" and assumes RFC-0959.

**Fix:** RFC-0962 §32 renames the object to `SessionCommitment`. Prose explicitly states it is **not** a `SettlementReceipt`; the two objects are disjoint. The envelope *shape* (canonical_ser + BLAKE3 + Ed25519) is borrowed; the binding (session_id vs ask_id) is distinct.

### R3-F4 — Two replay-defense indexes with overlapping names

**Defect:** RFC-0959 has `ConsumedReceiptIndex` (tracks `ReceiptId` per asker). RFC-0962 introduced `ConsumedSessionIndex` (tracks `session_id` per signer). Names suggest parallel but related functions; the cross-RFC relationship was undocumented.

**Fix:** RFC-0962 §145 (Role/Authority table) now states: "`ConsumedSessionIndex` is disjoint from RFC-0959's `ConsumedReceiptIndex`. Two indexes, two different replay surfaces." Reader knows the two are parallel and not redundant.

### R3-F5 — RFC-0962 session signature vs RFC-0965 capability signature ambiguity

**Defect:** RFC-0962 §8 says "Capability holder signature. Ed25519 over `canonical_ser(session_unsigned)`. Mandatory." RFC-0965 §6 has `holder_signature: Ed25519Signature` on the Capability envelope. Same `Ed25519Signature` type; reader cannot tell whether one signature suffices for both.

**Fix:** RFC-0962 §8 now explicitly distinguishes the two signatures: capability signature proves holder ownership; session signature proves authorization for this specific set of SQL operations. A capability signature alone is **not** sufficient to authorize a session; the session signature is always required.

### R3-F6 — Discriminator namespace collision between RFC-0964 (Constraint) and RFC-0965 (Caveat)

**Defect:** RFC-0964 §1 uses discriminator bytes `0x01-0x19` for Constraint variants. RFC-0965 §1.1 uses `0x01-0x0C` for RFC-0957 caveats and `0x10-0x18` for new caveats. A byte 0x05 means `MaxPerTx` (Constraint) or `After` (Caveat, deprecated). Receivers cannot disambiguate without context.

**Fix:** Both RFCs now open with a §0 "Wire-format envelope tag" introducing a **namespace prefix byte** that precedes every envelope. `0x01` = Constraint (RFC-0964); `0x02` = Caveat (RFC-0965); `0x03` = PermissionKind; `0x04` = ReservationState; `0x05` = Capability; `0x06` = ConsensusSession. Receivers dispatch on the namespace tag first; unknown tags fail-closed. Discriminator bytes within each envelope are local to their namespace.

### R3-F7 — RFC-0963 `shard_registry` schema missing `current_num_shards`

**Defect:** RFC-0963 §7 defined `shard_registry` with `num_shards_at_creation` but no `current_num_shards` column. After re-sharding, an old shard's record would show the network size at its birth, not the current size. Re-shard decisions require the current value.

**Fix:** Schema now has both `num_shards_at_creation` (historical) and `current_num_shards` (updated on every re-shard). Querying the current network size is single-row.

## R4 Self-Review (deeper cross-RFC sweep)

R4 deeper pass. Looked at catalog schemas, namespace-tag coverage, edge cases, state machine transitions, R3-F6 follow-on. 6 fixes applied.

### R4-F1 — Orphan `PermissionKind` namespace tag (R3-F6 follow-on)

**Defect:** R3-F6 added tag `0x03 = PermissionKind` to the namespace table, but `PermissionKind` is **never** a standalone envelope — it only appears as a `u8` value inside a `Permission` Caveat payload (RFC-0965 §3.2). The 0x03 tag is reserved for a thing that does not exist on the wire. Receivers dispatching on 0x03 would have no parser to invoke.

**Fix:** RFC-0964 §0 and RFC-0965 §0 now state: "PermissionKind and ReservationState are NOT standalone envelopes — they appear only as field values inside Caveat and Reservation envelopes respectively. They have no namespace tag of their own." Tag table reduced to 6 active tags (0x01-0x06) + reserved range.

### R4-F2 — `ConsensusSession.version_tag` collides with R3-F6 namespace tag

**Defect:** R3-F6 said `0x06 = ConsensusSession`. But RFC-0962 §4 has `version_tag: u8 (currently 1)` as the first field of the `ConsensusSession` struct. A receiver reading byte 0x01 would either see it as a Constraint envelope (R3-F6) or a version tag (RFC-0962). The two specs use the first byte of the same wire message for two different purposes.

**Fix:** R3-F6 namespace table corrected: the outer tag (1 byte) is the namespace tag, **precedes** the inner envelope. The inner envelope's `version_tag` field (if any) is inside the inner envelope and is independent. RFC-0962 §4 is unchanged structurally; the wrapper is just acknowledged. Tag 0x04 (not 0x06) = ConsensusSession after the table reshuffle to 6 active tags.

### R4-F3 — `nonce` type unspecified in RFC-0962

**Defect:** RFC-0962 §4 had `nonce: Nonce` (no type). Replay defense rule §6.2 step 6 keys on `(signer, nonce)` but the type was undefined. Implementers would have to guess (16 bytes? 32 bytes? arbitrary?).

**Fix:** Field type now `[u8; 32]` with comment explaining why: BLAKE3-derived for unique-per-session collision resistance. Differs from RFC-0959's `[u8; 16]` (which is 128-bit) — sessions need 256-bit because they're higher-frequency.

### R4-F6 — `shard_migration_log` state names covered only live-migration path

**Defect:** RFC-0963 §7 catalog `shard_migration_log.state` listed `Pending | DualWriting | Reading | Finalized | Aborted` (5 states). §4.1 (drain + refill) describes only 3 states. The catalog table is referenced from both §4.1 and §4.2, but the state list matches only §4.2.

**Fix:** Schema now has `strategy TEXT NOT NULL` (DrainRefill | LiveMigration) and the state list includes `Draining` for the drain-and-refill path: `Pending | DualWriting | Reading | Draining | Finalized | Aborted` (6 states). `strategy` field disambiguates which state set is active.

### R4-F7 — R3-F6 namespace tag ambiguous: outer-prefix or inner-first-byte?

**Defect:** R3-F6 introduced "outer prefix byte" but didn't specify whether it's a separate byte before the inner envelope, or the inner envelope's first byte. Receiver design was ambiguous.

**Fix:** RFC-0964 §0 and RFC-0965 §0 now explicitly model the wire as a two-layer envelope: 1-byte outer namespace tag, then namespace-specific inner envelope bytes. The inner envelope's own `version_tag` (if any) is inside the inner envelope and is independent of the outer tag.

### R4-F11 — Drain + refill has no upper bound

**Defect:** RFC-0963 §4.1 says "every node must process every event on N before N is retired." For a shard with billions of events, this could take days. No upper bound or progress indicator. A stalled drain blocks the `num_shards` change indefinitely.

**Fix:** §4.1 now specifies a `7-day` default drain timeout. Drains exceeding this abort and retry as live migration. Nodes publish throughput estimates every 1000 events to enable progress tracking.

## R5 Self-Review (implementation + test gap)

R5 pass: implementation/spec gap and missing test vectors. 7 fixes applied.

### R5-F1 — Stale Step 6 references in gap-matrix + test strategy

**Defect:** RFC-0960 §2 (gap-matrix), §18 (backwards compat) and §22 (test strategy) all still listed Step 6 as the `blake3::hash(b"escrow/v1")` placeholder. After R1-F1 + R2-F2 closeouts, the placeholder is gone but the references remained stale.

**Fix:** §2 gap-matrix updated: "Escrow hold/release = `quota_router_sm_engine::Reservation::mint()` (RFC-0960 §2.3; landed 2026-07-23) — Closed (R1-F1 closeout)". §18 + §22 references updated identically.

### R5-F2 — `ValidRange` ordering not specified (parser/evaluator divergence)

**Defect:** RFC-0964 §3.1 (ValidRange payload) defines `valid_after_unix` + `valid_until_unix` but never says what to do if `valid_after > valid_until`. A parser could accept the encoding; an evaluator could go either way (always-reject vs wrap-around).

**Fix:** §3.1 now states: "If `valid_after_unix > valid_until_unix`, the constraint is unsatisfiable (always-reject); parsers MUST accept the encoding but evaluators reject any operation under such a range." Cross-implementations have a single canonical behavior.

### R5-F3 — `block_height` replay semantics unspecified

**Defect:** RFC-0962 §4 envelope has `block_height: u64` and §6.2 step 4 verifies `wal_segment_hash`. A node replaying a session whose `block_height` is higher than its local chain head has no defined behavior — reject as "future" or queue for later?

**Fix:** §6.2 step 4 now explicitly says: "the node uses the envelope's `block_height` verbatim — it does **not** re-derive from local chain state. If the local chain has not yet reached that block height, the session is queued in a per-node `pending_sessions` table and replayed once sync catches up. A session whose `block_height` is **higher than the node's current head** is never rejected for 'future' content; it is just deferred."

### R5-F4 — `PerAssetSpendingCap` max-assets not bounded

**Defect:** RFC-0964 §3.2 has `caps: Vec<(asset_id, amount_micro)>` with no maximum. A 1000-asset cap would be 9 + 48*1000 = 48,009 bytes — far above the G5 design goal "max ≤ 256 bytes".

**Fix:** §3.2 now enforces `N ≤ 5` at parse time: "1 asset = 57 bytes, 5 assets = 249 bytes, 6 assets = 297 (rejected)". Cross-implementations reject any encoding > 256 bytes.

### R5-F5 — `audit_window` field name has three aliases

**Defect:** After R3-F2 we normalized the *type* (`u64` seconds) but three names remain in the doc stack: `audit_window` (RFC-0960 §2.2 caveat semantic view + §2.3 Reservation struct prose), `audit_window_secs` (live code), `duration_secs` (RFC-0965 §3.5 caveat payload). All three refer to the same `u64` seconds field but readers might think they're distinct.

**Fix:** §2.2 caveat view now says: "`audit_window: Option<u64>` (Caveat::AuditWindow(d_secs); same field, same unit as Reservation.audit_window_secs (live code))". §2.3 prose cross-references all three names. Reader sees they're identical.

### R5-F7 — `consensus_sessions` catalog has no `zk_proof` column

**Defect:** RFC-0962 §13 has `has_zk_proof BOOLEAN` but no actual `zk_proof` column or `proof_system` / `verifier_key_id` fields. The ZK proof (RFC-0958 SessionProof) lives only in the envelope; the catalog can't index or query it.

**Fix:** Schema now has `zk_proof BLOB NULL`, `proof_system TEXT NULL` (R1CS|PLONK|STWO|Groth16), `verifier_key_id BLOB NULL`. Plus a partial index `ix_sessions_proof ON proof_system WHERE zk_proof IS NOT NULL` for fast "find all STWO-proven sessions" queries.

### R5-F8 — RFC-0961 forbidden timestamp list missing PostgreSQL aliases

**Defect:** §4.1 forbidden list includes `NOW`, `CURRENT_TIMESTAMP`, `LOCALTIMESTAMP` but not `STATEMENT_TIMESTAMP()`, `TRANSACTION_TIMESTAMP()`, `clock_timestamp()`. Reader might assume only the listed names are forbidden.

**Fix:** §4.1 now explicitly lists all four PostgreSQL time-function aliases as forbidden. Parsers reject any function whose name matches the forbidden set (case-insensitive).

## Authors

- Author: @cipherocto (S04 + S05 grand-design work)
- Contributor: @mmacedoeu (capability-bound vault direction; Phase 1-5 research synthesis)

## Maintainers

- Maintainer: @mmacedoeu
- Maintainer: @cipherocto

## Summary

This RFC establishes the **canonical value-layer architecture** for CipherOcto's economic operating system. It introduces four primitives (**Vault**, **Capability**, **Reservation**, **Settlement**), twenty-three reusable **Constraints**, an **audit-window** state machine extension, an **event-sourced ledger** (balances are projections, not state), a declarative **Economic VM**, and a **Consensus Session** compatibility layer that preserves the enterprise programming model while replacing the trust model with cryptographic capability authorization.

Together these primitives constitute CipherOcto's value-layer primitive set. Concrete protocol-level artifacts (`Constraint` encoding, `Capability` macaroon format extensions, event log schema, CONSENSUS_SAFE SQL mode, ConsensusSession object) are specified by companion RFCs (see §Dependencies).

## Motivation

### The gap

Per `docs/research/2026-07-22-value-transfer-model-internal-landscape.md` Phase 1, CipherOcto's existing economic primitives are insufficient:

| Need | Today | Gap |
|---|---|---|
| Per-DID OCTO-W account | `octo_w_balances(key_id, ...)` keyed by API key | **No DID-keyed accounts** |
| Value transfer between DIDs | none | **No `transfers` table** |
| Escrow hold/release | `quota_router_sm_engine::Reservation::mint()` (RFC-0960 §2.3; landed 2026-07-23) | **Closed (R1-F1 closeout)** |
| Multi-token (role tokens) | none | **OCTO-A/B/D/etc. not in any schema** |
| Capability-gated spend | RFC-0957 channel layer exists, no oracle | **No per-capability spending policy** |
| Negative-balance defense | `Balance::deduct` uses `saturating_sub` (balance.rs:27) | **Silent over-spend bug** |
| Audit trail | none | **No per-transfer event log** |

Per RFC-0959 v1.0, the settlement receipt primitive ships; the value flow that settlement triggers does not. RFC-0957 §discharge assumes "Channel provider evaluates its own predicate (escrow balance, ...)" but the escrow table is missing — circular dependency.

### The inversion

Asset-centric chains start from money: `Alice → transfer → Bob`. CipherOcto's primary purpose is not currency — it is **delegating expensive computation**. The first-class object should not be a balance. The first-class object should be **authorization to consume scarce resources**.

| Asset-centric | Constraint-centric (CipherOcto) |
|---|---|
| Asset | Resource |
| Account | Vault |
| Transfer | Policy |
| Balance | Capability |
| Tx | Reservation |
| Block | Settlement |
| State | Ledger |

### The breakthrough hypothesis

Per Phase 1 §5:

> The breakthrough is not picking a ledger model — it's recognizing that for cipherocto's use case, the spend authority IS the capability token, and the underlying ledger is just a constraint oracle.

This RFC codifies that hypothesis. Capability IS the spend authority. Ledger IS the constraint oracle. Transfers are a consequence of settlement, never a primitive.

## Dependencies

**Requires:**

- RFC-0957 (Economics): Capability Token Format — Accepted; macaroon substrate extended here
- RFC-0958 (Economics): ZK Capability Subclass — Accepted; ZK proof binding for capabilities
- RFC-0959 (Economics): Ask Settlement Chain — Accepted; settlement receipt primitive
- RFC-0126 (Numeric): Deterministic Serialization — Accepted; canonical_ser for all primitive fields
- RFC-0102 (Numeric): Wallet Cryptography — Accepted; `Transfer { sender, receiver, token, amount }` sketch

**Companion RFCs:**

- RFC-0961 (Economics): Deterministic SQL Classification — `CIPHERO_SQL` language spec — **Draft (2026-07-22)**
- RFC-0962 (Economics): ConsensusSession Object Protocol — wire protocol + ZK circuit for batch signature — **Draft (2026-07-22)**
- RFC-0963 (Economics): Resource Shard Routing — shard routing by `vault_id` — Planned

**Requires (companion RFCs draft, not yet numbered):**

- RFC-0964 (Economics): Constraint Encoding Standard — canonical encoding for all 23 Constraint variants — Planned
- RFC-0965 (Economics): Capability Extension Format — additions to RFC-0957 macaroon format — Planned

**Not Requires (parallel primitives):**

- RFC-0955 (Economics): Model Liquidity Layer — coexistence; MLL is a marketplace of model ownership; this RFC is the value layer
- RFC-0909 (Economics): Deterministic Quota Accounting — coexistence per RFC-0959 v1.0

## Dependency Validation

| Dependency | Type | Current Status | Assumed Before Accept? | Hard-block? |
|------------|------|----------------|------------------------|-------------|
| RFC-0957 | Requires | Accepted | Already | No |
| RFC-0958 | Requires | Accepted | Already | No |
| RFC-0959 | Requires | Accepted (v1.0 2026-07-20) | Already | No |
| RFC-0126 | Requires | Accepted | Already | No |
| RFC-0102 | Requires | Accepted | Already | No |
| RFC-0961 | Companion | **Draft (2026-07-22)** | Yes (IA-1) | YES |
| RFC-0962 | Companion | **Draft (2026-07-22)** | Yes (IA-2) | YES |
| RFC-0963 | Companion | Planned | Best-effort | No |
| RFC-0964 | Companion | Planned | Best-effort | No |
| RFC-0965 | Companion | Planned | Best-effort | No |

**DAG check:** `0960 ← {0957, 0958, 0959, 0126, 0102, 0961*, 0962*, 0963*, 0964*, 0965*}` — acyclic.

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| **G1** | Make capability the spend authority | Every value-moving operation signed by a `Capability` |
| **G2** | Make transfer a consequence, not a primitive | No `Transfer` type in canonical schema; only `EventLog` entries |
| **G3** | Audit-window dispute in state machine | Settled → Auditable → Released transition with `Frozen` branch |
| **G4** | Event-sourced ledger | All state is `SUM(events)` projection; no mutable balance rows as source of truth |
| **G5** | Cross-primitive reuse via Constraint | Single 23-variant `Constraint` set covers all features from time locks to cross-chain atomic swaps |
| **G6** | Enterprise compatibility | ORMs, JDBC, stored procedures work unchanged via `ConsensusSession` |

## Specification

### §1 Seven Layers

| Layer | Purpose | CipherOcto primitive |
|---|---|---|
| 1. Resources | Scarce physical or logical capacity | `ResourceSpec` |
| 2. Assets | Accounting representation | `OCTO`, `OCTO-A`, `OCTO-B`, `OCTO-S`, `OCTO-W` (whitepaper §3) |
| 3. Vaults | Programmable containers, scoped by Owner DID | `Vault` (§2.1) |
| 4. Capabilities | Delegated spend rights | `Capability` (§2.2) |
| 5. Reservations | Temporary commitments (escrow/pre-auth) | `Reservation` (§2.3) |
| 6. Settlements | Cryptographically verified completion | `Settlement` (§2.4) |
| 7. Ledger | Append-only event log | `transfer_events` (§2.5) |

### §2 Primitives

#### §2.1 Vault

```text
Vault {
    vault_id:        VaultID,
    owner_did:       DID,
    token:           AssetID,
    policy:          VaultPolicy,
    current_state:   VaultState ∈ {Active | Frozen | Retired},
    parent_vault:    Option<VaultID>,
    created_at:      Timestamp,
    metadata:        Metadata,
}
```

Semantic vault types (each is a `Vault` with a typed `policy`):

| Type | Purpose |
|---|---|
| Provider Vault | Holds provider earnings (e.g., OpenAI shadow) |
| Marketplace Vault | Holds marketplace fees |
| Escrow Vault | Holds in-flight reservations (§2.3) |
| Treasury Vault | Holds governance funds |
| Mission Vault | Per-mission scoped vault |
| Node Vault | Per-node operational vault |
| DAO Vault | DAO-controlled funds |
| Liquidity Vault | AMM-style liquidity |
| Compliance Vault | Compliance-gated funds |
| Regional Vault | Geographic partitioning |

Hierarchical vaults form a capability-security lattice (grand design §11):

```text
Global Treasury
  └─ Regional Treasury (Americas)
       └─ Marketplace Vault (US-East)
            └─ Provider Vault (OpenAI shadow)
                 └─ Mission Vault (gpt-4-eu-prod)
                      └─ Task Vault (batch-2026-07-22)
                           └─ Capability (Claude, 50 OCTO-W, daily)
                                └─ Reservation (req-12345)
                                     └─ Settlement (proof-abc)
```

Each child vault inherits parent policy; child cannot violate parent constraints.

#### §2.2 Capability

RFC-0957 already defines `Capability` as a **macaroon v1** with first-party + third-party caveats + discharge bag. RFC-0960 **does not redefine** the macaroon — it **adds new caveat types** that capture the extended fields below. Attenuation invariant (add-only, monotonic restriction) is preserved by RFC-0957.

```text
// Conceptual capability — RFC-0960-extended via caveat DSL. See RFC-0965 for the
// concrete encoding of each new caveat type.
Capability (RFC-0957 macaroon) {
    // RFC-0957-defined fields (unchanged):
    root_key_id:        KeyID,            // per-audience unlinkable
    issuer_did:         DID,
    holder_did:         DID,              // holder signature bound via RFC-0009 Ed25519
    caveats:            Vec<Caveat>,      // first-party + third-party
    discharges:         DischargesBag,    // third-party discharge macaroons
    holder_signature:   Ed25519Signature, // RFC-0009 substrate

    // RFC-0960 extension — all of the following are *caveat variants*, not new fields.
    // Listed here in human-readable form; canonical encoding per RFC-0965.
    caveats_semantic_view:
        vault_id:             VaultID,            // Caveat::Vault(vault_id)
        permission_kind:      PermissionKind,     // Caveat::Permission(kind)
        valid_after:          Timestamp,          // Caveat::ValidAfter(ts)
        expires_at:           Timestamp,          // Caveat::ExpiresAt(ts)  (or ValidRange)
        max_uses:             u32,                // Caveat::MaxUses(n) (0 = unlimited)
        audit_window:         Option<u64>,        // Caveat::AuditWindow(d_secs); same field, same unit as Reservation.audit_window_secs (live code)
        redemption_context:   Bytes,              // Caveat::RedemptionContext(ctx)
        parent_capability:    Option<CapabilityID>,// Caveat::WrappedOnly(parent)
        factory:              Option<Vet>,        // Caveat::Factory(vet) — see §10.7
}
```

`uses_consumed` is a **projection**: read from `capability_events` log; not stored as mutable state.

Example capability in human-readable form (encoded as caveats per RFC-0957 caveat DSL):

```text
caveats:
  Vault("vault-uuid-1")
  Permission(NativeTokenTransfer)
  ValidRange(2026-07-22 00:00 UTC, 2027-01-01 00:00 UTC)
  MaxPerTx(MicroOCTO_W(50_000_000))
  AllowedDestinations({"marketplace-octo", "did:octo:claude-sonnet"})
  AuditWindow(24h)
  MaxUses(100)
  Nonce(<random-128-bit>)
```

`PermissionKind` separates the type of action from the constraints. Reuse the same `RateLimit` constraint across many kinds:

```text
PermissionKind ∈ {
    NativeTokenTransfer,   // transfer of the vault's native asset
    ERC20TokenTransfer,    // transfer of a specific non-native asset
    ContractCall,          // call an external handler with a vet
    Reservation,           // create / transition a Reservation
    VaultMutation,         // mutate Vault metadata (NOT balance; balance is event-projected)
}

// PermissionKind is a *companion-RFC enumeration* (RFC-0965). Adding a new kind
// is a backwards-compatible variant add; old nodes ignore unknown kinds only if
// the verifier fails closed (default policy). New kinds MUST ship with cross-impl
// test vectors (per RFC-0957 TV discipline).
```

#### §2.3 Reservation

```text
Reservation {
    reservation_id:  ReservationID,
    vault_id:        VaultID,
    capability_id:   CapabilityID,
    resources:       ResourceSpec,
    amount:          MicroOCTO_W,
    expires_at:      Timestamp,
    state:           ReservationState,
    settlement_ref:  Option<SettlementID>,
}

ReservationState ∈ {
    Reserved,     // pre-auth holds the amount
    Executing,    // provider is working
    Settled,      // proof attached, transfers drafted
    Auditable,    // inside dispute window
    Released,     // amount moved; terminal
    Expired,      // no settlement arrived before deadline
    Cancelled,    // explicit cancel by holder
    Frozen,       // dispute in progress
}
```

Reservations are first-class blockchain objects. Step 6 of the 11-step exercise now constructs a real `Reservation` row via `quota_router_sm_engine::Reservation::mint()` (landed 2026-07-23, R1-F1 closeout). The prior `blake3::hash(b"escrow/v1")` placeholder is removed.

#### §2.4 Settlement — alias to RFC-0959 `SettlementReceipt`

RFC-0959 v1.0 (Accepted 2026-07-20) defines `SettlementReceipt { envelope, router_signature }` with `envelope.receipt_id = BLAKE3(canonical_ser(event, nonce, settled_at_unix))` and `event.cost` bound into `settlement_hash = BLAKE3(canonical_ser(cap_root_hash, ask_id, invocation_hash, canonical_axes_consumed, cost))`. RFC-0960 does **not** redefine this primitive.

```text
// RFC-0959 v1.0 (Accepted, authoritative):
SettlementReceipt {
    envelope: {
        receipt_id:        ReceiptId,       // = BLAKE3(canonical_ser(event, nonce, settled_at_unix))
        event:             SettlementEvent, // includes cost bound into settlement_hash
        nonce:             [u8; 16],
        settled_at_unix:   u64,
    },
    router_signature:           Ed25519Signature,
}

// RFC-0960 extension — added link to Reservation. One Reservation has at most one
// SettlementReceipt (enforced by a partial-unique index in the event log).
struct Reservation { ...; settlement_ref: Option<ReceiptId>; ... }

// When Reservation transitions Auditable → Released:
//   1. Look up SettlementReceipt by ReceiptId
//   2. Apply its settlement_hash to transfer_events
//   3. Mark ReceiptId consumed (RFC-0959 ConsumedReceiptIndex)
// When Frozen → Dispute → Rollback:
//   1. Do NOT mark ReceiptId consumed
//   2. Write transfer_correction events pointing at the original transfer_events
```

Settlement consumes reservations by attaching `settlement_ref` to a `Reservation` and triggering the RFC-0959 consumed-receipt-index path.

#### §2.5 Transfer (consequence, not primitive)

The canonical schema has **no `transfers` table**. The canonical schema has one append-only log:

```sql
CREATE TABLE transfer_events (
    event_id         BIGINT       NOT NULL,
    event_type       TEXT         NOT NULL,    -- 'TransferApplied' | 'TransferCorrected' | 'Mint' | 'Burn'
    tx_id            BYTES        NOT NULL,    -- 32-byte; groups atomic event sets
    schema_version   INT          NOT NULL,
    visibility       TEXT         NOT NULL,    -- 'Public' | 'Confidential' | 'Private'
    timestamp_unix   BIGINT       NOT NULL,
    attributes       BYTES        NOT NULL,    -- canonical_ser(Tag, key, value) triples
    corrections      BYTES        NULL,        -- canonical_ser(Vec<event_id>) ascending
    signature        BYTES        NOT NULL,
    zk_proof         BYTES        NULL,        -- present iff visibility='Private'

    -- Projected columns (denormalised for index speed; recomputable from log):
    from_vault_id    BYTES        NULL,        -- NULL = mint
    to_vault_id      BYTES        NULL,        -- NULL = burn
    amount_micro     BIGINT       NULL,
    asset_id         BYTES        NULL,
    settlement_ref   BYTES        NULL,        -- ReceiptId if triggered by settlement
    PRIMARY KEY (event_id)
);

-- Invariant: at most one TransferApplied event per settlement_ref (enforced
-- by partial-unique index on (settlement_ref) WHERE event_type='TransferApplied').
CREATE UNIQUE INDEX uq_transfer_per_settlement
    ON transfer_events (settlement_ref)
    WHERE event_type = 'TransferApplied' AND settlement_ref IS NOT NULL;
```

The "struct" view that downstream code reads:

```text
TransferRow {           // PROJECTION — never writeable directly; only readable
    transfer_id:    EventId,
    settlement_id:  Option<ReceiptId>, // = event.attributes['settlement_ref']
    from_vault:     Option<VaultID>,
    to_vault:       Option<VaultID>,
    amount:         MicroOCTO_W,
    kind:           TransferKind,      // Mint | Burn | TransferApplied | TransferCorrected
    timestamp:      Timestamp,
}
```

Balance = `SUM(in) - SUM(out) - SUM(active escrow holds)` over `transfer_events`. Materialised as a cached projection (the existing `octo_w_balances` table). The cache is a cache — the source is the log. Direct UPDATE on the cache from non-event-log code paths is forbidden (Phase 4 §4.4 trigger).

### §3 Constraint Set

> **Normative reference:** This section is **conceptual** only. The canonical
> binary encoding, discriminator-byte mapping, length-prefix format, wire-format
> namespace tag, and cross-chain interop shape are defined in **RFC-0964**
> (Constraint Encoding Standard). Any code implementing constraints MUST
> conform to RFC-0964 §1 (variant enumeration) and §0 (wire-format envelope tag).
> The `caveats_semantic_view` on `Capability` (RFC-0960 §2.2) carries
> constraints via the macaroon caveat DSL; the constraint-encoding-to-caveat
> mapping is defined in **RFC-0965** (Capability Extension Format) §3.

23 constraints. Each is a reusable policy module. Combinations express all classical token features (time locks, vesting, liquidity locks, governance locks, rate limits, multi-sig, etc.).

```text
Constraint ∈ {
    // Time
    ValidRange { valid_after: Timestamp, valid_until: Timestamp },
    NotBefore(timestamp),
    UnlockAfter(block_height),
    Period { max_per_period, period_duration_secs },

    // Spend caps
    MaxPerTx { amount },
    PerAssetSpendingCap { caps: Map<AssetID, u128> },
    RateLimit { max_per_window, window },

    // Destination / intent
    AllowedDestinations { dids: HashSet<DID> },
    DeniedDestinations { dids: HashSet<DID> },
    IntentBound { message_template: Bytes },

    // Co-signing
    MultiSig { n: u32, signers: HashSet<DID> },
    RequireReceiptSignatureBy { did: DID },

    // Caller binding
    CallerBound { holder: DID },

    // Use count
    MaxUses { count: u32 },
    SingleUse {},

    // Policy delegation
    AllowIf { predicate, step_budget: u32 },
    VerifierRequired { circuit_id: Bytes32 },

    // Composition
    WrappedOnly {},
    SponsoredBy { vault: VaultID },
    CoordinatorCanSubmit { coordinator: DID },

    // Vesting / time-lock
    LinearRelease { start, end, cliff },
    CliffVesting { until, pct, period },
    LiquidityLock { until },
    GovernanceLock { while_vote_active },

    // Compliance
    ComplianceHold { threshold, delay },
}
```

Canonical encoding in RFC-0964 (companion). Each `Constraint` is a tagged-union variant with deterministic field ordering per RFC-0126.

Reuse table:

| Need | Constraint | Production precedent |
|---|---|---|
| Time lock | `NotBefore` / `UnlockAfter` | Bitcoin CLTV, Ethereum timelock |
| Vesting | `LinearRelease` | Sablier, OpenZeppelin VestingWallet |
| Cliff | `CliffVesting` | OpenZeppelin VestingWallet |
| Liquidity lock | `LiquidityLock` | Unicrypt, Team.Finance |
| Governance lock | `GovernanceLock` | Compound Governor, OpenZeppelin Timelock |
| Multi-sig | `MultiSig` | Gnosis Safe |
| Rate limit | `RateLimit` | EIP-7702, ERC-7715 |
| AI spend cap | `RateLimit` per-window token counts | not native anywhere |

### §4 Audit Window — Reservation state machine (separate from RFC-0959 receipt state)

RFC-0959 v1.0 defines `SettlementReceipt` state as `Minted → Settled → Consumed`. RFC-0960 does **not** alter this. RFC-0960 adds a **separate** state machine on `Reservation`, the audit-window lifecycle. The two machines are coupled via `Reservation.settlement_ref`:

```text
// Reservation state machine (RFC-0960; lives on the Reservation row)
Reserved        (pre-auth holds the amount; no SettlementReceipt yet)
  ↓
Executing       (provider is working)
  ↓
Settled         (SettlementReceipt attached via reservation.settlement_ref; inside audit window)
  ↓
Auditable       (audit window open; transfers drafted but not applied)
  ↓
Released        (audit window closed; transfers applied; terminal)
  │
  └─→ Frozen     (dispute filed)
        ↓
      Dispute
        ↓
      Rollback  or  Uphold
```

State transitions are themselves events in `transfer_events` (event_type = `ReservationUpdated`). The audit window clock starts when `SettlementReceipt.settled_at_unix` is set; the window length comes from `Reservation.audit_window` (a `u64` duration in **seconds**, identical to the `AuditWindow(duration_secs)` caveat payload in RFC-0965 §3.5 — same field, same unit, one canonical name).

Coupling to RFC-0959:

```text
Reservation: Settled + audit_window: 24h
   ↓  (audit_window expires, no dispute)
Reservation: Released
   ↓
SettlementReceipt: Consumed   (RFC-0959 ConsumedReceiptIndex marks receipt_id)
   ↓
transfer_events: 1 row appended (event_type='TransferApplied')

Reservation: Settled → Frozen (dispute filed inside audit_window)
   ↓
SettlementReceipt: NOT consumed (RFC-0959 ConsumedReceiptIndex untouched)
   ↓
transfer_events: 0 TransferApplied; instead, 1 TransferCorrected row
   (attributes carry the original transfer's data + corrections=[original_event_id])
```

`audit_window: Option<u64>` (seconds) on `Reservation` (carried as `Caveat::AuditWindow(duration_secs: u64)` on the originating `Capability`) controls dispute period. The `u64` is **seconds**, identical in name + type + unit to RFC-0965 §3.5's `duration_secs` field and the live code's `audit_window_secs` field on `quota_router_sm_engine::Reservation`.

| Window | Default use case |
|---|---|
| 0 (none) | High-trust: `Settled` immediately transitions to `Released` with no `Auditable` phase |
| 24h | AI marketplace settlements (default) |
| 7d | Treasury vaults |
| 30d | Multi-jurisdictional compliance |

If fraud discovered inside window: `Settled → Frozen → Dispute → Rollback`. Rollback writes a `TransferCorrected` event (Datomic-style `:correction/for`) referencing the original event; the original event stays in the log; the projection layer recomputes balances from the corrected chain.

### §5 Event-Sourced Ledger

Reject mutable balance rows as canonical state. Append-only event log:

```text
VaultCreated
CapabilityGranted
CapabilityAttenuated
CapabilityExpired
CapabilityRevoked
ReservationCreated
ReservationUpdated        // state transitions
SettlementCompleted
TransferApplied
DisputeOpened
DisputeResolved
VaultFrozen
VaultRetired
```

Event schema:

```text
Event {
    event_id:       EventId,           // global monotonic u64
    event_type:     EventType,
    tx_id:          TxId,              // atomic grouping
    schema_version: u32,               // forward-compat
    visibility:     Visibility ∈ {Public, Confidential, Private},
    timestamp:      Timestamp,
    attributes:     Vec<(Bytes, Bytes)>,
    corrections:    Vec<EventId>,      // Datomic-style :correction/for
    signature:      Signature,
    proof:          Option<ZKProof>,   // for Private events
}
```

Balances are projections computed from events. The `octo_w_balances` table that exists today is a **cache (projection)** of the event log, not the source of truth. The Phase 1 finding ("saturating_sub on `Balance::deduct`" at balance.rs:27) is the bug you get when the cache pretends to be the source.

Advantages:

- Perfect audit (every state change is an event)
- Deterministic replay (genesis → head recomputes identical projections)
- ZK-friendly (event log is a Merkle chain; prove balance ∈ [a,b] without revealing events)
- Sync-friendly (RFC-0862 + OctoSync ship event batches; no UPDATE conflicts)
- Rollback-friendly (revert last N events during dispute resolution)

### §6 Economic VM

A **declarative, deterministic, loop-free** policy language. Not Turing-complete. Not a smart-contract platform.

```text
ALLOW
  spend up_to 50 OCTO-W
  IF
    time > cliff
    AND reputation > 900
    AND remaining_budget > cost
    AND gpu_available
    AND price <= oracle_price * 1.05
    AND counterparty in allowlist
```

Properties:

- No loops, no recursion, no arbitrary storage
- Deterministic (provable by construction; ZK-friendly)
- Compiles to RFC-0126 canonical_ser
- Verifier is a small state machine — cheap to run in router, gateway, or ZK circuit
- Bounded evaluation cost — `step_budget: u32` in `AllowIf` constraint (per Phase 3 §3.5)
- No DoS via expensive policies

The EVM name clash is intentional: Economic VM, not Ethereum VM.

### §7 Atomic Swaps and Cross-Chain

Don't think bridges. Think multi-settlement.

```text
MultiSettlement {
    id,
    participants: [
        { chain: "Ethereum",  reservation: R_eth,  proof: HTLC_preimage },
        { chain: "Bitcoin",   reservation: R_btc,  proof: witness },
        { chain: "CipherOcto", reservation: R_octo, proof: settlement_hash },
    ],
    completion: AllRequired,
}
```

Completion requires every proof. All or nothing. No bridge contract, no wrapped asset, no custodian.

Cross-chain capability:

```text
Capability {
    ...
    secured_by: CrossChainBacking::BitcoinHTLC { ... }
    // OR
    secured_by: CrossChainBacking::EthereumProof { ... }
}
```

A capability can carry proof that its authority is itself backed by an external chain's lock. The CipherOcto capability IS the cross-chain primitive — not a bridge, but a delegation backed by an external proof.

### §8 Hierarchical Vaults and Capability Spending Graph

Vaults form a hierarchy (§2.1). Each vault owns capabilities; each capability spends from one vault.

```text
Alice
  ├── Mission A        20 OCTO-W   capability
  ├── Claude            50 OCTO-W   capability
  ├── GPT               10 OCTO-W   capability
  └── Daily Budget     100 OCTO-W   capability
```

Each node is a capability. Not a balance. Each capability carries its own constraints.

Owner never spends directly. Capabilities spend.

`WrappedOnly` constraint enforces that a capability is only usable through a parent capability. Supports hierarchical delegation.

### §9 Horizontal Scalability — Resource Sharding

Ledger is horizontally partitioned by `vault_id` (not by event type — see Phase 4 §6.4 for the consistency rationale).

```text
events_vault_<hash_prefix_0>
events_vault_<hash_prefix_1>
...
events_vault_<hash_prefix_f>
```

Shard routing:

- Vault-scoped writes hit one shard
- Each shard publishes its own Merkle root
- Cross-shard settlements use `MultiSettlement` (§7)
- Vault-scoped reads use the partition's index

Bottleneck shifts from "transfers per second" to "independent resource commitments per second."

### §10 Consensus Sessions

The billion-dollar opportunity: **preserve the enterprise programming model, replace only the trust model.**

#### §10.1 The problem

```text
Enterprise:   Login → Session → Many operations → Logout
Blockchain:   Key → Sign → One transaction → Forget everything
```

The mental models are opposite. Every ORM, every stored procedure, every framework is built around the session model.

#### §10.2 The proposal

```text
Login
  ↓
OIDC | LDAP | Kerberos | SAML | OAuth | SAP RFC | Oracle JDBC
  ↓
Identity Verified
  ↓
Capability minted  (per RFC-0957 + RFC-0965 extensions)
  ↓
Capability Session  (in-memory, ephemeral)
  ↓
SQL Statements     (execute under capability)
```

SQL executes under capabilities instead of passwords.

#### §10.3 Deterministic SQL Engine

Stoolap already gives us DDL, indexes, views, foreign keys, window functions, joins. Add `CIPHERO_SQL` mode that forbids:

```text
NOW()         RAND()        HTTP()       FILE()
CURRENT_TIME  UUID_RANDOM   NET.HTTP     FILE.READ
```

… and allows everything else. Deterministic by construction. Matches RFC-0102, RFC-0104, RFC-0110, RFC-0126, RFC-0127.

`CIPHERO_SQL` spec in RFC-0961 (companion). This RFC only specifies the high-level mode.

#### §10.4 Stored Procedures survive

```sql
CREATE PROCEDURE close_month()
LANGUAGE CIPHERO_SQL
DETERMINISTIC
AS $$
    INSERT INTO monthly_summary
    SELECT event_seq / 1000000 AS month_bucket, SUM(amount_micro)
    FROM transfer_events
    WHERE event_seq < $block_start_seq
    GROUP BY 1;
$$;
```

`CIPHERO_SQL` rejects loops, side effects, time, randomness. `DETERMINISTIC` is enforced at parse time + verified at runtime.

#### §10.5 ORMs and JDBC work

```text
Hibernate        Entity Framework     Diesel
SQLAlchemy       Django ORM           Prisma
```

Ship `cipherocto-jdbc` driver (`jdbc:cipherocto://cluster`) that wraps a `Connection` over a `Capability Session` and signs the WAL block.

#### §10.6 WAL as Transaction

```text
LSN 1000
  → 100 SQL operations
  → Hash
  → One signature
  → Consensus
```

One signature for 100 statements. Application keeps session semantics. Consensus only sees immutable WAL segments (RFC-0862 + OctoSync).

#### §10.7 Identity Translation Gateway + Vet Factory

```text
LDAP / Active Directory / Kerberos / SAML / OAuth / mTLS / SSH key
  ↓
Capability
```

Or even better, emulate **services** as first-class actors:

```text
SAP → Capability
Oracle ERP → Capability
CRM → Capability
Warehouse → Capability
```

Systems become first-class actors. Exactly how enterprise systems actually work.

**Vet factory.** Per RFC-0957 caveat DSL + RFC-0965 caveat types, a `Caveat::Factory(vet)` carries a pre-validated invocation that an enterprise system may need to deploy before redeeming the capability. A **vet** is a structured object (canonicalised by RFC-0126):

```text
Vet {
    target_vault_id:    VaultID,           // the vault the capability redeems against
    action_template:    CanonicalAction,   // selector + arg shape; not raw bytes
    required_caller:    Option<DID>,       // who must invoke (default = capability holder)
    pre_conditions:     Vec<Constraint>,   // must all hold at redemption time
    expiry_for_deploy:  Timestamp,         // hard deadline for deploying + redeeming
}
```

`action_template` is a typed invocation shape, NOT opaque `Bytes`. The verifier runs the same constraint pipeline against the deployed target before redeeming. Raw bytes are rejected because they're a known phishing vector (EIP-7715 §Security Considerations).

Common vet pattern: "create a vault at `target_vault_id` with policy X, then redeem capability Y against it." Used by hierarchical vault creation (grand design §11) and cross-DAO delegation (Phase 3 §1.5).

#### §10.8 Compatibility Levels

```text
Level 1  ANSI SQL
Level 2  PostgreSQL-compatible
Level 3  Enterprise (Oracle/SAP extensions)
Level 4  Deterministic Blockchain (CIPHERO_SQL + capability-checked UPDATEs)
```

Migrate incrementally. Each level is a superset of the previous. Tooling per level (Phase 5 §6):

| Level | Toolchain |
|---|---|
| 1 | `cipherocto-jdbc` |
| 2 | + `cipherocto-logical-repl` |
| 3 | + `cipherocto-oracle-adapter`, `cipherocto-sap-rfc` |
| 4 | + `cipherocto-hibernate-dialect`, capability framework |

#### §10.9 ConsensusSession object

```text
ConsensusSession {
    session_id:        SessionID,
    capability:        CapabilityID,
    sql_statements:    Vec<CanonicalSQL>,
    stored_procs:      Vec<ProcInvocation>,
    ddl_changes:       Vec<DDLOperation>,
    wal_segment_hash:  Hash,                 // RFC-0862 segment commitment
    signature:         Signature,            // capability holder signs
    timestamp:         Timestamp,
}
```

One signed session object in the ledger. Internally many SQL ops. Externally indistinguishable from a regular database session to the application.

Wire protocol in RFC-0962 (companion). ZK circuit for batch signature in RFC-0962 §6.

### §11 Strategic Positioning

CipherOcto's architecture is closer to a **deterministic resource coordination network** than to "another blockchain with AI features":

- Blockchain = consensus substrate
- Primary abstraction = lifecycle of scarce resources
- Tokens = accounting representation of resources

The seven-layer model naturally accommodates:

- Time locks, vesting, lockups, liquidity locks → reusable constraints
- Atomic swaps, cross-chain → multi-settlement protocols
- Audit windows, disputes, delayed release → settlement state machine
- Massive horizontal scalability → resource shards + append-only events
- Enterprise migration → Consensus Sessions

The bottleneck shifts from "transfers per second" to "independent resource commitments per second" — a much better fit for decentralized AI infrastructure.

## Backwards Compatibility

This RFC introduces new primitives (`Vault`, `Capability`, `Reservation`, `Settlement`) and a new event log (`transfer_events`). Existing structures are unaffected:

| Existing | Coexistence |
|---|---|
| `octo_w_balances` table | Remains as a **projection cache**; not the source of truth. New code uses `transfer_events` log |
| `Balance::deduct` (`balance.rs:27`) | Bug fixed in the same revision: `saturating_sub` → checked subtraction that returns `Err(InsufficientBalance)` |
| RFC-0959 settlement chain | Continues as settlement receipt layer |
| RFC-0957 macaroon format | Extended via RFC-0965 (companion); old macaroons remain valid |
| `quota-router-core` 11-step exercise | Step 6 escrow = real `Reservation::mint()` (landed 2026-07-23; R1-F1 closeout). The 13/13 exercise tests pass after the change |

## Security Considerations

### Threat 1: Capability replay

A capability signed for `valid_until` is replayed after expiration. **Mitigation:** `expires_at` checked at every redemption site; capability carries `nonce` for replay protection; `MaxUses` / `SingleUse` enforce consumption count.

### Threat 2: Constraint bypass

A capability holder finds a way to spend without checking constraints. **Mitigation:** capability verifier runs **before** any state mutation; rejected operations do not reach the event log.

### Threat 3: Audit window dispute gaming

A holder settles a high-value transfer, then files a dispute within the audit window to reclaim the funds. **Mitigation:** `Frozen` state freezes the transfer; dispute resolution requires a higher-authority capability (governance). Repeated false-dispute attempts reduce reputation.

### Threat 4: Event log divergence

Two nodes apply the same events in different orders; balance projections diverge. **Mitigation:** shard routing by `vault_id` (Phase 4 §6.4); intra-shard writes serialize via consensus; cross-shard writes use `MultiSettlement`.

### Threat 5: Privacy leak via public event log

Confidential or private events leak holder state through public event attributes. **Mitigation:** `Confidential` events encrypt payload + carry commitment; `Private` events carry ZK proof of correctness; all attribute keys are committed before insertion.

### Threat 6: ConsensusSession signature forgery

An attacker forges a `ConsensusSession` signature to apply N SQL ops under a stolen capability. **Mitigation:** capability is bound to holder public key via `CallerBound` constraint; signature is verified per RFC-0957 macaroon rules; WAL segment hash binds all statements.

## Test Strategy

### Unit tests (per primitive)

| Primitive | Test |
|---|---|
| `Vault` | Creation, hierarchy, freeze/retire |
| `Capability` | Grant, attenuate, revoke, max_uses enforcement |
| `Reservation` | State machine transitions, audit window, frozen branch |
| `Settlement` | MultiSettlement completion, rollback |
| `Constraint` (23 variants) | Each constraint tested with positive + negative cases |
| Event log | Append-only invariant, correction semantics, projection rebuild |

### Integration tests

| Scenario | Test |
|---|---|
| 11-step exercise | Step 6 escrow becomes a real `Reservation` row; full pipeline still green |
| Enterprise migration | PostgreSQL logical replication → CipherOcto via `CIPHERO_SUBSCRIPTION` |
| Cross-shard settlement | `MultiSettlement` atomicity test |
| Audit window | Settled → Frozen → Dispute → Rollback test |

### Reference Implementation

Companion crate: `cipherocto-vault` — implements `Vault`, `Capability`, `Reservation`, `Settlement`, `Constraint` (23 variants), and the audit window state machine.

## Open Questions

| Question | Status |
|---|---|
| Canonical `Constraint` encoding | RFC-0964 (companion, planned) |
| `Capability` macaroon format extensions | RFC-0965 (companion, planned) |
| `CIPHERO_SQL` full language spec | RFC-0961 (companion, **Draft 2026-07-22**) |
| `ConsensusSession` wire protocol | RFC-0962 (companion, **Draft 2026-07-22**) |
| Resource shard routing algorithm | RFC-0963 (companion, planned) |
| Hierarchical vault policy lattice | Defer to v1.1 — capability-security lattice well-studied (KeyKOS, E, Capsicum) |

## References

### Internal research

- `docs/research/2026-07-22-value-transfer-model-internal-landscape.md` — Phase 1 internal scan (gap analysis)
- `docs/research/2026-07-22-grand-design-vaults-capabilities-reservations.md` — Phase 2 grand design synthesis
- `docs/research/2026-07-22-external-capability-based-spend-systems.md` — Phase 3 (EIP-7715, EIP-4337, Starknet, Sui, MACI)
- `docs/research/2026-07-22-event-sourced-ledger-precedents.md` — Phase 4 (Datomic, EventStoreDB, Kafka, Cosmos)
- `docs/research/2026-07-22-enterprise-migration-playbooks.md` — Phase 5 (PostgreSQL, ShardingSphere, Hibernate)

### Internal RFCs

- RFC-0957 (Economics): Capability Token Format — Accepted
- RFC-0958 (Economics): ZK Capability Subclass — Accepted
- RFC-0959 (Economics): Ask Settlement Chain — Accepted v1.0
- RFC-0126 (Numeric): Deterministic Serialization — Accepted
- RFC-0102 (Numeric): Wallet Cryptography — Accepted
- RFC-0862 (Networking): Stoolap Sync Layer — Accepted
- RFC-0909 (Economics): Deterministic Quota Accounting — Accepted (coexistence)

### External

- EIP-7715 (wallet permissions)
- EIP-4337 (account abstraction)
- EIP-7702 (EOA delegation)
- Starknet session keys + AA
- Sui object-capability model
- Aztec AuthWit
- MACI (Minimal Anti-Collusion Infrastructure)
- Datomic (ARAR + time model)
- EventStoreDB (streams + categories)
- Apache Kafka (log compaction + partitioning)
- PostgreSQL logical replication
- ShardingSphere (Database Plus)
- Hibernate ORM

## Copyright

Copyright and related rights waived via [CC0](https://creativecommons.org/publicdomain/zero/1.0/).
