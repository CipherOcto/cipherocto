---
title: "RFC-0952: Additional Providers (Databricks, Perplexity)"
status: Accepted
version: 0.1.0
created: 2026-05-18
updated: 2026-05-18
authors:
  - quota-router team
related:
  - RFC-0902 (Multi-Provider Routing and Load Balancing)
  - RFC-0930 (Provider Inference from Model String)
  - RFC-0928 (Deployment Configuration Schema)
---

# RFC-0952: Additional Providers (Databricks, Perplexity)

## Status

Accepted

## Summary

Add native HTTP provider support for Databricks (DBRX models) and Perplexity to expand quota-router's native_http provider coverage from 11 to 13 providers.

**Current State:** Both providers already exist in the pyo3 layer (`crates/quota-router-pyo3/src/providers/databricks.rs` and `perplexity.rs`). This RFC adds native_http implementations for litellm-mode (RFC-0917) support, enabling direct HTTP proxy without Python dependency.

## Dependencies

**Requires:**

- RFC-0902 (Economics): Multi-Provider Routing and Load Balancing
- RFC-0930 (Economics): Provider Inference from Model String

**Optional:**

- RFC-0941 (Economics): Streaming Parity

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Full native_http provider parity | 13 providers |
| G2 | Streaming support | Both providers |
| G3 | Model inference | Auto-detect from prefix |
| G4 | Error mapping | RFC-0908 taxonomy |

## Motivation

litellm supports 100+ providers. quota-router currently supports 44 providers (py_bridge) + 11 (native_http). Adding Databricks and Perplexity closes the gap for users who depend on:
- Databricks: DBRX models, Unity Catalog integration, enterprise AI platform
- Perplexity: Search-augmented models, real-time information retrieval

## Specification

### Provider Registration

Following the `HttpProviderFactory` pattern from `crates/quota-router-core/src/native_http/mod.rs`:

```rust
// Register Databricks provider
HttpProviderFactory::register("databricks", || Box::new(DatabricksProvider::new()));

// Register Perplexity provider
HttpProviderFactory::register("perplexity", || Box::new(PerplexityProvider::new()));
```

### Model String Inference

```rust
// Databricks
"databricks/dbrx-instruct" -> Provider::Databricks
"databricks/dbrx-base" -> Provider::Databricks

// Perplexity
"perplexity/sonar-small-online" -> Provider::Perplexity
"perplexity/sonar-medium-online" -> Provider::Perplexity
"perplexity/sonar-large-online" -> Provider::Perplexity
```

### Databricks Provider

```rust
struct DatabricksProvider {
    client: reqwest::Client,
    workspace_url: String,
    api_key: String,
}

impl HttpProvider for DatabricksProvider {
    async fn completion(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let url = format!("{}/serving-endpoints/{}/invocations", self.workspace_url, request.model);
        // Use OpenAI-compatible format
        let response = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;
        // Parse OpenAI-compatible response
    }

    async fn streaming_completion(&self, request: CompletionRequest, tx: Sender<Chunk>) -> Result<(), ProviderError> {
        // SSE streaming with OpenAI format
    }
}
```

### Perplexity Provider

```rust
struct PerplexityProvider {
    client: reqwest::Client,
    api_key: String,
}

impl HttpProvider for PerplexityProvider {
    async fn completion(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let url = "https://api.perplexity.ai/chat/completions";
        // Perplexity uses OpenAI-compatible format
        let response = self.client.post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;
        // Parse response (includes citations)
    }

    async fn streaming_completion(&self, request: CompletionRequest, tx: Sender<Chunk>) -> Result<(), ProviderError> {
        // SSE streaming with OpenAI format
    }
}
```

### Databricks-Specific Features

```rust
struct DatabricksConfig {
    workspace_url: String,           // "https://dbc-xxx.databricks.com"
    api_key: String,                 // Databricks PAT token
    serving_endpoint: Option<String>, // Custom endpoint name
}
```

### Perplexity-Specific Features

```rust
struct PerplexityResponse {
    // Standard OpenAI fields
    id: String,
    model: String,
    choices: Vec<Choice>,
    usage: Usage,
    // Perplexity-specific
    citations: Option<Vec<String>>,  // Source citations
}

struct PerplexityRequest {
    // Standard OpenAI fields
    model: String,
    messages: Vec<Message>,
    // Perplexity-specific
    return_citations: Option<bool>,
    search_domain_filter: Option<Vec<String>>,
    search_recency_filter: Option<String>,  // "day", "week", "month", "year"
}
```

### Environment Variables

```bash
# Databricks (aligned with existing pyo3 implementation)
DATABRICKS_BASE_URL=https://dbc-xxx.databricks.com
DATABRICKS_API_KEY=dapi-xxx

# Perplexity
PERPLEXITY_API_KEY=pplx-xxx
```

### Error Mapping

Following RFC-0920 error taxonomy:

| Provider Error | HTTP Status | quota-router Error |
|---------------|-------------|-------------------|
| Invalid API key | 401 | AuthenticationError |
| Rate limit exceeded | 429 | RateLimitError |
| Model not found | 404 | ModelNotFoundError |
| Invalid request | 400 | InvalidRequestError |
| Context window exceeded | 400 | ContextLengthExceededError |
| Content policy violation | 400 | ContentFilterError |
| Server error | 500 | ProviderError |
| Connection timeout | 504 | GatewayTimeoutError |

### Config Integration

```yaml
# config.yaml
model_list:
  - model_name: dbrx
    litellm_params:
      model: databricks/dbrx-instruct
      api_key: os.environ['DATABRICKS_API_KEY']
      api_base: os.environ['DATABRICKS_BASE_URL']

  - model_name: perplexity-sonar
    litellm_params:
      model: perplexity/sonar-large-online
      api_key: os.environ['PERPLEXITY_API_KEY']
```

## Acceptance Criteria

- [ ] Databricks provider completes chat/completions requests
- [ ] Perplexity provider completes chat/completions requests
- [ ] Streaming works for both providers
- [ ] Model string inference works ("databricks/*", "perplexity/*")
- [ ] Environment variable configuration works
- [ ] Error mapping follows RFC-0920 taxonomy
- [ ] Perplexity citations are preserved in response
- [ ] Config validation accepts both providers
- [ ] All existing tests pass

## Key Files

| File | Change |
|------|--------|
| `crates/quota-router-core/src/native_http/databricks.rs` | New - Databricks provider |
| `crates/quota-router-core/src/native_http/perplexity.rs` | New - Perplexity provider |
| `crates/quota-router-core/src/native_http/mod.rs` | Register new providers |
| `crates/quota-router-core/src/config.rs` | Add provider configs |
| `crates/quota-router-core/src/error.rs` | Map provider errors |

## Security Considerations

- API keys MUST be stored securely (environment variables or secret manager)
- Databricks workspace URL MUST be validated (HTTPS only)
- Perplexity API key MUST be masked in logs

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-05-18 | Initial draft |
