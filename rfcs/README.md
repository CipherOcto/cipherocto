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
Draft → Review → Accepted → Final
       → Rejected → Archived
       → Superseded → Archived
       → Deprecated → Archived
```

| Status         | Description                      | Location             |
| -------------- | -------------------------------- | -------------------- |
| **Draft**      | Open for discussion              | `rfcs/0000-title.md` |
| **Review**     | PR submitted, community feedback | PR comment thread    |
| **Accepted**   | Approved, create missions        | `rfcs/XXXX-title.md` |
| **Final**      | Implemented and verified         | `rfcs/XXXX-title.md` |
| **Rejected**   | Declined, record reasoning       | `rfcs/archived/`     |
| **Superseded** | Replaced by newer RFC            | `rfcs/archived/`     |
| **Deprecated** | Still supported but discouraged  | `rfcs/archived/`     |

---

## RFC Template

For the complete template with all required sections, see [RFC-0000-template.md](0000-template.md).

Key sections include:

- Dependencies
- Design Goals
- Determinism Requirements
- Security Considerations
- Adversarial Review
- Economic Analysis
- Compatibility
- Test Vectors
- Version History

```markdown
# RFC-XXXX (Category): [Title]

## Status

Draft | Review | Accepted | Final | Rejected | Superseded | Deprecated

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

## RFC Numbering (Category-Based System)

```
0000-0099: Process/Meta (governance, mission, architecture)
0100-0199: Numeric (DFP, DQA, DNT, crypto, linear algebra)
0200-0299: Storage (vector-SQL, persistence)
0300-0399: Retrieval (RAG, vector search, query routing)
0400-0499: Agents (runtime, memory, reasoning, orgs)
0500-0599: AI Execution (VM, transformers, MoE, training)
0600-0699: Proof Systems (verification, consensus, aggregation)
0700-0799: Consensus (sharding, DAG, DA)
0800-0899: Networking (P2P, hardware registry)
0900-0999: Economics (markets, tokenomics)
```

---

## RFC Index by Category

### Process & Meta (RFC-0000-0099)

| RFC                     | Title                            | Status   | Description                   |
| ----------------------- | -------------------------------- | -------- | ----------------------------- |
| RFC-0000 (Process/Meta) | CipherOcto Architecture Overview | Draft    | System architecture           |
| RFC-0001 (Process/Meta) | Mission Lifecycle                | Accepted | Mission framework             |
| RFC-0002 (Process/Meta) | Agent Manifest Specification     | Accepted | Agent definition              |
| RFC-0003 (Process/Meta) | Deterministic Execution Standard | Draft    | Core determinism requirements |
| RFC-0004 (Process/Meta) | Implementation Roadmap           | Draft    | Phased implementation plan    |

### Numeric (RFC-0100-0199)

| RFC                | Title                                 | Status | Description                                 |
| ------------------ | ------------------------------------- | ------ | ------------------------------------------- |
| RFC-0102 (Numeric) | Wallet Cryptography                   | Draft  | Wallet security and key management          |
| RFC-0104 (Numeric) | Deterministic Floating-Point (DFP)    | Draft  | Deterministic floating-point types          |
| RFC-0105 (Numeric) | Deterministic Quant Arithmetic (DQA)  | Draft  | Quantized arithmetic types                  |
| RFC-0106 (Numeric) | Deterministic Numeric Tower (DNT)     | Superseded | Replaced by 0110-0115 (Track B)      |
| RFC-0107 (Numeric) | Deterministic Transformer Circuit     | Draft  | Transformer circuit design                  |
| RFC-0108 (Numeric) | Deterministic Training Circuits       | Draft  | Training circuit design                     |
| RFC-0109 (Numeric) | Deterministic Linear Algebra Engine   | Draft  | Vector/matrix operations                    |
| RFC-0110 (Numeric) | Deterministic BIGINT                 | Accepted | Arbitrary-precision integers               |
| RFC-0111 (Numeric) | Deterministic DECIMAL                | Draft  | Extended precision decimals (i128)         |
| RFC-0112 (Numeric) | Deterministic Vectors (DVEC)         | Draft  | Vector operations for AI inference         |
| RFC-0113 (Numeric) | Deterministic Matrices (DMAT)         | Draft  | Matrix operations for linear algebra      |
| RFC-0114 (Numeric) | Deterministic Activation Functions    | Accepted | ReLU, Sigmoid, Tanh for ML            |
| RFC-0115 (Numeric) | Deterministic Tensors (DTENSOR)       | Planned | N-dimensional tensors (Phase 4)          |
| RFC-0116 (Numeric) | Unified Deterministic Execution Model | Draft  | Unified execution framework                 |
| RFC-0126 (Numeric) | Deterministic Canonical Serialization (DCS) | Accepted | Cross-language deterministic serialization for consensus |

### Storage (RFC-0200-0299)

| RFC                | Title                                    | Status | Description                        |
| ------------------ | ---------------------------------------- | ------ | ---------------------------------- |
| RFC-0200 (Storage) | Production Vector-SQL Storage            | Draft  | Vector storage with SQL interface  |
| RFC-0201 (Storage) | Binary BLOB Type for Hash Storage       | Draft  | Native blob type for crypto hashes |

### Retrieval (RFC-0300-0399)

| RFC                  | Title                                   | Status | Description                         |
| -------------------- | --------------------------------------- | ------ | ----------------------------------- |
| RFC-0300 (Retrieval) | Verifiable AI Retrieval                 | Draft  | Deterministic retrieval foundations |
| RFC-0301 (Retrieval) | Retrieval Architecture & Read Economics | Draft  | Retrieval system design + economics |
| RFC-0302 (Retrieval) | Retrieval Gateway & Query Routing       | Draft  | Query routing layer                 |
| RFC-0303 (Retrieval) | Deterministic Vector Index (HNSW-D)     | Draft  | ANN index                           |
| RFC-0304 (Retrieval) | Verifiable Vector Query Execution       | Draft  | Query layer                         |

### Agents (RFC-0400-0499)

| RFC               | Title                                     | Status | Description                            |
| ----------------- | ----------------------------------------- | ------ | -------------------------------------- |
| RFC-0410 (Agents) | Verifiable Agent Memory                   | Draft  | Agent memory with cryptographic proofs |
| RFC-0411 (Agents) | Knowledge Market & Verifiable Data Assets | Draft  | Data ownership and trading             |
| RFC-0412 (Agents) | Verifiable Reasoning Traces               | Draft  | Agent reasoning verification           |
| RFC-0413 (Agents) | State Virtualization for Massive Scaling  | Draft  | Virtualized state for agents           |
| RFC-0414 (Agents) | Autonomous Agent Organizations            | Draft  | Agent governance structures            |
| RFC-0415 (Agents) | Alignment & Control Mechanisms            | Draft  | Agent safety and control               |
| RFC-0416 (Agents) | Self-Verifying AI Agents                  | Draft  | Agents that verify themselves          |
| RFC-0450 (Agents) | Verifiable Agent Runtime (VAR)            | Draft  | Agent execution                        |

### AI Execution (RFC-0500-0599)

| RFC                     | Title                                | Status | Description                                  |
| ----------------------- | ------------------------------------ | ------ | -------------------------------------------- |
| RFC-0520 (AI Execution) | Deterministic AI Virtual Machine     | Draft  | VM for AI model execution                    |
| RFC-0521 (AI Execution) | Verifiable Large Model Execution     | Draft  | Large model verification                     |
| RFC-0522 (AI Execution) | Mixture-of-Experts                   | Draft  | MoE architecture for decentralized inference |
| RFC-0523 (AI Execution) | Scalable Verifiable AI Execution     | Draft  | Unified scalable execution                   |
| RFC-0550 (AI Execution) | Verifiable RAG Execution (VRE)       | Draft  | RAG pipelines                                |
| RFC-0555 (AI Execution) | Deterministic Model Execution Engine | Draft  | Transformer execution                        |

### Proof Systems (RFC-0600-0699)

| RFC                      | Title                                    | Status | Description                           |
| ------------------------ | ---------------------------------------- | ------ | ------------------------------------- |
| RFC-0615 (Proof Systems) | Probabilistic Verification Markets       | Draft  | Market for probabilistic verification |
| RFC-0616 (Proof Systems) | Proof Market & Hierarchical Inference    | Draft  | Distributed inference + proof market  |
| RFC-0630 (Proof Systems) | Proof-of-Inference Consensus             | Draft  | Consensus for inference results       |
| RFC-0631 (Proof Systems) | Proof-of-Dataset Integrity               | Draft  | Dataset integrity verification        |
| RFC-0650 (Proof Systems) | Proof Aggregation Protocol               | Draft  | Aggregating proofs efficiently        |
| RFC-0651 (Proof Systems) | Proof Market & Hierarchical Verification | Draft  | Verification layer                    |

### Consensus (RFC-0700-0799)

| RFC                  | Title                        | Status | Description                  |
| -------------------- | ---------------------------- | ------ | ---------------------------- |
| RFC-0740 (Consensus) | Sharded Consensus Protocol   | Draft  | Sharded blockchain consensus |
| RFC-0741 (Consensus) | Parallel Block DAG           | Draft  | DAG-based block structure    |
| RFC-0742 (Consensus) | Data Availability & Sampling | Draft  | DAS protocol                 |

### Networking (RFC-0800-0899)

| RFC                   | Title                        | Status | Description                    |
| --------------------- | ---------------------------- | ------ | ------------------------------ |
| RFC-0843 (Networking) | OCTO-Network Protocol        | Draft  | Network protocol specification |
| RFC-0845 (Networking) | Hardware Capability Registry | Draft  | Hardware capability tracking   |

### Economics (RFC-0900-0999)

| RFC                  | Title                           | Status | Description                       |
| -------------------- | ------------------------------- | ------ | --------------------------------- |
| RFC-0900 (Economics) | AI Quota Marketplace Protocol   | Draft  | Marketplace for AI compute quotas |
| RFC-0901 (Economics) | Quota Router Agent              | Draft  | Agent for routing requests        |
| RFC-0910 (Economics) | Inference Task Market           | Draft  | Market for inference tasks        |
| RFC-0950 (Economics) | Agent Mission Marketplace (AMM) | Draft  | Mission marketplace               |
| RFC-0955 (Economics) | Model Liquidity Layer           | Draft  | Tokenized AI models               |
| RFC-0956 (Economics) | Model Liquidity Layer (MLL) v2  | Draft  | Tokenized AI models (updated)     |

### Archived

| RFC                | Title                      | Status                           |
| ------------------ | -------------------------- | -------------------------------- |
| RFC-0103 (Storage) | Unified Vector-SQL Storage | Superseded by RFC-0200 (Storage) |
| RFC-0106 (Numeric) | Deterministic Numeric Tower | Superseded by 0110-0115          |

---

## RFC Folder Structure

RFCs are organized by status and category per the BLUEPRINT:

```
rfcs/
├── draft/
│   ├── process/       (0000-0099)
│   ├── numeric/       (0100-0199)
│   ├── storage/      (0200-0299)
│   ├── retrieval/    (0300-0399)
│   ├── agents/       (0400-0499)
│   ├── ai-execution/ (0500-0599)
│   ├── proof-systems/(0600-0699)
│   ├── consensus/    (0700-0799)
│   ├── networking/   (0800-0899)
│   └── economics/    (0900-0999)
├── planned/
│   ├── process/
│   ├── numeric/
│   ├── proof-systems/
│   └── economics/
├── accepted/
├── final/
└── archived/
```

See [docs/BLUEPRINT.md](../docs/BLUEPRINT.md) for the full specification.

```
Determinism Standard (RFC-0003 Process/Meta) ← Foundation
        ↓
Numeric Foundation (RFC-0104/0105 DFP/DQA)
        ↓
BIGINT & DECIMAL (RFC-0110/0111)
        ↓
Vectors & Matrices (RFC-0112/0113)
        ↓
Activation Functions (RFC-0114)
        ↓
Linear Algebra (RFC-0109 Numeric)
        ↓
Vector Index (RFC-0303 Retrieval) → Vector Storage (RFC-0200 Storage)
        ↓
Vector Query (RFC-0304 Retrieval)
        ↓
RAG Execution (RFC-0550 AI Execution)
        ↓
Agent Runtime (RFC-0450 Agents)
        ↓
Mission Marketplace (RFC-0950 Economics)
        ↓
Proof Verification (RFC-0651 Proof Systems)
        ↓
Model Execution (RFC-0555 AI Execution)
        ↓
Model Liquidity (RFC-0956 Economics)
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
