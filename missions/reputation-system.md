# Mission: Reputation System

## Status
Open

## RFC
RFC-0100: AI Quota Marketplace Protocol
RFC-0101: Quota Router Agent Specification
RFC-0102: Wallet Cryptography Specification

## Blockers / Dependencies

- **Blocked by:** Mission: Token Swap Integration (should be in progress)
- **Blocked by:** Mission: Multi-Provider Support (should be in progress)

## Acceptance Criteria

- [ ] Provider reputation scores
- [ ] Seller reputation scores
- [ ] Reputation displayed in market listings
- [ ] Reputation affects routing priority
- [ ] Early contributor multiplier tracking
- [ ] Reputation history

## Description

Build a reputation system that tracks provider and seller reliability, enabling trust-based routing and rewarding early contributors.

## Technical Details

### Reputation Score

```rust
struct Reputation {
    wallet: Address,
    role: ReputationRole,

    // Metrics
    total_transactions: u64,
    successful_transactions: u64,
    failed_transactions: u64,
    average_latency_ms: u64,

    // Calculated
    score: u8,  // 0-100
    tier: ReputationTier,

    // Early contributor
    joined_at: DateTime,
    multiplier: f64,
}

enum ReputationRole {
    Provider,
    Seller,
    Router,
}

enum ReputationTier {
    New,
    Bronze,
    Silver,
    Gold,
    Platinum,
}
```

### Reputation Tiers

| Tier | Score | Multiplier | Requirements |
|------|-------|------------|--------------|
| **new** | 0-20 | 1x | < 10 transactions |
| **bronze** | 21-40 | 1.5x | 10+ transactions, >80% success |
| **silver** | 41-60 | 2x | 50+ transactions, >90% success |
| **gold** | 61-80 | 3x | 100+ transactions, >95% success |
| **platinum** | 81-100 | 5x | 500+ transactions, >99% success |

### Early Contributor Multipliers

| Cohort | Multiplier | Deadline |
|--------|------------|----------|
| First 100 | 10x | First 30 days |
| Next 400 | 5x | First 60 days |
| Next 1000 | 2x | First 90 days |

### Reputation Commands

```bash
# View own reputation
quota-router reputation show

# View provider reputation
quota-router reputation provider --name openai

# View seller reputation
quota-router reputation seller --wallet 0x...

# View leaderboard
quota-router reputation leaderboard

# Check multiplier status
quota-router reputation multiplier
```

### Reputation in Market

```
Listing displayed:
┌─────────────────────────────────────┐
│ Provider: OpenAI                    │
│ Price: 1 OCTO-W/prompt             │
│ Reputation: Gold (3x)              │
│ Success rate: 97%                  │
│ Avg latency: 200ms                  │
└─────────────────────────────────────┘
```

## Dependencies

- Mission: Token Swap Integration (should be in progress)
- Mission: Multi-Provider Support (should be in progress)

## Implementation Notes

1. **On-chain** - Reputation stored on blockchain for transparency
2. **Calculated** - Score derived from metrics, not manually set
3. **Historical** - Full history maintained

## Claimant

<!-- Add your name when claiming -->

## Pull Request

<!-- PR number when submitted -->

---

**Mission Type:** Implementation
**Priority:** Low
**Phase:** Reputation
