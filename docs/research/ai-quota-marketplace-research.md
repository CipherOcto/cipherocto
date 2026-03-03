# Research: AI Quota Marketplace for Developer Bootstrapping

## Executive Summary

This research investigates using AI API quota trading as a bootstrapping mechanism for CipherOcto. Developers would contribute spare API quota to the network, earning OCTO-W tokens that can be used to purchase more quota when needed or swapped for other tokens. The goal is to create a self-bootstrapping ecosystem where early contributors receive multiplier rewards.

## Problem Statement

CipherOcto needs a mechanism to:
1. Attract initial developers to the network
2. Create utility for OCTO-W token immediately
3. Enable developers to monetize unused AI API quotas
4. Create a marketplace for quota trading across timezones

## Research Scope

- What's included: Token mechanics, marketplace design, security model
- What's excluded: Blockchain implementation details, specific provider integrations

---

## Personas

| Persona | Role | Description |
|---------|------|-------------|
| **Provider** | Seller | Developer with unused AI API quota who lists it on the market |
| **Consumer** | Buyer | Developer who needs more quota than they have |
| **Router** | Infrastructure | Agent that routes prompts based on policy and balance |

---

## Findings

### Similar Approaches

| Project | Approach | Lessons |
|--------|----------|---------|
| GPU.miners | Compute sharing | Works but limited to GPU compute |
| API marketplaces | Centralized reselling | Trust issues, fees |
| Timezone arbitrage | Existing in informal networks | Proves demand exists |

### Technical Requirements

1. **Local Proxy** - Routes API requests without exposing keys
2. **Quota Metering** - Track usage per 5-hour window
3. **Market Engine** - Match buyers/sellers, settle in OCTO-W
4. **Token Swaps** - OCTO-W ↔ OCTO-D ↔ OCTO

### Latency Considerations

| Scenario | Expected Latency | Notes |
|----------|-----------------|-------|
| Direct (no market) | 100-500ms | Normal API latency |
| Market route | +50-200ms | Network hop through seller proxy |
| Multi-route | +100-500ms | Fallback through multiple sellers |

**Acceptable degradation:** Up to 2x baseline latency acceptable for market-sourced quota.

### Market Dynamics

| Model | Description | Pros | Cons |
|-------|-------------|------|------|
| **Fixed price** | Set price per prompt, static | Simple, predictable | May not reflect demand |
| **Dynamic AMM** | Automated market maker | Real-time pricing | Complex to implement |
| **Auction** | Bid for quota | Efficient pricing | Slower execution |
| **Reputation-weighted** | Higher rep = better price | Incentivizes quality | Requires reputation first |

**Recommendation:** Start with fixed price, evolve to reputation-weighted as network matures.

### Cost Normalization

Different providers have different pricing structures:

| Provider | Model | Cost per 1K input tokens | Cost per 1K output tokens |
|----------|-------|--------------------------|--------------------------|
| OpenAI | GPT-4 | $0.01 | $0.03 |
| OpenAI | GPT-3.5 | $0.0005 | $0.0015 |
| Anthropic | Claude 3 Opus | $0.015 | $0.075 |
| Anthropic | Claude 3 Haiku | $0.00025 | $0.00125 |
| Google | Gemini Pro | $0.00125 | $0.005 |

**Solution: Compute Units**

```typescript
// Normalize all models to compute units
const MODEL_WEIGHTS = {
  'gpt-4': 10,      // 10 units per prompt
  'gpt-3.5-turbo': 1, // 1 unit per prompt
  'claude-3-opus': 12,
  'claude-3-haiku': 1,
  'gemini-pro': 2,
};

// OCTO-W cost = base_units × model_weight
const BASE_COST = 1; // 1 OCTO-W minimum
```

This allows 1 OCTO-W to represent ~equivalent compute across providers.

### Token Mint/Burn Rules

| Event | Action | Details |
|-------|--------|---------|
| **List quota** | Mint | OCTO-W minted on successful listing |
| **Use quota** | Burn | OCTO-W burned on successful prompt delivery |
| **Dispute** | Slash | From seller stake, buyer refunded |
| **Listing cancelled** | No burn | Unused OCTO-W remains (no inflation) |

**Inflation Control:**
- Maximum OCTO-W supply: 1B tokens
- Mint only on verified usage (not listing)
- Protocol treasury provides initial liquidity

### Security Considerations

- API keys never leave developer's machine
- Requests routed through local proxy only
- OCTO-W balance required for each request
- No central authority holds credentials

### Prompt Privacy (Critical Clarification)

**IMPORTANT:** The current design routes prompts through seller's proxy, meaning:
- Seller **will see prompt content** when executing API calls
- This is a **trust assumption**, not a cryptographic guarantee
- End-to-end encryption is **NOT** currently implemented

**Future options to explore:**
- Trusted Execution Environments (TEE) for seller proxies
- ZK proofs of inference (research phase)
- TEE + remote attestation

For MVE, we accept this trust model with reputation as the mitigation.

### Dispute Resolution

| Issue | Resolution |
|-------|------------|
| Failed prompt after payment | Seller reputation penalty, refund from stake |
| Garbage/invalid response | Reputation hit, auto-blacklist |
| Seller offline mid-request | Retry with fallback provider |
| Insufficient OCTO-W | Request rejected before routing |

**Mechanism:** Sellers stake OCTO-W. If dispute proven, stake slashed and buyer refunded.

## Token Economics

| Token | Role |
|-------|------|
| OCTO-W | Quota currency + authorization grant |
| OCTO-D | Developer rewards |
| OCTO | Governance |

**Key insight:** OCTO-W serves as both currency AND metered access - 1 OCTO-W = 1 prompt request.

## Recommendations

### Recommended Approach

Implement as agent-based system where:
- Dev runs local quota router (agent)
- Agent policy determines routing behavior
- Marketplace is agent-to-agent

### Risks

| Risk | Mitigation |
|------|------------|
| API key exposure | Local proxy only, keys never leave machine |
| Abuse | Reputation system, stake requirements |
| Low liquidity | Bootstrap with early contributor incentives |

### Next Steps

- Create Use Case? **Yes**
- Explore further: Specific provider APIs, rate limiting

---

## References

- Parent Document: BLUEPRINT.md
- Leads to: docs/use-cases/ai-quota-marketplace.md

---

**Research Status:** Complete
**Recommended Action:** Proceed to Use Case
