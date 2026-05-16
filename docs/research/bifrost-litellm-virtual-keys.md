# Research: Virtual Keys Deep Comparison - Bifrost vs LiteLLM

**Date:** 2026-05-11  
**Status:** Companion Research  
**Parent:** [Bifrost vs LiteLLM Comparison](./bifrost-litellm-comparison.md)

---

## Table of Contents

1. [Data Structures](#1-data-structures)
2. [Budget Hierarchy](#2-budget-hierarchy)
3. [Rate Limiting](#3-rate-limiting)
4. [Enforcement Flow](#4-enforcement-flow)
5. [API Surface](#5-api-surface)
6. [In-Memory vs Persistent Storage](#6-in-memory-vs-persistent-storage)
7. [Self-Service Features](#7-self-service-features)

---

## 1. Data Structures

### 1.1 Bifrost Virtual Key Data Model

```go
type TableVirtualKey struct {
    ID              string
    Name            string
    Description     string
    Value           string                           // The actual VK string
    IsActive        bool
    TeamID          *string                          // FK to team
    CustomerID      *string                          // FK to customer
    Budgets         []TableBudget                    // Associated budgets
    RateLimit       *TableRateLimit                  // Rate limit for this VK
    ProviderConfigs []TableVirtualKeyProviderConfig // Per-provider config
}

type TableBudget struct {
    ID            string    // Primary key
    MaxLimit      float64   // Maximum spend allowed
    ResetDuration string    // e.g., "30s", "5m", "1h", "1d", "1w", "1M", "1Y"
    LastReset     time.Time // When budget was last reset
    CurrentUsage  float64   // Current spend
    VirtualKeyID  *string   // FK to VK
    TeamID        *string   // FK to team
    CustomerID    *string   // FK to customer
}

type TableRateLimit struct {
    ID                   string // Primary key
    TokenMaxLimit        *int64 // Max tokens per window
    TokenResetDuration   *string // Token window duration
    RequestMaxLimit      *int64 // Max requests per window
    RequestResetDuration *string // Request window duration
}

type TableVirtualKeyProviderConfig struct {
    Provider      string            // e.g., "openai", "anthropic"
    AllowedModels []string          // Models VK can use
    KeyIDs        []string          // Allowed provider key IDs
    Weight        float64           // Load balancing weight
    Budgets       []TableBudget     // Per-provider budgets
    RateLimit     *TableRateLimit   // Per-provider rate limits
}
```

### 1.2 LiteLLM Virtual Key Data Model

```python
class UserAPIKeyDict(TypedDict):
    token: str                          # Hashed API key
    key_alias: Optional[str]            # Human-readable alias
    key_name: Optional[str]             # Key name
    user_id: Optional[str]              # Owner user ID
    team_id: Optional[str]              # Owner team ID
    organization_id: Optional[str]       # Owner org ID
    max_budget: Optional[float]        # Key-level budget cap
    budget_duration: Optional[str]       # Budget reset period
    spend: float                       # Current spend
    max_parallel_requests: Optional[int]
    rpm_limit: Optional[int]            # Requests per minute
    tpm_limit: Optional[int]           # Tokens per minute
    allowed_model_region: Optional[str]
    model_max_budget: Optional[dict]    # Per-model budgets
    model_access_set: Optional[set]     # Allowed models
    blocked: bool
    litellm_budget_table: LiteLLM_BudgetTable
```

---

## 2. Budget Hierarchy

### 2.1 Bifrost Budget Hierarchy

```mermaid
graph TB
    subgraph Customer["Customer Level"]
        C_Budget[Customer Budget<br/>MaxLimit, ResetDuration]
    end
    
    subgraph Team["Team Level"]
        T_Budget[Team Budget<br/>MaxLimit, ResetDuration]
    end
    
    subgraph VK["Virtual Key Level"]
        VK_Budget[VK Budget<br/>MaxLimit, ResetDuration]
        VK_RateLimit[VK Rate Limit<br/>TokenMax, RequestMax]
        
        subgraph ProviderConfig["Per-Provider Config"]
            PC1_Budget[OpenAI Budget]
            PC1_RL[OpenAI Rate Limit]
            PC2_Budget[Anthropic Budget]
            PC2_RL[Anthropic Rate Limit]
        end
    end
    
    Customer --> Team
    Team --> VK
    VK --> ProviderConfig
    
    style Customer fill:#e1f5fe
    style Team fill:#fff3e0
    style VK fill:#e8f5e9
    style ProviderConfig fill:#fce4ec
```

### 2.2 LiteLLM Budget Hierarchy

```mermaid
graph TB
    subgraph Org["Organization Level"]
        O_Budget[Organization Budget<br/>max_budget]
    end
    
    subgraph TeamLevel["Team Level"]
        Tm_Budget[Team Budget<br/>max_budget, soft_budget]
    end
    
    subgraph Key["Key Level"]
        K_Budget[Key Budget<br/>max_budget]
        
        subgraph ModelBudget["Per-Model Budget"]
            M1_Budget[Model: gpt-4<br/>budget_limit]
            M2_Budget[Model: claude-3<br/>budget_limit]
        end
    end
    
    Org --> TeamLevel
    TeamLevel --> Key
    Key --> ModelBudget
    
    style Org fill:#e1f5fe
    style TeamLevel fill:#fff3e0
    style Key fill:#e8f5e9
    style ModelBudget fill:#fce4ec
```

### 2.3 Budget Resolution Comparison

```mermaid
graph LR
    subgraph Bifrost["Bifrost Budget Resolution"]
        B1[Collect ALL budgets<br/>from hierarchy] --> B2[Check each budget<br/>in parallel]
        B2 --> B3{ANY budget<br/>has capacity?}
        B3 -->|Yes| B4[ALLOW request]
        B3 -->|No| B5[DENY request]
    end
    
    subgraph LiteLLM["LiteLLM Budget Resolution"]
        L1[Check Org Budget] --> L2{Exceeded?}
        L2 -->|Yes| L8[DENY - Org exceeded]
        L2 -->|No| L3[Check Team Budget]
        L3 --> L4{Exceeded?}
        L4 -->|Yes| L7[DENY - Team exceeded]
        L4 -->|No| L5[Check Key Budget]
        L5 --> L6{Exceeded?}
        L6 -->|Yes| L9[DENY - Key exceeded]
        L6 -->|No| L10[ALLOW request]
    end
    
    style Bifrost fill:#e8f5e9
    style LiteLLM fill:#fff3e0
```

### 2.4 Hierarchy Comparison Table

| Aspect | Bifrost | LiteLLM |
|--------|---------|---------|
| **Levels** | 4 (Customer → Team → VK → Provider) | 4 (Org → Team → Key → Model) |
| **Parallel Check** | Yes - any budget with capacity allows | No - sequential, first failure wins |
| **Per-Provider** | Yes - full budget hierarchy | No - only model-level |
| **Customer Level** | Yes | No (organization) |
| **Self-Service** | Yes - quota endpoint | No |

---

## 3. Rate Limiting

### 3.1 Bifrost Rate Limiting Architecture

```mermaid
graph TB
    subgraph Request["Incoming Request"]
        Tokens[Token Count]
        ReqID[Request ID]
    end
    
    subgraph RateLimitResolver["Rate Limit Resolver"]
        direction TB
        RL1[Collect Rate Limits<br/>from hierarchy] --> RL2[Check Token Usage<br/>Sliding Window]
        RL2 --> RL3[Check Request Usage<br/>Sliding Window]
        RL3 --> RL4{Within Limits?}
        RL4 -->|Yes| RL5[ALLOW]
        RL4 -->|No| RL6[DENY<br/>Return Retry-After]
    end
    
    Request --> RateLimitResolver
    
    subgraph RateLimits["Rate Limit Sources"]
        RL_VK[VK Rate Limit<br/>TokenMax, RequestMax]
        RL_PC1[Provider Config RL<br/>OpenAI]
        RL_PC2[Provider Config RL<br/>Anthropic]
    end
    
    RateLimits --> RateLimitResolver
    
    style RateLimitResolver fill:#e3f2fd
    style RateLimits fill:#fce4ec
```

### 3.2 LiteLLM Rate Limiting Architecture

```mermaid
graph TB
    subgraph Request["Incoming Request"]
        Tokens[Token Count]
        ReqCount[Request Count]
    end
    
    subgraph ParallelLimiter["Parallel Request Limiter v3"]
        direction TB
        PL1[Check RPM<br/>requests_made vs rpm_limit]
        PL1 --> PL2[Check TPM<br/>current_tpm vs tpm_limit]
        PL2 --> PL3{Within Limits?}
        PL3 -->|Yes| PL4[ALLOW]
        PL3 -->|No| PL5[DENY<br/>Fixed 60s window]
    end
    
    Request --> ParallelLimiter
    
    subgraph LimitSource["Limit Source"]
        L_RPM[RPM from key<br/>metadata]
        L_TPM[TPM from key<br/>metadata]
    end
    
    LimitSource --> ParallelLimiter
    
    style ParallelLimiter fill:#e3f2fd
    style LimitSource fill:#fff3e0
```

### 3.3 Rate Limiting Comparison

| Aspect | Bifrost | LiteLLM |
|--------|---------|---------|
| **Token Limits** | Native sliding window | Derived from TPM |
| **Request Limits** | Native sliding window | Derived from RPM |
| **Sliding Window** | Yes - precise tracking | Fixed 1-minute window |
| **Provider-Level** | Yes - per VK provider config | No - global only |
| **Distributed Sync** | Via Redis | Via Redis dual-cache |
| **Custom Durations** | Yes - any duration | No - fixed 1 minute |

---

## 4. Enforcement Flow

### 4.1 Bifrost Enforcement Flow

```mermaid
flowchart TD
    A[Request arrives] --> B{Extract VK<br/>from Header}
    B -->|Found| C[Lookup VK with<br/>Full Hierarchy]
    B -->|Not Found| Z[DENY - No VK]
    
    C --> D[Create EvaluationRequest]
    
    D --> E[BudgetResolver.CheckBudget]
    D --> F[RateLimitResolver.CheckRateLimit]
    
    E -->|BudgetResult| G{AllowRequest?}
    F -->|RateLimitResult| H{Allowed?}
    
    G -->|Yes| I{Allowed?}
    G -->|No| J[DENY - Budget<br/>exhausted]
    H -->|Yes| I
    H -->|No| K[DENY - Rate limit<br/>exceeded]
    
    I -->|Yes| L[Proceed to<br/>Routing Layer]
    I -->|No| M[DENY - Combined<br/>Decision]
    
    L --> N[Return Decision:<br/>ALLOW]
    J --> N
    K --> N
    M --> N
    
    style A fill:#e8f5e9
    style L fill:#c8e6c9
    style Z fill:#ffcdd2
    style J fill:#ffcdd2
    style K fill:#ffcdd2
    style M fill:#ffcdd2
```

### 4.2 LiteLLM Enforcement Flow

```mermaid
flowchart TD
    A[Request arrives] --> B[user_api_key_auth.py<br/>auth_check]
    
    B --> C[get_user_api_key_info<br/>from DB/Cache]
    C --> D[Get UserAPIKeyDict]
    
    D --> E{Org Budget<br/>Exceeded?}
    E -->|Yes| Z[DENY - Org budget<br/>BudgetExceededError]
    E -->|No| F{Team Budget<br/>Exceeded?}
    
    F -->|Yes| Y[DENY - Team budget<br/>BudgetExceededError]
    F -->|No| G{Key Budget<br/>Exceeded?}
    
    G -->|Yes| X[DENY - Key budget<br/>BudgetExceededError]
    G -->|No| H{Model Budget<br/>Exceeded?}
    
    H -->|Yes| W[DENY - Model budget<br/>BudgetExceededError]
    H -->|No| I[parallel_request_limiter<br/>Check RPM/TPM]
    
    I --> J{Within<br/>Limits?}
    J -->|No| V[DENY - Rate limit<br/>ProxyRateLimitError]
    J -->|Yes| K[Proceed to<br/>Router]
    
    K --> L[Return allowed=True]
    Z --> L
    Y --> L
    X --> L
    W --> L
    V --> L
    
    style A fill:#e8f5e9
    style K fill:#c8e6c9
    style Z fill:#ffcdd2
    style Y fill:#ffcdd2
    style X fill:#ffcdd2
    style W fill:#ffcdd2
    style V fill:#ffcdd2
```

### 4.3 Enforcement Comparison Table

| Aspect | Bifrost | LiteLLM |
|--------|---------|---------|
| **Auth Layer** | Governance plugin | Auth middleware |
| **Budget Check** | Parallel (any capacity) | Sequential (fail-fast) |
| **Rate Limit Check** | Separate resolver | Hook system |
| **Decision Time** | After budget evaluation | During auth phase |
| **VK Lookup** | Full hierarchy included | Separate queries |
| **Error Codes** | Structured `Decision` enum | Generic exceptions |

---

## 5. API Surface

### 5.1 Bifrost API Flow

```mermaid
sequenceDiagram
    participant Admin
    participant BifrostAPI as Bifrost API
    participant Store as Governance Store
    participant Redis as Redis Cache
    
    Admin->>BifrostAPI: POST /api/governance/virtual-keys
    Note over Admin,BifrostAPI: Create VK with budget & rate limits
    
    BifrostAPI->>Store: CreateVirtualKey(vk)
    Store->>Store: Write to PostgreSQL
    Store->>Redis: Invalidate cache
    Store->>Store: Update in-memory store
    BifrostAPI-->>Admin: 201 Created
    
    Note over Admin,BifrostAPI: Self-service quota (no admin auth)
    
    participant User
    User->>BifrostAPI: GET /api/governance/virtual-keys/quota
    Note over User,BifrostAPI: VK value = auth credential
    
    BifrostAPI->>Store: GetVirtualKeyWithUsage(vkValue)
    Store-->>BifrostAPI: {budgets, rateLimit, remaining}
    BifrostAPI-->>User: {virtual_key_name, budgets, rate_limit}
```

### 5.2 LiteLLM API Flow

```mermaid
sequenceDiagram
    participant Admin
    participant LiteLLMAPI as LiteLLM Proxy API
    participant DB as SQLAlchemy DB
    participant Redis as Redis Cache
    participant Memory as In-Memory Cache
    
    Admin->>LiteLLMAPI: POST /api/key/generate
    Note over Admin,LiteLLMAPI: Create key (team_id, max_budget, rpm_limit)
    
    LiteLLMAPI->>DB: Insert into LiteLLM_TeamMemberTable
    DB-->>LiteLLMAPI: key created
    LiteLLMAPI-->>Admin: {api_key, key_alias}
    
    Note over Admin,LiteLLMAPI: No self-service quota endpoint
    
    participant User
    User->>LiteLLMAPI: GET /api/key/info?api_key=sk-...
    Note over User,LiteLLMAPI: Must be key owner or admin
    
    LiteLLMAPI->>Memory: get_cache(api_key)
    Memory-->>LiteLLMAPI: cached?
    alt Cache miss
        LiteLLMAPI->>Redis: async_get_cache(api_key)
        Redis-->>LiteLLMAPI: cached?
        alt Redis miss
            LiteLLMAPI->>DB: get_key_info(api_key)
            DB-->>LiteLLMAPI: key info
        end
        LiteLLMAPI->>Redis: async_set_cache(api_key)
        LiteLLMAPI->>Memory: set_cache(api_key)
    end
    LiteLLMAPI-->>User: {key, spend, max_budget}<br/>⚠️ NO remaining quota
```

### 5.3 API Comparison Table

| Endpoint | Bifrost | LiteLLM |
|----------|---------|---------|
| Create VK | `POST /virtual-keys` | `POST /key/generate` |
| Self-service quota | Yes | No |
| Budget reset | Automatic | Budget duration |
| Per-provider config | Yes | No |
| Key restrictions | `key_ids` array | `models` array |
| Team assignment | FK reference | Separate endpoint |

---

## 6. Storage Architecture

### 6.1 Bifrost Storage Architecture

```mermaid
graph TB
    subgraph Write["Write Path"]
        W1[CreateVirtualKey] --> W2[Write to PostgreSQL]
        W2 --> W3[Invalidate Redis]
        W3 --> W4[Update In-Memory]
    end
    
    subgraph Read["Read Path"]
        R1[Read Request] --> R2{Redis Cache<br/>Hit?}
        R2 -->|Yes| R3[Return from Redis]
        R2 -->|No| R4{In-Memory<br/>Hit?}
        R4 -->|Yes| R5[Return from Memory]
        R4 -->|No| R6[Query PostgreSQL]
        R6 --> R7[Populate Caches]
        R7 --> R3
    end
    
    subgraph Stores["Storage Backends"]
        PG[(PostgreSQL<br/>SQLite)]
        RED[(Redis)]
        MEM[(&)sync.Map<br/>In-Memory]
    end
    
    W2 -.-> PG
    W3 -.-> RED
    W4 -.-> MEM
    R6 -.-> PG
    R7 -.-> RED
    R7 -.-> MEM
    
    style Write fill:#e8f5e9
    style Read fill:#e3f2fd
```

### 6.2 LiteLLM Storage Architecture

```mermaid
graph TB
    subgraph Read["Read Path"]
        R1[Read Request] --> R2{Memory Cache<br/>Hit?}
        R2 -->|Yes| R3[Return from Memory]
        R2 -->|No| R4{Redis Cache<br/>Hit?}
        R4 -->|Yes| R5[Return from Redis<br/>Populate Memory]
        R4 -->|No| R6[Query PostgreSQL]
        R6 --> R7[Populate Both Caches]
        R7 --> R3
    end
    
    subgraph Write["Write Path"]
        W1[Increment Spend] --> W2[Redis INCR<br/>Atomic]
        W2 --> W3[Background<br/>Sync to DB]
    end
    
    subgraph Stores["Storage Backends"]
        DB[(PostgreSQL<br/>MySQL)]
        RED[(Redis)]
        MEM[(In-Memory<br/>Dict)]
    end
    
    R6 -.-> DB
    W3 -.-> DB
    R7 -.-> RED
    R7 -.-> MEM
    W2 -.-> RED
    
    style Read fill:#e3f2fd
    style Write fill:#fff3e0
```

### 6.3 Storage Comparison Table

| Aspect | Bifrost | LiteLLM |
|--------|---------|---------|
| **Primary Store** | PostgreSQL/SQLite | PostgreSQL/MySQL |
| **Cache** | Redis (explicit) | Redis + In-Memory |
| **Write Pattern** | Write-through | Write-through |
| **Read Pattern** | Cache-first | Cache-first |
| **Distributed** | Redis sync | Redis dual-cache |
| **Transaction Safety** | GORM transactions | SQLAlchemy transactions |

---

## 7. Self-Service Features

### 7.1 Bifrost Self-Service Flow

```mermaid
sequenceDiagram
    participant User
    participant API as Bifrost API
    participant Store as Governance Store
    
    Note over User: Self-service endpoint - no admin auth!
    
    User->>API: GET /api/governance/virtual-keys/quota<br/>X-Virtual-Key: sk-bf-xxx
    
    API->>API: Extract VK from header
    API->>Store: GetVirtualKeyWithUsage(vkValue)
    
    Store-->>API: VK with all budgets & rate limits
    
    API->>API: Calculate remaining for each budget<br/>budget.MaxLimit - budget.CurrentUsage
    
    API-->>User: {
      virtual_key_name: "my-vk",
      is_active: true,
      budgets: [{
        id: "budget-123",
        max_limit: 100.00,
        current_usage: 45.50,
        remaining: 54.50,
        next_reset: "2026-06-01"
      }],
      rate_limit: {
        token_max_limit: 100000,
        token_usage: 25000,
        token_remaining: 75000,
        token_reset: "2026-05-11T16:00:00Z"
      }
    }
```

### 7.2 LiteLLM Self-Service (None)

```mermaid
sequenceDiagram
    participant User
    participant API as LiteLLM API
    participant DB as Database
    
    Note over User,API: NO self-service quota endpoint exists
    
    User->>API: GET /api/key/info?api_key=sk-...
    
    API->>API: Check: is admin OR owns key?
    
    alt Not authorized
        API-->>User: 403 Forbidden
    else Authorized
        API->>DB: get_key_info(api_key)
        DB-->>API: {key, spend, max_budget}
        API-->>User: {key: "sk-...xxx", spend: 45.50, max_budget: 100.00}<br/>⚠️ NO remaining calculation
    end
```

### 7.3 Self-Service Comparison Table

| Feature | Bifrost | LiteLLM |
|---------|---------|---------|
| **Quota endpoint** | Yes | No |
| **Remaining budget** | Yes | No (admin only) |
| **Rate limit status** | Yes | No |
| **Usage breakdown** | Yes | Partial |
| **Reset time** | Yes | No |
| **No admin auth** | Yes | No |

---

## 8. Key Feature Matrix

| Feature | Bifrost | LiteLLM |
|---------|---------|---------|
| Multi-tenant VKs | ✅ | ✅ |
| Budget hierarchy (4+ levels) | ✅ | ✅ |
| Per-provider budgets | ✅ | ❌ |
| Per-provider rate limits | ✅ | ❌ |
| Sliding window rate limits | ✅ | ❌ (fixed 1-min) |
| Key ID restrictions | ✅ | ❌ |
| Self-service quota | ✅ | ❌ |
| Distributed sync | ✅ | ✅ |
| In-memory cache | ✅ | ✅ |
| Redis cache | ✅ | ✅ |
| Transaction safety | ✅ | ✅ |
| Fail-fast budget check | ❌ (parallel) | ✅ |
| Soft budget (warnings) | ❌ | ✅ |

---

## 9. Summary

### Bifrost Advantages
- **Parallel budget evaluation**: Any budget with capacity allows the request
- **Per-provider configuration**: Granular control at provider level
- **Self-service quota**: Users can check their own usage without admin
- **Sliding window rate limits**: More accurate than fixed windows
- **Key ID restrictions**: Limit VK to specific provider API keys

### LiteLLM Advantages
- **Simpler model**: Sequential check is easier to understand
- **Soft budgets**: Warning before hard limit
- **Model access set**: Simple model restrictions
- **Mature ecosystem**: More integrations, larger community

---

**Next Steps:**
- [ ] Research: Load Balancing Deep Comparison
- [ ] Research: Caching Deep Comparison
- [ ] Research: Retry & Fallback Deep Comparison
- [ ] Research: Observability Deep Comparison