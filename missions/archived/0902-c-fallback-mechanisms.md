# Mission: RFC-0902-c: Fallback Mechanisms

## Status
Archived
Claimed

## RFC

RFC-0902 (Economics): Multi-Provider Routing and Load Balancing

## Dependencies

- Mission-0902-a: Routing Strategy Core
- Mission-0902-b: Advanced Routing Strategies

## Acceptance Criteria

- [x] Basic fallback configuration
- [x] Fallback chain execution (try next model on failure)
- [x] Content policy fallback mapping
- [x] Context window fallback mapping
- [x] Retry with exponential backoff
- [x] Max retries configuration
- [x] Unit tests for fallback logic

## Description

Implement fallback mechanisms based on LiteLLM's fallback handling:

- Route to alternate model group when primary fails
- Different fallback types for different error scenarios
- Configurable retry behavior

## Technical Details

### Fallback Types (LiteLLM reference)

| Type | Trigger | Description |
|------|---------|-------------|
| `fallbacks` | General errors | Route to next model group |
| `content_policy_fallbacks` | ContentPolicyViolationError | Map across providers |
| `context_window_fallbacks` | ContextWindowExceededError | Map to larger context models |

### Fallback Configuration

```yaml
router_settings:
  fallbacks:
    - model: gpt-3.5-turbo
      fallback_models:
        - gpt-4
        - claude-3-opus

  context_window_fallbacks:
    gpt-3.5-turbo: gpt-3.5-turbo-16k

  content_policy_fallbacks:
    gpt-4: claude-3-opus
```

### Fallback Execution

```
Request to Model A fails
    ↓
Check fallback list: [Model B, Model C]
    ↓
Try Model B
    ↓
Success → Return response
    ↓
Failure → Continue to Model C
    ↓
All fail → Return error
```

## Implementation Notes

**Files created:**
- `crates/quota-router-core/src/fallback.rs` - New fallback module

**Implemented:**
- RouterError enum with error type classification
- FallbackEntry and FallbackConfig structs
- FallbackExecutor with retry logic
- Exponential backoff calculation
- 3 fallback types: general, content_policy, context_window

**Tests:** 5 fallback tests passing (22 total)

---

**Claimant:** @claude-code

**Pull Request:** #

**Related RFCs:**
- RFC-0902: Multi-Provider Routing and Load Balancing
