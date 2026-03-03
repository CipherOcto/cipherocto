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

## Agent Communication Protocol

The router agent exposes an OpenAI-compatible local API:

```typescript
// Local proxy endpoint (OpenAI-compatible)
POST http://localhost:11434/v1/chat/completions
Authorization: Bearer local-only

{
  "model": "gpt-4",
  "messages": [{"role": "user", "content": "Hello"}]
}
```

This allows existing applications to route through the quota router without code changes.

### Communication Methods

| Method | Protocol | Use Case |
|--------|----------|----------|
| **Local proxy** | HTTP (OpenAI-compatible) | Primary - existing apps |
| **CLI** | Command line | Manual control |
| **IPC** | Unix socket | Advanced integrations |

## Fallback & Retry Logic

```typescript
interface RetryConfig {
  max_retries: number;
  retry_delay_ms: number;
  backoff_multiplier: number;
  max_backoff_ms: number;
}
```

### Fallback Chain

1. Try own provider (if OCTO-W balance available)
2. Try market quota (if auto-purchase enabled)
3. Try swap (if auto-swap enabled and threshold reached)
4. Return error (no options remaining)

## Unified Provider Schema

All providers expose a common interface:

```typescript
interface UnifiedProvider {
  name: string;
  endpoint: string;
  complete(prompt: UnifiedPrompt): Promise<UnifiedResponse>;
  getBalance(): Promise<number>;
  healthCheck(): Promise<boolean>;
}

interface UnifiedPrompt {
  model: string;
  messages: Message[];
  temperature?: number;
  max_tokens?: number;
}

interface UnifiedResponse {
  content: string;
  usage: { prompt: number; completion: number; total: number };
  latency_ms: number;
}
```

---

## Cost Normalization

The router must normalize costs across different providers:

```typescript
// Model weights (compute units per request)
const MODEL_WEIGHTS = {
  'gpt-4': 10,
  'gpt-3.5-turbo': 1,
  'claude-3-opus': 12,
  'claude-3-haiku': 1,
  'gemini-pro': 2,
  // Local models: varies by hardware
};

// Calculate OCTO-W cost
function calculateCost(model: string, inputTokens: number, outputTokens: number): number {
  const baseWeight = MODEL_WEIGHTS[model] || 1;
  const tokenFactor = (inputTokens + outputTokens) / 1000;
  return Math.ceil(baseWeight * tokenFactor);
}
```

*See Research doc for complete cost normalization specification.*

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

## References

- Parent Document: docs/use-cases/ai-quota-marketplace.md
- Research: docs/research/ai-quota-marketplace-research.md

## Open Questions

1. Default policy for new users?
2. How to handle provider outages mid-request?
3. Maximum number of providers per agent?

---

**Draft Date:** 2026-03-02
