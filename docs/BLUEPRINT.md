# The CipherOcto Blueprint

**How ideas become protocol reality.**

This is not documentation. This is process architecture.

---

## Philosophy

CipherOcto is not a repository. It is a protocol for autonomous intelligence collaboration.

Most open-source projects organize files. Successful protocols organize **decision flow**.

This Blueprint defines how work flows through CipherOcto—from idea to protocol evolution.

---

## The Core Separation

We maintain three distinct layers that must never mix:

| Layer | Purpose | Question | Blockchain Analogy |
|-------|---------|----------|-------------------|
| **Use Cases** | Intent | WHY? | Ethereum Vision |
| **RFCs** | Design | WHAT? | EIPs |
| **Missions** | Execution | HOW? | Implementation |

**Mix these layers and governance breaks.**

---

## Governance Stack

```
┌─────────────────────────────────────────────────────────────┐
│                     Idea Emerges                             │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  1️⃣ USE CASES — Intent Layer                               │
│  Location: docs/use-cases/                                  │
│                                                             │
│  Defines:                                                   │
│  - Problems to solve                                        │
│  - Narratives and motivation                                │
│  - Architectural direction                                  │
│                                                             │
│  Characteristics:                                           │
│  - Long-lived                                               │
│  - Descriptive                                              │
│  - Non-actionable                                            │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  2️⃣ RFCs — Protocol Design Layer                           │
│  Location: rfcs/                                            │
│                                                             │
│  Defines:                                                   │
│  - Specifications                                          │
│  - Constraints                                              │
│  - Interfaces                                               │
│  - Expected behavior                                        │
│                                                             │
│  Examples:                                                  │
│  - RFC-0001: Mission Lifecycle                              │
│  - RFC-0002: Agent Manifest Spec                            │
│  - RFC-0003: Storage Provider Protocol                      │
│                                                             │
│  Answer: "What must exist before implementation?"           │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  3️⃣ MISSIONS — Execution Layer                             │
│  Location: missions/                                        │
│                                                             │
│  A mission is a claimable unit of work.                     │
│  - Never conceptual                                         │
│  - Always executable                                         │
│  - Created ONLY after: Use Case → RFC → Mission             │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  4️⃣ AGENTS — Execution Actors                              │
│  Location: agents/                                          │
│                                                             │
│  Agents do NOT decide direction.                            │
│  They implement Missions derived from RFCs.                 │
│  This prevents AI chaos.                                    │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  5️⃣ ROADMAP — Temporal Layer                               │
│  Location: ROADMAP.md                                       │
│                                                             │
│  References:                                                │
│  - Use Cases                                                │
│  - RFC milestones                                           │
│  - Protocol phases                                          │
│                                                             │
│  Roadmap is navigation, NOT backlog.                        │
└─────────────────────────────────────────────────────────────┘
```

---

## Canonical Workflow

```
Idea
 │
 ▼
Use Case (WHY?)
 │
 ▼
RFC Discussion (WHAT?)
 │
 ├─ Draft RFC
 ├─ Community Review
 ├─ Revision
 └─ Accepted RFC
 │
 ▼
Mission Created (HOW?)
 │
 ▼
Agent/Human Claims Mission
 │
 ▼
Implementation (PR)
 │
 ▼
Review & Test
 │
 ▼
Merge
 │
 ▼
Protocol Evolution
```

**This is the only flow. Shortcuts create technical debt.**

---

## Artifact Types

### Use Case

**Location:** `docs/use-cases/`

**Template:**
```markdown
# Use Case: [Title]

## Problem
What problem exists?

## Motivation
Why does this matter for CipherOcto?

## Impact
What changes if this is implemented?

## Related RFCs
- RFC-XXXX
```

**Examples:**
- Decentralized Mission Execution
- Autonomous Agent Marketplace
- Hybrid AI-Blockchain Runtime

---

### RFC (Request for Comments)

**Location:** `rfcs/`

**Template:**
```markdown
# RFC-XXXX: [Title]

## Status
Draft | Accepted | Replaced | Deprecated

## Summary
One-paragraph overview.

## Motivation
Why this RFC?

## Specification
Technical details, constraints, interfaces.

## Rationale
Why this approach over alternatives?

## Implementation
Path to missions.

## Related Use Cases
- [Use Case Name](../../docs/use-cases/...)
```

**RFC Process:**
1. Draft RFC in `rfcs/0000-title.md`
2. Submit PR for discussion
3. Address feedback
4. Accepted → Renumbered
5. Rejected → Moved to `rfcs/archived/`

---

### Mission

**Location:** `missions/`

**Lifecycle:**
```
missions/open/     → Available to claim
missions/claimed/  → Someone working on it
missions/with-pr/  → PR submitted
missions/archived/ → Completed or abandoned
```

**Template:**
```markdown
# Mission: [Title]

## Status
Open | Claimed | In Review | Completed | Blocked

## RFC
RFC-XXXX

## Acceptance Criteria
- [ ] Criteria 1
- [ ] Criteria 2

## Claimant
@username

## Pull Request
#

## Notes
Implementation notes, blockers, decisions.
```

**Mission Rules:**
- Missions REQUIRE an approved RFC
- No RFC = Create one first
- One mission = One claimable unit
- Missions are timeboxed

---

## Agent Participation Model

### What Agents CAN Do

| Capability | Description |
|------------|-------------|
| Claim Missions | Pick up work from `missions/open/` |
| Implement Specs | Execute according to RFC |
| Write Tests | Ensure quality |
| Submit PRs | Standard contribution flow |

### What Agents CANNOT Do

| Restriction | Reason |
|-------------|--------|
| Create Use Cases | Human direction required |
| Accept RFCs | Governance decision |
| Bypass Missions | Chaos prevention |

### Agent Workflow

```
1. Agent reads missions/open/
2. Claims mission (moves to missions/claimed/)
3. Implements per RFC spec
4. Writes tests
5. Submits PR
6. Human review
7. Merge → mission to missions/archived/
```

---

## Human vs Agent Roles

| Activity | Human | Agent |
|----------|-------|-------|
| Define Use Cases | ✓ | ✗ |
| Write RFCs | ✓ | ✗ |
| Accept RFCs | ✓ | ✗ |
| Create Missions | ✓ | ✓ |
| Claim Missions | ✓ | ✓ |
| Implement RFCs | ✓ | ✓ |
| Review PRs | ✓ | ✗ |
| Merge to main | ✓ | ✗ |

**Humans govern. Agents implement.**

---

## RFC Acceptance Process

1. **Draft:** Author creates RFC PR
2. **Review:** Community discusses (7-day minimum)
3. **Decision:** Maintainers accept/reject
4. **Outcome:**
   - Accepted → Renumber, create Missions
   - Rejected → Archive with reasoning
   - Needs Work → Continue discussion

**Consensus Required:** At least 2 maintainer approvals, no blocking objections.

---

## Mission Lifecycle

```
┌──────────────┐
│ RFC Accepted │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Mission      │
│ Created      │  → missions/open/
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Claimed      │  → missions/claimed/
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ PR Submitted │  → missions/with-pr/
└──────┬───────┘
       │
       ├─ Accept → Archive (completed)
       └─ Reject → Return to claimed
```

**Timeouts:**
- Claimed mission: 14 days → Return to open
- PR in review: 7 days → Follow up or close

---

## Future Decentralization Path

### Phase 1: Foundation (Current)
- Human governance
- Centralized RFC process
- Mission-based execution

### Phase 2: Stakeholder Input
- OCTO token holders vote on RFCs
- Reputation-based weighting
- Agent representation

### Phase 3: Protocol Governance
- On-chain decision making
- Automated RFC acceptance
- Autonomous mission creation

**The Blueprint enables this evolution.**

---

## Repository Topology

```
cipherocto/
├── docs/
│   ├── BLUEPRINT.md           ← This document
│   ├── START_HERE.md
│   ├── ROLES.md
│   ├── ROADMAP.md
│   └── use-cases/             ← Intent layer
│       ├── decentralized-mission-execution.md
│       └── agent-marketplace.md
├── rfcs/                      ← Design layer
│   ├── README.md
│   ├── 0000-template.md
│   ├── 0001-mission-lifecycle.md
│   ├── 0002-agent-manifest.md
│   └── archived/
├── missions/                  ← Execution layer
│   ├── open/
│   ├── claimed/
│   ├── with-pr/
│   └── archived/
├── agents/
└── crates/
```

---

## Getting Started

**New Contributor Flow:**

1. Read `START_HERE.md`
2. Read `ROLES.md`
3. Read this `BLUEPRINT.md`
4. Browse `use-cases/` for context
5. Check `rfcs/` for active designs
6. Claim a mission from `missions/open/`

**Mission Creator Flow:**

1. Ensure RFC exists and is accepted
2. Create mission file in `missions/open/`
3. Define acceptance criteria
4. Link to RFC
5. Mark as ready to claim

**RFC Author Flow:**

1. Draft RFC from use case motivation
2. Submit PR for discussion
3. Address community feedback
4. Wait for acceptance
5. Create missions from accepted RFC

---

## Summary

**The CipherOcto Blueprint answers: "What do I do first?"**

- Understand the Use Case (WHY)
- Read the RFC (WHAT)
- Claim the Mission (HOW)

**Everything flows through this structure.**

When in doubt, return to the Blueprint.

---

*"We are not documenting a repository. We are defining how autonomous intelligence collaborates to build infrastructure."*
