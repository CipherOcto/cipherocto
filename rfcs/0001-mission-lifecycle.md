# RFC-0001: Mission Lifecycle

## Status
Accepted

## Summary
Define the standard lifecycle for missions in CipherOcto, from creation through completion, establishing clear states, transitions, and timeout rules for claimable work units.

## Motivation

CipherOcto scales through parallel execution by both humans and AI agents. Without a standardized mission lifecycle:

- Claimed work stagnates indefinitely
- No visibility into what's being worked on
- Unclear handoff between contributors
- No mechanism to detect abandoned missions

This RFC provides the governance framework for mission-based execution.

### Use Case Link
- [Decentralized Mission Execution](../docs/use-cases/decentralized-mission-execution.md)

## Specification

### Mission States

```
┌──────────────┐
│ OPEN         │  Available for claim
└──────┬───────┘
       │ claimed
       ▼
┌──────────────┐
│ CLAIMED      │  Someone is working on it
└──────┬───────┘
       │ PR submitted
       ▼
┌──────────────┐
│ IN_REVIEW    │  PR under review
└──────┬───────┘
       │
       ├─ accepted ──► COMPLETED
       │
       └─ rejected ──► CLAIMED (rework)
                              │
                              │ timeout (14d)
                              ▼
                           OPEN
```

### State Definitions

| State | Description | Valid Transitions |
|-------|-------------|-------------------|
| `OPEN` | Available to claim | → CLAIMED |
| `CLAIMED` | Someone assigned | → IN_REVIEW, → OPEN (timeout) |
| `IN_REVIEW` | PR submitted | → COMPLETED, → CLAIMED |
| `COMPLETED` | Merged to main | Terminal state |
| `BLOCKED` | Cannot proceed | → CLAIMED (unblocked) |
| `ARCHIVED` | Closed/abandoned | Terminal state |

### Mission File Format

```yaml
# missions/0001-add-mission-lifecycle.md

id: "0001"
title: "Add Mission Lifecycle"
status: "open"  # open | claimed | in_review | completed | blocked | archived
created_at: "2025-02-24T00:00:00Z"
updated_at: "2025-02-24T00:00:00Z"

rfc: "0001"

claimant: null  # @username when claimed
claimed_at: null

pull_request: null  # PR number when in review

acceptance_criteria:
  - "Mission state enum defined"
  - "State transition functions implemented"
  - "Timeout enforcement added"
  - "Tests for all transitions"

timeout_days: 14  # Days before auto-unclaim
```

### Timeout Rules

| State | Timeout | Action |
|-------|---------|--------|
| `CLAIMED` | 14 days | Return to `OPEN` |
| `IN_REVIEW` | 7 days | Request status update |
| `BLOCKED` | 30 days | Archive or reassign |

### Directory Structure

```
missions/
├── open/       # Available to claim
├── claimed/    # Currently assigned
├── with-pr/    # PR submitted
├── completed/  # Merged successfully
└── archived/   # Closed/abandoned
```

### API Surface

```rust
pub enum MissionStatus {
    Open,
    Claimed { claimant: String, claimed_at: DateTime },
    InReview { pr_number: u64 },
    Completed { merged_at: DateTime },
    Blocked { reason: String },
    Archived { reason: String },
}

pub struct Mission {
    pub id: String,
    pub title: String,
    pub status: MissionStatus,
    pub rfc: String,
    pub acceptance_criteria: Vec<String>,
    pub timeout_days: u64,
}

impl Mission {
    // State transitions
    pub fn claim(&mut self, claimant: &str) -> Result<(), Error>;
    pub fn submit_pr(&mut self, pr_number: u64) -> Result<(), Error>;
    pub fn complete(&mut self) -> Result<(), Error>;
    pub fn unclaim(&mut self) -> Result<(), Error>;

    // Timeouts
    pub fn check_timeout(&self) -> Option<MissionStatus>;
}
```

## Rationale

**Why mission-based instead of issue-based?**

Issues are discussions. Missions are claimable units of work. By separating them:
- Clear intent to execute
- Handoff mechanism between contributors
- Timeout enforcement
- Direct RFC traceability

**Why 14-day claim timeout?**

Long enough for substantial work, short enough to prevent stagnation. Contributors can re-claim if still engaged.

**Why separate `with-pr/` directory?**

Visibility into what's awaiting review prevents duplicate work and enables batch processing of PRs.

## Implementation

### Mission 1: Core Mission Types
- Define `MissionStatus` enum
- Implement `Mission` struct
- Add state transition validation

### Mission 2: Filesystem Backend
- Mission CRUD operations
- Directory-based state management
- YAML serialization

### Mission 3: CLI Commands
- `octo mission list`
- `octo mission claim <id>`
- `octo mission submit <pr>`
- `octo mission status <id>`

### Mission 4: Timeout Enforcement
- Background task to check timeouts
- Auto-unclaim stale missions
- Notifications for impending timeouts

## Impact

### Breaking Changes
None. This is new functionality.

### Migration Path
Existing issues can be tagged as missions. No data loss.

### Dependencies
- RFC-0002: Agent Manifest (missions may be claimed by agents)

## Related RFCs
- RFC-0002: Agent Manifest Specification

## References

- [GitHub Issues](https://guides.github.com/features/issues/)
- [Jira Workflow](https://www.atlassian.com/agile/tutorials/workflows)
- [Linear Issue Lifecycle](https://linear.app/)

---

**Acceptance Date:** 2025-02-24
**Implemented By:** [Mission List](../missions/open/)
