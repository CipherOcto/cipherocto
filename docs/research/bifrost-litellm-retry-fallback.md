# Research: Retry & Fallback Deep Comparison - Bifrost vs LiteLLM

**Date:** 2026-05-11  
**Status:** Companion Research  
**Parent:** [Bifrost vs LiteLLM Comparison](./bifrost-litellm-comparison.md)

---

## Table of Contents

1. [Retry Architecture](#1-retry-architecture)
2. [Retry Strategies](#2-retry-strategies)
3. [Error Classification](#3-error-classification)
4. [Backoff Algorithms](#4-backoff-algorithms)
5. [Fallback Mechanisms](#5-fallback-mechanisms)
6. [Rate Limit Handling](#6-rate-limit-handling)
7. [Timeout Handling](#7-timeout-handling)
8. [Configuration](#8-configuration)

---

## 1. Retry Architecture

### 1.1 Bifrost Retry Architecture

```mermaid
graph TB
    subgraph Request["Incoming Request"]
        Req[LLM Request]
        VK[Virtual Key]
    end
    
    subgraph RetryLayer["Bifrost Retry Layer"]
        direction TB
        Check{Retriable?<br/>Error Check}
        Backoff[Calculate<br/>Backoff]
        Wait[Sleep<br/>with Jitter]
        Retry[Retry<br/>Request]
        Fail[Final<br/>Failure]
    end
    
    subgraph ErrorTypes["Error Classification"]
        Net[Network Error<br/>Timeout, DNS]
        Rate[Rate Limit<br/>429, 429 with Retry-After]
        Server[Server Error<br/>500, 502, 503]
        Auth[Auth Error<br/>401, 403]
    end
    
    Request --> RetryLayer
    ErrorTypes --> Check
    
    style RetryLayer fill:#e8f5e9
    style ErrorTypes fill:#fff3e0
```

### 1.2 LiteLLM Retry Architecture

```mermaid
graph TB
    subgraph Request["Incoming Request"]
        Req[LLM Request]
        Key[API Key]
    end
    
    subgraph RetryLayer["LiteLLM Retry Layer"]
        direction TB
        Intercept[Exception<br/>Interceptor]
        Classify{Error<br/>Classification}
        Retry[Retry with<br/>Backoff]
        Cooldown[Mark<br/>Cooldown]
        Router[Route to<br/>Fallback]
    end
    
    subgraph ErrorTypes["Error Types"]
        E429[RateLimitError]
        E500[InternalServerError]
        ETimeout[TimeoutError]
        EAuth[AuthenticationError]
    end
    
    Request --> RetryLayer
    ErrorTypes --> Classify
    
    style RetryLayer fill:#e3f2fd
```

---

## 2. Retry Strategies

### 2.1 Bifrost Retry Flow

```mermaid
flowchart TD
    A[Request Execute] --> B{Success?}
    
    B -->|Yes| C[Return Response]
    
    B -->|No| D{Retriable<br/>Error?}
    
    D -->|Yes| E{Attempts<br/>< Max Retries}
    
    E -->|Yes| F[Calculate<br/>Backoff]
    
    F --> G[Sleep<br/>with Jitter]
    
    G --> H[Increment<br/>Attempt Counter]
    
    H --> A
    
    E -->|No| I[Final<br/>Failure]
    
    D -->|No| I
    
    style C fill:#c8e6c9
    style I fill:#ffcdd2
```

### 2.2 LiteLLM Retry Flow

```mermaid
flowchart TD
    A[Call LLM] --> B{Exception<br/>Occurred?}
    
    B -->|No| C[Return Response]
    
    B -->|Yes| D{Rate<br/>Limit?}
    
    D -->|Yes| E[Get Retry-After<br/>from Header]
    
    E --> F[Sleep Retry-After<br/>+ 2s buffer]
    
    D -->|No| G{Max<br/>Retries?}
    
    G -->|Yes| H[Check<br/>Fallbacks]
    
    G -->|No| I[Retry<br/>Request]
    
    H --> J{More<br/>Fallbacks?}
    
    J -->|Yes| K[Route to<br/>Next Fallback]
    
    J -->|No| L[Final<br/>Failure]
    
    I --> A
    K --> A
    
    style C fill:#c8e6c9
    style L fill:#ffcdd2
```

### 2.3 Retry Strategy Comparison

| Aspect | Bifrost | LiteLLM |
|--------|---------|---------|
| **Retry Count** | `max_retries` config | `num_retries` per call |
| **Retriable Check** | Error type classification | Exception type check |
| **Backoff** | Jittered exponential | Provider Retry-After |
| **Fallback** | Provider priority | Explicit fallbacks list |
| **Cooldown** | Via backoff | Separate cooldown system |

---

## 3. Error Classification

### 3.1 Bifrost Error Classification

```mermaid
graph TB
    subgraph Classification["Bifrost Error Classification"]
        direction TB
        
        subgraph AlwaysRetry["Always Retry"]
            A1[Timeout]
            A2[Connection Reset]
            A3[DNS Failure]
        end
        
        subgraph MaybeRetry["Maybe Retry"]
            M1[Rate Limit<br/>429]
            M2[Server Error<br/>500]
            M3[Service Unavailable<br/>503]
        end
        
        subgraph NeverRetry["Never Retry"]
            N1[Auth Error<br/>401]
            N2[Forbidden<br/>403]
            N3[Bad Request<br/>400]
            N4[Not Found<br/>404]
        end
    end
    
    style AlwaysRetry fill:#c8e6c9
    style MaybeRetry fill:#fff3e0
    style NeverRetry fill:#ffcdd2
```

```go
// Bifrost error classification
func isRetriableError(err error) bool {
    switch err {
    // Always retry
    case ErrTimeout, ErrConnectionReset, ErrDNSFailure:
        return true
    
    // Maybe retry - check status code
    case ErrHTTPStatus:
        statusCode := getHTTPStatusCode(err)
        switch statusCode {
        case 429: // Rate limit
            return true
        case 500, 502, 503, 504: // Server errors
            return true
        default:
            return false
        }
    
    // Never retry
    case ErrAuth, ErrForbidden, ErrBadRequest:
        return false
    
    default:
        return false
    }
}
```

### 3.2 LiteLLM Error Classification

```mermaid
graph TB
    subgraph Classification["LiteLLM Error Classification"]
        direction TB
        
        subgraph Retriable["Retriable"]
            R1[RateLimitError<br/>429]
            R2[InternalServerError<br/>500]
            R3[ServiceUnavailable<br/>503]
            R4[TimeoutError<br/>Request timeout]
            R5[APIError<br/>General API error]
        end
        
        subgraph NonRetriable["Non-Retriable"]
            NR1[AuthenticationError<br/>401]
            NR2[PermissionError<br/>403]
            NR3[BadRequestError<br/>400]
            NR4[NotFoundError<br/>404]
            NR5[ContextLimitExceeded<br/>Max tokens exceeded]
        end
    end
    
    style Retriable fill:#c8e6c9
    style NonRetriable fill:#ffcdd2
```

```python
# LiteLLM error classification
class RetriableErrors:
    RETRIABLE_ERROR_TYPES = (
        RateLimitError,
        InternalServerError,
        ServiceUnavailableError,
        TimeoutError,
        APIError,
    )

class NonRetriableErrors:
    NON_RETRIABLE_ERROR_TYPES = (
        AuthenticationError,
        PermissionError,
        BadRequestError,
        NotFoundError,
        ContextLimitExceededError,
        InvalidRequestError,
    )

def is_retriable(error: Exception) -> bool:
    return isinstance(error, RetriableErrors.RETRIABLE_ERROR_TYPES)
```

### 3.3 Error Classification Comparison

| Error Type | Bifrost | LiteLLM |
|------------|---------|---------|
| **429 Rate Limit** | Retriable | Retriable |
| **500 Server Error** | Retriable | Retriable |
| **502 Bad Gateway** | Retriable | Retriable |
| **503 Unavailable** | Retriable | Retriable |
| **Timeout** | Retriable | Retriable |
| **401 Auth** | Never | Never |
| **403 Forbidden** | Never | Never |
| **400 Bad Request** | Never | Never |

---

## 4. Backoff Algorithms

### 4.1 Bifrost Backoff Algorithm

```mermaid
graph LR
    subgraph Formula["Backoff Formula"]
        F1[Initial: 100ms]
        F2[Exponential: Initial × 2^attempt]
        F3[Capped: Min(exponential, max)]
        F4[Jitter: ±20% randomization]
    end
    
    F1 --> F2 --> F3 --> F4
    
    style Formula fill:#e8f5e9
```

```go
// Bifrost backoff calculation
func calculateBackoff(attempt int, config *NetworkConfig) time.Duration {
    // 1. Base exponential backoff
    baseBackoff := config.RetryBackoffInitial * time.Duration(1<<uint(attempt))
    
    // 2. Cap at maximum
    if baseBackoff > config.RetryBackoffMax {
        baseBackoff = config.RetryBackoffMax
    }
    
    // 3. Add jitter (±20%)
    jitter := float64(baseBackoff) * 0.2
    backoffWithJitter := baseBackoff + time.Duration(
        random.Float64()*jitter*2 - jitter,
    )
    
    return backoffWithJitter
}

// Example timeline:
// Attempt 0: 100ms base → 80-120ms
// Attempt 1: 200ms base → 160-240ms
// Attempt 2: 400ms base → 320-480ms
// Attempt 3: 800ms base → 640-960ms
// Attempt 4+: 10000ms cap → 8000-12000ms
```

### 4.2 LiteLLM Backoff Strategy

```mermaid
graph LR
    subgraph Strategy["LiteLLM Backoff Strategy"]
        direction TB
        S1[Check Retry-After<br/>Header from Provider]
        S2[If present: Use Provider<br/>Retry-After + 2s buffer]
        S3[If absent: Use<br/>default exponential]
    end
    
    style Strategy fill:#fff3e0
```

```python
# LiteLLM backoff calculation
def get_retry_after(error: RateLimitError) -> int:
    """Get retry-after from rate limit error or use default."""
    if hasattr(error, 'retry_after') and error.retry_after:
        return error.retry_after + 2  # Add 2 second buffer
    return 30  # Default 30 seconds

async def retry_with_backoff(
    request,
    max_retries: int = 3,
    initial_retry_delay: float = 0.5,
):
    for attempt in range(max_retries + 1):
        try:
            return await execute_request(request)
        except RateLimitError as e:
            if attempt == max_retries:
                raise
            retry_after = get_retry_after(e)
            await asyncio.sleep(retry_after)
        except RetriableError as e:
            if attempt == max_retries:
                raise
            delay = initial_retry_delay * (2 ** attempt)
            await asyncio.sleep(delay)
```

### 4.3 Backoff Comparison

| Aspect | Bifrost | LiteLLM |
|--------|---------|---------|
| **Initial Delay** | `retry_backoff_initial` (100ms) | 0.5s default |
| **Growth** | Exponential (×2) | Exponential (×2) |
| **Cap** | `retry_backoff_max` (10s) | Provider Retry-After |
| **Jitter** | ±20% | No jitter |
| **Rate Limit** | Standard backoff | Provider Retry-After |
| **Buffer** | N/A | +2 seconds |

---

## 5. Fallback Mechanisms

### 5.1 Bifrost Fallback via Provider Priority

```mermaid
graph TB
    subgraph VKConfig["Virtual Key Provider Config"]
        direction TB
        P1[Provider 1: OpenAI<br/>Weight: 99%]
        P2[Provider 2: Azure<br/>Weight: 1%]
        P3[Provider 3: Anthropic<br/>Fallback]
    end
    
    subgraph Selection["Provider Selection"]
        direction LR
        S1[Try Provider 1]
        S2{Provider 1<br/>Available?}
        S3[Try Provider 2]
        S4{Provider 2<br/>Available?}
        S5[Try Provider 3]
        S6{Provider 3<br/>Available?}
    end
    
    VKConfig --> Selection
    
    S1 --> S2
    S2 -->|Yes| Success1[Route to<br/>OpenAI]
    S2 -->|No| S3
    S3 --> S4
    S4 -->|Yes| Success2[Route to<br/>Azure]
    S4 -->|No| S5
    S5 --> S6
    S6 -->|Yes| Success3[Route to<br/>Anthropic]
    S6 -->|No| Fail[DENY - No<br/>Providers]
    
    style Success1 fill:#c8e6c9
    style Success2 fill:#c8e6c9
    style Success3 fill:#c8e6c9
    style Fail fill:#ffcdd2
```

### 5.2 LiteLLM Fallback Configuration

```mermaid
graph TB
    subgraph FallbackConfig["Fallback Configuration"]
        direction TB
        F1[Primary: gpt-4]
        F2[Fallback 1: gpt-3.5-turbo]
        F3[Fallback 2: claude-3-haiku]
    end
    
    subgraph Execution["Execution Flow"]
        direction LR
        E1[Call Primary<br/>gpt-4]
        E2{Error?}
        E3[Call Fallback 1<br/>gpt-3.5-turbo]
        E4{Error?}
        E5[Call Fallback 2<br/>claude-3-haiku]
        E6{Error?}
    end
    
    FallbackConfig --> Execution
    
    E1 --> E2
    E2 -->|No| Done1[Return<br/>Response]
    E2 -->|Yes| E3
    E3 --> E4
    E4 -->|No| Done2[Return<br/>Response]
    E4 -->|Yes| E5
    E5 --> E6
    E6 -->|No| Done3[Return<br/>Response]
    E6 -->|Yes| Fail[Return<br/>Error]
    
    style Done1 fill:#c8e6c9
    style Done2 fill:#c8e6c9
    style Done3 fill:#c8e6c9
    style Fail fill:#ffcdd2
```

```python
# LiteLLM fallback usage
response = await litellm.acompletion(
    model="gpt-4",
    messages=[{"role": "user", "content": "Hello"}],
    fallbacks=[
        {"model": "gpt-3.5-turbo"},
        {"model": "claude-3-haiku"},
    ],
    max_retries=2,
)
```

### 5.3 Fallback Comparison

| Aspect | Bifrost | LiteLLM |
|--------|---------|---------|
| **Fallback Model** | Provider-level | Explicit `fallbacks` param |
| **Priority** | Via provider weight | Ordered list |
| **Retry per Fallback** | Yes | Yes |
| **Budget Tracking** | Per-provider | Per-model |
| **Cache** | Uses VK cache | Separate cache |

---

## 6. Rate Limit Handling

### 6.1 Bifrost Rate Limit Handling

```mermaid
sequenceDiagram
    participant Client
    participant Bifrost
    participant Provider as LLM Provider
    
    Client->>Bifrost: Request
    Bifrost->>Provider: Forward Request
    
    Provider-->>Bifrost: 429 Too Many Requests<br/>Retry-After: 30
    
    Bifrost->>Bifrost: Check if Provider<br/>within VK rate limit
    
    alt Provider within VK limit
        Bifrost->>Bifrost: Sleep 32s (30 + buffer)
        Bifrost->>Provider: Retry Request
        Provider-->>Bifrost: 200 OK
        Bifrost-->>Client: Response
    else Provider exceeds VK limit
        Bifrost-->>Client: 429 Rate Limit<br/>"Virtual key rate limit exceeded"
    end
```

### 6.2 LiteLLM Rate Limit Handling

```mermaid
sequenceDiagram
    participant Client
    participant Router as LiteLLM Router
    participant Redis as Redis
    participant Provider as LLM Provider
    
    Client->>Router: Request
    Router->>Provider: Forward Request
    
    Provider-->>Router: 429 Rate Limit<br/>Retry-After: 60
    
    Router->>Redis: Set deployment<br/>cooldown: 60s
    Router->>Router: Try next deployment
    
    alt Has available deployment
        Router->>Provider: Retry with<br/>different deployment
        Provider-->>Router: 200 OK
        Router-->>Client: Response
    else No deployments
        Router-->>Client: 429 All deployments<br/>in cooldown
    end
```

### 6.3 Rate Limit Handling Comparison

| Aspect | Bifrost | LiteLLM |
|--------|---------|---------|
| **Response** | Honor Retry-After | Honor Retry-After |
| **Buffer** | Jitter-based | +2 seconds |
| **VK Limit Check** | Yes - parallel | N/A |
| **Deployment Cooldown** | Per-provider | Per-deployment |
| **Fallback** | Score-sorted providers | Cooldown retry |

---

## 7. Timeout Handling

### 7.1 Bifrost Timeout Handling

```mermaid
graph LR
    subgraph Timeout["Bifrost Timeout"]
        direction TB
        T1[Request Timeout<br/>default_request_timeout_in_seconds]
        T2[Connection Timeout<br/>connection_timeout_in_seconds]
        T3[Stream Timeout<br/>stream_idle_timeout_in_seconds]
    end
    
    subgraph Retry["On Timeout"]
        direction LR
        R1[Always retry<br/>timeout errors]
        R2[No attempt limit<br/>for timeouts]
        R3[Backoff before<br/>retry]
    end
    
    Timeout --> Retry
    
    style Timeout fill:#e8f5e9
    style Retry fill:#fff3e0
```

```go
// Bifrost timeout configuration
type NetworkConfig struct {
    DefaultRequestTimeoutInSeconds int `json:"default_request_timeout_in_seconds"`
    ConnectionTimeoutInSeconds     int `json:"connection_timeout_in_seconds"`
    StreamIdleTimeoutInSeconds    int `json:"stream_idle_timeout_in_seconds"`
    MaxConnsPerHost               int `json:"max_conns_per_host"`
}

// Timeout retry logic
func (r *RetryHandler) shouldRetry(err error, attempt int) bool {
    // ALWAYS retry on timeout errors
    if errors.Is(err, context.DeadlineExceeded) {
        return true
    }
    
    // For other errors, check retry policy
    return isRetriableError(err) && attempt < r.maxRetries
}
```

### 7.2 LiteLLM Timeout Handling

```mermaid
graph LR
    subgraph Timeout["LiteLLM Timeout"]
        direction TB
        T1[Request timeout<br/>timeout param]
        T2[Max timeout<br/>10 min default]
        T3[Stream timeout<br/>via timeout]
    end
    
    subgraph Retry["On Timeout"]
        direction LR
        R1[Check if retriable]
        R2[Retry if under<br/>num_retries]
        R3[Return error if<br/>exceeded]
    end
    
    Timeout --> Retry
    
    style Timeout fill:#fff3e0
    style Retry fill:#e3f2fd
```

```python
# LiteLLM timeout configuration
response = await litellm.acompletion(
    model="gpt-4",
    messages=[{"role": "user", "content": "Hello"}],
    timeout=60,  # Request timeout in seconds
    max_retries=3,
)

# Timeout handling
async def handle_timeout():
    try:
        response = await litellm.acompletion(...)
    except TimeoutError as e:
        if attempt < max_retries:
            return await retry_with_backoff(...)
        raise
```

### 7.3 Timeout Comparison

| Aspect | Bifrost | LiteLLM |
|--------|---------|---------|
| **Default Timeout** | 60s | 60s |
| **Config Location** | Network config | Request param |
| **Stream Timeout** | `stream_idle_timeout` | Via timeout |
| **Always Retry Timeout** | Yes | Yes (if retriable) |
| **Connection Timeout** | Separate config | Part of timeout |

---

## 8. Configuration

### 8.1 Bifrost Retry Configuration

```yaml
# Network configuration
network_config:
  # Retry settings
  max_retries: 3
  retry_backoff_initial: 100      # milliseconds
  retry_backoff_max: 10000        # milliseconds
  
  # Timeout settings
  default_request_timeout_in_seconds: 60
  connection_timeout_in_seconds: 10
  stream_idle_timeout_in_seconds: 120
  
  # Connection settings
  max_conns_per_host: 100
  enforce_http2: true

# Virtual key with retry behavior
virtual_key:
  name: "production-vk"
  provider_configs:
    - provider: "openai"
      allowed_models: ["gpt-4o"]
      # Retry behavior inherited from network_config
```

### 8.2 LiteLLM Retry Configuration

```yaml
# Router retry configuration
router:
  num_retries: 3
  request_timeout: 60
  timeout_buffer_seconds: 2  # Buffer for retry-after
  retry_after_timeout: true  # Honor Retry-After header
  
  # Fallback models
  model_list:
    - model_name: "gpt-4"
      litellm_params:
        model: "openai/gpt-4"
        fallbacks:
          - model: "gpt-3.5-turbo"
            max_retries: 2
          - model: "claude-3-haiku"
            max_retries: 1

# Per-request retry configuration
litellm_settings:
  set_verbose: True
  
# Custom retry logic
litellm.max_retries = 3
litellm.retry_after_function = custom_retry_after
```

### 8.3 Configuration Comparison

| Config | Bifrost | LiteLLM |
|--------|---------|---------|
| **Max Retries** | `max_retries` | `num_retries` |
| **Initial Backoff** | `retry_backoff_initial` | 0.5s default |
| **Max Backoff** | `retry_backoff_max` | Provider Retry-After |
| **Request Timeout** | `default_request_timeout` | `timeout` |
| **Connection Timeout** | Separate | Part of timeout |
| **Fallback** | Via provider priority | Via `fallbacks` param |
| **Retry-After** | Jitter-based | Honor + 2s buffer |

---

## 9. Key Feature Matrix

| Feature | Bifrost | LiteLLM |
|---------|---------|---------|
| Exponential backoff | ✅ | ✅ |
| Jitter | ✅ ±20% | ❌ |
| Max retries | ✅ Configurable | ✅ Per-request |
| Rate limit handling | ✅ Provider-aware | ✅ Cooldown |
| Timeout retry | ✅ Always | ✅ If retriable |
| Provider fallback | ✅ Weight-based | ✅ Explicit |
| Model fallback | ❌ | ✅ Via fallbacks |
| Cooldown system | Via backoff | Separate |
| Custom backoff | Via config | Via function |

---

## 10. Summary

### Bifrost Advantages
- **Jitter**: Prevents thundering herd with ±20% randomization
- **Provider fallback**: Natural priority via weights
- **Timeout always retried**: No configuration needed
- **Per-VK retry config**: Different policies per customer
- **Simple backoff**: Exponential with cap

### LiteLLM Advantages
- **Explicit fallbacks**: Clear model fallback chain
- **Provider Retry-After**: Honor provider timing
- **Per-request retries**: More flexible
- **Cooldown system**: Separate from retry
- **Larger ecosystem**: More provider support

---

**Next Steps:**
- [ ] Research: Observability Deep Comparison
- [ ] Research: Provider Support Deep Comparison
- [ ] Research: Architecture Deep Comparison
- [ ] Update main comparison document with links