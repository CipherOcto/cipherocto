# Research: Observability Deep Comparison - Bifrost vs LiteLLM

**Date:** 2026-05-11  
**Status:** Companion Research  
**Parent:** [Bifrost vs LiteLLM Comparison](./bifrost-litellm-comparison.md)

---

## Table of Contents

1. [Observability Stack](#1-observability-stack)
2. [Metrics](#2-metrics)
3. [Tracing](#3-tracing)
4. [Logging](#4-logging)
5. [Alerting](#5-alerting)
6. [Dashboard & UI](#6-dashboard--ui)
7. [Cost Attribution](#7-cost-attribution)
8. [Export & Integration](#8-export--integration)

---

## 1. Observability Stack

### 1.1 Bifrost Observability Stack

```mermaid
graph TB
    subgraph Collection["Metrics Collection"]
        M1[Prometheus<br/>Metrics]
        M2[OTLP<br/>Traces]
        M3[Structured<br/>Logs]
    end
    
    subgraph Storage["Storage Layer"]
        S1[(Prometheus<br/>Time-series)]
        S2[(Tempo/Jaeger<br/>Traces)]
        S3[(Loki<br/>Logs)]
    end
    
    subgraph Visualization["Visualization"]
        V1[Grafana<br/>Dashboards]
        V2[Web UI<br/>Built-in]
    end
    
    subgraph Alerting["Alerting"]
        A1[Alertmanager<br/>Prometheus]
        A2[Built-in<br/>Alerts]
    end
    
    Collection --> Storage
    Storage --> Visualization
    Storage --> Alerting
    
    style Collection fill:#e3f2fd
    style Storage fill:#e8f5e9
    style Visualization fill:#fff3e0
```

### 1.2 LiteLLM Observability Stack

```mermaid
graph TB
    subgraph Collection["Metrics Collection"]
        L1[Prometheus<br/>Metrics]
        L2[OpenTelemetry<br/>Traces]
        L3[Langfuse<br/>Callbacks]
        L4[LanceDB<br/>Quality]
    end
    
    subgraph Storage["Storage Layer"]
        St1[(Prometheus<br/>Metrics)]
        St2[(OTLP Endpoint<br/>Traces)]
        St3[(Database<br/>Spend Logs)]
        St4[(Langfuse<br/>Traces)]
    end
    
    subgraph Visualization["Visualization"]
        Vi1[Grafana<br/>Dashboards]
        Vi2[Helicone<br/>UI]
        Vi3[ArthTrack<br/>UI]
    end
    
    subgraph Alerting["Alerting"]
        Al1[Webhook<br/>Alerts]
        Al2[Slack/Email<br/>Notifications]
    end
    
    Collection --> Storage
    Storage --> Visualization
    Storage --> Alerting
    
    style Collection fill:#e3f2fd
    style Storage fill:#e8f5e9
    style Visualization fill:#fff3e0
```

---

## 2. Metrics

### 2.1 Bifrost Metrics

```mermaid
graph TB
    subgraph Categories["Bifrost Metric Categories"]
        direction TB
        
        subgraph Request["Request Metrics"]
            R1[request_count<br/>Total requests]
            R2[request_duration_ms<br/>Latency histogram]
            R3[request_success<br/>Success rate]
            R4[request_error<br/>Error rate by type]
        end
        
        subgraph Provider["Provider Metrics"]
            P1[provider_requests<br/>Per provider]
            P2[provider_latency<br/>Per provider latency]
            P3[provider_errors<br/>Per provider errors]
            P4[provider_score<br/>Health score]
        end
        
        subgraph Budget["Budget Metrics"]
            B1[budget_usage<br/>Per VK budget]
            B2[budget_remaining<br/>Per VK remaining]
            B3[budget_reset<br/>Next reset time]
        end
        
        subgraph Rate["Rate Limit Metrics"]
            L1[rate_limit_hits<br/>429 responses]
            L2[rate_limit_remaining<br/>Per VK limits]
        end
    end
    
    style Request fill:#e3f2fd
    style Provider fill:#e8f5e9
    style Budget fill:#fff3e0
    style Rate fill:#fce4ec
```

```go
// Bifrost metrics definitions
var (
    // Request metrics
    RequestTotal = prometheus.NewCounterVec(
        prometheus.CounterOpts{
            Name: "bifrost_requests_total",
            Help: "Total number of requests",
        },
        []string{"model", "provider", "virtual_key_id"},
    )
    
    RequestDuration = prometheus.NewHistogramVec(
        prometheus.HistogramOpts{
            Name:    "bifrost_request_duration_ms",
            Help:    "Request duration in milliseconds",
            Buckets: []float64{100, 500, 1000, 2000, 5000, 10000},
        },
        []string{"model", "provider"},
    )
    
    // Provider metrics
    ProviderScore = prometheus.NewGaugeVec(
        prometheus.GaugeOpts{
            Name: "bifrost_provider_score",
            Help: "Provider health score (0-1)",
        },
        []string{"provider", "model"},
    )
    
    // Budget metrics
    BudgetUsage = prometheus.NewGaugeVec(
        prometheus.GaugeOpts{
            Name: "bifrost_budget_usage",
            Help: "Current budget usage in USD",
        },
        []string{"virtual_key_id", "budget_id"},
    )
)
```

### 2.2 LiteLLM Metrics

```mermaid
graph TB
    subgraph Categories["LiteLLM Metric Categories"]
        direction TB
        
        subgraph Request["Request Metrics"]
            LR1[total_requests<br/>All requests]
            LR2[successful_requests<br/>Successful]
            LR3[failed_requests<br/>Failed]
            LR4[request_latency<br/>Latency]
        end
        
        subgraph Spend["Spend Metrics"]
            SP1[spend<br/>Total by model/provider]
            SP2[spend_per_user<br/>By user]
            SP3[spend_per_team<br/>By team]
            SP4[spend_per_key<br/>By API key]
        end
        
        subgraph Cache["Cache Metrics"]
            C1[cache_hits<br/>Cache hit count]
            C2[cache_misses<br/>Cache miss count]
            C3[cache_hit_rate<br/>Hit rate %]
        end
        
        subgraph Limit["Limit Metrics"]
            LM1[rpm_limit<br/>RPM usage]
            LM2[tpm_limit<br/>TPM usage]
        end
    end
    
    style Request fill:#e3f2fd
    style Spend fill:#e8f5e9
    style Cache fill:#fff3e0
    style Limit fill:#fce4ec
```

```python
# LiteLLM metrics (Prometheus)
LITELLM_METRICS = {
    "total_requests": Counter(
        "litellm_total_requests",
        "Total number of requests",
        ["model", "user", "team"]
    ),
    
    "total_spend": Counter(
        "litellm_total_spend",
        "Total spend in USD",
        ["model", "provider", "user"]
    ),
    
    "request_latency": Histogram(
        "litellm_request_latency",
        "Request latency in seconds",
        ["model", "provider"],
        buckets=[0.1, 0.5, 1, 2, 5, 10]
    ),
    
    "cache_hit_total": Counter(
        "litellm_cache_hit_total",
        "Total cache hits"
    ),
    
    "rpm_limit": Gauge(
        "litellm_rpm_limit",
        "RPM limit usage",
        ["api_key"]
    ),
}
```

### 2.3 Metrics Comparison

| Category | Bifrost | LiteLLM |
|----------|---------|---------|
| **Request Count** | ✅ `request_total` | ✅ `total_requests` |
| **Latency** | ✅ Histogram | ✅ Histogram |
| **Success Rate** | ✅ `request_success` | ✅ `successful_requests` |
| **Error Rate** | ✅ `request_error` | ✅ `failed_requests` |
| **Provider Score** | ✅ `provider_score` | ❌ |
| **Budget Usage** | ✅ `budget_usage` | ✅ `spend` |
| **Cache Hits** | ❌ | ✅ `cache_hit_total` |
| **Rate Limits** | ✅ `rate_limit_hits` | ✅ `rpm_limit` |

---

## 3. Tracing

### 3.1 Bifrost Tracing

```mermaid
sequenceDiagram
    participant Client
    participant B as Bifrost
    participant P as Provider
    participant OTEL as OTEL Collector
    
    Client->>B: Request (gpt-4o)
    
    B->>OTEL: Span: bifrost.request<br/>model=gpt-4o
    
    B->>B: Governance Check
    
    B->>OTEL: Span: governance.check<br/>decision=allow
    
    B->>B: Provider Selection
    
    B->>OTEL: Span: provider.select<br/>provider=openai
    
    B->>P: Forward Request
    
    P-->>B: Response
    
    B->>OTEL: Span: provider.response<br/>latency=1.2s
    
    B-->>Client: Response
    
    Note over OTEL: Complete trace with<br/>all spans
```

```go
// Bifrost tracing spans
func (b *Bifrost) handleRequest(ctx context.Context, req *LLMRequest) (*Response, error) {
    // Start root span
    ctx, span := otel.Tracer("bifrost").Start(ctx, "bifrost.request")
    defer span.End()
    
    span.SetAttributes(
        attribute.String("model", req.Model),
        attribute.String("virtual_key_id", req.VKID),
    )
    
    // Governance check span
    ctx, govSpan := otel.Tracer("bifrost").Start(ctx, "governance.check")
    govResult, err := b.governance.Check(ctx, req)
    govSpan.SetAttributes(
        attribute.String("decision", string(govResult.Decision)),
    )
    govSpan.End()
    
    // Provider selection span
    ctx, provSpan := otel.Tracer("bifrost").Start(ctx, "provider.select")
    provider, err := b.router.Select(ctx, req)
    provSpan.SetAttributes(
        attribute.String("provider", provider.Name),
    )
    provSpan.End()
    
    // Execute request with provider span
    resp, err := b.executeWithTracing(ctx, provider, req)
    
    return resp, err
}
```

### 3.2 LiteLLM Tracing

```mermaid
sequenceDiagram
    participant Client
    participant Router as LiteLLM Router
    participant P as Provider
    participant OTEL as OTEL Collector
    
    Client->>Router: Request (gpt-4o)
    
    Router->>OTEL: Span: litellm.request<br/>model=gpt-4o
    
    Router->>Router: Auth Check
    
    Router->>OTEL: Span: auth.check<br/>key_id=xxx
    
    Router->>Router: Budget Check
    
    Router->>OTEL: Span: budget.check<br/>spend=45.50
    
    Router->>Router: Deployment Select
    
    Router->>OTEL: Span: deployment.select<br/>deployment=openai/gpt-4o
    
    Router->>P: Forward Request
    
    P-->>Router: Response
    
    Router->>OTEL: Span: litellm.response<br/>latency=1.2s
    
    Router-->>Client: Response
```

```python
# LiteLLM tracing with OpenTelemetry
from opentelemetry import trace

tracer = trace.get_tracer("litellm")

@asynccontextmanager
async def traced_completion(model: str, messages: List[Dict]):
    with tracer.start_as_current_span("litellm.request") as span:
        span.set_attribute("model", model)
        
        # Auth check
        with tracer.start_as_current_span("auth.check"):
            await auth_check()
        
        # Budget check
        with tracer.start_as_current_span("budget.check"):
            await budget_check()
        
        # Deployment selection
        with tracer.start_as_current_span("deployment.select"):
            deployment = await select_deployment()
            span.set_attribute("deployment", deployment.id)
        
        # Execute
        with tracer.start_as_current_span("llm.call"):
            response = await call_provider(deployment)
        
        return response
```

### 3.3 Tracing Comparison

| Aspect | Bifrost | LiteLLM |
|--------|---------|---------|
| **Format** | OTLP | OTLP |
| **Spans** | Governance, Select, Execute | Auth, Budget, Deploy, Call |
| **Attributes** | VK ID, Provider, Model | Key ID, Team, Model |
| **Parent Span** | Request-level | Request-level |
| **Error Recording** | ✅ | ✅ |
| **Custom Spans** | ✅ | ✅ |

---

## 4. Logging

### 4.1 Bifrost Logging

```mermaid
graph TB
    subgraph LogTypes["Bifrost Log Categories"]
        direction TB
        
        subgraph Access["Access Logs"]
            A1[Request/Response<br/>Full payload]
            A2[Latency<br/>Per request]
            A3[VK ID<br/>Correlation]
        end
        
        subgraph Governance["Governance Logs"]
            G1[Budget check<br/>results]
            G2[Rate limit<br/>decisions]
            G3[VK validation<br/>results]
        end
        
        subgraph System["System Logs"]
            S1[Startup/Shutdown]
            S2[Configuration<br/>changes]
            S3[Health checks]
        end
        
        subgraph Debug["Debug Logs"]
            D1[Provider selection<br/>reasoning]
            D2[Retry attempts]
            D3[Cache hits/misses]
        end
    end
    
    style Access fill:#e3f2fd
    style Governance fill:#e8f5e9
    style System fill:#fff3e0
    style Debug fill:#fce4ec
```

```go
// Bifrost structured logging
type LogEntry struct {
    Timestamp   time.Time `json:"timestamp"`
    Level       string    `json:"level"`
    Message     string    `json:"message"`
    VirtualKey  string    `json:"virtual_key_id,omitempty"`
    RequestID   string    `json:"request_id"`
    Model       string    `json:"model,omitempty"`
    Provider    string    `json:"provider,omitempty"`
    LatencyMs   int64     `json:"latency_ms,omitempty"`
    Error       string    `json:"error,omitempty"`
    Metadata    map[string]interface{} `json:"metadata,omitempty"`
}

// Example log entries
logger.Info("Request completed",
    "virtual_key_id", vk.ID,
    "request_id", requestID,
    "model", "gpt-4o",
    "provider", "openai",
    "latency_ms", 1200,
    "status", "success",
)

logger.Info("Governance check",
    "virtual_key_id", vk.ID,
    "decision", "allow",
    "budget_remaining", 54.50,
    "rate_limit_remaining", 75000,
)
```

### 4.2 LiteLLM Logging

```mermaid
graph TB
    subgraph LogTypes["LiteLLM Log Categories"]
        direction TB
        
        subgraph Proxy["Proxy Logs"]
            LP1[Incoming requests]
            LP2[Auth results]
            LP3[Spend tracking]
        end
        
        subgraph Model["Model Logs"]
            LM1[Request to provider]
            LM2[Response from provider]
            LM3[Provider errors]
        end
        
        subgraph Router["Router Logs"]
            LR1[Deployment selection]
            LR2[Retry attempts]
            LR3[Rate limit hits]
        end
        
        subgraph Audit["Audit Logs"]
            AU1[Key creation]
            AU2[Config changes]
            AU3[Budget alerts]
        end
    end
    
    style Proxy fill:#e3f2fd
    style Model fill:#e8f5e9
    style Router fill:#fff3e0
    style Audit fill:#fce4ec
```

```python
# LiteLLM structured logging
import logging
from litellm.proxy.utils import LogHandler

# Configure logging
logging.basicConfig(
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)

# Request logging
logger.info(
    f"Request completed:",
    extra={
        "model": "gpt-4",
        "user": user_id,
        "team": team_id,
        "spend": 0.05,
        "latency": 1.2,
        "status": "success"
    }
)

# Spend tracking log
logger.info(
    f"Spend logged:",
    extra={
        "api_key": api_key[:8] + "...",
        "model": "gpt-4",
        "spend": 0.05,
        "total_spend": 45.50,
        "budget_remaining": 54.50
    }
)
```

### 4.3 Logging Comparison

| Aspect | Bifrost | LiteLLM |
|--------|---------|---------|
| **Format** | Structured JSON | Structured JSON |
| **Request Logging** | ✅ Full payload | ✅ Lite |
| **Governance Logs** | ✅ | Via callbacks |
| **Spend Logs** | ✅ | ✅ |
| **Audit Logs** | ✅ | ✅ |
| **Debug Logs** | ✅ | Via verbose |
| **Log Levels** | DEBUG, INFO, WARN, ERROR | DEBUG, INFO, WARNING, ERROR |

---

## 5. Alerting

### 5.1 Bifrost Alerting

```mermaid
graph TB
    subgraph AlertRules["Bifrost Alert Rules"]
        direction TB
        
        subgraph Budget["Budget Alerts"]
            BA1[Budget > 80%]
            BA2[Budget exhausted]
            BA3[Budget reset<br/>approaching]
        end
        
        subgraph Health["Health Alerts"]
            HA1[Error rate > 5%]
            HA2[Latency > 5s P99]
            HA3[Provider down]
        end
        
        subgraph Rate["Rate Limit Alerts"]
            RA1[Rate limit<br/>exhausted]
            RA2[Rate limit<br/>approaching]
        end
        
        subgraph System["System Alerts"]
            SA1[Instance down]
            SA2[Redis<br/>disconnected]
            SA3[High memory<br/>usage]
        end
    end
    
    style Budget fill:#fff3e0
    style Health fill:#ffcdd2
    style Rate fill:#fce4ec
    style System fill:#e3f2fd
```

```yaml
# Bifrost alert rules (Prometheus/Alertmanager)
groups:
  - name: bifrost.budget
    rules:
      - alert: BudgetExhausted
        expr: bifrost_budget_usage / bifrost_budget_limit >= 1.0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Budget exhausted for VK {{ $labels.virtual_key_id }}"
          description: "Budget usage at {{ $value | humanizePercentage }}"
      
      - alert: BudgetWarning
        expr: bifrost_budget_usage / bifrost_budget_limit >= 0.8
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Budget warning for VK {{ $labels.virtual_key_id }}"
  
  - name: bifrost.health
    rules:
      - alert: HighErrorRate
        expr: rate(bifrost_request_error_total[5m]) / rate(bifrost_requests_total[5m]) > 0.05
        for: 2m
        labels:
          severity: critical
```

### 5.2 LiteLLM Alerting

```mermaid
graph TB
    subgraph AlertRules["LiteLLM Alert Rules"]
        direction TB
        
        subgraph Spend["Spend Alerts"]
            SA1[Daily spend<br/>threshold]
            SA2[Budget<br/>exceeded]
            SA3[Unexpected<br/>spend spike]
        end
        
        subgraph Limit["Limit Alerts"]
            LA1[RPM limit<br/>approaching]
            LA2[TPM limit<br/>approaching]
        end
        
        subgraph Error["Error Alerts"]
            EA1[High error<br/>rate]
            EA2[Specific<br/>error type]
        end
        
        subgraph Custom["Custom Alerts"]
            CA1[Webhook<br/>integration]
            CA2[Slack<br/>notification]
        end
    end
    
    style Spend fill:#fff3e0
    style Limit fill:#e8f5e9
    style Error fill:#ffcdd2
    style Custom fill:#e3f2fd
```

```yaml
# LiteLLM alerting configuration
litellm_settings:
  # Slack alerting
  alert_types:
    - "spending_limit_exceeded"
    - "budget_exceeded"
    - "llm_api_error"
  
  alerting webhook:
    url: "https://hooks.slack.com/..."
    level: "all"  # or "error" only
    
  # Alerting thresholds
  max_budget_alert:
    enabled: true
    threshold: 0.8  # Alert at 80% of budget
  
  daily_spend_alert:
    enabled: true
    threshold: 100.00  # Alert if daily spend exceeds $100
```

### 5.3 Alerting Comparison

| Alert Type | Bifrost | LiteLLM |
|------------|---------|---------|
| **Budget Exhausted** | ✅ Prometheus | ✅ Webhook |
| **Budget Warning** | ✅ 80% threshold | ✅ Configurable |
| **High Error Rate** | ✅ Prometheus | ✅ Via callbacks |
| **High Latency** | ✅ P99 threshold | ❌ Built-in |
| **Rate Limit** | ✅ Prometheus | ❌ Built-in |
| **Slack Alert** | Via Alertmanager | ✅ Native |
| **Webhook** | Via Alertmanager | ✅ Native |
| **Custom Rules** | ✅ Prometheus | ❌ Limited |

---

## 6. Dashboard & UI

### 6.1 Bifrost Dashboard

```mermaid
graph TB
    subgraph Dashboards["Bifrost Dashboard Screens"]
        direction TB
        
        subgraph Overview["Overview"]
            OV1[Total Requests]
            OV2[Success Rate]
            OV3[Avg Latency]
            OV4[Active VKs]
        end
        
        subgraph Providers["Provider Health"]
            PR1[Provider scores]
            PR2[Latency P50/P95/P99]
            PR3[Error rates]
            PR4[Capacity usage]
        end
        
        subgraph Budgets["Budget Monitoring"]
            BU1[Budget by VK]
            BU2[Budget burn rate]
            BU3[Reset countdown]
        end
        
        subgraph Usage["Usage Analytics"]
            US1[Request volume<br/>over time]
            US2[Top models]
            US3[Top providers]
            US4[Cost breakdown]
        end
    end
    
    style Overview fill:#e8f5e9
    style Providers fill:#e3f2fd
    style Budgets fill:#fff3e0
    style Usage fill:#fce4ec
```

### 6.2 LiteLLM Dashboard

```mermaid
graph TB
    subgraph Dashboards["LiteLLM Dashboard Screens"]
        direction TB
        
        subgraph Spend["Spend Dashboard"]
            SP1[Total Spend]
            SP2[Spend by Model]
            SP3[Spend by Team]
            SP4[Spend by Key]
        end
        
        subgraph Usage["Usage Analytics"]
            US1[Request count]
            US2[Latency P50/P95/P99]
            US3[Token usage]
            US4[Cache hit rate]
        end
        
        subgraph Keys["Key Management"]
            KY1[Active Keys]
            KY2[Key Spend]
            KY3[Key Limits]
        end
        
        subgraph Logs["Request Logs"]
            LG1[Recent requests]
            LG2[Error logs]
            LG3[Trace viewer]
        end
    end
    
    style Spend fill:#e8f5e9
    style Usage fill:#e3f2fd
    style Keys fill:#fff3e0
    style Logs fill:#fce4ec
```

### 6.3 Dashboard Comparison

| Screen | Bifrost | LiteLLM |
|--------|---------|---------|
| **Overview** | ✅ | ✅ |
| **Request Volume** | ✅ | ✅ |
| **Latency** | ✅ P50/P95/P99 | ✅ P50/P95/P99 |
| **Error Rate** | ✅ | ✅ |
| **Budget/Spend** | ✅ | ✅ |
| **Provider Health** | ✅ | ❌ |
| **Provider Scores** | ✅ | ❌ |
| **Key Management** | ✅ | ✅ |
| **Request Logs** | ✅ | ✅ |
| **Trace Viewer** | Via OTEL | Via Langfuse |

---

## 7. Cost Attribution

### 7.1 Bifrost Cost Attribution

```mermaid
graph TB
    subgraph Attribution["Bifrost Cost Attribution"]
        direction TB
        
        subgraph Level1["Virtual Key Level"]
            V1[Total spend<br/>per VK]
            V2[Budget remaining]
            V3[Cost by provider]
        end
        
        subgraph Level2["Provider Level"]
            P1[Cost per provider]
            P2[Cost per model]
            P3[Token costs]
        end
        
        subgraph Level3["Time Level"]
            T1[Daily spend]
            T2[Monthly spend]
            T3[Burn rate]
        end
    end
    
    style Level1 fill:#e8f5e9
    style Level2 fill:#e3f2fd
    style Level3 fill:#fff3e0
```

```go
// Bifrost cost calculation
type CostBreakdown struct {
    VirtualKeyID string
    TotalUSD     float64
    
    ByProvider map[string]float64
    ByModel    map[string]float64
    ByDay      map[string]float64
    
    TokensUsed    int64
    AvgCostPerToken float64
}

// Calculate cost per VK
func (c *CostTracker) GetCostBreakdown(vkID string) (*CostBreakdown, error) {
    breakdown := &CostBreakdown{
        VirtualKeyID: vkID,
        ByProvider:   make(map[string]float64),
        ByModel:      make(map[string]float64),
        ByDay:        make(map[string]float64),
    }
    
    // Query all requests for VK
    requests, err := c.store.GetRequestsForVK(vkID)
    for _, req := range requests {
        cost := calculateCost(req.Model, req.TokensUsed)
        breakdown.TotalUSD += cost
        breakdown.ByProvider[req.Provider] += cost
        breakdown.ByModel[req.Model] += cost
        breakdown.ByDay[req.Timestamp.Format("2006-01-02")] += cost
        breakdown.TokensUsed += req.TokensUsed
    }
    
    return breakdown, nil
}
```

### 7.2 LiteLLM Cost Attribution

```mermaid
graph TB
    subgraph Attribution["LiteLLM Cost Attribution"]
        direction TB
        
        subgraph Level1["Key Level"]
            K1[Total spend<br/>per key]
            K2[Remaining budget]
            K3[Spend history]
        end
        
        subgraph Level2["Team Level"]
            T1[Team spend]
            T2[Team budget]
            T3[Member breakdown]
        end
        
        subgraph Level3["Org Level"]
            O1[Org total spend]
            O2[Org budget]
            O3[Team breakdown]
        end
        
        subgraph Level4["Model Level"]
            M1[Cost per model]
            M2[Token counts]
            M3[Provider costs]
        end
    end
    
    style Level1 fill:#e8f5e9
    style Level2 fill:#e3f2fd
    style Level3 fill:#fff3e0
    style Level4 fill:#fce4ec
```

```python
# LiteLLM cost tracking
async def log_spend(
    api_key: str,
    model: str,
    provider: str,
    tokens_used: int,
    cost: float,
    user_id: Optional[str] = None,
    team_id: Optional[str] = None,
):
    # Log to database
    await db.litellm_spend_logs.insert(
        api_key=api_key,
        model=model,
        provider=provider,
        total_tokens=tokens_used,
        spend=cost,
        user_id=user_id,
        team_id=team_id,
        timestamp=datetime.utcnow(),
    )
    
    # Update Redis counters
    await redis.incr(f"spend:key:{api_key}", cost)
    await redis.incr(f"spend:team:{team_id}", cost)
    await redis.incr(f"spend:model:{model}", cost)

# Get spend by key
async def get_key_spend(api_key: str) -> float:
    return await redis.get(f"spend:key:{api_key}")

# Get spend by team
async def get_team_spend(team_id: str) -> float:
    return await redis.get(f"spend:team:{team_id}")
```

### 7.3 Cost Attribution Comparison

| Attribution | Bifrost | LiteLLM |
|-------------|---------|---------|
| **Per VK/Key** | ✅ | ✅ |
| **Per Team** | ✅ | ✅ |
| **Per Org** | ❌ (Customer) | ✅ |
| **Per Provider** | ✅ | ✅ |
| **Per Model** | ✅ | ✅ |
| **Per Day** | ✅ | ✅ |
| **Token Count** | ✅ | ✅ |
| **Real-time** | ✅ Redis | ✅ Redis |

---

## 8. Export & Integration

### 8.1 Bifrost Export

```mermaid
graph TB
    subgraph Export["Bifrost Export Options"]
        direction TB
        
        E1[Prometheus<br/>Metrics]
        E2[OTLP<br/>Traces]
        E3[Loki<br/>Logs]
        E4[Grafana<br/>Dashboard JSON]
        E5[REST API<br/>Metrics endpoint]
    end
    
    style Export fill:#e8f5e9
```

### 8.2 LiteLLM Export

```mermaid
graph TB
    subgraph Export["LiteLLM Export Options"]
        direction TB
        
        L1[Prometheus<br/>Metrics]
        L2[OpenTelemetry<br/>Traces]
        L3[Helicone<br/>Integration]
        L4[Langfuse<br/>Integration]
        L5[ArthTrack<br/>Integration]
        L6[REST API<br/>Metrics]
        L7[Database<br/>Direct Query]
    end
    
    style Export fill:#e3f2fd
```

### 8.3 Export Comparison

| Export | Bifrost | LiteLLM |
|--------|---------|---------|
| **Prometheus** | ✅ `/metrics` | ✅ `/metrics` |
| **OTLP Traces** | ✅ | ✅ |
| **Loki Logs** | ✅ | Via callbacks |
| **Grafana** | ✅ JSON | ✅ |
| **Helicone** | ❌ | ✅ |
| **Langfuse** | ❌ | ✅ |
| **ArthTrack** | ❌ | ✅ |
| **REST API** | ✅ | ✅ |
| **DB Direct** | ❌ | ✅ |

---

## 9. Key Feature Matrix

| Feature | Bifrost | LiteLLM |
|---------|---------|---------|
| Prometheus metrics | ✅ | ✅ |
| OTLP tracing | ✅ | ✅ |
| Structured logging | ✅ | ✅ |
| Budget alerts | ✅ | ✅ |
| Spend tracking | ✅ | ✅ |
| Provider scores | ✅ | ❌ |
| Latency P50/P95/P99 | ✅ | ✅ |
| Cache metrics | ❌ | ✅ |
| Grafana dashboards | ✅ | ✅ |
| Slack alerting | Via Alertmanager | ✅ Native |
| Webhook alerts | Via Alertmanager | ✅ Native |
| Third-party integrations | Limited | Langfuse, Helicone |

---

## 10. Summary

### Bifrost Advantages
- **Provider scoring**: Health scores enable proactive alerting
- **Provider-level metrics**: Granular provider monitoring
- **Built-in Grafana**: Complete observability stack
- **Governance logging**: Full audit trail
- **Prometheus-native**: Standard metrics format

### LiteLLM Advantages
- **Third-party integrations**: Helicone, Langfuse, ArthTrack
- **Cache metrics**: Built-in cache hit rate
- **Team/Org hierarchy**: Multi-level spend attribution
- **Slack alerts**: Native integration
- **Larger ecosystem**: More export options

---

**Next Steps:**
- [ ] Research: Provider Support Deep Comparison
- [ ] Research: Architecture Deep Comparison
- [ ] Create decision matrix for CipherOcto integration
- [ ] Update main comparison document with links