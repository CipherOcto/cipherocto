# RFC-0101: Quota Router Agent Specification

## Status
Draft

## Summary

Define the agent-based quota router that handles prompt routing based on user policy, OCTO-W balance, and market availability.

## Motivation

Create a customizable routing system where:
- Developers can define their own policies
- Routing behavior is agent-driven
- Security is maintained (keys never leave machine)
- Multiple strategies can be employed

## Specification

### Agent Architecture

```typescript
interface QuotaRouterAgent {
  // Identity
  agent_id: string;
  owner_wallet: string;

  // Configuration
  config: RouterConfig;

  // State
  balance: OCTO-W;
  status: AgentStatus;

  // Capabilities
  providers: Provider[];
}
```

### Router Policies

```typescript
type RoutingPolicy =
  | 'cheapest'
  | 'fastest'
  | 'quality'
  | 'balanced'
  | 'custom';

interface RouterConfig {
  // Policy selection
  policy: RoutingPolicy;

  // Price limits
  max_price_per_prompt: OCTO-W;
  monthly_budget: OCTO-W;

  // Provider preferences
  preferred_providers: string[];
  fallback_order: string[];

  // Auto-recharge
  auto_recharge_enabled: boolean;
  auto_recharge_threshold: OCTO-W;
  recharge_source: 'wallet' | 'swap-osto-d' | 'swap-octo';

  // Fallback
  allow_market_purchase: boolean;
  market_max_price: OCTO-W;
}
```

### Policy Behaviors

| Policy | Behavior |
|--------|----------|
| **cheapest** | Route to cheapest available provider |
| **fastest** | Route to fastest responding provider |
| **quality** | Prefer higher-quality models |
| **balanced** | Mix of price/speed/quality |
| **custom** | User-defined rules |

### Request Flow

```
User calls: agent.routePrompt("Hello")
              │
              ▼
         Check OCTO-W balance
              │
     ┌───────┴───────┐
     ▼               ▼
  Enough          Not enough
     │               │
     ▼               ▼
 Check policy    Check auto-recharge
     │               │
     ▼               ▼
Find provider   Attempt recharge
     │               │
     ▼               ▼
Route request  If failed: return error
     │               │
     ▼               ▼
Return result  If success: route request
```

### Provider Integration

```typescript
interface Provider {
  name: string;
  api_type: 'openai' | 'anthropic' | 'google' | 'custom';

  // Connection
  endpoint: string;
  auth_method: 'bearer' | 'api-key';

  // State
  status: 'available' | 'rate-limited' | 'error';

  // Metrics
  latency_ms: number;
  success_rate: number;
}
```

### Local Security

```typescript
// API keys stored locally, encrypted
interface SecureKeyStore {
  // Keys never transmitted
  store(provider: string, encryptedKey: Buffer): void;

  // Only gives access, not the key
  createProviderAccess(provider: string): ProviderAccess;

  // Keys never leave this machine
}

// Request routing - key used locally only
async function routeWithKey(
  provider: Provider,
  prompt: string,
  key: SecureKeyRef
): Promise<string>;
```

### Market Integration

```typescript
interface MarketClient {
  // Query available quota
  async findListings(
    provider: string,
    minPrompts: number,
    maxPrice: OCTO-W
  ): Promise<QuotaListing[]>;

  // Purchase from market
  async purchaseFromMarket(
    listingId: string,
    prompts: number
  ): Promise<QuotaPurchase>;

  // Get current prices
  async getMarketPrices(): Promise<MarketPrices>;
}
```

### Token Swaps

```typescript
interface SwapClient {
  // Swap tokens
  async swap(
    fromToken: 'OCTO-W' | 'OCTO-D' | 'OCTO',
    toToken: 'OCTO-W' | 'OCTO-D' | 'OCTO',
    amount: bigint
  ): Promise<SwapResult>;

  // Get exchange rate
  async getRate(
    fromToken: string,
    toToken: string
  ): Promise<bigint>;
}
```

## Implementation

### MVE Features

1. **Local router** - Basic prompt routing with single provider
2. **Balance display** - Show OCTO-W balance
3. **Manual list** - List quota for sale (CLI command)
4. **Basic policy** - cheapest/fallback only

### Phase 2 Features

1. **Multi-provider** - Support multiple API providers
2. **Market integration** - Auto-purchase from market
3. **Auto-recharge** - Swap when balance low
4. **Policy engine** - All policy types

### Phase 3 Features

1. **Custom policies** - User-defined rules
2. **Reputation** - Provider trust scores
3. **Analytics** - Usage dashboards

## Rationale

### Why Agent-Based?

1. **Customizable** - Each developer can define their own policy
2. **Portable** - Agent moves with the developer
3. **Flexible** - New policies can be added easily
4. **Decentralized** - No central router, peer-to-peer

### Why Local Key Storage?

1. **Security** - API keys never transmitted
2. **Trust** - No third-party holds credentials
3. **Simplicity** - No key management infrastructure

## Related Use Cases

- [AI Quota Marketplace for Developer Bootstrapping](../../docs/use-cases/ai-quota-marketplace.md)

## Related RFCs

- RFC-0100: AI Quota Marketplace Protocol

## Open Questions

1. Default policy for new users?
2. How to handle provider outages mid-request?
3. Maximum number of providers per agent?

---

**Draft Date:** 2026-03-02
