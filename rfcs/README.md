# CipherOcto RFCs

**Request for Comments** — Protocol Design Layer

RFCs define **WHAT** must exist before implementation begins.

Inspired by:

- [Rust RFCs](https://github.com/rust-lang/rfcs)
- [Ethereum EIPs](https://eips.ethereum.org/)
- [Internet RFC Process](https://www.rfc-editor.org/)

---

## What is an RFC?

An RFC is a **design specification**, not implementation.

**RFC answers:**

- What are we building?
- What are the constraints?
- What are the interfaces?
- What is the expected behavior?

**RFC does NOT answer:**

- How do we implement it? (→ Missions)
- Who will implement it? (→ Agents/Humans)
- When will it be done? (→ Roadmap)

---

## RFC Lifecycle

```
Draft → Review → Accepted | Rejected | Archived
```

| Status       | Description                      | Location             |
| ------------ | -------------------------------- | -------------------- |
| **Draft**    | Open for discussion              | `rfcs/0000-title.md` |
| **Review**   | PR submitted, community feedback | PR comment thread    |
| **Accepted** | Approved, create missions        | `rfcs/XXXX-title.md` |
| **Rejected** | Declined, record reasoning       | `rfcs/archived/`     |
| **Replaced** | Superseded by newer RFC          | `rfcs/archived/`     |

---

## RFC Template

```markdown
# RFC-XXXX: [Title]

## Status

Draft | Accepted | Rejected | Replaced | Deprecated

## Summary

One-paragraph overview of the proposal.

## Motivation

Why is this RFC needed? What problem does it solve?

### Use Case Link

Link to the motivating use case in `docs/use-cases/`.

## Specification

Detailed technical specification:

- Data structures
- APIs/interfaces
- Protocols
- Constraints
- Error handling

## Rationale

Why this approach over alternatives?
What trade-offs were made?

## Implementation

Path from RFC to Missions:

- Mission 1: [description]
- Mission 2: [description]
- Mission 3: [description]

## Impact

What does this change?

- Breaking changes?
- Migration path?
- Dependencies?

## Related RFCs

- RFC-XXXX
- RFC-YYYY

## References

Links to external specs, prior art, discussions.
```

---

## RFC Process

### 1. Draft RFC

Create `rfcs/0000-your-title.md` using the template.

```bash
# Start with template
cp rfcs/0000-template.md rfcs/0000-my-proposal.md
```

### 2. Submit for Review

Create PR: `rfcs: RFC-XXXX: [Title]`

Include in description:

- Link to use case (if applicable)
- Summary of change
- Request for reviewers

### 3. Discussion Period

- **Minimum 7 days** for substantial RFCs
- **3 days** for minor clarifications
- All feedback must be addressed

### 4. Decision

**Acceptance Criteria:**

- At least 2 maintainer approvals
- No blocking objections
- Technical soundness verified

**Possible Outcomes:**

| Outcome             | Action                                      |
| ------------------- | ------------------------------------------- |
| **Accepted**        | Renumber to next available, create missions |
| **Rejected**        | Move to `rfcs/archived/` with reasoning     |
| **Request Changes** | Continue discussion, resubmit               |
| **Postpone**        | Not now, keep in `rfcs/` as Draft           |

### 5. Implementation

Once accepted:

1. RFC is numbered (e.g., `0001`)
2. Missions created in `missions/open/`
3. Agents/humans claim missions
4. PRs reference RFC

---

## RFC Numbering

- **0000**: Draft RFCs (unproposed)
- **0001-0999**: Core protocol
- **1000-1999**: Agent system
- **2000-2999**: Network layer
- **3000-3999**: Cryptography
- **4000-4999**: Tokenomics
- **5000-5999**: Governance
- **9000-9999**: Meta/Process

---

## Active RFCs

| RFC      | Title                        | Status   |
| -------- | ---------------------------- | -------- |
| RFC-0001 | Mission Lifecycle            | Accepted |
| RFC-0002 | Agent Manifest Specification | Accepted |

---

## RFC Index by Category

### Core Foundation (RFC-0100-0106)

| RFC                                | Title                                | Description                                     |
| ---------------------------------- | ------------------------------------ | ----------------------------------------------- |
| RFC-0100                           | AI Quota Marketplace Protocol        | Marketplace for AI compute quotas               |
| RFC-0101                           | Quota Router Agent                   | Agent for routing requests to quota markets     |
| RFC-0102                           | Wallet Cryptography                  | Wallet security and key management              |
| RFC-0104 | Deterministic Floating-Point (DFP) | Deterministic floating-point types |
| RFC-0105 | Deterministic Quant Arithmetic (DQA) | Quantized arithmetic types |
| RFC-0106                           | Deterministic Numeric Tower (DNT)    | **Foundational** - Complete numeric type system |

### Vector Storage & Retrieval (RFC-0107-0113)

| RFC      | Title                                     | Description                            |
| -------- | ----------------------------------------- | -------------------------------------- |
| RFC-0107 | Production Vector-SQL Storage Engine v2   | Vector storage with SQL interface      |
| RFC-0108 | Verifiable AI Retrieval                   | Deterministic retrieval foundations    |
| RFC-0109 | Retrieval Architecture & Read Economics   | Retrieval system design + economics    |
| RFC-0110 | Verifiable Agent Memory                   | Agent memory with cryptographic proofs |
| RFC-0111 | Knowledge Market & Verifiable Data Assets | Data ownership and trading             |
| RFC-0113 | Retrieval Gateway & Query Routing         | Query routing layer                    |

### Agent Systems (RFC-0114-0119)

| RFC      | Title                                    | Description                           |
| -------- | ---------------------------------------- | ------------------------------------- |
| RFC-0114 | Verifiable Reasoning Traces              | Agent reasoning verification          |
| RFC-0115 | Probabilistic Verification Markets       | Market for probabilistic verification |
| RFC-0116 | Unified Deterministic Execution Model    | Unified execution framework           |
| RFC-0117 | State Virtualization for Massive Scaling | Virtualized state for agents          |
| RFC-0118 | Autonomous Agent Organizations           | Agent governance structures           |
| RFC-0119 | Alignment & Control Mechanisms           | Agent safety and control              |

### AI Execution (RFC-0120-0125)

| RFC      | Title                                 | Description                                  |
| -------- | ------------------------------------- | -------------------------------------------- |
| RFC-0120 | Deterministic AI Virtual Machine      | VM for AI model execution                    |
| RFC-0121 | Verifiable Large Model Execution      | Large model verification                     |
| RFC-0122 | Mixture-of-Experts                    | MoE architecture for decentralized inference |
| RFC-0123 | Scalable Verifiable AI Execution      | Unified scalable execution                   |
| RFC-0124 | Proof Market & Hierarchical Inference | Distributed inference + proof market         |
| RFC-0125 | Model Liquidity Layer                 | Tokenized AI models                          |

### Deterministic AI Stack (RFC-0130-0134)

| RFC      | Title                             | Description                     |
| -------- | --------------------------------- | ------------------------------- |
| RFC-0130 | Proof-of-Inference Consensus      | Consensus for inference results |
| RFC-0131 | Deterministic Transformer Circuit | Transformer circuit design      |
| RFC-0132 | Deterministic Training Circuits   | Training circuit design         |
| RFC-0133 | Proof-of-Dataset Integrity        | Dataset integrity verification  |
| RFC-0134 | Self-Verifying AI Agents          | Agents that verify themselves   |

### Network & Consensus (RFC-0140-0146)

| RFC      | Title                        | Description                    |
| -------- | ---------------------------- | ------------------------------ |
| RFC-0140 | Sharded Consensus Protocol   | Sharded blockchain consensus   |
| RFC-0141 | Parallel Block DAG           | DAG-based block structure      |
| RFC-0142 | Data Availability & Sampling | DAS protocol                   |
| RFC-0143 | OCTO-Network Protocol        | Network protocol specification |
| RFC-0144 | Inference Task Market        | Market for inference tasks     |
| RFC-0145 | Hardware Capability Registry | Hardware capability tracking   |
| RFC-0146 | Proof Aggregation Protocol   | Aggregating proofs efficiently |

### Implementation (RFC-0147)

| RFC      | Title                  | Description                |
| -------- | ---------------------- | -------------------------- |
| RFC-0147 | Implementation Roadmap | Phased implementation plan |

### Deterministic AI Stack v2 (RFC-0148-0156)

| RFC      | Title                                           | Description              |
| -------- | ----------------------------------------------- | ------------------------ |
| RFC-0148 | Deterministic Linear Algebra Engine (DLAE)      | Vector/matrix operations |
| RFC-0149 | Deterministic Vector Index (HNSW-D)             | ANN index                |
| RFC-0150 | Verifiable Vector Query Execution (VVQE)        | Query layer              |
| RFC-0151 | Verifiable RAG Execution (VRE)                  | RAG pipelines            |
| RFC-0152 | Verifiable Agent Runtime (VAR)                  | Agent execution          |
| RFC-0153 | Agent Mission Marketplace (AMM)                 | Mission marketplace      |
| RFC-0154 | Proof Market & Hierarchical Verification (PHVN) | Verification layer       |
| RFC-0155 | Deterministic Model Execution Engine (DMEE)     | Transformer execution    |
| RFC-0156 | Model Liquidity Layer (MLL)                     | Tokenized AI models     |

### Archived

| RFC      | Title                      | Status                 |
| -------- | -------------------------- | ---------------------- |
| RFC-0103 | Unified Vector-SQL Storage | Superseded by RFC-0107 |

---

## Quick Reference: The Stack

```
Numeric Foundation (RFC-0106)
        ↓
Linear Algebra (RFC-0148)
        ↓
Vector Index (RFC-0149) → Vector Storage (RFC-0107)
        ↓
Vector Query (RFC-0150)
        ↓
RAG Execution (RFC-0151)
        ↓
Agent Runtime (RFC-0152)
        ↓
Mission Marketplace (RFC-0153)
        ↓
Proof Verification (RFC-0154)
        ↓
Model Execution (RFC-0155)
        ↓
Model Liquidity (RFC-0156)
```

---

## Submitting an RFC

**Before writing an RFC:**

1. Check `docs/use-cases/` for motivation
2. Search existing RFCs for similar work
3. Discuss in issue/forums first (optional but recommended)

**When to write an RFC:**

- ✓ New protocol feature
- ✓ Breaking change
- ✓ New agent capability
- ✓ Architecture change
- ✓ Standard/specification

**When NOT to write an RFC:**

- ✗ Bug fixes (just fix them)
- ✗ Documentation improvements
- ✗ Internal refactoring
- ✗ Test additions

---

## RFC Review Guidelines

**For Reviewers:**

- Focus on technical merit
- Consider long-term implications
- Suggest alternatives if concerns
- Explain reasoning for objections

**For Authors:**

- Address all feedback
- Update spec based on discussion
- Withdraw if consensus impossible
- Be willing to compromise

---

## FAQ

### Q: Can I implement without an RFC?

A: Only for bug fixes, docs, tests. New features require RFC.

### Q: How long does RFC review take?

A: Plan for 2-4 weeks including discussion and revisions.

### Q: Can RFCs be changed after acceptance?

A: Minor clarifications: yes. Major changes: new RFC.

### Q: What if my RFC is rejected?

A: It's archived with reasoning. You can revise and resubmit.

### Q: Do agents participate in RFCs?

A: Agents can provide input, but humans accept/reject.

---

**See [`BLUEPRINT.md`](../docs/BLUEPRINT.md) for how RFCs fit into the overall governance flow.**
