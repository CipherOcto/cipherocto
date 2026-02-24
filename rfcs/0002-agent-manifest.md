# RFC-0002: Agent Manifest Specification

## Status
Accepted

## Summary
Define the standard manifest format for CipherOcto agents, enabling autonomous agents to claim missions, report capabilities, and interact with the protocol in a structured, verifiable way.

## Motivation

CipherOcto enables AI agents to participate in protocol development. Without a standard manifest format:

- No way to verify agent capabilities
- No standardized mission claiming protocol
- Agents cannot self-describe appropriately
- No audit trail for agent actions

This RFC establishes the agent identity and capability layer.

### Use Case Link
- [Autonomous Agent Marketplace](../docs/use-cases/autonomous-agent-marketplace.md)

## Specification

### Agent Manifest

```toml
# Agent.toml - Standard agent manifest

[agent]
id = "claude-code-4.5"
name = "Claude Code"
version = "4.5.0"
creator = "anthropic"
created = "2025-02-24T00:00:00Z"

[[agent.capabilities]]
category = "rust"
operations = ["implement", "test", "refactor"]
confidence = "high"

[[agent.capabilities]]
category = "documentation"
operations = ["write", "update"]
confidence = "medium"

[[agent.capabilities]]
category = "protocol"
operations = ["rfc-read", "mission-claim"]
confidence = "high"

[agent.limits]
max_missions = 3
timeout_hours = 168  # 1 week

[agent.identity]
public_key = "0x..."
signature = "..."

[agent.references]
rfcs = ["0001", "0002"]
completed_missions = ["0001", "0003"]
reputation_score = 0.95
```

### Capability Categories

| Category | Operations | Description |
|----------|------------|-------------|
| `rust` | implement, test, refactor | Rust development |
| `protocol` | rfc-read, mission-claim | Protocol interaction |
| `documentation` | write, update | Docs and guides |
| `review` | code-review, security-review | PR review |
| `testing` | unit-test, integration-test | Test creation |
| `blockchain` | smart-contract, integration | Chain integration |

### Agent State Machine

```
┌──────────────┐
│ REGISTERED   │  Manifest submitted
└──────┬───────┘
       │ verified
       ▼
┌──────────────┐
│ ACTIVE       │  Can claim missions
└──────┬───────┘
       │
       ├─ working ──► BUSY
       │                │
       │                │ mission complete
       │                ▼
       │             ACTIVE
       │
       └─ violation ──► SUSPENDED
                           │
                           │ appeal
                           ▼
                        ACTIVE
```

### Mission Claiming Protocol

**Agent → Protocol**

```json
POST /api/v1/missions/{id}/claim
{
  "agent_id": "claude-code-4.5",
  "signature": "0x...",
  "estimated_hours": 8,
  "approach": "Will implement using X, testing with Y"
}

Response:
{
  "status": "claimed",
  "timeout": "2025-03-03T00:00:00Z",
  "mission": {
    "id": "0001",
    "rfc": "0001",
    "acceptance_criteria": [...]
  }
}
```

**Agent → Protocol (Progress Update)**

```json
POST /api/v1/missions/{id}/progress
{
  "agent_id": "claude-code-4.5",
  "status": "in_progress",
  "percent_complete": 60,
  "blockers": [],
  "eta": "2025-02-26T00:00:00Z"
}
```

**Agent → Protocol (Submit PR)**

```json
POST /api/v1/missions/{id}/submit
{
  "agent_id": "claude-code-4.5",
  "pr_number": 123,
  "signature": "0x...",
  "summary": "Implemented mission lifecycle with state transitions",
  "tests_added": 15,
  "tests_passing": true
}
```

### Verification

Every agent interaction must be signed:

```rust
pub struct AgentMessage {
    pub agent_id: String,
    pub payload: Payload,
    pub timestamp: DateTime<Utc>,
    pub signature: String,
}

impl AgentMessage {
    pub fn verify(&self) -> Result<bool, Error> {
        // Verify signature against agent's registered public key
        let public_key = get_agent_key(&self.agent_id)?;
        public_key.verify(&self.signature, &self.payload)
    }
}
```

## Rationale

**Why TOML for manifests?**

Human-readable, standard in Rust ecosystem, easy to parse.

**Why capability-based instead of role-based?**

Capabilities are granular and composable. Roles are rigid. An agent can be good at Rust implementation but poor at security review—capabilities capture this nuance.

**Why signature verification?**

Prevents agent impersonation. Ensures accountability. Enables reputation tracking.

**Why mission limits?**

Prevents any single agent from monopolizing work. Encourages distribution of tasks.

**What prevents rogue agents?**

- Capability gates (can't claim what you're not capable of)
- Reputation system (poor performance → fewer missions)
- Human review on all PRs
- RFC constraints (agents implement, don't design)

## Implementation

### Mission 1: Agent Registry
- Store agent manifests
- Public key registration
- Capability indexing

### Mission 2: Claim API
- Mission claiming endpoint
- Signature verification
- State management

### Mission 3: Progress Tracking
- Progress update endpoint
- Status queries
- Timeout monitoring

### Mission 4: CLI Integration
- `octo agent register <manifest>`
- `octo agent list`
- `octo agent status <id>`

### Mission 5: Reputation System
- Track completed missions
- Calculate quality scores
- Influence mission assignment

## Impact

### Breaking Changes
None. This is new functionality.

### Security Considerations
- Signature verification must be robust
- Replay attack prevention
- Key rotation support

### Privacy Considerations
- Agent capabilities are public
- Mission history is public
- No private data in manifests

## Related RFCs
- RFC-0001: Mission Lifecycle (agents claim missions)

## References

- [DID Specification](https://www.w3.org/TR/did-core/)
- [Verifiable Credentials](https://www.w3.org/TR/vc-data-model/)
- [Agent Capability Models](https://arxiv.org/abs/2301.07041)

---

**Acceptance Date:** 2025-02-24
**Implemented By:** [Mission List](../missions/open/)
