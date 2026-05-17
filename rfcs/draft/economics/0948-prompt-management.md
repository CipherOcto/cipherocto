# RFC-0948 (Economics): Prompt Management

## Status

Draft

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

Define a prompt management system for quota-router that enables centralized storage, versioning, deployment, and A/B testing of prompt templates. Provides enterprise users with prompt lifecycle management integrated with the completion endpoints.

## Dependencies

**Requires:**

- RFC-0903 (Economics): Virtual API Key System
- RFC-0932 (Economics): Team Management
- RFC-0914 (Economics): Stoolap-only persistence (storage backend)

**Optional:**

- RFC-0904 (Economics): Real-Time Cost Tracking (per-prompt cost tracking)
- RFC-0947 (Economics): Callback System (prompt change notifications)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | <1ms overhead | Template resolution latency |
| G2 | Full versioning | Semantic versioning with rollback |
| G3 | A/B testing | Traffic splitting between versions |
| G4 | Per-team isolation | Team-scoped prompt access |

## Motivation

### Problem

Enterprise LLM deployments need centralized prompt management:

1. **Version Control** — Track prompt changes, rollback to previous versions
2. **Consistency** — Ensure all applications use the same prompts
3. **A/B Testing** — Test prompt variants with traffic splitting
4. **Collaboration** — Teams share and iterate on prompts
5. **Compliance** — Audit trail of prompt changes

### Use Cases

- **Customer support**: Standardized system prompts for support bots
- **Code generation**: Tested and validated code generation prompts
- **Content creation**: Brand-consistent content prompts
- **Data extraction**: Structured extraction prompts with known accuracy

## Specification

### Prompt Template

```rust
// Dependencies: chrono (for DateTime<Utc>), serde (Serialize, Deserialize)
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    /// Unique prompt ID
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Semantic version (e.g., "1.2.0")
    pub version: String,
    /// Team that owns this prompt
    pub team_id: Option<String>,
    /// Template content with {{variable}} placeholders
    pub template: String,
    /// Default variable values
    #[serde(default)]
    pub defaults: HashMap<String, String>,
    /// Model this prompt is optimized for
    pub model: Option<String>,
    /// Tags for organization
    pub tags: Vec<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Created by user
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVersion {
    /// Prompt ID
    pub prompt_id: String,
    /// Version string
    pub version: String,
    /// Template content
    pub template: String,
    /// Change description
    pub changelog: String,
    /// Is this the active version?
    pub active: bool,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Created by
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptFilter {
    /// Filter by team ID
    pub team_id: Option<String>,
    /// Filter by name (substring match)
    pub name: Option<String>,
    /// Filter by tags (all must match)
    pub tags: Option<Vec<String>>,
    /// Filter by model
    pub model: Option<String>,
    /// Pagination: limit
    pub limit: Option<u32>,
    /// Pagination: offset
    pub offset: Option<u32>,
}

/// Prompt fields to add to ChatCompletionRequest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptFields {
    /// Prompt ID to resolve before completion
    pub prompt_id: Option<String>,
    /// Variables for template rendering
    pub prompt_variables: Option<HashMap<String, String>>,
}
```

### ChatCompletionRequest Extension

The existing `ChatCompletionRequest` struct MUST be extended with prompt fields:

```rust
// Add to existing ChatCompletionRequest in types.rs
pub struct ChatCompletionRequest {
    // ... existing fields (model, messages, temperature, etc.)

    /// Optional prompt ID for template resolution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_id: Option<String>,

    /// Variables for prompt template rendering
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_variables: Option<HashMap<String, String>>,
}
```

JSON serialization:
```json
{
  "model": "gpt-4",
  "prompt_id": "customer-support-v1",
  "prompt_variables": {
    "customer_name": "John",
    "issue": "billing"
  },
  "messages": [{"role": "user", "content": "Help me"}]
}
```

### Template Variables

```rust
/// Template variable syntax: {{variable_name}}
/// Supports: {{variable}}, {{variable | default}}, {{variable | truncate:100}}
pub struct TemplateEngine;

impl TemplateEngine {
    /// Render template with variables
    pub fn render(
        template: &str,
        variables: &HashMap<String, String>,
        defaults: &HashMap<String, String>,
    ) -> Result<String> {
        // Find all {{variable}} patterns
        // Replace with variable value or default
        // Apply filters (truncate, upper, lower, etc.)
    }
}

/// Supported filters
pub enum TemplateFilter {
    /// Default value: {{var | default:"fallback"}}
    Default(String),
    /// Truncate: {{var | truncate:100}}
    Truncate(usize),
    /// Uppercase: {{var | upper}}
    Upper,
    /// Lowercase: {{var | lower}}
    Lower,
    /// Strip whitespace: {{var | strip}}
    Strip,
}
```

### Template Injection Prevention

Variable values MUST be treated as literal text, not template syntax. The template engine MUST:

1. **Single-pass rendering**: After substituting a variable, do NOT re-scan the result for `{{...}}` patterns
2. **No nested templates**: Variable values containing `{{` are rendered literally
3. **HTML escaping**: For web contexts, escape `<`, `>`, `&`, `"`, `'` in variable values
4. **Length limits**: Variable values > 10KB are rejected

```rust
impl TemplateEngine {
    /// Sanitize variable value to prevent injection
    fn sanitize_value(value: &str) -> String {
        // 1. Reject if value contains {{ (prevents nested template injection)
        // 2. HTML-escape special characters
        // 3. Truncate to max length (10KB)
    }
}
```

### Prompt Registry

```rust
/// Prompt registry — stores and serves prompts
/// Thread-safe: shared across proxy workers via Arc<RwLock<PromptRegistry>>
pub struct PromptRegistry {
    /// Storage backend (stoolap)
    storage: PromptStorage,
    /// In-memory cache (LRU) — RwLock for concurrent reads
    cache: RwLock<LruCache<String, PromptTemplate>>,
    /// A/B test state — RwLock for concurrent reads
    ab_tests: RwLock<HashMap<String, AbTest>>,
}

// Usage in proxy:
// let registry = Arc::new(RwLock::new(PromptRegistry::new(...)));
// Multiple proxy workers share the same Arc instance.
// Reads (resolve, list) acquire read lock.
// Writes (create, update, delete) acquire write lock.
// Cache hits avoid storage access entirely.


impl PromptRegistry {
    /// Get prompt by ID (latest active version)
    pub async fn get(&self, prompt_id: &str) -> Result<PromptTemplate>;

    /// Get prompt by ID and version
    pub async fn get_version(&self, prompt_id: &str, version: &str) -> Result<PromptTemplate>;

    /// Create new prompt
    pub async fn create(&self, prompt: PromptTemplate) -> Result<String>;

    /// Update prompt (creates new version)
    pub async fn update(&self, prompt_id: &str, template: &str, changelog: &str) -> Result<String>;

    /// Rollback to previous version
    pub async fn rollback(&self, prompt_id: &str, version: &str) -> Result<()>;

    /// Delete prompt
    pub async fn delete(&self, prompt_id: &str) -> Result<()>;

    /// List prompts (with filters)
    pub async fn list(&self, filter: PromptFilter) -> Result<Vec<PromptTemplate>>;

    /// Resolve prompt with A/B testing
    pub async fn resolve(&self, prompt_id: &str) -> Result<PromptTemplate>;
}
```

### A/B Testing

```rust
/// A/B test configuration
pub struct AbTest {
    /// Prompt ID
    pub prompt_id: String,
    /// Version A (control)
    pub version_a: String,
    /// Version B (treatment)
    pub version_b: String,
    /// Traffic weight for version B (0.0-1.0)
    pub weight_b: f64,
    /// Start time
    pub start_at: DateTime<Utc>,
    /// End time
    pub end_at: Option<DateTime<Utc>>,
    /// Metrics collected
    pub metrics: AbTestMetrics,
}

impl AbTest {
    /// Select version based on traffic weight
    /// request_id: API key ID (from X-Api-Key header or virtual key)
    /// This ensures same API key always gets same version during an A/B test.
    pub fn select_version(&self, request_id: &str) -> String {
        // Use deterministic hashing of request_id
        // Ensures same request always gets same version
        let hash = hash(request_id);
        if (hash % 1000) as f64 / 1000.0 < self.weight_b {
            self.version_b.clone()
        } else {
            self.version_a.clone()
        }
    }
}

// request_id source priority:
// 1. API key ID (from validated X-Api-Key header) — preferred
// 2. X-Request-Id header (if present)
// 3. Generated UUID (fallback, less useful for consistency)


#[derive(Debug, Default)]
pub struct AbTestMetrics {
    pub requests_a: u64,
    pub requests_b: u64,
    pub avg_latency_a: f64,
    pub avg_latency_b: f64,
    pub error_rate_a: f64,
    pub error_rate_b: f64,
    pub avg_tokens_a: u64,
    pub avg_tokens_b: u64,
}

// Metrics collection:
// - resolve() increments requests_a or requests_b counter
// - Completion response handler updates latency/token metrics
// - Error handler updates error_rate metrics
// - Metrics stored in AbTest struct (persisted to stoolap periodically)
// - GET /prompts/:id/ab-test returns current metrics snapshot
```

### Configuration

```yaml
# In config.yaml
prompts:
  enabled: true
  storage: stoolap  # or sqlite
  cache_size: 1000  # LRU cache entries
  cache_ttl: 300    # seconds

  # Default prompt (used when no prompt_id specified)
  default_prompt: null

  # Per-team prompt isolation
  team_isolation: true
```

### API Endpoints

```rust
// Prompt CRUD
GET    /prompts                    // List prompts
POST   /prompts                    // Create prompt
GET    /prompts/:id                // Get prompt (latest active)
GET    /prompts/:id/:version       // Get specific version
PUT    /prompts/:id                // Update prompt (creates new version)
DELETE /prompts/:id                // Delete prompt
POST   /prompts/:id/rollback       // Rollback to version

// Prompt versions
GET    /prompts/:id/versions       // List all versions
POST   /prompts/:id/versions/:v/activate  // Activate version

// A/B testing
POST   /prompts/:id/ab-test        // Start A/B test
GET    /prompts/:id/ab-test        // Get A/B test status
DELETE /prompts/:id/ab-test        // Stop A/B test

// Usage with completion
POST   /v1/chat/completions
{
  "model": "gpt-4",
  "prompt_id": "customer-support-v1",
  "prompt_variables": {
    "customer_name": "John",
    "issue": "billing question"
  },
  "messages": [...]
}
```

### Integration with Completion Endpoints

```rust
/// Resolve prompt and inject into messages
pub async fn resolve_prompt(
    registry: &PromptRegistry,
    request: &mut ChatCompletionRequest,
) -> Result<()> {
    if let Some(prompt_id) = &request.prompt_id {
        // Resolve prompt (with A/B testing)
        let prompt = registry.resolve(prompt_id).await?;

        // Render template with variables
        let rendered = TemplateEngine::render(
            &prompt.template,
            &request.prompt_variables.as_ref().unwrap_or(&HashMap::new()),
            &prompt.defaults,
        )?;

        // Inject as system message
        request.messages.insert(0, Message {
            role: "system".to_string(),
            content: rendered,
        });
    }
    Ok(())
}
```

### LiteLLM Interface Parity

> **Note:** LiteLLM does not have a native prompt management API. The interface below is a custom extension that provides similar functionality to external prompt platforms (LangSmith, Helicone) but integrated directly into quota-router. This is NOT a LiteLLM-compatible API — it is a new feature for enterprise users.

```python
# Python SDK — custom prompt management API (not in LiteLLM)
import quota_router

# Use prompt template
response = quota_router.completion(
    model="gpt-4",
    prompt_id="customer-support-v1",
    prompt_variables={
        "customer_name": "John",
        "issue": "billing question",
    },
    messages=[{"role": "user", "content": "Help me with my bill"}],
)

# Create prompt
quota_router.prompts.create(
    name="customer-support",
    template="You are a support agent for {{company}}. Customer: {{customer_name}}. Issue: {{issue}}.",
    model="gpt-4",
)
```

## Error Handling

| Error | HTTP Status | Behavior |
|-------|-------------|----------|
| `prompt_id` not found | 404 | Return error, do NOT pass to provider |
| `prompt_id` exists but no active version | 404 | Return error with "No active version" |
| Template variable missing, no default | 400 | Return error with missing variable name |
| Template rendering failure | 500 | Return error, do NOT pass raw template |
| A/B test ended | N/A | Use `version_a` (control) as fallback |
| A/B test not found | 404 | Return error |
| Storage backend unavailable | 503 | Return error, do NOT use stale cache |
| Cache miss + storage timeout | 504 | Return error after 5s timeout |

### Error Response Format

```json
{
  "error": {
    "type": "prompt_error",
    "code": "prompt_not_found",
    "message": "Prompt 'invalid-id' not found",
    "param": "prompt_id"
  }
}
```

### Fallback Behavior

When prompt resolution fails:
1. Return HTTP error to client (do NOT silently skip)
2. Do NOT pass unresolved template to provider
3. Log error with request_id for debugging
4. If A/B test ended, use control version (version_a)

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Prompt resolution | <1ms | Cache hit |
| Prompt resolution | <5ms | Cache miss (DB lookup) |
| Template rendering | <1ms | Per template |
| A/B test selection | <0.1ms | Hash-based |
| Storage overhead | <1KB | Per prompt version |

## Security Considerations

| Threat | Impact | Mitigation |
|--------|--------|------------|
| Prompt injection via templates | High | Sanitize template variables |
| Unauthorized prompt access | Medium | Team-scoped access control |
| Prompt exfiltration | Medium | Audit log, access controls |
| Template DoS | Medium | Limit template size, recursion depth |
| A/B test manipulation | Low | Deterministic hashing, server-side |

## Adversarial Review

| Threat | Impact | Mitigation |
|--------|--------|------------|
| Template injection via variables | High | Sanitize all variable values |
| Prompt version tampering | High | Immutable versions, audit log |
| A/B test gaming | Medium | Deterministic assignment, no client control |
| Cache poisoning | Medium | TTL-based expiry, team isolation |
| Template recursion | High | Max depth limit (5), no self-reference |

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/prompts/mod.rs` | New — prompt registry, template engine |
| `crates/quota-router-core/src/prompts/storage.rs` | New — stoolap-backed storage |
| `crates/quota-router-core/src/prompts/template.rs` | New — template rendering engine |
| `crates/quota-router-core/src/prompts/ab_test.rs` | New — A/B testing logic |
| `crates/quota-router-core/src/config.rs` | Add PromptConfig |
| `crates/quota-router-core/src/admin.rs` | Add prompt CRUD endpoints |
| `crates/quota-router-core/src/proxy.rs` | Resolve prompt before provider call |
| `crates/quota-router-core/src/python_sdk/mod.rs` | Add Python prompt support |

## Implementation Phases

### Phase 1: Core Infrastructure

- [ ] Define PromptTemplate, PromptVersion types
- [ ] Implement PromptRegistry with stoolap storage
- [ ] Implement TemplateEngine with variable substitution
- [ ] Add PromptConfig to config.rs

### Phase 2: API & Integration

- [ ] Add prompt CRUD endpoints to admin API
- [ ] Integrate prompt resolution into proxy.rs
- [ ] Add prompt_id to ChatCompletionRequest
- [ ] Implement prompt caching (LRU)

### Phase 3: Versioning & A/B Testing

- [ ] Implement version management (create, rollback, activate)
- [ ] Implement A/B testing (traffic splitting, metrics)
- [ ] Add version listing and comparison

### Phase 4: Python SDK & Advanced

- [ ] Add Python SDK prompt support
- [ ] Implement per-team prompt isolation
- [ ] Add prompt analytics (usage, cost per prompt)
- [ ] Add prompt import/export

## Future Work

- F1: Prompt marketplace (share prompts across teams)
- F2: Prompt optimization (automated prompt improvement)
- F3: Prompt analytics dashboard
- F4: Prompt templates library (pre-built templates)
- F5: Multi-modal prompt support (images, audio)

## Rationale

### Why Stoolap Storage?

- No external dependencies (Redis, PostgreSQL)
- Single binary deployment
- Consistent with quota-router's storage strategy
- Sufficient for prompt management workloads

### Why Deterministic A/B Testing?

- Same request always gets same version (consistency)
- No client-side manipulation
- Reproducible results for debugging
- No sticky sessions required

### Why Immutable Versions?

- Audit trail (who changed what, when)
- Safe rollback (previous versions are preserved)
- A/B testing (multiple versions coexist)
- Compliance (version history is permanent)

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| External prompt platform (LangSmith) | Feature-rich | External dependency |
| Git-based prompts | Version control native | Complex, slow |
| Database-only (no cache) | Simple | Slow for high-QPS |
| Client-side templates | No server overhead | No versioning, no A/B testing |

## Test Vectors

```rust
#[test]
fn test_template_rendering() {
    let template = "Hello {{name}}, your order {{order_id}} is {{status}}.";
    let variables = HashMap::from([
        ("name".to_string(), "John".to_string()),
        ("order_id".to_string(), "12345".to_string()),
        ("status".to_string(), "shipped".to_string()),
    ]);
    let result = TemplateEngine::render(template, &variables, &HashMap::new()).unwrap();
    assert_eq!(result, "Hello John, your order 12345 is shipped.");
}

#[test]
fn test_template_default_filter() {
    let template = "Hello {{name | default:World}}";
    let result = TemplateEngine::render(template, &HashMap::new(), &HashMap::new()).unwrap();
    assert_eq!(result, "Hello World");
}

#[test]
fn test_ab_test_deterministic() {
    let test = AbTest {
        prompt_id: "test".to_string(),
        version_a: "1.0".to_string(),
        version_b: "2.0".to_string(),
        weight_b: 0.5,
        start_at: Utc::now(),
        end_at: None,
        metrics: AbTestMetrics::default(),
    };
    // Same request_id always gets same version
    let v1 = test.select_version("req-123");
    let v2 = test.select_version("req-123");
    assert_eq!(v1, v2);
}

#[test]
fn test_template_truncate_filter() {
    let template = "{{bio | truncate:10}}";
    let variables = HashMap::from([
        ("bio".to_string(), "This is a very long biography".to_string()),
    ]);
    let result = TemplateEngine::render(template, &variables, &HashMap::new()).unwrap();
    assert_eq!(result, "This is a ");
}

#[test]
fn test_ab_test_weight_boundaries() {
    let mut test = AbTest {
        prompt_id: "test".to_string(),
        version_a: "1.0".to_string(),
        version_b: "2.0".to_string(),
        weight_b: 0.0,  // All traffic to version A
        start_at: Utc::now(),
        end_at: None,
        metrics: AbTestMetrics::default(),
    };
    assert_eq!(test.select_version("any"), "1.0");

    test.weight_b = 1.0;  // All traffic to version B
    assert_eq!(test.select_version("any"), "2.0");
}
```

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1 | 2026-05-17 | Initial draft |
| v2 | 2026-05-17 | Round 1 fixes: duplicate deps removed, ChatCompletionRequest extension specified, PromptFilter defined, Error Handling section added, request_id source specified, concurrent access (RwLock), Message struct fixed, A/B weight single source (AbTest only) |

## Related RFCs

- RFC-0903 (Economics): Virtual API Key System
- RFC-0932 (Economics): Team Management
- RFC-0904 (Economics): Real-Time Cost Tracking
- RFC-0947 (Economics): Callback System

## Related Use Cases

- Enhanced Quota Router Gateway
- Enterprise AI Gateway
- LiteLLM Drop-in Replacement
