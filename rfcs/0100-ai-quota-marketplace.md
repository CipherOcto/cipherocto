# RFC-0100: AI Quota Marketplace Protocol

## Status
Draft

## Summary

Define the protocol for trading AI API quotas between developers using OCTO-W tokens as both currency and authorization grant.

## Motivation

Enable developers to:
- Contribute spare AI API quota to the network
- Earn OCTO-W tokens for contributed quota
- Purchase quota from other developers when needed
- Swap OCTO-W for other tokens (OCTO-D, OCTO)

This creates immediate utility for OCTO-W and bootstraps the developer network.

## Specification

### Core Concepts

```typescript
// Quota listing
interface QuotaListing {
  id: string;
  provider: 'openai' | 'anthropic' | 'google' | 'other';
  prompts_remaining: number;
  price_per_prompt: number; // in OCTO-W
  seller_wallet: string;
  status: 'active' | 'exhausted' | 'cancelled';
}

// Quota purchase
interface QuotaPurchase {
  listing_id: string;
  buyer_wallet: string;
  prompts_requested: number;
  total_cost: OCTO-W;
  timestamp: number;
}

// Token balance
interface QuotaRouter {
  wallet: string;
  octo_w_balance: OCTO-W;
  api_key: string; // encrypted, never transmitted
  proxy_port: number;
  status: 'online' | 'offline';
}
```

### Token Economics

| Action | Token |
|--------|-------|
| Contribute 1 prompt | +1 OCTO-W |
| Purchase 1 prompt | -1 OCTO-W |
| Minimum listing | 10 prompts |

### Routing Protocol

```typescript
interface RouterConfig {
  // Policy
  max_price_per_prompt: OCTO-W;
  preferred_providers: string[];
  fallback_enabled: boolean;
  fallback_timeout_ms: number;

  // Security
  require_minimum_balance: OCTO-W;
  auto_recharge_enabled: boolean;
  auto_recharge_source: 'wallet' | 'swap';
}
```

### Market Operations

```typescript
// List quota for sale
async function listQuota(
  prompts: number,
  pricePerPrompt: OCTO-W
): Promise<QuotaListing>;

// Purchase quota
async function purchaseQuota(
  listingId: string,
  prompts: number
): Promise<QuotaPurchase>;

// Route prompt through network
async function routePrompt(
  prompt: string,
  config: RouterConfig
): Promise<string>;
```

## Implementation

### Phase 1: Local Router

1. **Quota Router Agent** - CLI tool developers run locally
2. **API Key Management** - Encrypted local storage
3. **Balance Checking** - Before each request
4. **Request Routing** - Through local proxy

### Phase 2: Market

1. **Listing Registry** - On-chain or off-chain
2. **Order Matching** - Buyer ↔ Seller
3. **Settlement** - OCTO-W transfer
4. **State Sync** - Between agents

### Phase 3: Swaps

1. **OCTO-W ↔ OCTO-D** - At market rate
2. **OCTO ↔ OCTO-W** - At market rate
3. **Auto-swap** - When balance low, swap to continue

## Security

| Mechanism | Purpose |
|----------|---------|
| Local proxy only | API keys never leave machine |
| Balance check first | Prevent overspending |
| Stake requirement | Prevent spam/abuse |
| Reputation system | Build trust |

## Related Use Cases

- [AI Quota Marketplace for Developer Bootstrapping](../../docs/use-cases/ai-quota-marketplace.md)

## Observability

The marketplace must support logging without exposing sensitive data:

```typescript
interface MarketTelemetry {
  // What we log (no PII)
  event: 'purchase' | 'listing' | 'swap' | 'dispute';
  timestamp: number;
  provider: string;
  octo_w_amount: number;
  latency_ms: number;
  success: boolean;

  // What we DON'T log
  // - Prompt content
  // - API keys
  // - Wallet addresses (use hash instead)
}
```

## Security & Privacy

| Concern | Mitigation |
|---------|------------|
| API key exposure | Local proxy only, keys never transmitted |
| Prompt privacy | End-to-end encryption, marketplace sees metadata only |
| Wallet privacy | Pseudonymous addresses |
| Data residency | No central storage |

## Related RFCs

- RFC-0101: Quota Router Agent Specification
- RFC-XXXX: Token Swap Protocol (future)
- RFC-XXXX: Reputation System (future)

## References

- Parent Document: docs/use-cases/ai-quota-marketplace.md
- Research: docs/research/ai-quota-marketplace-research.md

## Open Questions

1. On-chain vs off-chain listing registry?
2. Minimum stake for sellers?
3. How to handle failed requests (refund OCTO-W)?

---

**Draft Date:** 2026-03-02
