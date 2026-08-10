# 2026-08-10 — Future Work In Scope: Consolidated Plan

**Status:** Active (local scratchpad per `[[docs-plans-scratchpad]]`)
**Session:** 2026-08-10 wave 5 closure
**Inputs:**

- RFC-0010 v1.3 amendment (commit `2ace15e9`)
- Mission 0871b-storage-backend (commit `50ee5ca3`)
- Mission-gap-closure-priorities-2026-08-10 memory
- Wave-3-plan-correction-2026-08-10 memory

## TL;DR

Pulled ALL on-critical-path Future Work items (RFC-0010 F2/F6/F7/F8 +
RFC-0009 §Capability Keys evolution + RFC-0862 §Future Work F8) into
the active plan. Each item now has:

- A mission file in `missions/open/`
- An owner (placeholder `@unassigned`)
- A schedule (wave assignment)
- A substrate RFC / mission pointer
- Test vector discipline per BLUEPRINT §RFC Process

Off-path items (storage/numeric/proof-system) stay OUT per session
hygiene — they're independent tracks with no DAG dependency on the
current mission queue.

## What was in scope before this plan

```
WAVE 1-3 (landed on next, awaiting push per [[feedback_initiation_user_only]]):
  - 0957 Phase 2 a/b/c/d (commit 90306f45 / 5cda2eb7 / b19fe57f / 9fe9071c)
  - 0871e-phase5b atomic drain (commit ebdbf4cd)
  - 0871e-phase5c pricing policy (commit 0a5570bb)
  - 0959-c4 CompositeCapabilityCatalog (commit bbb70bc0)
  - 0959 placeholder identity binding (commit db75a0e7)
  - 0871-phase5 router dispatch wiring (commit 82e700ce)
  - 0871e-phase5b Stoolap ledger (commit 2b24796c)
  - RFC-0010 v1.3 storage extension (commit 2ace15e9)
  - Mission 0871b-storage-backend filed (commit 50ee5ca3)
```

## What is in scope after this plan

### Wave 4 — Substrate landing (RFC amendments + substrate impls)

| Mission                                  | Substrate                   | Owner       | Schedule               |
| ---------------------------------------- | --------------------------- | ----------- | ---------------------- |
| 0871b-storage-backend                    | RFC-0010 v1.3               | @unassigned | claim 2026-08-11+      |
| 0009-v12-identity-evolution              | RFC-0009 v1.2 draft         | @unassigned | review R1 2026-08-11+  |
| 0871e-f7-cross-instance-did-coordination | RFC-0010 v1.3 + RFC-0862 F8 | @unassigned | gated on approach pick |

### Wave 5 — Feature extensions (depend on Wave 4 substrate)

| Mission                            | Substrate                             | Owner       | Schedule                       |
| ---------------------------------- | ------------------------------------- | ----------- | ------------------------------ |
| 0010-f2-multi-chain-did-resolution | RFC-0010 v1.4 amendment               | @unassigned | gated on 0871b-storage-backend |
| 0871b-cross-domain-resolution-impl | RFC-0871 §Future Work + `DidRegistry` | @unassigned | gated on 0871b-storage-backend |
| 0010-f8-rich-did-documents         | RFC-0010 v1.5 amendment               | @unassigned | gated on 0871b-storage-backend |

### Wave 6 — Follow-ons (gated on Wave 5)

| Mission                               | Substrate               | Owner       | Schedule                                |
| ------------------------------------- | ----------------------- | ----------- | --------------------------------------- |
| 0871e-phase5c-1-cross-instance-drain  | RFC-0862 v1.2 amendment | @unassigned | gated on approach pick                  |
| RFC-0862 F8 writer election promotion | RFC-0862 v1.2 amendment | @unassigned | parallel with 0871e-phase5c-1           |
| 0871c reputation-anchor-node          | RFC-0968 substrate      | @unassigned | open, partial scope (independent track) |

## Critical Path (linearized)

```
RFC-0010 v1.3 ✓ (commit 2ace15e9)
    └→ 0871b-storage-backend (READY)
            ├→ 0010-f2-multi-chain-did-resolution (gated)
            ├→ 0871b-cross-domain-resolution-impl (gated)
            ├→ 0010-f8-rich-did-documents (gated)
            └→ 0871e-f7-cross-instance-did-coordination (gated)

RFC-0009 v1.2 DRAFT (lands this session)
    └→ 0009-v12-identity-evolution (mission filed, R1 review pending)
            └→ 0957-f F4 V2 bundling (gated — Band B substrate)

RFC-0862 v1.2 (F8 promotion, parallel)
    └→ 0871e-phase5c-1-cross-instance-drain (gated — approach pick)
    └→ 0871e-f7-cross-instance-did-coordination (gated — same approach pick)
```

## Approach Pick Required (user direction pending)

Two substrate decisions block wave 4/5/6:

1. **`DrainCoordinator` approach** (mission `0871e-phase5c-1`)
   candidates: 2PC / centralized aggregator / CRDT LWW.
   Recommendation: **Option B (centralized aggregator)** — production
   HA deployments already elect a writer for `DatabaseSyncAdapter`
   (RFC-0862 §Roles); piggybacking avoids a separate consensus layer.

2. **`DidWriteCoordinator` approach** (mission `0871e-f7`) — MUST
   match `DrainCoordinator` pick (same substrate, same tradeoff).

User direction needed before either mission is claimed.

## Mission Files Filed This Session

| Mission                                  | File                                                        | Status     |
| ---------------------------------------- | ----------------------------------------------------------- | ---------- |
| 0010-f2-multi-chain-did-resolution       | `missions/open/0010-f2-multi-chain-did-resolution.md`       | unassigned |
| 0871b-cross-domain-resolution-impl       | `missions/open/0871b-cross-domain-resolution-impl.md`       | unassigned |
| 0871e-f7-cross-instance-did-coordination | `missions/open/0871e-f7-cross-instance-did-coordination.md` | unassigned |
| 0010-f8-rich-did-documents               | `missions/open/0010-f8-rich-did-documents.md`               | unassigned |
| 0009-v12-identity-evolution              | `missions/open/0009-v12-identity-evolution.md`              | unassigned |

## RFC Drafts Filed This Session

| RFC           | File                                                | Status                        |
| ------------- | --------------------------------------------------- | ----------------------------- |
| RFC-0009 v1.2 | `rfcs/draft/process/0009-identity-evolution-v12.md` | Draft (R1 review 2026-08-11+) |

## What is explicitly OUT (off-critical-path)

These items have NO DAG dependency on the current mission queue. They
stay in their respective RFC §Future Work sections and ship on their
own cadence per RFC process:

- RFC-0853 F1 (PQC identity substrate) — post-v2.0 (PQC migration
  timeline)
- RFC-0870 F1-F9 (BootstrapOrchestrator, signed peer announcements,
  DHT routing, on-chain settlement, etc.) — independent networking track
- RFC-0851 F1-F4 (Hierarchical/stealth/partial topology discovery) —
  independent bootstrap track
- RFC-0902 F1-F2 (Market-based / custom routing rules) — LiteLLM
  feature parity track
- RFC-0109 F1-F5 (Deterministic tensor / convolution / attention /
  transformer / ANN) — RFC-0104 DFP track
- RFC-0126 §Future Work — deterministic serialization extensions
- RFC-0862 F1-F11 except F8 — sync protocol extensions
- RFC-0853 F2/F4-F8 — HSM integration / ZK identity proofs / etc.
- RFC-0204 F1-F5 — SQL expression compiler extensions
- RFC-0201 F1-F3 — blob streaming / compression / partial reads
- RFC-0202 §Future Work — BigInt/decimal extensions
- RFC-0630 F1-F4 — proof-of-inference consensus extensions
- RFC-0949 F1-F4 — enterprise SSO extensions
- RFC-0850 / 0850p-c / 0855 / 0855p-c §Future Work — networking
  extensions
- RFC-0863 / 0863p-a §Future Work — general transport extensions
- RFC-0861 §Future Work — coordinator admin refinements
- RFC-0971 §Future Work — destination node role consolidation
- RFC-0970 §Future Work — forwarding hop auth extensions
- RFC-0958 §Future Work — ZK capability extensions
- RFC-0955 F1-F3 — model liquidity layer extensions
- RFC-0926 §Future Work — penalty latency scoring extensions
- RFC-0960 / 0961 / 0962 / 0963 / 0964 / 0965 / 0967 §Future Work —
  CIPHERO-SQL / consensus / resource shard / constraint / capability
  / policy extensions
- RFC-0900 / 0903 / 0904 / 0905 / 0907 / 0908 / etc. — LiteLLM
  parity feature backlog
- RFC-0855 / 0855p-b / 0855p-c §Future Work — mission overlay
  extensions

## Push Gate (per [[feedback_initiation_user_only]] + [[git-workflow]])

12 commits queued on `next` branch:

```
2b24796c feat(quota-router-storage): 0871e-phase5b-stoolap-ledger
bbb70bc0 feat(octo-cap-macaroon): 0959-c4 CompositeCapabilityCatalog
db75a0e7 feat(quota-router-core, ...): 0959 placeholder identity binding
82e700ce  feat(quota-router-core,octo-wallet-node): 0871-phase5 router dispatch wiring
0a5570bb  feat(octo-paid-query): 0871e-phase5c pricing policy
ebdbf4cd  feat(octo-paid-query, octo-wallet-node): 0871e-phase5b atomic drain
90306f45  refactor(octo-wallet): 0957-phase2a macaroon substrate
5cda2eb7  refactor(octo-wallet): 0957-phase2b macaroon substrate
b19fe57f  refactor(octo-wallet): 0957-phase2c macaroon substrate
9fe9071c  refactor(octo-wallet): 0957-phase2d macaroon substrate
2ace15e9  docs(rfc-0010): v1.3 amendment — DidRegistry storage trait extension
50ee5ca3  docs(mission): file 0871b-storage-backend (unblocked by RFC-0010 v1.3)
```

Plus new commits this session (5 mission files + 1 RFC draft + memory
update + plan). User must explicitly authorize `git push origin next`.

## Next Steps

1. **Claim 0871b-storage-backend** (highest priority — unblocks 4
   downstream missions).
2. **Open R1 review of RFC-0009 v1.2** (gates 0957-f F4 V2 bundling).
3. **User direction on `DrainCoordinator` approach pick** (gates wave 6).
4. **Push the 12+ queued commits** (user-authorized remote write).
5. **Schedule memory hygiene sweep** — update
   `mission-gap-closure-priorities-2026-08-10` to reflect F1/F2/F3/F4
   closeouts + new missions filed.

## Cross-references

- [[rfc-0010-v13-storage-extension]] — v1.3 closeout memory
- [[wave-3-plan-correction-2026-08-10]] — drift context
- [[mission-gap-closure-priorities-2026-08-10]] — STALE priority memory
  (needs update next sweep)
- [[cipherocto-design-principles]] — layer discipline
- [[feedback_initiation_user_only]] — push gate
- [[docs-plans-scratchpad]] — local-only file convention
