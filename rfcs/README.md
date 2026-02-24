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

| Status | Description | Location |
|--------|-------------|----------|
| **Draft** | Open for discussion | `rfcs/0000-title.md` |
| **Review** | PR submitted, community feedback | PR comment thread |
| **Accepted** | Approved, create missions | `rfcs/XXXX-title.md` |
| **Rejected** | Declined, record reasoning | `rfcs/archived/` |
| **Replaced** | Superseded by newer RFC | `rfcs/archived/` |

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

| Outcome | Action |
|---------|--------|
| **Accepted** | Renumber to next available, create missions |
| **Rejected** | Move to `rfcs/archived/` with reasoning |
| **Request Changes** | Continue discussion, resubmit |
| **Postpone** | Not now, keep in `rfcs/` as Draft |

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

| RFC | Title | Status | Link |
|-----|-------|--------|------|
| RFC-0001 | Mission Lifecycle | Accepted | [0001-mission-lifecycle.md](0001-mission-lifecycle.md) |
| RFC-0002 | Agent Manifest Specification | Accepted | [0002-agent-manifest.md](0002-agent-manifest.md) |

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
