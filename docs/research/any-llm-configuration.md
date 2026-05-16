# Research: any-llm Configuration & Architecture

**Date:** 2026-05-13  
**Status:** External Reference  
**Purpose:** Understand configuration patterns for CipherOcto integration

---

## Overview

**any-llm** is a Python-based LLM gateway/proxy from Mozilla AI with two main components:
1. **SDK** - Unified Python client for multiple LLM providers
2. **Gateway** - FastAPI-based proxy server with API key management

---

## Architecture

```mermaid
graph TB
    subgraph SDK["any-llm SDK"]
        Core[any_llm.py<br/>Core interface]
        Providers[providers/<br/>Provider implementations]
        Types[types/<br/>Type definitions]
    end
    
    subgraph Gateway["any-llm Gateway"]
        API[FastAPI<br/>REST API]
        Auth[Auth<br/>API Key validation]
        DB[(SQLite/PostgreSQL<br/>Keys + Pricing)]
        RateLimiter[Rate Limiter<br/>RPM limiting]
        Metrics[Prometheus<br/>Metrics]
    end
    
    subgraph Providers["Supported Providers"]
        P1[OpenAI]
        P2[Anthropic]
        P3[Vertex AI]
        P4[40+ More]
    end
    
    SDK --> Providers
    Gateway --> Providers
    
    style SDK fill:#e8f5e9
    style Gateway fill:#e3f2fd
```

---

## Configuration System

### 1. Configuration Loading (GatewayConfig)

```mermaid
graph LR
    subgraph Sources["Configuration Sources"]
        Y[YAML File<br/>config.yml]
        E[Environment<br/>Variables]
        A[Arguments<br/>CLI flags]
    end
    
    subgraph Pydantic["Pydantic Settings"]
        M[GatewayConfig<br/>BaseSettings]
        V[Validation<br/>& Defaults]
    end
    
    Sources --> Pydantic
    
    style Pydantic fill:#e8f5e9
```

**Configuration Priority (highest to lowest):**
1. CLI arguments
2. Environment variables (prefix: `GATEWAY_`)
3. YAML config file
4. Pydantic defaults

### 2. GatewayConfig Model

```python
# src/any_llm/gateway/core/config.py

class GatewayConfig(BaseSettings):
    """Gateway configuration with support for YAML files and environment variables."""
    
    model_config = SettingsConfigDict(
        env_prefix="GATEWAY_",      # GATEWAY_DATABASE_URL, GATEWAY_MASTER_KEY, etc.
        env_file=".env",
        case_sensitive=False,
        extra="ignore",
    )
    
    # Database
    database_url: str = "sqlite:///./any-llm-gateway.db"
    auto_migrate: bool = True
    
    # Server
    host: str = "0.0.0.0"
    port: int = 8000
    master_key: str | None = None
    
    # Rate Limiting
    rate_limit_rpm: int | None = None
    
    # CORS
    cors_allow_origins: list[str] = []
    
    # Providers
    providers: dict[str, dict[str, Any]] = {}
    
    # Pricing
    pricing: dict[str, PricingConfig] = {}
    
    # Observability
    enable_metrics: bool = False
    bootstrap_api_key: bool = True
```

### 3. Config Loading Flow

```python
def load_config(config_path: str | None = None) -> GatewayConfig:
    """Load configuration from file and environment variables."""
    config_dict: dict[str, Any] = {}
    
    if config_path and Path(config_path).exists():
        with open(config_path, encoding="utf-8") as f:
            yaml_config = yaml.safe_load(f)
            if yaml_config:
                config_dict = _resolve_env_vars(yaml_config)
    
    return GatewayConfig(**config_dict)
```

**Environment Variable Resolution:**
```yaml
# Supports ${VAR_NAME} syntax in YAML
database_url: "postgresql://user:${DB_PASSWORD}@host/db"
```

---

## YAML Configuration

### Example config.yml

```yaml
# docker/config.example.yml

# Database
database_url: "postgresql://gateway:gateway@postgres:5432/gateway"

# Server
host: "0.0.0.0"
port: 8000

# Security
master_key: YOUR_MASTER_KEY_HERE

# Rate Limiting (requests per minute per user)
rate_limit_rpm: 60

# Observability
enable_metrics: true

# Pre-configured provider credentials
providers:
  vertexai:
    credentials: "/app/service_account.json"
    project: YOUR_GCP_PROJECT_ID_HERE
    location: "us-central1"

  openai:
    api_key: YOUR_OPENAI_API_KEY_HERE
    api_base: "https://api.openai.com/v1"
    # client_args are passed to the provider's client initialization
    # client_args:
    #   custom_headers:
    #     X-Custom-Header: "custom-value"
    #   timeout: 60

  anthropic:
    api_key: YOUR_ANTHROPIC_API_KEY_HERE

  mistral:
    api_key: YOUR_MISTRAL_API_KEY_HERE

# Model pricing (USD per million tokens)
pricing:
  openai:gpt-4o:
    input_price_per_million: 5.00
    output_price_per_million: 15.00
```

---

## API Key Management

### Key Generation

```mermaid
graph LR
    G[Generate] --> V[Validate Format]
    V --> H[Hash SHA-256]
    H --> S[Store in DB]
    
    S --> Format["Format: gw-{48 chars}<br/>Example: gw-abc123..."]
    
    style G fill:#e8f5e9
    style Format fill:#fff3e0
```

```python
def generate_api_key() -> str:
    """Generate a new API key with prefix."""
    api_key = f"gw-{secrets.token_urlsafe(48)}"
    validate_api_key_format(api_key)
    return api_key

def hash_key(api_key: str) -> str:
    """Hash an API key using SHA-256."""
    validate_api_key_format(api_key)
    return hashlib.sha256(api_key.encode()).hexdigest()
```

### Key Validation

```mermaid
flowchart TD
    A[Request with X-AnyLLM-Key header] --> B[Extract API Key]
    B --> C[Hash the key]
    C --> D[Lookup in DB]
    D --> E{Found?}
    E -->|No| F[401 Unauthorized]
    E -->|Yes| G[Check rate limit]
    G --> H[Allow request]
    F --> I[Return Error]
    H --> J[Proxy to provider]
    
    style F fill:#ffcdd2
    style H fill:#c8e6c9
```

---

## Provider Configuration

### SDK Provider Dependencies (pyproject.toml)

```mermaid
graph TB
    subgraph Core["Core Dependencies"]
        C1[pydantic>2,<3]
        C2[openai>=1.99.3]
        C3[anthropic>=0.83.0]
        C4[httpx]
    end
    
    subgraph Providers["Provider Groups"]
        P1[cloud<br/>Vertex AI, Bedrock]
        P2[specialized<br/>Mistral, Cohere, Groq]
        P3[local<br/>Ollama, LM Studio]
        P4[enterprise<br/>Watsonx, Azure]
    end
    
    subgraph Optional["Optional Groups"]
        O1[platform<br/>OTEL tracing]
        O2[gateway<br/>FastAPI, uvicorn, SQLAlchemy]
    end
    
    Core --> Providers
    Core --> Optional
    
    style Core fill:#e8f5e9
    style Providers fill:#e3f2fd
    style Optional fill:#fff3e0
```

### Supported Provider Matrix

| Provider | Dependency | Config Fields |
|----------|------------|---------------|
| OpenAI | built-in | `api_key`, `api_base` |
| Anthropic | built-in | `api_key` |
| Azure OpenAI | `azure*` | `api_key`, `api_base` |
| Vertex AI | `vertexai` | `credentials`, `project`, `location` |
| Mistral | `mistral` | `api_key` |
| Cohere | `cohere` | `api_key` |
| Groq | `groq` | `api_key` |
| AWS Bedrock | `bedrock` | via boto3 |
| Ollama | `ollama` | `api_base` (local) |
| LM Studio | `lmstudio` | `api_base` (local) |
| Together | `together` | `api_key` |
| DeepSeek | `deepseek` | `api_key` |

---

## Rate Limiting

```mermaid
graph TB
    subgraph RateLimit["Rate Limiting"]
        RPM[RPM Limit<br/>Requests/Minute]
        InMemory[(In-Memory<br/>Counter)]
        Redis[(Optional<br/>Redis)]
    end
    
    subgraph Check["Per-Request Check"]
        Get[Get current count]
        Inc[Increment]
        Check{Count <= Limit?}
        Check -->|Yes| Allow[Allow]
        Check -->|No| Deny[Deny 429]
    end
    
    RateLimit --> Check
    
    style RateLimit fill:#e8f5e9
    style Allow fill:#c8e6c9
    style Deny fill:#ffcdd2
```

---

## Observability

### Prometheus Metrics

```python
# Metrics exposed at /metrics
- gateway_requests_total{provider, model, status}
- gateway_request_duration_seconds{provider, model}
- gateway_tokens_total{provider, model, type}
- gateway_spend_total{provider, model}
```

### Logging

```python
# Via log_config.py
- Structured JSON logs
- Request/response logging
- Error tracking
```

---

## Deployment

### Docker Compose

```yaml
services:
  gateway:
    image: ghcr.io/mozilla-ai/any-llm/gateway:latest
    ports:
      - "8000:8000"
    volumes:
      - ./config.yml:/app/config.yml
    command: ["any-llm-gateway", "serve", "--config", "/app/config.yml"]
    depends_on:
      postgres:
        condition: service_healthy

  postgres:
    image: postgres:16-alpine
    environment:
      - POSTGRES_USER=gateway
      - POSTGRES_PASSWORD=gateway
      - POSTGRES_DB=gateway
    volumes:
      - postgres_data:/var/lib/postgresql/data
```

### CLI Commands

```bash
# Serve the gateway
any-llm-gateway serve --config config.yml

# Initialize database
any-llm-gateway init-db --config config.yml

# Run migrations
any-llm-gateway migrate --revision head
```

---

## Key Patterns for CipherOcto

### 1. Configuration Pattern

```python
# Use Pydantic Settings with YAML + env vars
class CipherOctoConfig(BaseSettings):
    model_config = SettingsConfigDict(
        env_prefix="CIPHER_",
        env_file=".env",
        case_sensitive=False,
    )
```

### 2. API Key Pattern

```python
# Key format: cipher_{random}
# Hash: SHA-256
# Store: Database with hash lookup
```

### 3. Provider Config Pattern

```python
# Provider configs in YAML
providers:
  openai:
    api_key: ${OPENAI_API_KEY}
    # Client args passed to SDK init
```

### 4. Gateway Pattern

```python
# FastAPI with middleware stack
app = FastAPI()
app.add_middleware(SecurityHeadersMiddleware)
app.add_middleware(CORSMiddleware, ...)
# Register routers
register_routers(app)
```

---

## Summary

| Aspect | any-llm Pattern |
|--------|-----------------|
| **Config** | YAML + Pydantic Settings + env vars |
| **API Keys** | `gw-` prefix, SHA-256 hash |
| **Providers** | Per-provider config in YAML |
| **Gateway** | FastAPI + middleware |
| **Database** | SQLite (dev) / PostgreSQL (prod) |
| **Rate Limiting** | In-memory RPM counter |
| **Metrics** | Prometheus at /metrics |

---

**Next Steps:**
- [ ] Extract reusable config patterns for CipherOcto
- [ ] Design CipherOcto virtual key system
- [ ] Create protocol comparison with Bifrost/LiteLLM