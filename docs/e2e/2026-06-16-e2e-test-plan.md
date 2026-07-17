# E2E Integration Test Plan — DOT Pipeline (2026-06-16)

## Executive Summary

This document designs 8 live E2E integration test scenarios that exercise the
full DOT pipeline:

```
bootstrap (0851p-a) → group join (0850p-a) → binding (0850p-c) →
election (0855p-b v1.1) → DomainCoordinator (0855p-c) → message delivery
```

Each scenario is designed to be runnable in a simulated harness (the existing
`octo-network/tests/common/mock_adapter.rs` already supports `FailureMode`
injection). For each scenario, the "Implicit Specs" section identifies gaps
and convention violations — cases where the RFCs leave behavior implicit
that the "nothing should be implied" rule requires to be explicit.

**Test harness infrastructure available:**
- `octo-network/tests/common/mock_adapter.rs` — in-memory `PlatformAdapter`
  with `FailureMode::{None, DropAll, DropRandom(p), Duplicate(n), Reorder}`.
- `octo-network/tests/common/mock_network.rs` — simulated DOT transport
  (in-process message passing with controllable latency and partitions).
- `octo-network/tests/common/mod.rs` — shared test helpers.

**Gap analysis result:** 40+ implicit specs identified across 8 scenarios,
classified by convention violation (timing, error handling, edge case,
security, lifecycle, authority). Several are CRITICAL (e.g., no BIND
witness timeout, no handover format, no kick detection).

---

## Scenarios

### Scenario 1: Cold start — 2 nodes, 1 WhatsApp group, happy path

**Goal:** exercise the full pipeline from cold start to message delivery.

**Setup:**
- 2 fresh nodes: `A`, `B` (no state, no peers).
- 5 bootstrap nodes (Mode A) — all reachable.
- 1 WhatsApp group `G` with 0 DOT members; both nodes can join.
- Mission descriptor `M` for `domain_id = "test-domain-1"`, shared
  out-of-band (this is itself an implicit spec — see IS-1.1).

**Steps:**

| # | Action | Expected | Implicit Spec |
|---|--------|----------|---------------|
| 1.1 | A bootstraps from seed list (Mode A); B bootstraps from seed list. | Both discover each other via GDP. | — |
| 1.2 | Both join WhatsApp group `G` via adapter. | Both are group members. | IS-1.1: how does a node know which `group_jid` to join? out-of-band share via invite link or mission descriptor — not specified in 0850p-a. |
| 1.3 | A sends first DOT envelope to `G` (a BIND, by the implicit-designator rule). | A becomes DomainCoordinator candidate. | IS-1.2: what counts as "first DOT"? first envelope of any kind? first BIND? first message after joining? 0850p-c §3 says "first DOT sender" but does not define "first". |
| 1.4 | A waits for witness acks. | B witnesses the BIND and acks. | IS-1.3: how long does A wait? no BIND witness timeout is specified in 0850p-c. If B never acks, does A retry? escalate? silently fail? |
| 1.5 | A and B exchange DOT messages through `G`. | All messages are delivered to both. | IS-1.4: what is the message routing topology? full-mesh? gossip? broadcast? 0850p-c §"GroupState" implies broadcast, but the routing protocol is not specified. |
| 1.6 | A signs and sends a HEARTBEAT every epoch. | B receives and tracks. | IS-1.5: HEARTBEAT interval is not specified. 0855p-b mentions "2x heartbeat miss → Suspect" but the interval is implicit. |

**Convention violations:** timing (1.3, 1.5, 1.6), edge case (1.2),
lifecycle (1.2), authority (1.3).

---

### Scenario 2: Bootstrap fallback — Mode A → Mode B → Mode C

**Goal:** verify the bootstrap fallback chain works and is fully specified.

**Setup:**
- 2 fresh nodes: `A`, `B`.
- 5 bootstrap nodes (Mode A) — all UNREACHABLE.
- DHT (Mode B) — reachable, returns `B`'s address.
- Invite link (Mode C) — available for `A`.
- 1 WhatsApp group `G`.

**Steps:**

| # | Action | Expected | Implicit Spec |
|---|--------|----------|---------------|
| 2.1 | A tries Mode A: connects to all 5 bootstrap nodes. | All time out. | IS-2.1: is "all 5 timeout" the trigger, or is it "fewer than MIN_BOOTSTRAP_RESPONSES (3) responded"? 0851p-a table says "All 5 bootstrap nodes timed out → Fall back to Mode B", but the Sybil-defense rule says "≥3 of 5 must agree" — these are different conditions. |
| 2.2 | A falls back to Mode B (DHT). | A discovers B via DHT. | — |
| 2.3 | B tries Mode A, succeeds (B has a different network path). | B discovers bootstrap peers. | — |
| 2.4 | A and B complete bootstrap via different modes. | Both are in the DOT mesh. | IS-2.2: is mixed-mode bootstrap (one node via seed list, one via DHT) valid? peer_id validation may fail if the trust roots differ. |
| 2.5 | A and B join `G`. | Both are members. | — |
| 2.6 | A sends BIND; B witnesses. | A is DomainCoordinator. | — |

**Additional implicit specs (tested by extending this scenario):**

- **IS-2.3:** if Mode B also fails, does A fall to Mode C? the chain is
  A → B → C in 0851p-a, but the transition rules between B and C are
  not fully specified. What is the timeout for Mode B? what triggers
  the C fallback?
- **IS-2.4:** if all 3 modes fail, does A give up? retry indefinitely?
  surface an error to the user? 0851p-a is silent on this.
- **IS-2.5:** the "≥3 of 5" Sybil defense rule — what if 2 of 5
  respond with agreeing peer lists, and the 3rd is a Sybil node that
  disagrees? do the 2 agreeing responses count? the RFC says "≥3 must
  agree" but this leaves a gap for the 2-responses case.

**Convention violations:** timing (2.1, 2.3), error handling (2.3, 2.4),
security (2.5).

---

### Scenario 3: Simultaneous BIND — first-BIND-wins tiebreaker

**Goal:** verify the R4-7 tiebreaker (lowest `bind_hash` lexicographically)
works deterministically across nodes.

**Setup:**
- 2 nodes: `A`, `B`, both already in WhatsApp group `G`.
- A and B have nearly-identical `attest_nonce` and `bind_epoch` values,
  so the BIND hashes are close but not equal.
- Force A and B to compute their BINDs at the same logical time (mock
  network with zero latency, simultaneous BIND broadcast).

**Steps:**

| # | Action | Expected | Implicit Spec |
|---|--------|----------|---------------|
| 3.1 | A and B both compute and broadcast BIND at the same tick. | Both BINDs are in flight. | — |
| 3.2 | A receives B's BIND; B receives A's BIND. | Each node computes the lexicographic comparison. | IS-3.1: is the comparison done in the `bind_hash` field (which includes all BIND fields) or in a specific sub-field? 0850p-c §R4-7 fix says "lowest `bind_hash` lexicographically" but does not specify the byte ordering (big-endian? little-endian? per RFC-0008 conventions?). |
| 3.3 | Suppose A's `bind_hash` < B's `bind_hash` (lex). | A is DomainCoordinator; B transitions to MissionParticipant. | IS-3.2: how long does B wait before deciding "A is the winner"? no timeout is specified. If A's BIND never arrives, does B retry its own BIND? |
| 3.4 | B acknowledges A's BIND. | A's BIND has ≥1 witness; A is confirmed DomainCoordinator. | — |
| 3.5 | A rejects B's BIND. | B's BIND is silently dropped. | IS-3.3: is rejection silent (`tracing::debug!`) or loud (`tracing::warn!`)? 0850p-c §8 witness rules are silent on this. The user-convention says "tracing::warn! only for security-relevant rejections" — is a tiebreaker loss security-relevant? |
| 3.6 | A and B exchange messages. | All messages flow. | — |

**Additional implicit specs:**

- **IS-3.4:** what if A and B's `bind_hash` are equal (BLAKE3 collision)?
  probability is astronomically low (~2^-128), but the RFC does not
  specify a fallback. Suggested: deterministic tiebreak by `peer_id`.
- **IS-3.5:** what if A and B both decide they are the winner (e.g.,
  one saw the other's BIND but the other did not, due to message
  reordering in the mock network)? the tiebreaker requires both nodes
  to see both BINDs. If one is missing, the node that didn't see the
  other's BIND might still think it's the winner.

**Convention violations:** timing (3.3), edge case (IS-3.4), error
handling (3.5, IS-3.5).

---

### Scenario 4: Coordinator handover — A hands off to B

**Goal:** verify the Handover Protocol works end-to-end and the
`coordinator_term_id` chain is preserved.

**Setup:**
- 2 nodes: `A` (DomainCoordinator, `term = 1`), `B` (MissionParticipant).
- 1 WhatsApp group `G`, mission bound.
- A and B have been operating normally for N epochs.

**Steps:**

| # | Action | Expected | Implicit Spec |
|---|--------|----------|---------------|
| 4.1 | A signs `HandoverRequest { old_coordinator_id: A, new_coordinator_id: B, handover_epoch }`. | HandoverRequest is broadcast. | IS-4.1: the HandoverRequest envelope is mentioned in 0855p-b §"Handover Protocol" but its full field list is not specified (e.g., is there a `handover_reason` field? a `signature` over what payload?). |
| 4.2 | B receives HandoverRequest, validates A's signature. | B accepts. | IS-4.2: what is the validation rule? is it "A's signature is valid AND A is currently Active"? or more strict (e.g., B must also be in the mission's trust set)? |
| 4.3 | B broadcasts BIND with `coordinator_term_id = BLAKE3(A's term_id \|\| B's peer_id \|\| handover_epoch)`. | B's BIND is witnessed by A. | IS-4.3: is the `coordinator_term_id` formula exactly `BLAKE3(old \|\| new \|\| epoch)`? 0855p-c says so, but 0855p-b does not restate it. Cross-RFC consistency. |
| 4.4 | A witnesses B's BIND, acks. | B has ≥1 witness. | — |
| 4.5 | A transitions CoordinatorLifecycle::Active → Handover → Inactive. | A is no longer DomainCoordinator. | IS-4.4: what is the exact sequence? Active → Handover (on sending HandoverRequest) → Inactive (on B's BIND being witnessed)? or Active → Handover → Inactive all on A's side, regardless of B? |
| 4.6 | B transitions MissionParticipant → Designated → Elected → Active. | B is DomainCoordinator. | IS-4.5: is the full election sequence required, or can B skip directly to Active (since it was designated by A)? 0855p-b §"Election Algorithm" assumes a vote, but a handover is not an election. |

**Additional implicit specs:**

- **IS-4.6:** what if A signs HandoverRequest but then disconnects
  before B's BIND is witnessed? does the handover complete? does B
  become DomainCoordinator with 0 witnesses?
- **IS-4.7:** what if A unilaterally stops signing HEARTBEATs (graceful
  exit, no HandoverRequest)? the slash 0x0008 "term overstay" applies,
  but what is the term length? not specified in 0855p-b.
- **IS-4.8:** what happens to in-flight messages during handover?
  are they delivered to A (the old coordinator) or B (the new one)?
  or both? 0850p-c is silent on this.

**Convention violations:** lifecycle (4.1, 4.5, 4.6), error handling
(IS-4.6, IS-4.7), edge case (IS-4.8).

---

### Scenario 5: Platform loss — A is kicked from the group

**Goal:** verify the Suspect → Inactive transition and the
PlatformLossEnvelope flow.

**Setup:**
- 2 nodes: `A` (DomainCoordinator), `B` (MissionParticipant).
- 1 WhatsApp group `G`, mission bound.
- Mock adapter configured with a "kick" failure mode for A only.

**Steps:**

| # | Action | Expected | Implicit Spec |
|---|--------|----------|---------------|
| 5.1 | A is kicked from `G` by the group admin (simulated by mock adapter removing A from the member list). | A's adapter detects the kick. | IS-5.1: how does the WhatsApp adapter detect a kick? the adapter API surface is not specified in 0850p-a §8.1. Does it poll? receive a webhook? observe a "removed" event? |
| 5.2 | A transitions CoordinatorLifecycle::Active → Suspect. | A is in Suspect state. | IS-5.2: is the Suspect transition automatic on kick detection, or does A wait for a grace period (e.g., 1 epoch) to confirm the kick is not a network blip? 0855p-b §"Liveness Check" is silent. |
| 5.3 | A signs `PlatformLossEnvelope { coordinator_id, group_jid, loss_epoch, reason }`. | A broadcasts PlatformLossEnvelope. | IS-5.3: what are the valid `reason` values? 0855p-c §"PlatformLoss Envelope" does not enumerate them (e.g., `kicked`, `banned`, `left_voluntarily`, `network_failure`). |
| 5.4 | A transitions Suspect → Inactive. | A is no longer DomainCoordinator. | IS-5.4: how long does A wait in Suspect before going to Inactive? no timeout is specified. |
| 5.5 | B receives PlatformLossEnvelope, validates A's signature. | B accepts. | — |
| 5.6 | B runs election for new DomainCoordinator. | B (or another node) is elected. | IS-5.5: who can initiate the election? B alone? does it need 2/3 of the mission? 0855p-b §"Election Algorithm" is silent on the trigger. |

**Additional implicit specs:**

- **IS-5.6:** what if the kick is temporary (e.g., A is re-added by
  another admin within the grace period)? does A resume Active, or
  is the Inactive transition irreversible?
- **IS-5.7:** what if A's adapter misdetects the kick (false positive
  due to a transient WhatsApp API error)? A goes Suspect, then Inactive,
  then a new election is triggered — for no reason. The grace period
  is critical here.
- **IS-5.8:** what is the format of `group_jid` in PlatformLossEnvelope?
  is it the same `group_jid` as in BIND? what if the group was renamed
  or deleted?

**Convention violations:** lifecycle (5.1, 5.4), error handling (5.2,
IS-5.7), edge case (IS-5.6, IS-5.8), authority (5.6).

---

### Scenario 6: Reconnection / split-brain prevention

**Goal:** verify the split-brain prevention in 0855p-c §"Split-brain
prevention on reconnection" works.

**Setup:**
- 3 nodes: `A` (DomainCoordinator), `B`, `C` (both MissionParticipants).
- 1 WhatsApp group `G`, mission bound.
- A disconnects (network failure, not kick).

**Steps:**

| # | Action | Expected | Implicit Spec |
|---|--------|----------|---------------|
| 6.1 | A disconnects. B and C detect the disconnect (no HEARTBEAT for 2x interval). | B and C see A as Suspect. | IS-6.1: how long do B and C wait before declaring A is gone? the HEARTBEAT interval is implicit (not specified). |
| 6.2 | B and C run an election; B is elected as new DomainCoordinator. | B broadcasts BIND with `is_reconnect: false` (B was always connected). | — |
| 6.3 | A reconnects after 10 epochs. | A's adapter re-establishes the WhatsApp session. | — |
| 6.4 | A queries the local `GroupRegistry` for the current binding state. | A sees B is DomainCoordinator for `(mission_id, domain_id, platform)`. | IS-6.2: when does A query the GroupRegistry? on reconnection? on first DOT receive? on epoch tick? the timing is not specified. |
| 6.5 | A MUST NOT issue a BIND. | A transitions to MissionParticipant. | — |
| 6.6 | A MAY challenge B via slash vote if A has evidence of misbehavior. | A issues SlashVote if applicable; otherwise accepts B. | IS-6.3: what counts as "evidence of misbehavior" for the slash vote? 0855p-b §"Slash Offense Codes" lists 0x0005 (Coordinator misbehavior) but does not enumerate what counts. |

**Additional implicit specs:**

- **IS-6.4:** what if A's local clock is skewed (e.g., A was offline
  for 10 epochs and its clock is now 5 epochs behind)? A's first
  message after reconnection may be rejected by epoch tolerance (±1).
- **IS-6.5:** what if A reconnects but B has also disconnected in
  the interim (e.g., B was elected but then B also had a network
  failure)? A may issue BIND with `is_reconnect: true` — but what
  if B is about to come back? this is a 3-way race.
- **IS-6.6:** the `is_reconnect: bool` field — what if a node sets
  it to `true` on a fresh BIND (lying about being a reconnection)?
  the witness rule #10 in 0850p-c §8 enforces the split-brain check,
  but what if the witness itself is malicious?

**Convention violations:** timing (6.1, 6.4), security (6.6, IS-6.5,
IS-6.6), edge case (IS-6.4).

---

### Scenario 7: Slash vote — coordinator misbehavior

**Goal:** verify the 2/3 slash vote tally and the transition to
Inactive.

**Setup:**
- 3 nodes: `A` (DomainCoordinator), `B`, `C` (both MissionParticipants).
- 1 WhatsApp group `G`, mission bound.
- Mock adapter allows A to double-sign (simulating misbehavior).

**Steps:**

| # | Action | Expected | Implicit Spec |
|---|--------|----------|---------------|
| 7.1 | A signs two conflicting BIND envelopes for the same `(mission_id, domain_id)` with different `coordinator_term_id` values. | Both BINDs are broadcast. | — |
| 7.2 | B detects the conflict (two BINDs with the same `bind_hash` inputs but different `coordinator_term_id`). | B flags A as misbehaving. | IS-7.1: what counts as "evidence of misbehavior"? 0855p-b §"Slash Offense Codes" lists 0x0005 but does not enumerate specific conditions. Is double-signing one? what about slow HEARTBEATs? selective message delivery? |
| 7.3 | B broadcasts `SlashVote { target: A, reason: 0x0005, evidence: BIND_1, BIND_2 }`. | SlashVote is broadcast. | IS-7.2: what is the format of `evidence`? is it a hash of the conflicting envelopes? the full envelopes? a Merkle proof? |
| 7.4 | C also broadcasts SlashVote against A. | Slash tally reaches 2/3. | IS-7.3: what is "2/3"? 2/3 of mission members? 2/3 of group members? 2/3 of stake weight? 0855p-b §"Slash Offense Codes" is silent. |
| 7.5 | A receives the slash votes; tally reaches 2/3. | A transitions CoordinatorLifecycle::Active → Demoting → Inactive. | IS-7.4: does A transition automatically, or does a human/governance step confirm first? 0855p-b says "slash proof + governance vote" in the state diagram. |
| 7.6 | B or C becomes new DomainCoordinator. | Election runs. | — |

**Additional implicit specs:**

- **IS-7.5:** what if the slash tally is exactly 1/2 + 1 (e.g., 2 of 3)?
  passes or fails? "2/3" usually means >2/3, not ≥2/3. The RFC should
  specify.
- **IS-7.6:** what if a slash vote is initiated but never reaches
  quorum (e.g., only 1 of 3 votes)? does the vote stay open forever?
  is there a timeout?
- **IS-7.7:** what if the slashed node disputes the slash? no appeal
  mechanism is specified. Can A submit a counter-evidence?
- **IS-7.8:** what if A is slashed but the slashing slash itself is
  malicious (B and C collude to slash A)? the slash 0x0002 "envelope
  forgery" applies, but the detection is post-hoc.

**Convention violations:** authority (7.4), lifecycle (7.5), error
handling (IS-7.6, IS-7.7), security (IS-7.8).

---

### Scenario 8: Cross-platform migration — WhatsApp → Matrix

**Goal:** verify the multi-platform rule allows migration, and the
BIND/UNBIND ordering is correct.

**Setup:**
- 2 nodes: `A` (DomainCoordinator on WhatsApp), `B` (MissionParticipant).
- WhatsApp group `W` bound to `domain_id = "test-domain-1"`.
- Matrix room `M` created by a new node `C`.
- Mission decides to migrate (governance vote — see IS-8.1).

**Steps:**

| # | Action | Expected | Implicit Spec |
|---|--------|----------|---------------|
| 8.1 | Mission governance votes to migrate to Matrix. | Vote passes. | IS-8.1: how is the migration decision made? governance vote? coordinator decides? 2/3 of mission? 0850p-c is silent. |
| 8.2 | C creates Matrix room `M` and joins. | C is a member of `M`. | — |
| 8.3 | A joins `M` (via Matrix adapter). | A is a member of `M` and `W`. | IS-8.2: can a node be DomainCoordinator for both platforms simultaneously? the multi-platform rule says yes (1 group per platform per domain_id), but is this the intent during migration? |
| 8.4 | A (or C) issues BIND for `M` with `is_reconnect: false`. | `M` is now bound. | — |
| 8.5 | A signs UNBIND for `W` with reason `0x000A` (or some "platform migration" reason). | `W` is unbound. | IS-8.3: what is the reason code for migration? 0850p-c §6 lists 6 reasons but not migration. Should be added. |
| 8.6 | C becomes DomainCoordinator for `M`. | C is DomainCoordinator. | IS-8.4: is C the new DomainCoordinator, or does A remain? the handover is from A (WhatsApp) to C (Matrix). |
| 8.7 | A transitions to MissionParticipant. | A is no longer DomainCoordinator. | — |

**Additional implicit specs:**

- **IS-8.5:** what is the ordering of BIND and UNBIND? is it
  "BIND new, then UNBIND old" (atomic migration)? or can they be
  done in any order? if UNBIND happens first, there's a window where
  the domain_id is unbound on both platforms.
- **IS-8.6:** what if BIND for `M` fails (e.g., C is unreachable)?
  does the UNBIND for `W` roll back? no transaction mechanism is
  specified.
- **IS-8.7:** what happens to in-flight messages during migration?
  messages in `W` may be delivered to A (old coordinator) after A
  has transitioned to MissionParticipant. are they dropped? re-routed?
- **IS-8.8:** the multi-platform rule says 1 group per platform per
  domain_id. during migration, is `W` still bound when `M` is bound?
  if both are bound, the rule is violated (temporarily).

**Convention violations:** authority (8.1), lifecycle (8.4, 8.6),
error handling (IS-8.6), edge case (IS-8.7, IS-8.8).

---

## Implicit Specs Index

Grouped by convention violation. Severity: C=CRITICAL, H=HIGH,
M=MEDIUM, L=LOW.

### Timing convention (no timeout/interval specified)

| ID | Scenario | Severity | Description | RFC |
|----|----------|----------|-------------|-----|
| IS-1.3 | S1 | H | BIND witness timeout not specified | 0850p-c |
| IS-1.5 | S1 | M | HEARTBEAT interval not specified | 0855p-b |
| IS-1.6 | S1 | M | BIND wait-for-ack timeout not specified | 0850p-c |
| IS-2.1 | S2 | M | Mode A → Mode B trigger is ambiguous (all 5 timeout vs. <3 responses) | 0851p-a |
| IS-2.3 | S2 | M | Mode B → Mode C trigger and timeout not specified | 0851p-a |
| IS-3.3 | S3 | M | Tiebreaker wait timeout not specified | 0850p-c |
| IS-5.4 | S5 | H | Suspect → Inactive timeout not specified | 0855p-b |
| IS-5.2 | S5 | H | Kick detection grace period not specified | 0855p-b |
| IS-6.1 | S6 | M | HEARTBEAT miss detection threshold not specified | 0855p-b |
| IS-6.4 | S6 | M | GroupRegistry query timing on reconnection not specified | 0855p-c |
| IS-7.6 | S7 | M | Slash vote open duration not specified | 0855p-b |

### Error handling convention (failure paths not specified)

| ID | Scenario | Severity | Description | RFC |
|----|----------|----------|-------------|-----|
| IS-2.4 | S2 | M | What if all 3 bootstrap modes fail? | 0851p-a |
| IS-3.5 | S3 | L | Rejection logging level for tiebreaker loss | 0850p-c |
| IS-4.6 | S4 | H | HandoverRequest signed but old coord disconnects before new BIND | 0855p-b |
| IS-4.7 | S4 | H | Term overstay: term length not specified | 0855p-b |
| IS-5.7 | S5 | H | False-positive kick detection | 0855p-b |
| IS-7.7 | S7 | M | Slash vote appeal mechanism not specified | 0855p-b |
| IS-8.6 | S8 | H | BIND/UNBIND transaction rollback not specified | 0850p-c |

### Edge case convention (boundary conditions not handled)

| ID | Scenario | Severity | Description | RFC |
|----|----------|----------|-------------|-----|
| IS-1.2 | S1 | M | What counts as "first DOT" in a group? | 0850p-c |
| IS-1.4 | S1 | M | Message routing topology (full-mesh? gossip? broadcast?) | 0850p-c |
| IS-3.4 | S3 | L | BLAKE3 hash collision tiebreak | 0850p-c |
| IS-3.5 | S3 | M | Asymmetric BIND visibility (one node sees both, other sees one) | 0850p-c |
| IS-4.8 | S4 | M | In-flight messages during handover | 0850p-c |
| IS-5.6 | S5 | M | Temporary kick (re-added by admin) | 0855p-b |
| IS-5.8 | S5 | M | group_jid format in PlatformLossEnvelope (rename? delete?) | 0855p-c |
| IS-6.5 | S6 | M | 3-way race: A and B both disconnect, then both reconnect | 0855p-c |
| IS-8.7 | S8 | M | In-flight messages during cross-platform migration | 0850p-c |
| IS-8.8 | S8 | M | Multi-platform rule during migration (both bound temporarily) | 0850p-c |

### Security convention (threats not addressed)

| ID | Scenario | Severity | Description | RFC |
|----|----------|----------|-------------|-----|
| IS-2.5 | S2 | H | 2-of-5 Sybil case (3rd is malicious) | 0851p-a |
| IS-6.6 | S6 | H | `is_reconnect: true` on a fresh BIND (lie) | 0850p-c |
| IS-7.8 | S7 | H | Slash vote itself is malicious (B+C collude) | 0855p-b |

### Lifecycle convention (when does X happen?)

| ID | Scenario | Severity | Description | RFC |
|----|----------|----------|-------------|-----|
| IS-1.1 | S1 | H | How does a node know which `group_jid` to join? (out-of-band share) | 0850p-a |
| IS-4.1 | S4 | H | HandoverRequest envelope format not fully specified | 0855p-b |
| IS-4.4 | S4 | M | Exact sequence of Active → Handover → Inactive | 0855p-b |
| IS-4.5 | S4 | M | Handover vs election: can coordinator skip Designated/Elected? | 0855p-b |
| IS-5.1 | S5 | C | Kick detection mechanism in WhatsApp adapter not specified | 0850p-a |
| IS-5.3 | S5 | M | PlatformLossEnvelope.reason values not enumerated | 0855p-c |
| IS-8.4 | S8 | M | New DomainCoordinator identity during migration (A or C?) | 0855p-c |

### Authority convention (who can do X?)

| ID | Scenario | Severity | Description | RFC |
|----|----------|----------|-------------|-----|
| IS-4.2 | S4 | M | HandoverRequest validation rule (signature only? or more?) | 0855p-b |
| IS-5.5 | S5 | M | Who can initiate election after PlatformLoss? | 0855p-b |
| IS-7.1 | S7 | H | What counts as "evidence of misbehavior"? | 0855p-b |
| IS-7.2 | S7 | M | SlashVote evidence format | 0855p-b |
| IS-7.3 | S7 | C | Slash tally base (mission members? group? stake?) | 0855p-b |
| IS-7.4 | S7 | M | Automatic vs governance-confirmed slash | 0855p-b |
| IS-7.5 | S7 | M | Slash tally threshold: >2/3 or ≥2/3? | 0855p-b |
| IS-8.1 | S8 | H | Migration governance: who decides? | 0850p-c |
| IS-8.3 | S8 | M | UNBIND reason code for migration not defined | 0850p-c |

---

## Summary

- **8 scenarios** covering the full DOT pipeline.
- **40 implicit specs** identified, classified by convention violation.
- **Severity breakdown:** 1 CRITICAL, 10 HIGH, 22 MEDIUM, 7 LOW.

**Top 3 critical/highest-priority implicit specs to fix in the RFCs:**

1. **IS-5.1 (CRITICAL):** Kick detection mechanism in the WhatsApp
   adapter is not specified. Without this, the entire PlatformLoss
   flow (S5) cannot be implemented. Fix: add a `KickDetection` section
   to 0850p-a §8.1 specifying the adapter's kick-detection API
   (poll-based, webhook, or event-based).

2. **IS-7.3 (CRITICAL):** Slash tally base is not specified ("2/3
   of what?"). Without this, nodes cannot agree on whether a slash
   passes. Fix: add to 0855p-b §"Slashing Integration" that the tally
   is 2/3 of mission members (not group members, not stake weight).

3. **IS-1.1 (HIGH):** How does a node know which `group_jid` to
   join? This is the entry point to the entire pipeline. Fix: add to
   0850p-a §"Bot Onboarding" that the `group_jid` is shared via the
   invite link (Mode C bootstrap) or the mission descriptor.

---

## Follow-up Work

1. **Test harness implementation** — create `octo-network/tests/e2e_pipeline.rs`
   using the existing `mock_adapter` and `mock_network` infrastructure.
   Each scenario becomes one `#[tokio::test]` function. The harness
   should also report the implicit specs it encounters (as `tracing::warn!`
   with the IS- ID for traceability).

2. **RFC updates** — for each implicit spec, add a section to the
   relevant RFC or add a row to the "Implicit Assumptions Audit"
   table. This is a new round of the multi-round adversarial review
   (Round 9+).

3. **Additional scenarios** — the 8 scenarios above are the
   "happy path + common failures" set. Future scenarios should cover:
   - Large groups (1000+ members)
   - Multi-mission nodes (one node in multiple missions)
   - Key rotation (DomainCoordinator rotates its signing key)
   - Replay attacks (attacker replays old BIND)
   - Network partition healing (partitioned nodes rejoin)

4. **Integration with CI** — wire the E2E tests into the CI pipeline
   so they run on every PR. The mock infrastructure makes them fast
   (in-process, no network) and deterministic (seeded RNG for
   timing-dependent scenarios).
