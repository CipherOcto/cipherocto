# RFC-0855p-e (Networking): HandoverRequest Envelope & Mission Coordinator Term Handover

## Status

Draft (2026-09-02) — spec elaboration closed (v1.2); supersedes v1.1 baseline. Mission Coordinator only; DomainCoordinator uses platform-mediated handover per RFC-0855p-c §4.

## Authors

- @mmacedoeu

## Maintainers

- @mmacedoeu

## Summary

Spec the DOT/1/HANDOVER_REQUEST (HORQ) + HANDOVER_ACK (HOAK) + HANDOVER_DONE (HODN) envelopes for Mission Coordinator term handover. Closes scenario family S-C4. Builds on RFC-0855p-b §"Slash tally observability" + RFC-0855p-b §B "Slash Offense Codes" + RFC-0855p-b §"Data Structures" (CoordinatorLifecycle 8-state machine). Explicitly excludes DomainCoordinator (RFC-0855p-c §4 platform-mediated path). Bare RFC numbers only.

## Dependencies

- RFC-0850 (Networking) — Deterministic Overlay Transport
- RFC-0855p-b — Mission Coordinator Lifecycle (slash tally observability, Slash Offense Codes §B, CoordinatorLifecycle state machine)
- RFC-0855p-c — DomainCoordinator Role (excluded scope — see Layer placement)
- RFC-0850p-c — Transport Group Binding Ceremony (GroupBinding)
- RFC-0126 — DCS
- Layer-A primitives: BLAKE3-256, Ed25519 — reference only, never redefine

## Layer placement

| Concern                                                                     | Layer       | Justification                                                                           |
| --------------------------------------------------------------------------- | ----------- | --------------------------------------------------------------------------------------- |
| `HandoverEnvelope` outer wire (10-byte canonical header per RFC-0850p-c §A) | **Layer B** | Transport envelope; no Layer-C role knowledge embedded in outer wire                    |
| `HandoverPayload` inner DCS (HORQ/HOAK/HODN semantic body)                  | **Layer B** | Wire-format semantics; canonical form per RFC-0126                                      |
| `sender_state_snapshot_ordinal` + `snapshot_proof`                          | **Layer B** | Opaque ordinal + BLAKE3 commitment; decouples from upstream `CoordinatorLifecycle` enum |
| `typed_discriminator handover_reason_type_id` + opaque `reason_payload`     | **Layer C** | Coordinator-kind semantic; typed-discriminator avoids central enum                      |
| Race resolution (lex tiebreak + broadcast-before-transition ordering)       | **Layer C** | Consensus protocol; depends on consensus-aggregated state                               |
| Quorum check (`hodn_quorum` defense-in-depth)                               | **Layer C** | Coordinator-side governance policy                                                      |
| `slash_tally_hash` reference + slash tally carry-over                       | **Layer C** | Slash substrate is Layer C per CLAUDE.md §Architectural Principles                      |
| Re-export alias `CoordinatorLifecycle = rfc_0855p_b::CoordinatorLifecycle`  | **Layer C** | Alias only; no redefinition of upstream Layer-C type                                    |
| `hodn_quorum` const fn (with `assert!` defense-in-depth per L3 M6 finding)  | **Layer C** | Coordinator-side governance policy                                                      |
| `horq_quorum` const fn (with `assert!` defense-in-depth per L3 M4 finding)  | **Layer C** | HORQ receipt-site mirror of `hodn_quorum` invariant                                     |
| `HandoverCancelPayload` inner DCS (`b"HORC"` semantic body)                 | **Layer B** | Wire-format semantics; canonical form per RFC-0126                                      |
| `SlashTallyUpdate` (local typed enum, slash tally carry-over semantics)     | **Layer C** | Coordinator-kind governance state; typed-discriminator pattern                          |
| `SlashReasonCode(pub u32)` newtype + named consts                           | **Layer C** | Type-level cross-namespace collision guard                                              |
| `GroupBindingRef` re-export (`pub use rfc_0850p_c::GroupBinding`)           | **Layer B** | Re-export only; no redefinition of upstream Layer-B substrate                           |
| `HANDOVER_RACE_WINDOW: u64 = 5` const (added v1.1 per W10 L3 C3 finding)    | **Layer C** | HORQ race-resolution time-bound (witness dedup window)                                  |

Direction A→B→C/D/E verified: this RFC depends on RFC-0855p-b (Layer C governance), RFC-0850p-c (Layer B transport), RFC-0126 (Layer A canonical encoding). No upward dependency.

### Layer placement constants block

```rust
/// Forward epoch-skew tolerance for envelope recipients (added v0.9 per L2 C1 finding).
/// Recipients reject envelopes where `current_epoch > local_epoch + MAX_FSKEW_EPOCHS`.
/// Shared across HORQ/HOAK/HODN/HORC envelope family per RFC-0855p-d1 §Layer placement table.
pub const MAX_FSKEW_EPOCHS: u64 = 4;
/// HORQ concurrent-broadcast tiebreak window (split v1.2 per W11 L1 H1 finding — was triple-overloaded):
/// recipients lex-tiebreak two HORQ broadcasts whose `current_epoch` falls within this window.
pub const HANDOVER_RACE_WINDOW: u64 = 5;
/// HORQ backward-replay cutoff (added v1.2 per W11 L1 H1 finding — split from `HANDOVER_RACE_WINDOW`):
/// recipients ignore HORQ broadcasts whose `current_epoch` is more than `HORQ_BACKWARD_WINDOW` epochs
/// behind the local head, preventing late-arriving duplicate HORQ from spurious successor dedup state.
pub const HORQ_BACKWARD_WINDOW: u64 = 5;
/// Handover forward-skew boost (reserved v1.2 per W11 L1 H1 finding — split from `HANDOVER_RACE_WINDOW`):
/// additional forward tolerance if handover needs lockout-transition semantics distinct from family-wide
/// `MAX_FSKEW_EPOCHS = 4`. Currently UNUSED in envelope acceptance (recipients use `MAX_FSKEW_EPOCHS` only,
/// per W11 L1 H5 cross-family consistency fix); reserved for future cross-RFC justification.
pub const HANDOVER_FORWARD_SKEW_BOOST: u64 = 0;
```

## Design Goals

1. Atomic handover across mesh — no double-coordinator window.
2. Slash tally carry-over — successor inherits canonical tally from incumbent.
3. Group binding transfer — incumbent's `GroupBinding` set handed to successor.
4. Witness quorum ≥ 2/3 — single hostile witness cannot complete handover.
5. Replay-resistance — nonce + current_epoch in every hash, recipient dedup window.
6. Race-resolution determinism — lexicographic coordinator_id tiebreak + epoch monotonicity.
7. Slash-protection — `Suspect`/`Demoting` cannot self-initiate; only `Active` coordinators may issue HORQ.
8. Backward compat — non-handover-aware clients keep working (no state regression).

## Motivation

Coordinator lifecycle handover gap — `Active → Handover` transition has no envelope defined in RFC-0855p-b. RFC-0855p-b specifies the 8-state machine (Active/Handover/Demoting/etc.) and slash tally observability but leaves the handover envelope shape undefined. RFC-0855p-e closes that gap with the 3-envelope handshake (HORQ/HOAK/HODN).

## Scope (explicit)

In scope: Mission Coordinator term handover via HORQ/HOAK/HODN.

Out of scope:

- DomainCoordinator handover — use RFC-0855p-c §4 platform-mediated path (`PlatformEvent::AdminTransfer` via WhatsApp `participant promote` or equivalent).
- Witness role handover — Witness is a Mission Participant slash-vote role per RFC-0855p-b §"Roles and Authorities", NOT a coordinator; no handover envelope needed.
- Election — separate from handover; covered by RFC-0855p-b §"Election Algorithm (per governance model)".
- Emergency takeover — covered by `Demoting` path per RFC-0855p-b, not by handover.

## Roles and Authorities

Per RFC-0855p-b §"Roles and Authorities", four roles exist: Mission Coordinator, Mission Creator, Mission Participant, Slashing Adjudicator. Witness is a Mission Participant sub-role (slash-vote). DomainCoordinator listed as FUTURE in RFC-0855p-b, defined separately in RFC-0855p-c.

| Role                                             | Initiate HORQ     | Receive HORQ  | Issue HOAK | Issue HODN              | Slash-relevant                |
| ------------------------------------------------ | ----------------- | ------------- | ---------- | ----------------------- | ----------------------------- |
| Incumbent Mission Coordinator (Active)           | YES               | —             | —          | —                       | YES (Demoting on quorum fail) |
| Incumbent Mission Coordinator (Suspect/Demoting) | NO                | —             | —          | —                       | YES (already slashing)        |
| Successor Candidate                              | —                 | YES           | —          | YES (after HOAK quorum) | YES (false acceptance)        |
| Mission Participant (Witness sub-role)           | —                 | YES           | YES        | —                       | YES (false attestation)       |
| Mission Creator                                  | —                 | YES (passive) | NO         | NO                      | NO                            |
| Slashing Adjudicator                             | —                 | YES (passive) | NO         | NO                      | YES (resolves disputes)       |
| DomainCoordinator                                | NO (out of scope) | NO            | NO         | NO                      | NO                            |

Note: RFC-0855p-b §"Roles and Authorities" §5 lists DomainCoordinator as FUTURE — not implemented by this RFC. Adding CoordinatorKind variants follows the typed-discriminator pattern (§Extension over enumeration) when DomainCoordinator or other roles (Capability/Reputation/Market) land.

## Adversary Analysis

### 5-Question Test

**Q1: What does the adversary want?**

- Double-coordinator: same term, two Active coordinators simultaneously.
- Slash evasion: handover to evade pending slash.
- Witness capture: malicious quorum completes fake handover.
- Successor sybil: adversary installs own key as successor.
- Replay: replay old HORQ to re-trigger handover after term change.

**Q2: How does adversary attack?**

- Race: broadcast HORQ twice simultaneously to confuse recipients.
- Bribe witnesses: 1/3 witness set colludes.
- Sybil successor: register many candidates, elect weakest.
- Replay: capture HORQ from prior term, replay in new term.
- Slash evasion: enter Handover to escape Demoting state.

**Q3: What makes attack hard?**

- Witness quorum ≥ 2/3 (collusion needs 67% not 33%).
- Successor eligibility verified per RFC-0855p-b §"Election Algorithm (per governance model)" (stake + reputation).
- Nonce + current_epoch in all hashes prevents replay across terms.
- Race resolution deterministic (lex coordinator_id + epoch check).
- Slash tally carry-over: handover does not clear pending slashes.

**Q4: What if adversary controls network?**

- DOT/1 layer guarantees transport reliability per RFC-0850.
- Recipients log all envelopes; out-of-order delivery resolved via `coordinator_term_id` binding.
- Network partition: HORQ timeout `HANDOVER_TIMEOUT = 500 epochs` (matches RFC-0855p-b §"Handover Protocol" E2E IS-4.4 fix) → incumbent slashed if quorum not reached.

**Q5: What if adversary controls 1/3 witnesses?**

- Cannot reach 2/3 quorum alone.
- Cannot block quorum if other 2/3 honest (quorum is over, not veto).
- Partial colluders get slashed on false attestation (reason `SlashReasonCode::FalseAttestation = 0x0013` per 0855p-b pending §B amendment; 0x000C is RESERVED, NOT a slash reason per RFC-0855p-b §B).

## Implicit Assumptions Audit

1. `witness_set_size ≥ 3` — otherwise 2/3 quorum collapses to 1 or 2 witnesses (meaningless). Configurable per mission; minimum enforced at mission creation.
2. HORQ/HOAK/HODN transport reliability — DOT/1 layer guarantees delivery per RFC-0850 §11 "Reliability Model". No application-layer retransmit needed.
3. `current_epoch` monotonicity — local epoch monotonic per RFC-0855p-b §"B'. Genesis Constants"; recipients verify `|local_epoch − envelope_epoch| ≤ MAX_FSKEW_EPOCHS = 4` (aligned v0.8 per L1 C5 + L5 H1 findings; the bound is shared across the envelope family per RFC-0855p-d §Layer placement constants block and is auditable independent of `RACE_EPOCHS = 32` backward-replay bound). Constant: `pub const MAX_FSKEW_EPOCHS: u64 = 4;` declared in this RFC §Layer placement constants block; recipients reject envelopes where `current_epoch > local_epoch + MAX_FSKEW_EPOCHS` before signature verification.
4. Successor key eligibility — successor_id verified election-eligible per RFC-0855p-b §"Election Algorithm (per governance model)" before HORQ broadcast. Incumbent signs HORQ only after eligibility check.
5. Slash tally canonical form — BLAKE3 hash of sorted ballots per RFC-0855p-b §"Slash tally cryptographic finality". Witnesses recompute + verify before HOAK.
6. Group binding transfer — incumbent's `GroupBinding` set is exactly the set successor needs; no partial handover.
7. Pending envelopes — pending in-flight envelopes transferred via `pending_envelopes_hash` (Merkle-leaf-style: `BLAKE3-256("DOT/1/HANDOVER/pending" || BLAKE3-256(canonical_dcs(env_1)) || BLAKE3-256(canonical_dcs(env_2)) || …)`); successor resumes processing after fetching each envelope referenced by the hash from the mesh (mesh fanout: incumbent's peers within `pending_broadcast_window = 32 epochs` replicate pending envelopes; successor queries peers by `(mission_id, envelope_hash)` to recover). Transfer protocol (added v0.7 per L3 M5 finding): incumbent publishes pending envelope list alongside HODN; successor subscribes via mesh gossip and confirms each via `pending_envelopes_hash` membership check before re-emitting any state-changing envelope.

## Security Considerations

- **Quorum threshold (2/3, not 1)** — single hostile witness cannot complete handover. Witness set must be ≥ 3 for meaningful quorum.
- **Race-resolution determinism** — lex `coordinator_id` tiebreak via BLAKE3_keyed("DOT/1/HANDOVER/coordinator_id_lex", coordinator_id) canonical form (per §BLAKE3 Construction Conventions; canonical form prevents fork preference divergence between clients using different lex ordering) + `current_epoch` monotonicity check. Loser's HORQ silently dropped; no slash for race (deterministic, not misbehavior). The local `Active → Handover` transition for the winner happens ONLY AFTER HORQ quorum-confirm, so the loser cleanly stays `Active` (no rollback needed). HORQ broadcast carries the incumbent's `sender_state_snapshot: CoordinatorLifecycle` typed snapshot inside the signed envelope; recipients reject HORQ whose snapshot ≠ `Active`. Canonical path (specified v1.1 per W9 L3 C3 finding — resolves chicken-and-egg: recipient must verify snapshot, but snapshot is consensus-aggregated and recipient needs second-witness ACK to bootstrap trust): recipients reject HORQ whose `sender_state_snapshot_ordinal != SenderStateSnapshotOrdinal::Active` UNLESS the HORQ carries ≥ 1 second-witness HOAK attesting to the incumbent's Active state at `current_epoch - 1`. Second-witness HOAK has the same shape as a regular HOAK but carries a dedicated `attests_to_predecessor_state: bool = true` flag (added v1.1 per W9 L3 C3). This breaks the chicken-and-egg by bootstrapping Active state attestation through witness quorum rather than relying on the recipient's own consensus-aggregated view (which lags behind by `MAX_FSKEW_EPOCHS`).
- **Replay protection** — `nonce: [u8; 16]` + `current_epoch: u64` in every hash. Recipient verifies nonce not seen for `(coordinator_id, mission_id, coordinator_term_id)` tuple in last `HORQ_BACKWARD_WINDOW = 5 epochs` (consistent with §Layer placement constants block).
- **Signature continuity** — HORQ signed by incumbent, HOAK signed by witness, HODN signed by successor. `handover_request_hash` (BLAKE3 of HORQ envelope) referenced in HOAK + HODN binds the chain. **Hash-chain binding** (added v0.8 per L3 C4 finding): recipient of HODN MUST verify `handover_request_hash == BLAKE3-256(canonical_dcs(HORQ_envelope_with_signature_zeroed))` (the HORQ envelope the HODN claims to accept MUST hash to the HODN's `handover_request_hash`); recipients MUST recompute the hash from the cached HORQ envelope and reject HODN on mismatch. Without this verification, a successor could HODN a different / substituted HORQ than what witnesses attested to, breaking the chain.
- **Slash tally integrity** — canonical hash per RFC-0855p-b §"Slash tally cryptographic finality" verified by all witnesses before HOAK. Tampering causes HOAK rejection + witness slash.
- **Successor eligibility** — successor_id verified per RFC-0855p-b §"Election Algorithm (per governance model)" (stake + reputation thresholds). Invalid successor → HORQ rejected at recipient, no slash (recipient's local check, not network consensus).
- **Incumbent double-handover** — incumbent cannot issue HORQ while in `Handover` state (state machine invariant). Issuing HORQ from any state other than `Active` is malformed and rejected.
- **Incumbent mid-handover signing authority** (added v0.6, lockout scope clarified v0.8; HORQ carve-out enumerated v0.8 per L3 C3 finding) — recipients enforce an incumbent-HORQ-in-flight lockout: once an incumbent broadcasts HORQ for the current `coordinator_term_id`, recipients reject any state-changing envelope signed by that incumbent (under the same `coordinator_id`) until either (a) HODN quorum-confirms and the lockout transfers to the successor, or (b) the HORQ times out / fails quorum and the lockout clears. This closes the window where a malicious incumbent could finalize malicious state changes between HORQ broadcast and HODN. The incumbent MAY sign envelopes related to the handover itself, specifically: HORQ itself (the initial broadcast that triggers the lockout is itself signed by the incumbent; lockout scope does NOT retroactively forbid the initial HORQ); HORQ replay-cancel via `HANDOVER_REQUEST_CANCEL` (b"HORC") subtype ONLY. The incumbent MAY NOT sign any other state-changing envelope. **Critical: the lockout does NOT permit the incumbent to sign HOAK envelopes** — HOAK is witness-attestation of HORQ (subtype b"HOAK" is witness-signed per §Roles and Authorities; incumbent is not in the witness set during their own handover). The incumbent's lockout carve-out covers TWO subtypes: `b"HORQ"` (the initial triggering broadcast) and `b"HORC"` (mid-handover cancel); see §Handover Envelope Subtypes for the canonical subtype tag table. Recipients track the lockout per `(coordinator_id, coordinator_term_id)`; lockout state is consensus-aggregated so a forked client cannot bypass it. **Post-HODN scope** (clarified v0.8 per L3 H1 finding): once HODN quorum-confirms, the lockout transfers to the successor; HORC signed by the incumbent is FORBIDDEN post-HODN (incumbent is no longer the active coordinator; any HORC after HODN is a stale-key replay attack and MUST be rejected). Recipients MUST verify lockout status before accepting any HORC. Lockout scope clarification addresses L1 C7 + L3 M6 findings (incumbent HOAK signing was a v0.6/v0.7 contradiction; corrected v0.8).

## Specification

### Envelope Types Added

```rust
pub const HANDOVER_REQUEST: [u8; 4] = *b"HORQ";
pub const HANDOVER_ACK: [u8; 4] = *b"HOAK";
pub const HANDOVER_DONE: [u8; 4] = *b"HODN";
```

| Envelope Type            | Subtype tag | Direction                          | Description                                                                                       |
| ------------------------ | ----------- | ---------------------------------- | ------------------------------------------------------------------------------------------------- |
| `DOT/1/HANDOVER_REQUEST` | `b"HORQ"`   | Incumbent → mesh (broadcast)       | Mission Coordinator initiates handover to successor                                               |
| `DOT/1/HANDOVER_ACK`     | `b"HOAK"`   | Witness → mesh (broadcast)         | Witness confirms HORQ (quorum ≥ 2/3 required)                                                     |
| `DOT/1/HANDOVER_DONE`    | `b"HODN"`   | New Coordinator → mesh (broadcast) | Successor confirms receipt + acceptance                                                           |
| `DOT/1/HANDOVER_CANCEL`  | `b"HORC"`   | Incumbent → mesh (broadcast)       | Incumbent replay-cancel during lockout (added v0.8; full subtype per §Handover Envelope Subtypes) |

All three envelopes share the canonical 10-byte header layout from RFC-0850p-c §A (Layer-B wire format):

```text
offset 0..4   = envelope_type        [u8; 4]   // b"DOT1"
offset 4..8   = envelope_subtype     [u8; 4]   // b"HORQ" | b"HOAK" | b"HODN"
offset 8..10  = version              [u16 BE]  // 0x0001
offset 10..   = payload (DCS-encoded HandoverEnvelope body)
```

The 10-byte header is the wire discriminator + version tag; the remainder is the DCS-encoded envelope body. Recipients reject envelopes with `envelope_type != b"DOT1"` or unknown `envelope_subtype` before any payload decode (fail-closed dispatch per §Extension over enumeration).

### Layer-B Wire Envelope (outer)

```rust
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct HandoverEnvelope {
    pub envelope_type: [u8; 4],         // b"DOT1"
    pub envelope_subtype: [u8; 4],      // b"HORQ" | b"HOAK" | b"HODN"
    pub version: u16,                   // 0x0001
    pub mission_id: [u8; 16], // truncated BLAKE3-256(canonical_dcs(mission_did)) per RFC-0009 §Identity (corrected v0.8 per L1 C3 finding: field is 16B, derivation is 16B; v0.7 32B field would have required either zero-padding at serialize (changes digest) or truncation at encode (changes wire format))
    pub coordinator_term_id: [u8; 32],  // current term (binding across HORQ→HOAK→HODN)
    pub current_epoch: u64,
    pub nonce: [u8; 16],
    pub payload_hash: [u8; 32],         // BLAKE3-256 keyed-hash of inner HandoverPayload (see §Payload Hash)
    // typed-discriminator `SenderStateSnapshotOrdinal(pub u8)` newtype (v0.8 per L2 M4 finding) over the upstream
    // `CoordinatorLifecycle` 8-state enum (RFC-0855p-b §"Data Structures"). Replacing the
    // Layer-C `CoordinatorLifecycle` type with a newtype ordinal decouples the
    // Layer-B wire envelope from the upstream Layer-C type definition:
    // clients on different 0855p-b versions can still verify the snapshot
    // via `snapshot_proof` (which is version-stable per the binding below)
    // without agreeing on the upstream enum byte representation.
    // `SenderStateSnapshotOrdinal(0)` reserved (invalid; §"Sender State
    // Snapshot Ordinals" maps `Active = 1`, `Handover = 2`, `Demoting = 3`,
    // `Inactive = 4`, `Suspect = 5`, `Rotating = 6`, `Demoted = 7`,
    // `Revoked = 8`).
    //
    // snapshot_proof binding (corrected v0.8 per L1 H2 + L3 H2 findings):
    // version-stable byte form (no upstream enum dependency). The v0.7
    // binding `canonical_dcs(CoordinatorLifecycle)` falsely claimed
    // version-decoupling but actually locked substrate to the upstream
    // enum encoding. New binding: `BLAKE3_keyed("DOT/1/HANDOVER/snapshot",
    // &[(snapshot_proof_version: u16).to_be_bytes(),
    // (sender_state_snapshot_ordinal: u8).to_be_bytes()])` — both inputs
    // are version-stable wire primitives. The `snapshot_proof_version`
    // prefix enables future snapshot_proof algorithm migration.
    pub sender_state_snapshot_ordinal: SenderStateSnapshotOrdinal,
    pub snapshot_proof_version: u16, // wire field (added v0.8 per L3 C2 finding); snapshot_proof algorithm version tag; enables future snapshot_proof algorithm migration without breaking wire compatibility
    pub snapshot_proof: [u8; 32], // BLAKE3_keyed("DOT/1/HANDOVER/snapshot", snapshot_proof_version: u16_be || ordinal: u8_be)
    pub payload: HandoverPayload,       // typed payload (Layer C, no role-kind enum)
    pub signature: [u8; 64],            // Ed25519
}
```

### Payload Hash (domain-separated)

```text
payload_hash = BLAKE3-256("DOT/1/HANDOVER/payload" || canonical_dcs(payload_bytes))
```

The domain string `"DOT/1/HANDOVER/payload"` is prepended to the canonical DCS bytes of `payload` before hashing. This binds the hash to the envelope family (HORQ/HOAK/HODN) and prevents cross-protocol hash confusion. `canonical_dcs` is the canonical-DCS bytes form per RFC-0126.

### BLAKE3 Construction Conventions

Across the HORQ/HOAK/HODN envelope family, BLAKE3 is used in two distinct construction modes:

- **Domain-separated plain BLAKE3** (for payload commitments): the domain string is prepended to the canonical DCS bytes before hashing. Used for `payload_hash`, `ack_hash`, `done_hash`, `pending_envelopes_hash`, `new_term_id`. Each uses a distinct domain string (`"DOT/1/HANDOVER/payload"`, `"DOT/1/HANDOVER/ack"`, `"DOT/1/HANDOVER/done"`, `"DOT/1/HANDOVER/pending"`, `"DOT/1/HANDOVER/new_term"`) so cross-field collisions are prevented.
- **Keyed BLAKE3** (for ID/ref references): BLAKE3 keyed-hash with a fixed protocol key. Reserved for ID and reference fields where keyed-hash adds a second authentication factor. Used for the lex-tiebreak canonical form in §"Handover Race Resolution" (BLAKE3_keyed("DOT/1/HANDOVER/coordinator_id_lex", coordinator_id)).
- **`slash_tally_hash` is OUT OF SCOPE for this RFC's domain-separation scheme.** The canonical form is owned by RFC-0855p-b §"Slash tally cryptographic finality" (plain `BLAKE3-256(canonical_dcs(sorted_ballots))`); 0855p-e references it as an opaque 32-byte digest. Witnesses MUST use the RFC-0855p-b canonical computation when recomputing for HOAK verification; applying 0855p-e's domain prefix to `slash_tally_hash` would break interop with RFC-0855p-b slash-tally substrate.

This convention is documented so implementers do not substitute plain BLAKE3 for keyed BLAKE3 or vice versa — both modes have distinct threat models and are NOT interchangeable.

### Layer-C Semantic Payload (inner)

```rust
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct HandoverPayload {
    // Typed-discriminator per §Extension over enumeration. RFC-0855p-b §"Roles and Authorities" §1 = Mission Coordinator (type_id = 0x0100 per namespace table below). Future roles (Capability Coordinator, Reputation Coordinator) get new type_ids via the registry.
    pub coordinator_role_type_id: u32,
    pub coordinator_id: [u8; 32],
    pub successor_id: [u8; 32],
    // new_term_id = BLAKE3-256("DOT/1/HANDOVER/new_term" || mission_id || coordinator_term_id || successor_id || current_epoch_be)
    // mission_id = first 16 bytes of BLAKE3-256(canonical_dcs(mission_did)) per RFC-0009; prevents cross-mission term-id collision when
    // same coordinator/successor pair operates multiple missions concurrently (added v0.7 per L3 H2 finding).
    // Domain prefix prevents cross-protocol collision with `slash_tally_hash` and other 32-byte BLAKE3 digests.
    pub new_term_id: [u8; 32],
    pub handover_reason_type_id: HandoverReasonTypeId, // typed-discriminator newtype; see §Handover Reason Type IDs
    pub reason_payload: BoundedBytes<256>,      // opaque reason-specific payload (DCS bytes)
    // Opaque 32-byte reference to canonical slash tally per RFC-0855p-b §"Slash tally cryptographic finality".
    // 0855p-e does NOT domain-separate this hash; the canonical form is owned by RFC-0855p-b and witnesses
    // MUST recompute using the RFC-0855p-b recipe. Witnesses fetch by (mission_id, coordinator_term_id,
    // current_epoch) and verify match — no lookback window.
    pub slash_tally_hash: [u8; 32],
    pub group_bindings: BoundedVec<GroupBindingRef, MAX_GROUP_BINDINGS_PER_HANDOVER>, // bounded 4096; reject oversize at DCS decode
    pub pending_envelope_count_be: u16, // count of pending envelopes referenced in pending_envelopes_hash; bounded by MAX_PENDING_ENVELOPES_PER_HODN = 1024 (added v1.1 per W9 L3 C5 finding)
    pub pending_envelope_hashes: BoundedVec<[u8; 32], MAX_PENDING_ENVELOPES_PER_HODN>, // explicit list of BLAKE3-256(canonical_dcs(env_i)) for each pending envelope; bounded; recipients fetch each from mesh by (mission_id, envelope_hash) (added v1.1 per W9 L3 C5 — explicit list removes ambiguity in pending_envelopes_hash Merkle-leaf reconstruction)
    // pending_envelopes_hash = BLAKE3-256("DOT/1/HANDOVER/pending" || BLAKE3-256(canonical_dcs(env_1)) || BLAKE3-256(canonical_dcs(env_2)) || …)
    // Each envelope is hashed individually FIRST (Merkle-leaf-style), then the inner-root list is hashed
    // under the domain prefix. Prevents concatenation collisions between distinct pending-envelope
    // queues that happen to produce the same plain BLAKE3 digest.
    pub pending_envelopes_hash: [u8; 32],
}

/// Maximum number of group bindings transferable in a single HORQ.
/// Matches the attestation-pattern bound; reject oversize at DCS decode.
pub const MAX_GROUP_BINDINGS_PER_HANDOVER: u16 = 4096;
pub const MAX_PENDING_ENVELOPES_PER_HODN: u16 = 1024; // added v1.1 per W9 L3 C5 finding — bounds the pending-envelope count transferred in a single HODN; prevents unbounded DoS amplification where a malicious incumbent references millions of pending envelopes forcing successor to fetch+verify each; recipient rejects HODN with pending_envelope_count > MAX_PENDING_ENVELOPES_PER_HODN before Merkle-leaf computation; 1024 chosen to allow legitimate handover bursts while bounding worst-case wire size (~544 KiB per HODN matching MAX_GROUP_BINDINGS_PER_HANDOVER worst case)
// Worst-case wire size (L1 C8 / L3 M2 findings, corrected v0.8): `GroupBindingRef`
// size = 32 (binding_id) + 32 (parent_domain_id) + 32 (group_pubkey) + 4
// (envelope version) + 1 (policy_version) + 32 (signature) = 133 bytes
// (estimate, see RFC-0850p-c §"GroupBindingRef wire layout" for canonical
// bytes). 4096 × 133 = ~545 KiB per HODN. DCS decode MUST enforce a hard
// `MAX_GROUP_BINDINGS_PER_HANDOVER = 4096` cap BEFORE Merkle-leaf computation to
// bound CPU/IO; without it a malicious incumbent could publish 4096 bindings
// with adversarial payloads to OOM recipients during successor-takeover
// window. Substrate MUST reject at DCS decode (not post-hash) — recipient
// MUST NOT begin hashing until `group_bindings.len() <=
// MAX_GROUP_BINDINGS_PER_HANDOVER` is verified. (Note: the v0.7 wording
// referenced `pending_broadcast_window = 32` which is the pending-envelope
// mesh-fanout TIME window in epochs per §Implicit Assumptions Audit #7 — not
// a binding count; corrected v0.8.)

// `HANDOVER_TALLY_WINDOW_EPOCHS` deprecated in v0.6: slash-tally fetch key is
// `(mission_id, coordinator_term_id, current_epoch)` (RFC-0855p-b canonical,
// append-only). A lookback window silently dropped slashes issued in the
// last N epochs, contradicting "slashes do not expire on handover".
// Reserved for potential future use (e.g., slash-event staleness check).
#[deprecated(note = "removed in v0.6; slash tally is fetched at current_epoch, no lookback")]
pub const HANDOVER_TALLY_WINDOW_EPOCHS: u64 = 0;
```

### Coordinator Role Type ID namespace

```text
0x0100       = Mission Coordinator (RFC-0855p-b §"Roles and Authorities" §1)
0x0101-0x01FF = RFC-allocated future roles (Capability Coordinator, Reputation Coordinator, etc.)
0x0200-0xFFFF = user extension (registry-registered; must pass CoordinatorRoleTypeId::is_reserved(id) → false check)
```

`CoordinatorRoleTypeId::is_reserved(id: u32) -> bool` returns true iff `id ∈ {0x0100..=0x01FF}`. Recipients reject any HORQ whose `coordinator_role_type_id` is reserved and not present in the registered role set, OR is in `0x0200..=0xFFFF` and not in the registry. Note: `0x0001` is the slash reason `double-sign` per RFC-0855p-b §B; type IDs must NEVER collide with slash reason codes.

### Handover Reason Type IDs

`HandoverReason` is replaced by a typed-discriminator `handover_reason_type_id: u32` + opaque `reason_payload: BoundedBytes<256>`. The typed-discriminator pattern prevents the central-enum drift hazard (§Extension over enumeration) and allows user-extensible triggering reasons.

```text
0x0010       = Voluntary        (RFC-0855p-e default for handover-by-request)
0x0011       = Scheduled        (e.g., term limit reached)
0x0012       = MissionTerminated (mission owner requests end-of-term)
0x0013-0x0016 = reserved (slash reason space per RFC-0855p-b §B amendment; distinct newtype from `SlashReasonCode` enforces compile-time separation, but value-level collision must be guarded per cross-RFC critical sequencing — see F-7 BLOCKING acceptance gate)
0x0017-0x001F = reserved for future RFC-allocated reasons
0x0020-0x00FF = RFC-allocated future reasons
0x0100-0xFFFF = user extension (registry-registered)
```

HORQ is Voluntary-Handover-only per RFC-0855p-b §"Handover Protocol"; Forced (ForcedHandover) and Emergency (EmergencyHandover) handovers use RFC-0855p-b's dedicated envelope types, NOT HORQ.

// HandoverAck payload
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct HandoverAckPayload {
pub handover_request_hash: [u8; 32], // = BLAKE3-256(DCS(HandoverEnvelope { signature: [0u8; 64], ..envelope })); signature is zeroed before hashing
pub witness_id: [u8; 32], // raw 32-byte array; no endianness (DCS-array framing per RFC-0126)
pub witness_epoch: u64, // big-endian when serialized for ack_hash computation
pub attests_to_predecessor_state: bool, // (added v1.1 per W10 L3 C1 finding) — when true, this HOAK is a second-witness attestation of the incumbent's Active state at `attested_epoch`; canonical path for HORQ snapshot chicken-and-egg per §Race-resolution determinism; recipients MUST NOT accept a HORQ with sender_state_snapshot_ordinal != Active unless this HOAK is supplied alongside
pub attested_epoch: u64, // (added v1.1 per W10 L3 H finding) — epoch at which the predecessor's Active state is being attested; required when attests_to_predecessor_state=true; for the canonical path this MUST equal HORQ.current_epoch_be - 1
pub ack_hash: [u8; 32], // BLAKE3-256("DOT/1/HANDOVER/ack" || handover_request_hash || witness_id_be || witness_epoch_be || (attests_to_predecessor_state as u8_be) || attested_epoch_be); u64 fields encoded big-endian; witness_id is raw 32 bytes (DCS-array)
}

// HandoverDone payload
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct HandoverDonePayload {
pub handover_request_hash: [u8; 32],
pub new_coordinator_id: [u8; 32], // recipient MUST verify new_coordinator_id == horq.successor_id; reject HODN on mismatch (local check, no slash)
pub accepted_epoch: u64,
pub done_hash: [u8; 32], // BLAKE3-256("DOT/1/HANDOVER/done" || handover_request_hash || new_coordinator_id || accepted_epoch_be); u64 fields encoded big-endian
}

````

### Layer-C Substrate Types (defined locally in 0855p-e)

```rust
/// Sender State Snapshot Ordinals (added v0.8 per L5 C2 finding).
///
/// Maps `CoordinatorLifecycle` (RFC-0855p-b §"Data Structures" 8-state
/// enum) to a stable `u8` ordinal for Layer-B wire use. The ordinal is
/// independent of the upstream enum variant index (which can shift as
/// RFC-0855p-b adds `Suspended` / `Archived` / `Frozen` states); the
/// ordinal is a wire-stable contract.
///
/// `SenderStateSnapshotOrdinal(0)` is reserved (invalid); never assigned.
/// `#[non_exhaustive]` (added v1.1 per W10 L2 H1 finding) prevents external
/// crates from exhaustively matching all ordinal variants — recipients MUST
/// handle unknown ordinals (defensive: fail-closed; recipients MUST reject
/// HORQ whose `sender_state_snapshot_ordinal` is not in the known set
/// `{1..=8}` OR is not accompanied by a second-witness HOAK with
/// `attests_to_predecessor_state = true` per §Race-resolution determinism).
/// The non-exhaustive struct attribute ensures cross-crate additions to the
/// ordinal set force explicit handling rather than silent pattern-match fallthrough.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
#[non_exhaustive]
pub struct SenderStateSnapshotOrdinal(u8);

impl SenderStateSnapshotOrdinal {
    pub const Active: SenderStateSnapshotOrdinal = SenderStateSnapshotOrdinal(1);
    pub const Handover: SenderStateSnapshotOrdinal = SenderStateSnapshotOrdinal(2);
    pub const Demoting: SenderStateSnapshotOrdinal = SenderStateSnapshotOrdinal(3);
    pub const Inactive: SenderStateSnapshotOrdinal = SenderStateSnapshotOrdinal(4);
    pub const Suspect: SenderStateSnapshotOrdinal = SenderStateSnapshotOrdinal(5);
    pub const Rotating: SenderStateSnapshotOrdinal = SenderStateSnapshotOrdinal(6);
    pub const Demoted: SenderStateSnapshotOrdinal = SenderStateSnapshotOrdinal(7);
    pub const Revoked: SenderStateSnapshotOrdinal = SenderStateSnapshotOrdinal(8);

    /// Validated constructor (added v1.2 per W11 L1 H2 finding — field made private
    /// to block external crates from constructing invalid `SenderStateSnapshotOrdinal(0)`
    /// reserved or `SenderStateSnapshotOrdinal(255)` unknown ordinals).
    pub fn new(v: u8) -> Result<Self, InvalidOrdinal> {
        if !(1..=8).contains(&v) {
            return Err(InvalidOrdinal(v));
        }
        Ok(Self(v))
    }
}

#[derive(Dcs, Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidOrdinal(pub u8);
````

### Handover Envelope Subtypes

```rust
/// Subtype tag constants (RFC-0850p-c §A canonical 4-byte tags).
pub const HANDOVER_REQUEST: [u8; 4] = *b"HORQ";
pub const HANDOVER_REQUEST_CANCEL: [u8; 4] = *b"HORC"; // incumbent replay-cancel (added v0.8 per L1 C6 finding)
pub const HANDOVER_ACK: [u8; 4] = *b"HOAK";
pub const HANDOVER_DONE: [u8; 4] = *b"HODN";
```

```rust
// HandoverCancelEnvelope (added v0.8 per L1 C6 finding; payload_hash + [u8;32] term_id reconciliation v0.9 per L3 H findings).
// Subtype `b"HORC"`. Incumbent's mid-handover replay-cancel envelope;
// recipient-side rule (lockout exception) references this subtype.
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct HandoverCancelEnvelope {
    pub envelope_type: [u8; 4],
    pub envelope_subtype: [u8; 4], // b"HORC"
    pub version: u16,
    pub mission_id: [u8; 16], // 16B BLAKE3-256(canonical_dcs(mission_did)) per RFC-0009 §Identity (corrected v0.8 per L1 C3 finding)
    pub coordinator_id: [u8; 32],
    pub coordinator_term_id: [u8; 32], // reconciled v0.9 per L3 H1 finding: matches HandoverEnvelope.coordinator_term_id type to provide a single term-id namespace; u64 was type/semantic mismatch with HandoverEnvelope [u8; 32] field
    pub cancel_reason_type_id: HandoverReasonTypeId,
    pub cancel_reason_payload: BoundedBytes<256>, // bounded to prevent unbounded-Vec DoS amplification (corrected v0.8 per L3 C1 finding)
    pub payload_hash: [u8; 32], // added v0.9 per L3 H2 finding; domain-prefixed v1.1 per W9 L3 C4 finding: BLAKE3-256("DOT/1/HANDOVER/cancel_payload" || canonical_dcs(cancel_reason_type_id_be: u32 || cancel_reason_payload)) — domain separation prevents collision with HORQ `payload_hash` (which uses "DOT/1/HANDOVER/payload") when an attacker submits a HORC with cross-namespace payload bytes; mirrors HandoverEnvelope.payload_hash fail-closed decode ordering (commitment check before signature verification)
    pub current_epoch_be: u64,
    pub nonce: [u8; 16],
    pub incumbent_signature: [u8; 64], // Ed25519 over all preceding fields including payload_hash
}

// Replay-key tuple: (b"HORC", mission_id, coordinator_id, coordinator_term_id, current_epoch_be, nonce)
// mission_id added v0.8 per L3 H4 finding: prevents cross-mission replay-key collision when same
// incumbent coordinator_id operates multiple missions concurrently.
// Encoding normalization (corrected v0.9 per L3 M finding): current_epoch_be is canonical BE bytes (RFC-0126 array-of-u8 form);
// u64 fields in replay tuples across HORQ/HOAK/HODN/HORC family use canonical BE bytes to prevent cross-subtype dedup collision.
```

### Layer-C Substrate Types (defined locally in 0855p-e)

```rust
/// Slash tally update: minimal BLAKE3-256 reference into the canonical tally.
/// Defined here (not re-exported from RFC-0855p-b) because RFC-0855p-b only mentions
/// the name in prose (not a `pub` item). This type shadows the prose-only name.
///
/// **TEMPORARY DEFINITION**: this local definition lives in Layer-C 0855p-e substrate
/// until the shared `octo-coordinator-types` crate lands (post-acceptance mission).
/// When that crate extracts, both `SlashTallyUpdate` and `SlashReasonCode` move to
/// the shared crate and `0855p-e` re-imports them as Layer-B types, mirroring the
/// `CoordinatorLifecycle` re-export pattern.
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SlashTallyUpdate {
    pub slash_tally_hash: [u8; 32],
}

/// Slash offense reason code (typed-discriminator, newtype over u32).
/// Defined here (not re-exported from RFC-0855p-b) because RFC-0855p-b only enumerates
/// reason codes in prose (not a `pub` item). This type shadows the prose-only name.
///
/// **TEMPORARY DEFINITION**: this local definition lives in Layer-C 0855p-e substrate
/// until the shared `octo-coordinator-types` crate lands (post-acceptance mission).
/// When that crate extracts, `SlashReasonCode` moves to the shared crate.
/// Wrapped in an opaque newtype so cross-namespace collisions with `HandoverReasonTypeId`
/// are caught at compile time (different types, not different values).
/// Allocations per RFC-0855p-b §B (with 0855p-e amendments noted):
///   0x0001-0x0009 = reserved (0855p-b §B)
///   0x000A        = PlatformMigration
///   0x000B        = is_reconnect_lie
///   0x000C-0x000D = RESERVED (NOT slash reasons; 0855p-b §"Sub-DC delegation protocol")
///   0x000E-0x0011 = 0850p-family
///   0x0012        = CrossPlatformWitnessCollusion
///   0x0013        = FalseAttestation         (0855p-b pending §B amendment — added by RFC-0855p-e)
///   0x0014        = QuorumTimeout            (0855p-b pending §B amendment — added by RFC-0855p-e)
///   0x0015        = TallyTamper              (0855p-b pending §B amendment — added by RFC-0855p-e)
///   0x0016        = LateDelivery             (0855p-b pending §B amendment — added by RFC-0855p-e; next free ID in 0x0013-0xFFFF slash-reason space)
///   0x0017-0xFFFF = reserved
#[derive(Dcs, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct SlashReasonCode(pub u32);

impl SlashReasonCode {
    pub const FalseAttestation: SlashReasonCode = SlashReasonCode(0x0013);
    pub const QuorumTimeout: SlashReasonCode = SlashReasonCode(0x0014);
    pub const TallyTamper: SlashReasonCode = SlashReasonCode(0x0015);
    pub const LateDelivery: SlashReasonCode = SlashReasonCode(0x0016);
}

/// Handover reason type ID (typed-discriminator, newtype over u32).
/// Cross-namespace-distinct from `SlashReasonCode` at compile time.
/// Allocations per §"Handover Reason Type IDs".
#[derive(Dcs, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct HandoverReasonTypeId(pub u32);

impl HandoverReasonTypeId {
    pub const Voluntary: HandoverReasonTypeId = HandoverReasonTypeId(0x0010);
    pub const Scheduled: HandoverReasonTypeId = HandoverReasonTypeId(0x0011);
    pub const MissionTerminated: HandoverReasonTypeId = HandoverReasonTypeId(0x0012);
    // 0x0013..0x0016 are allocated in RFC-0855p-b §B amendment for slash
    // reason codes (`FalseAttestation`, `QuorumTimeout`, `TallyTamper`,
    // `LateDelivery`). Distinct newtype from `SlashReasonCode` enforces
    // compile-time separation; the 0x0013..0x0016 slot is reserved here
    // per cross-RFC critical sequencing (see F-7 BLOCKING acceptance gate)
    // to prevent future collision if a third namespace lands at this
    // allocator.
    pub const _SlashRangeReserved: HandoverReasonTypeId = HandoverReasonTypeId(0x0013);
}
```

### Re-exports (split by substrate home)

```rust
// From RFC-0855p-b (Layer-B slash tally + lifecycle substrate).
// Per §Stable Abstractions Principle, slash tally canonical form lives in RFC-0855p-b.
// CoordinatorLifecycle IS a `pub` item in 0855p-b; Layer-C consumes via alias (matching the
// GroupBindingRef pattern below) so the Layer-B substrate home is explicit at the
// consumption site — direction rule A→B→C stays auditable. Adding `pub use` here would
// create two import paths (alias + re-export) and hide the substrate home.
pub use rfc_0855p_b::CoordinatorLifecycle;

// From RFC-0850p-c (Layer-B transport group binding ceremony).
// GroupBinding is defined in 0850p-c, NOT 0855p-b. Layer-C consumes Layer-B GroupBinding
// per direction rule A→B→C (Layer-C never redefines a Layer-B type).
pub use rfc_0850p_c::GroupBinding;
```

### Tally hash construction (used in `hodn_quorum` witness verification)

W witnesses compute `slash_tally_hash = BLAKE3-256(canonical_dcs(sorted_ballots))` over the same ballot list the incumbent referenced. Witnesses MUST verify the recomputed hash equals the HORQ `slash_tally_hash` before signing HOAK; mismatch → reject HORQ + flag incumbent (`SlashReasonCode::TallyTamper = 0x0015`).

Witness self-slash rule: a witness is slashed for a tally-attestation mismatch ONLY when the witness's own HOAK independently attests to a tally hash that the witness did not actually compute (i.e., the witness attested to a hash without verifying it). A witness that recomputes the hash, finds a mismatch with the incumbent's HORQ, and rejects the HORQ is NOT slashed — the witness acted correctly. The witness-side slash is per-witness attestation-validity logic owned by the slash-tally substrate (post-acceptance mission), not the handover envelope.

### Tally payload size bound

HORQ MUST NOT inline a full `SlashTallyUpdate` payload (only the BLAKE3 reference). The fixed-size `slash_tally_hash: [u8; 32]` field carries the reference inline; no separate `MAX_TALLY_BYTES` constant is required (the field is bounded by its type).

### State Machine

Mission Coordinator handover adds ONE state to `CoordinatorLifecycle` (RFC-0855p-b §"Data Structures" 8-state machine):

- `Handover` (between `Active` and `Inactive`): HORQ broadcast, awaiting HOAK quorum.

`HandoverComplete` is REMOVED — `Handover → Inactive` is the only successor-Active transition. There is exactly ONE terminal-for-term successor state, eliminating the two-terminal-state ambiguity.

Transitions:

- `Active → Handover`: signed HORQ broadcast by incumbent (sender_state_snapshot = Active verified by recipients).
- `Handover → Demoting`: witness quorum < 2/3 within `HANDOVER_TIMEOUT = 500 epochs` (matches RFC-0855p-b §"Handover Protocol" E2E IS-4.4 fix) → incumbent slashed with reason `SlashReasonCode::QuorumTimeout = 0x0014` (per RFC-0855p-b pending §B amendment).
- `Handover → Inactive`: witness quorum ≥ 2/3 + HODN broadcast → incumbent `Inactive`, successor `Active`.

### Witness Quorum

HORQ quorum: `HORQ_QUORUM = ceil(witness_set_size × HANDOVER_QUORUM_NUMERATOR / HANDOVER_QUORUM_DENOMINATOR)`. HODN quorum (successor's acceptance quorum): `HODN_QUORUM = ceil(successor_member_set_size × HANDOVER_QUORUM_NUMERATOR / HANDOVER_QUORUM_DENOMINATOR)`. Both quorums share the same NUMERATOR/DENOMINATOR constants. Single hostile witness cannot complete handover on either side. Minimum `witness_set_size = 3` and `successor_member_set_size = 3` (otherwise the 2/3 quorum is meaningless).

`hodn_quorum` defense-in-depth (v0.7, see const fn body): `assert!(successor_member_set_size >= 3)` mirrors the §Layer placement constants block witness quorum invariant. `horq_quorum` defense-in-depth (added v0.8 per L3 M4 finding; tightened v0.9 per L3 H3 finding): same `assert!(witness_set_size >= 3, "horq_quorum requires witness_set_size >= 3")` mirror at the HORQ receipt site; without it, bypass / legacy config path (witness_set_size = 1 or 2) yields `ceil(1×2/3)=1` or `ceil(2×2/3)=2` → single-witness handover completes. Substrate MUST invoke the const fn at the receipt site AND document runtime validation in the calling code (no OR alternative — hand-computed quorum paths bypass the assert entirely and enable single-witness handover; substrate lint / fixture test must reject hand-computed quorum paths at code review).

```rust
/// Witness quorum threshold numerator (e.g., 2 for 2/3 BFT).
pub const HANDOVER_QUORUM_NUMERATOR: u64 = 2;
/// Witness quorum threshold denominator (e.g., 3 for 2/3 BFT).
pub const HANDOVER_QUORUM_DENOMINATOR: u64 = 3;
// Re-export from RFC-0855p-d3 (canonical home for Layer-C quorum polling; v1.4 per W12 L2 M4 finding).
// Implementations may use `rfc_0855p_d3::hodn_quorum(witness_set_size)`; e-specific
// constant-numerator/denominator folding retained for HORQ-side mirror below.
pub use crate::rfc_0855p_d3::hodn_quorum;

/// Derivation helper: HORQ receipt-side mirror of `hodn_quorum` (added v0.8 per L3 M4 finding).
/// Returns `ceil(witness_set_size × HANDOVER_QUORUM_NUMERATOR / HANDOVER_QUORUM_DENOMINATOR)`.
/// Without the `assert!` mirror, a legacy / misconfigured mission with `witness_set_size < 3`
/// would yield `ceil(1×2/3)=1` or `ceil(2×2/3)=2` → single-witness handover completes,
/// bypassing the BFT invariant. Substrate MUST enforce the assert at the receipt site.
pub const fn horq_quorum(witness_set_size: usize) -> usize {
    assert!(witness_set_size >= 3, "horq_quorum requires witness_set_size >= 3");
    (witness_set_size * HANDOVER_QUORUM_NUMERATOR as usize + HANDOVER_QUORUM_DENOMINATOR as usize - 1) / HANDOVER_QUORUM_DENOMINATOR as usize
}
```

### Handover Race Resolution

If two HORQ broadcasts occur simultaneously (within `HANDOVER_RACE_WINDOW = 5 epochs`):

1. HORQ is broadcast BEFORE the local `Active → Handover` transition. The transition happens AFTER quorum-confirm (HOAK quorum met).
2. Compare `coordinator_id` lexicographically on the canonical tiebreak form: `tiebreak_digest = BLAKE3_keyed("DOT/1/HANDOVER/coordinator_id_lex", coordinator_id)`, then lex-compare the 32-byte digests. The keyed form prevents cross-implementer disagreement between raw-byte and DCS-encoded lex inputs. Recipients compute the same digest and observe the same winner.
3. Lower `tiebreak_digest` wins; higher loses.
4. Loser's HORQ silently dropped; no slash for race (deterministic, not misbehavior). Loser's local state stays `Active` (no rollback needed because the local transition happens after quorum-confirm, not at broadcast time).
5. Guardrails: race rejection MUST NOT modify the incumbent's slash tally and MUST NOT publish any `GroupBinding` change. Witnesses observe the winner's HORQ, not the loser's; the loser's state stays `Active` with no tally/grouping side effects.

### Replay Protection

Every HORQ/HOAK/HODN includes `nonce: [u8; 16]` + `current_epoch: u64`. Each recipient verifies:

- `nonce` not seen for `(coordinator_id, mission_id, coordinator_term_id)` tuple in last `HORQ_BACKWARD_WINDOW = 5 epochs` (consistent with §Layer placement constants block; replaces phantom `HANDOVER_REPLAY_WINDOW = 1000`).
- `current_epoch` within ±`MAX_FSKEW_EPOCHS = 4` of local epoch (consistent with §Layer placement constants block; replaces v0.x ±1 wording that conflicted with the family-wide `MAX_FSKEW_EPOCHS = 4` bound).
- `payload_hash == BLAKE3-256("DOT/1/HANDOVER/payload" || canonical_dcs(payload_bytes))` (wire-payload binding, domain-separated).
- HOAK: when `attests_to_predecessor_state=true`, `attested_epoch == HORQ.current_epoch - 1` (added v1.2 per W11 L3 M-fbat finding — closing the documented invariant at the acceptance site).
- HOAK second-witness quorum gate (added v1.2 per W12 L3 M1 finding — closes control gap): when accepting a HORQ whose `sender_state_snapshot_ordinal != SenderStateSnapshotOrdinal::Active`, recipients MUST count distinct `attests_to_predecessor_state=true` HOAK signatures per `(coordinator_id, coordinator_term_id, current_epoch)` and reject unless the count reaches `horq_quorum(witness_set_size)` (the HORQ-side mirror constant per §Layer placement). Without this gate, a single forged predecessor-state HOAK would bootstrap the non-canonical path with no consensus. Distinct-signer enforcement reuses `signers_bitmap.count_ones() == attestations.len()` invariant per RFC-0855p-d3 §Aggregate Acceptance Step 9.

### Slash Tally Carry-Over

HORQ MUST reference the canonical slash tally via `slash_tally_hash: [u8; 32]` per RFC-0855p-b §"Slash tally cryptographic finality" (plain `BLAKE3-256(canonical_dcs(sorted_ballots))`). Successor inherits the tally by fetching the canonical tally at `(mission_id, coordinator_term_id, current_epoch)` using the hash reference. Witnesses recompute + verify the hash matches before HOAK. Fetch at `current_epoch` (not `current_epoch − lookback`) ensures slashes issued in the immediate-prior epochs are carried over — successor inherits the complete tally history. (Removed in v0.6: `HANDOVER_TALLY_WINDOW_EPOCHS` lookback was deprecated; the canonical tally is append-only at `current_epoch`, no lookback window.)

### Slash tally canonical form

Per RFC-0855p-b §"Slash tally cryptographic finality": tally = BLAKE3 of sorted ballot list. Witness recomputes + verifies before signing HOAK. Mismatch → reject HORQ + flag incumbent (reason `SlashReasonCode::TallyTamper = 0x0015` per RFC-0855p-b pending §B amendment).

## RFC-0008 Execution Class Mapping

(Added v1.1 per W10 L5 C1 finding — mandatory BLUEPRINT §RFC Process template sub-section.)

| Operation                                                    | Class | Justification                                                                                    |
| ------------------------------------------------------------ | ----- | ------------------------------------------------------------------------------------------------ |
| HORQ (HandoverRequest) envelope derive + recipient accept    | A     | BLAKE3 keyed_hash derivation + domain separation + Ed25519 sig; deterministic; no external state |
| HOAK (HandoverAck) envelope verify                           | A     | Witness Ed25519 sig + ack_hash + attested_epoch check; deterministic                             |
| HODN (HandoverDone) envelope verify                          | A     | Witness-set Ed25519 sig aggregation + done_hash + hodn_quorum check; deterministic               |
| HORC (HandoverCancel) envelope verify                        | A     | Incumbent Ed25519 sig + payload_hash domain check + lockout status check; deterministic          |
| Race resolution (lex tiebreak + broadcast-before-transition) | C     | Consensus-aggregated state; depends on quorum; non-deterministic under partition                 |
| Quorum check (`hodn_quorum` defense-in-depth)                | C     | Coordinator-side governance policy                                                               |
| Slash tally carry-over                                       | C     | Slash substrate is Layer C; depends on RFC-0855p-b slash tally state                             |
| Pending envelope transfer (mesh fanout)                      | C     | Cross-node coordination; non-deterministic under network partition                               |

## Determinism Requirements

(Added v1.1 per W10 L5 H1 finding — mandatory BLUEPRINT §RFC Process template sub-section.)

- **Race-resolution determinism**: lex `coordinator_id` tiebreak via BLAKE3_keyed("DOT/1/HANDOVER/coordinator_id_lex", coordinator_id) canonical form. Loser's HORQ silently dropped; no slash for race. Same input → same output across all recipients.
- **Replay-key tuple encoding**: all `u64` and `current_epoch` fields in HORQ/HOAK/HODN/HORC replay tuples use canonical BE bytes (RFC-0126 array-of-u8 form). Prevents cross-subtype dedup collision.
- **Domain separation**: each BLAKE3 commitment uses a distinct domain prefix (`"DOT/1/HANDOVER/payload"`, `"DOT/1/HANDOVER/ack"`, `"DOT/1/HANDOVER/done"`, `"DOT/1/HANDOVER/pending"`, `"DOT/1/HANDOVER/new_term"`, `"DOT/1/HANDOVER/cancel_payload"`, `"DOT/1/HANDOVER/snapshot"`). Prevents cross-field concatenation collisions.
- **Epoch monotonicity**: recipients reject envelopes where `current_epoch > local_epoch + MAX_FSKEW_EPOCHS` (forward tolerance window; aligned to `MAX_FSKEW_EPOCHS = 4` per RFC-0855p-d1 §Layer placement table; handover-specific lockout carve-out for the incumbent is bounded separately by `HANDOVER_RACE_WINDOW` in §Handover Race Resolution, NOT added to the forward-skew bound).
- **Cross-replica determinism**: given identical observed envelope sequences, every replica reaches the same coordinator term transition (subject to consensus-aggregated witness quorum).
- **Canonical HORQ ordering**: HORQ broadcast ordering follows `(current_epoch_be, coordinator_id_lex)`. Two recipients observing the same set of HORQ broadcasts reach identical race-resolution decisions.

## Lifecycle Requirements

(Added v1.1 per W10 L5 H2 finding — mandatory BLUEPRINT §RFC Process template sub-section.)

| Constant                            | Value                                | Purpose                                                                                                                                                                                                                                  |
| ----------------------------------- | ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `HANDOVER_TIMEOUT`                  | 500 epochs                           | HORQ→HODN completion deadline (matches RFC-0855p-b); mismatch escalates to slash                                                                                                                                                         |
| `HANDOVER_RACE_WINDOW`              | 5 epochs                             | (added v1.1 per W10 L5 H3 — previously prose-only) — Concurrent HORQ collision window; lex tiebreak applied within window                                                                                                                |
| `HANDOVER_REPLAY_WINDOW`            | 1000 epochs                          | Recipient dedup window; envelopes older than this may be safely discarded                                                                                                                                                                |
| `MAX_FSKEW_EPOCHS`                  | 4                                    | Forward epoch-skew tolerance for envelope acceptance                                                                                                                                                                                     |
| `MAX_PENDING_ENVELOPES_PER_HODN`    | 1024                                 | (added v1.1 per W10 L3 C5) — bound on pending envelope count transferred per HODN                                                                                                                                                        |
| `MAX_GROUP_BINDINGS_PER_HANDOVER`   | 4096                                 | Bound on GroupBinding set transferred per HODN                                                                                                                                                                                           |
| `HORC_QUORUM_THRESHOLD`             | `ceil(witness_set_size × 2/3)` HOAKs | Second-witness predecessor-state attestation threshold (raised v1.2 per W11 L3 H3 finding — single-HOAK threshold permits single-witness forged `attests_to_predecessor_state` to bypass canonical 2/3 invariant for the exception path) |
| `HODN_QUORUM_NUMERATOR/DENOMINATOR` | 2/3                                  | Witness quorum threshold for HODN acceptance                                                                                                                                                                                             |

State machine lifecycle: `Active → Handover → (successor Active | Incumbent Demoting)` per RFC-0855p-b. HORQ broadcasts enter `Handover`; HODN quorum-confirm transitions successor to `Active` and incumbent to `Inactive` (or `Demoting` if quorum fails). Lockout scope: incumbent locked from state-changing envelopes after HORQ broadcast; lockout transfers to successor post-HODN.

## Performance Targets

- HORQ→HODN median latency: < 500 epochs (witness quorum dependent).
- Slash tally aggregation time: < 50 epochs per witness.
- Witness attestation timeout: 500 epochs (`HANDOVER_TIMEOUT`, matches RFC-0855p-b §"Handover Protocol").
- Replay window check: O(log N) per recipient (binary search on nonce log).

## Compatibility

Backward compat with non-handover-aware clients: After HODN, OLD clients continue to recognize the incumbent (no state regression); NEW clients recognize the successor. This acceptable divergence is documented: old clients see incumbent Active; new clients see successor Active. Acceptable per RFC-0855p-b §"Election Algorithm (per governance model)" fallback path. Legacy followers without handover support drop with clear error per the same fallback path.

**Migration period**: there is a single canonical view per `CoordinatorLifecycle` for the handover epoch — NEW clients converge on the successor once HODN is quorum-confirmed. The OLD/NEW divergence window is bounded by `HANDOVER_REPLAY_WINDOW = 1000 epochs` after HODN; clients are expected to upgrade before this window closes. After the migration window expires, only the canonical successor-state view is propagated, and clients that have not upgraded lose visibility into the mission (they must either upgrade or drop). This bounded-window amplification guardrail prevents minority client sets from sustaining divergent views indefinitely.

**Migration safety invariant** (added v0.6): after HODN quorum-confirm, no NEW envelope of any kind (state-changing, aggregate, route) MUST be accepted against the incumbent's `coordinator_id` for any state mutation, regardless of client view, until the OLD/NEW divergence window closes. A stale view held by an OLD client cannot be quoted as ground truth by an attacker to push post-handover state changes under the incumbent's identity; recipients MUST refuse such envelopes on incumbent `coordinator_id` regardless of the attacker's claimed fork preference. This is a recipient MUST-rule for the migration window.

## Test Vectors

### TV-HO-1: Valid Mission Coordinator handover with 3 witnesses

- Mission: `mission_id = 0x00...01`.
- Incumbent: `coordinator_id = 0xAA...AA`, `coordinator_term_id = 0x00...01`, `handover_reason_type_id = 0x0010` (Voluntary), `coordinator_role_type_id = 0x0100` (Mission Coordinator).
- Successor: `successor_id = 0xBB...BB` (election-eligible per RFC-0855p-b §"Election Algorithm (per governance model)").
- `new_term_id = BLAKE3-256("DOT/1/HANDOVER/new_term" || mission_id || coordinator_term_id || successor_id || current_epoch_be)` = `BLAKE3(0x00...01 || mission_id || 0x00...01 || 0xBB...BB || 0x00...00 0x00 0x03 E8)` (epoch 1000; mission_id field is the canonical 16-byte truncated BLAKE3-256 of the mission DID per RFC-0009 §Identity, NOT a hash of coordinator_id — including `mission_id` prevents cross-mission term-id collision when same coordinator / successor pair operates multiple missions concurrently).
- Witness set: `{0xC1...C1, 0xC2...C2, 0xC3...C3}`, size = 3, HORQ_QUORUM = 2.
- HORQ signed by incumbent at epoch 1000, sender_state_snapshot_ordinal = 1 (`Active` per §"Sender State Snapshot Ordinals").
- HOAK signed by `0xC1...C1` at epoch 1001 + by `0xC2...C2` at epoch 1002 (2/3 quorum met).
- HODN signed by successor `0xBB...BB` at epoch 1003, new_coordinator_id = 0xBB...BB verified equal to HORQ.successor_id.
- Result: incumbent → `Inactive`, successor → `Active` at epoch 1003.

### TV-HO-2: ACK-with-slash-tally carry-over (fetch at HORQ `current_epoch`, no lookback — `HANDOVER_TALLY_WINDOW_EPOCHS` deprecated v0.6 per L3 M1 finding; v0.7/v0.8 lookback wording drift corrected L1 C4)

- Slash tally: 2 historical slashes (`SlashReasonCode::LateDelivery` × 2) for incumbent.
- HORQ includes `slash_tally_hash = BLAKE3(canonical_sorted_ballots)` referencing canonical tally per RFC-0855p-b.
- Witnesses fetch the canonical tally by `(mission_id, coordinator_term_id, current_epoch)`, verify BLAKE3 of sorted ballots matches; both HOAK with tally attestation.
- Successor inherits 2-slash history on `Active` transition.

### TV-HO-3: HODN envelope with new_coordinator_id

- Same setup as TV-HO-1 through HOAK quorum.
- HODN payload: `handover_request_hash = BLAKE3-256(DCS(HandoverEnvelope { signature: [0u8; 64], ..envelope }))` (signature zeroed before hashing), `new_coordinator_id = 0xBB...BB`, `accepted_epoch = 1003`.
- `done_hash = BLAKE3-256("DOT/1/HANDOVER/done" || handover_request_hash || new_coordinator_id || accepted_epoch_be)` (domain-prefixed per §BLAKE3 Construction Conventions; prevents collision with `payload_hash` / `ack_hash` / `pending_envelopes_hash` / `new_term_id` Merkle-leaf-style commitments).
- Signed by successor's Ed25519 key.

### TV-HO-4: Race-rejection (two HORQ within race window)

- Two HORQ broadcasts at epoch 1000 from `coordinator_id_A = 0xAA...AA` and `coordinator_id_B = 0xBB...BB` (within `HANDOVER_RACE_WINDOW = 5 epochs`).
- Both broadcasts happen with sender_state_snapshot = Active, BEFORE any local `Active → Handover` transition.
- Lex comparison: `0xAA < 0xBB` → A wins.
- B's HORQ silently dropped; no local transition for B; no tally change; no GroupBinding publication.
- A transitions to `Handover` only after HOAK quorum confirms A's HORQ.
- No slash for B (race is deterministic, not misbehavior).

### TV-HO-5: Invalid-successor-pubkey rejection

- Successor `successor_id = 0xDE...AD` NOT election-eligible per RFC-0855p-b §"Election Algorithm (per governance model)" (fails stake threshold).
- HORQ broadcast; recipient's local eligibility check rejects.
- No state transition; incumbent stays `Active`.
- Rejection logged; no slash (recipient-local check, not network consensus).

### TV-HO-6: Replay rejection (nonce seen)

- HORQ with `nonce = 0x00...01` at epoch 1000, `coordinator_term_id = 0x00...01`.
- Same HORQ replayed at epoch 1100 (within `HANDOVER_REPLAY_WINDOW = 1000 epochs`).
- Recipient dedup log catches `(mission_id = 0x00...01, coordinator_term_id = 0x00...01, nonce = 0x00...01)` already seen.
- Replay silently dropped.

### TV-HO-7: payload_hash domain separation

- HORQ payload bytes = canonical DCS of HandoverPayload.
- `payload_hash = BLAKE3-256("DOT/1/HANDOVER/payload" || canonical_dcs(payload_bytes))`.
- Verifier recomputes with domain string prepended; mismatch (e.g., hash computed without domain) → reject.

### TV-HO-8: sender_state_snapshot non-Active rejection

- HORQ with `sender_state_snapshot_ordinal = 2` (`Handover` per §"Sender State Snapshot Ordinals"; incumbent already mid-handover).
- Recipients reject because consensus-aggregated view shows sender state ≠ Active.
- No state transition; no slash (recipient-local check).

### TV-HO-9: HOAK second-witness quorum gate

- HORQ with `sender_state_snapshot_ordinal != SenderStateSnapshotOrdinal::Active` (e.g. `Suspect = 5`).
- `witness_set_size = 3`; `horq_quorum(3) = ceil(3 × 2/3) = 2`.
- 1 second-witness HOAK with `attests_to_predecessor_state = true` arrives; `count(attests_to_predecessor_state=true) = 1 < horq_quorum(3) = 2`.
- Recipient rejects under the HOAK second-witness quorum gate (added v1.2 per W12 L3 M1 finding — closes control gap from §Race-resolution determinism).
- 2 distinct second-witness HOAKs (distinct `witness_id`, same `attested_epoch`) → `count = 2 >= horq_quorum(3) = 2`; gate passes; HORQ proceeds through normal acceptance (BLAKE3 verify + signature checks + nonce-unconsumed).

## Alternatives Considered

- **Rotate-by-epoch** — rejected: no operator agency, removes mission-specific expertise.
- **Election-by-stake** — rejected: handover ≠ election; RFC-0855p-b covers election; handover is post-election transition.
- **Handover-by-request** — current (HORQ/HOAK/HODN): explicit handshake with witness attestation + slash tally carry-over.
- **Emergency-takeover** — rejected: emergency is `Demoting` path per RFC-0855p-b, not handover.
- **Parent-override** — N/A: Mission Coordinator has no parent (DomainCoordinator parent → child is platform-mediated via RFC-0855p-c §4).
- **Single-ack handshake** — rejected: quorum needs explicit collection across witnesses, not single-witness claim.

## Implementation Phases

- **Phase 1**: HORQ + HOAK + HODN wire envelope + DCS derive; outer `HandoverEnvelope` + inner `HandoverPayload`; race resolution (lex tiebreak + broadcast-before-transition ordering); replay protection (nonce log + epoch tolerance + domain-separated payload_hash); successor eligibility verification (recipient-local check); `sender_state_snapshot_ordinal` rejection; HODN `new_coordinator_id == successor_id` Recipient Verification; `new_term_id` / `pending_envelopes_hash` / `handover_request_hash` derivation rules; `hodn_quorum` const fn (with `assert!` defense-in-depth).
- **Phase 2**: HODN + state machine (`Handover` added to `CoordinatorLifecycle`; `HandoverComplete` removed).
- **Phase 3**: Slash-tally carry-over via `slash_tally_hash` reference at `current_epoch` (no lookback) + witness quorum validator (`SlashTallyUpdate` + `SlashReasonCode` local types in Layer-C substrate, scheduled to lift into shared `octo-coordinator-types` crate post-acceptance per F-7 critical-sequencing note).
- **Phase 4**: Migration tooling + substrate test vectors (state-machine unit tests + quorum boundary tests + slash-tally carry-over integration tests).
- **Phase 5**: Operator surface — `octo coordinator handover status` + `octo coordinator handover history` CLI subcommands (per RFC-0011 substrate, amendment chain); audit log surface for handover events (per RFC-0011-a audit subcommands); observability hooks (Prometheus metrics: `handover_inflight_count`, `handover_quorum_age_epochs`, `handover_pending_envelopes_carried`). Audit/CLI/observability is Phase 5, not Phase 4 — Phase 4 is substrate-only. (Phase numbering corrected v0.8 per L5 C1 finding: prior versions duplicated Phase 3.)

## Key Files to Modify

- `crates/octo-network/src/dot/handover.rs` — `HandoverEnvelope` + `HandoverPayload` + alias `CoordinatorLifecycle` / `GroupBindingRef` + typed-discriminator `handover_reason_type_id` + BLAKE3-keyed lex tiebreak; state machine integration (`Handover` state added; `HandoverComplete` removed); quorum + race resolution + `sender_state_snapshot` verification + incumbent-HORQ-in-flight lockout (per §Security Considerations v0.6 addition); `slash_tally_hash` reference + integration with RFC-0855p-b substrate; local `SlashTallyUpdate` + `SlashReasonCode` + `HandoverReasonTypeId` types. (Substrate-truth: all handover types currently live in one module; module-level split into `handover_state.rs` / `coordinator_handover.rs` / `slash_tally_carryover.rs` is a follow-on refactor when substrate complexity justifies it.)
- `missions/archived/0855p-e-handover-request-envelope.md` (companion mission — Status: Completed).

## Economic Analysis

- **Slash tally carry-over** — slashes inherit to successor; successor's reputation reflects historical misbehavior. Slashes do not expire on handover (per RFC-0855p-b §"Slashing Integration").
- **Witness incentives** — witnesses that HOAK a bad handover are themselves slashed with reason `SlashReasonCode::FalseAttestation = 0x0013` (per RFC-0855p-b pending §B amendment) for false attestation. Witnesses vote but receive NO automatic reward per RFC-0855p-b §"Slashing Integration". If reward sharing is desired for honest witnesses, defer to RFC-0917 / RFC-0960.
- **Dual-stake model reference** — Mission Coordinator term handover inherits the dual-stake model from `docs/04-tokenomics/token-design.md` §10: incumbent's `OCTO + role` stake is locked for the duration of the term + `HANDOVER_REPLAY_WINDOW`; successor's `OCTO + role` stake locks on `Active` transition for `new_term_id`. Single-stake carve-outs (e.g., recorder / wallet per RFC-0011-d §7.5 footnote) do not apply — Mission Coordinator is dual-stake by construction (W6 L5 L2 fix: added v0.8 dual-stake reference per BLUEPRINT §Economic Analysis template line 680 requirement).
- **Stake locking** — incumbent's OCTO-O stake remains locked until HODN or `Demoting` transition (per RFC-0855p-b §"Slashing Integration"). Successor's stake locked on `Active` transition for `new_term_id`.

## Future Work

- F-1: Slash tally serialization — already specified inline via local `SlashTallyUpdate` type in §"Layer-C Substrate Types".
- F-2: Group binding transfer — specified in §"State Machine".
- F-3: Handover race handling — specified in §"Handover Race Resolution".
- F-4: Handover revocation — `Demoting` path per RFC-0855p-b covers this.
- F-5: Witness ACK aggregation — specified in §"Witness Quorum".
- F-6: Pending envelope transfer — F-2 covers via `pending_envelopes_hash`.
- F-7: Slash tally amendment — 0855p-b pending §B amendment entries (`FalseAttestation 0x0013` / `QuorumTimeout 0x0014` / `TallyTamper 0x0015` / `LateDelivery 0x0016`) defined here; 0855p-b §B will be amended to add the entries. **BLOCKING ACCEPTANCE GATE:** this RFC MUST NOT be promoted to Accepted until EITHER (a) RFC-0855p-b §B amendment merges with the four `SlashReasonCode` entries above, OR (b) `SlashReasonCode` + `HandoverReasonTypeId` lift into a shared `octo-coordinator-types` crate shipping in the same release. Without one of these, slash-tally substrate recipients reading 0855p-b see 0x0013..0x0016 as unallocated codes and silently drop slashes, enabling incumbent-forced false HOAK quorum with no penalty. Acceptance precondition checked at promotion time per BLUEPRINT §RFC Acceptance Process.
- F-8: User-extension coordinator roles — Capability Coordinator / Reputation Coordinator / Market Coordinator register new `coordinator_role_type_id` values in `0x0200-0xFFFF` per the §"Coordinator Role Type ID namespace" table.

## Rationale

- **Why 3-envelope handshake (HORQ→HOAK→HODN) vs single ack**: quorum needs explicit collection across witnesses, not single-witness claim. Three stages separate incumbent initiation, witness attestation, and successor acceptance.
- **Why local SlashTallyUpdate + SlashReasonCode types (not re-export from 0855p-b)**: 0855p-b mentions these names in prose only; they are NOT `pub` items in 0855p-b substrate. Defining them locally in 0855p-e §"Layer-C Substrate Types" gives a concrete Layer-C substrate home; future `octo-coordinator-types` crate can lift these into a shared crate between 0855p-b and 0855p-e.
- **Why typed-discriminator over enum**: per §Extension over enumeration, future coordinator roles (Capability Coordinator, Reputation Coordinator, Market Coordinator) land via registry without central enum edit. `coordinator_role_type_id: u32` references the RFC-allocated namespace table (§"Coordinator Role Type ID namespace"); unknown / unregistered type_ids fail-closed. The same pattern is used for `handover_reason_type_id` (replacing the closed `HandoverReason` enum).
- **Why HANDOVER_TIMEOUT = 500 epochs (not 100)**: matches RFC-0855p-b §"Handover Protocol" E2E IS-4.4 fix; cross-document consistency prevents incumbent-slashing bugs at the boundary.
- **Why slash_tally_hash reference (not inline tally)**: inlining a full sorted-ballots tally bloats HORQ envelopes (DoS vector); a BLAKE3 reference with witness-side re-computation is the canonical pattern per RFC-0855p-b §"Slash tally cryptographic finality".
- **Why broadcast-before-transition for race resolution**: incumbent's local `Active → Handover` transition happens AFTER HOAK quorum-confirm, so the loser cleanly stays `Active` with no rollback needed. Race rejection MUST NOT modify the incumbent's slash tally and MUST NOT publish any `GroupBinding` change.
- **Why GroupBinding re-export from 0850p-c (not 0855p-b)**: GroupBinding is defined in RFC-0850p-c (Transport Group Binding Ceremony), not RFC-0855p-b. Layer-C consumes Layer-B GroupBinding per direction rule A→B→C; Layer-C never redefines a Layer-B type.
- **Why Mission Coordinator only**: DomainCoordinator has platform-mediated handover per RFC-0855p-c §4 (`PlatformEvent::AdminTransfer` via WhatsApp `participant promote`). Mixing the two paths in one envelope would conflate transport-layer handover (this RFC) with platform-mediated handover (RFC-0855p-c).
- **Why witness quorum ≥ 2/3**: BFT threshold; tolerates 1/3 malicious witnesses. Single-witness ack is too weak (1 hostile witness completes handover); 100% quorum is too strong (1 offline witness blocks handover permanently).
- **Why lex coordinator_id tiebreak for race**: deterministic, no external randomness, no leader election. Same input → same output across all recipients.

## Version History

| Version | Date       | Changes                                                                                         |
| ------- | ---------- | ----------------------------------------------------------------------------------------------- |
| 0.1     | 2026-06-17 | Initial stub. See `docs/research/2026-09-02-0855p-e-vh-fix-log.md` §v0.1.                       |
| 0.2     | 2026-06-17 | 10-byte header + inline SlashTally/CoordinatorRole + phantom removal..                          |
| 0.3     | 2026-09-01 | Strip preliminary + DCS wire/payload split + quorum≥2/3..                                       |
| 0.4     | 2026-09-01 | Slash codes + role ns 0x0100 + HORQ pre-broadcast + typed-discriminator..                       |
| 0.5     | 2026-09-01 | Domain-separated hashes + newtypes + 0x0016 LateDelivery + BLAKE3 conventions..                 |
| 0.6     | 2026-09-02 | Header consts + F-7 sequencing + lockout rule + Merkle pending..                                |
| 0.7     | 2026-09-02 | F-7 BLOCKING + mission_id BLAKE3 + snapshot_proof + pending fanout..                            |
| 0.8     | 2026-09-02 | mission_id 32→16B + HORC envelope + sender_state_snapshot newtype..                             |
| 0.9     | 2026-09-02 | HORC bounded payload + replay tuple + lockout carve-out enumerates..                            |
| 1.0     | 2026-09-02 | MAX_FSKEW_EPOCHS pub const + pub use + HORC payload_hash + OR→AND..                             |
| 1.1     | 2026-09-02 | HORQ snapshot canonical path + HORC payload domain prefix + MAX_PENDING_ENVELOPES_PER_HODN..    |
| 1.2     | 2026-09-02 | attests_to_predecessor_state + HANDOVER_RACE_WINDOW const + 3 BLUEPRINT template sub-sections.. |

## Appendices

(Added v1.2 per W11 L5 H4 finding — mandatory BLUEPRINT §RFC Process template sub-section.)

### A. Sender State Snapshot Ordinal mapping

Wire-stable ordinal mapping for `SenderStateSnapshotOrdinal` (v1.2 field made private per W11 L1 H2 finding):

| Ordinal | Lifecycle state    |
| ------- | ------------------ |
| 0       | Reserved (invalid) |
| 1       | Active             |
| 2       | Handover           |
| 3       | Demoting           |
| 4       | Inactive           |
| 5       | Suspect            |
| 6       | Rotating           |
| 7       | Demoted            |
| 8       | Revoked            |

### B. Handover envelope subtype cross-reference

| Subtype tag | Envelope name   | Signer              |
| ----------- | --------------- | ------------------- |
| `b"HORQ"`   | HandoverRequest | Incumbent           |
| `b"HOAK"`   | HandoverAck     | Witness             |
| `b"HODN"`   | HandoverDone    | Successor           |
| `b"HORC"`   | HandoverCancel  | Incumbent (lockout) |

## Related RFCs

- RFC-0850 — Deterministic Overlay Transport
- RFC-0855p-b — Mission Coordinator Lifecycle (slash tally observability §"Slash tally observability", Slash Offense Codes §B, CoordinatorLifecycle §"Data Structures")
- RFC-0855p-c — DomainCoordinator Role (excluded scope — platform-mediated handover §4)
- RFC-0850p-c — Transport Group Binding Ceremony
- RFC-0008 — Slash Offense Code registry (slash_reason_code space 0x0001-0xFFFF)
- RFC-0009 — Identity substrate (`mission_id` truncation to 16-byte BLAKE3-256(mission_did) per §Identity)

## Related Use Cases

- `docs/use-cases/mission-coordinator-lifecycle.md` — Coordinator Handover
- `docs/research/networking-rfc-cross-reference-analysis.md` — Scenario family S-C4
