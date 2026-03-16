# RFC-0911 (Economics): Capability-Based API Keys

## Status

Planned

## Authors

- Author: @cipherocto

## Summary

Define a **capability-based API key system** that extends RFC-0903 with fine-grained, programmable permissions. Keys contain capabilities that define exactly what operations are allowed, enabling role-based access control beyond simple route matching.

## Dependencies

**Requires:**

- RFC-0903: Virtual API Key System

**Optional:**

- RFC-0910: Pricing Table Registry

## Motivation

RFC-0903 provides:

- `allowed_routes`: coarse route permissions
- `key_type`: LLM_API, MANAGEMENT, READ_ONLY, DEFAULT
- Team-based budgets

This is still **coarse-grained**. Real systems need:

- Per-model limits
- Per-request budget caps
- Provider-specific restrictions
- Time-based access windows

## Design

### Capability Structure

```rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A single capability grant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    /// Resource pattern (e.g., "openai:gpt-4", "anthropic:*", "*:*")
    pub resource: String,
    /// Maximum tokens per request (0 = unlimited)
    pub max_tokens_per_request: Option<u32>,
    /// Maximum requests per minute (0 = unlimited)
    pub max_requests_per_minute: Option<u32>,
    /// Budget limit for this capability (in deterministic units)
    pub budget_limit: Option<u64>,
    /// Time window restrictions
    pub time_window: Option<TimeWindow>,
    /// Custom conditions
    pub conditions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Hours of day (0-23) when access is allowed
    pub allowed_hours: Vec<u8>,
    /// Days of week (0-6, Sunday = 0) when access is allowed
    pub allowed_days: Vec<u8>,
}

/// Full key capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyCapabilities {
    /// List of capability grants
    pub capabilities: Vec<Capability>,
    /// Fallback: if no capability matches, deny by default
    pub default_deny: bool,
}

impl Default for KeyCapabilities {
    fn default() -> Self {
        Self {
            capabilities: Vec::new(),
            default_deny: true,
        }
    }
}
```

### Example Capabilities

```json
{
  "capabilities": [
    {
      "resource": "openai:gpt-4",
      "max_tokens_per_request": 4000,
      "max_requests_per_minute": 10
    },
    {
      "resource": "anthropic:claude-3-opus",
      "max_tokens_per_request": 8000,
      "budget_limit": 1000000
    },
    {
      "resource": "openai:embeddings",
      "max_requests_per_minute": 100
    },
    {
      "resource": "*:*",
      "time_window": {
        "allowed_hours": [9, 10, 11, 12, 13, 14, 15, 16, 17],
        "allowed_days": [1, 2, 3, 4, 5]
      }
    }
  ],
  "default_deny": true
}
```

### Capability Matching

```rust
/// Check if a request matches any capability
pub fn check_capability(
    capabilities: &KeyCapabilities,
    request: &RequestContext,
) -> Result<CapabilityMatch, CapabilityError> {
    for cap in &capabilities.capabilities {
        if matches_resource(&cap.resource, &request.resource) {
            // Check limits
            if let Some(max_tokens) = cap.max_tokens_per_request {
                if request.tokens > max_tokens {
                    return Err(CapabilityError::TokenLimitExceeded {
                        requested: request.tokens,
                        limit: max_tokens,
                    });
                }
            }

            // Check time window if specified
            if let Some(ref window) = cap.time_window {
                check_time_window(window, request.timestamp)?;
            }

            // All checks passed
            return Ok(CapabilityMatch {
                capability: cap.clone(),
                budget_remaining: cap.budget_limit,
            });
        }
    }

    // No matching capability
    if capabilities.default_deny {
        Err(CapabilityError::NoMatchingCapability)
    } else {
        Ok(CapabilityMatch {
            capability: Capability {
                resource: "*".to_string(),
                ..Default::default()
            },
            budget_remaining: None,
        })
    }
}

fn matches_resource(pattern: &str, resource: &str) -> bool {
    // Simple wildcard matching: "provider:model" or "*:*" or "openai:*"
    let pattern_parts: Vec<&str> = pattern.split(':').collect();
    let resource_parts: Vec<&str> = resource.split(':').collect();

    if pattern_parts.len() != 2 || resource_parts.len() != 2 {
        return false;
    }

    (pattern_parts[0] == "*" || pattern_parts[0] == resource_parts[0])
        && (pattern_parts[1] == "*" || pattern_parts[1] == resource_parts[1])
}
```

### API Key Extension

```rust
/// Extended API key with capabilities (extends RFC-0903)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    // ... RFC-0903 fields ...

    /// Capability-based permissions (replaces allowed_routes)
    pub capabilities: KeyCapabilities,

    /// Hash of capabilities for deterministic enforcement
    pub capabilities_hash: [u8; 32],
}

impl ApiKey {
    /// Compute deterministic hash of capabilities
    pub fn compute_capabilities_hash(&self) -> [u8; 32] {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        let serialized = serde_json::to_string(&self.capabilities).unwrap();
        hasher.update(serialized.as_bytes());
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }
}
```

### Integration with RFC-0903

The capability system **extends** RFC-0903, not replaces it:

| RFC-0903 Field   | Capability Equivalent                |
| ---------------- | ------------------------------------ |
| `allowed_routes` | `Capability.resource` patterns       |
| `key_type`       | Mapped to default capabilities       |
| `rpm_limit`      | `Capability.max_requests_per_minute` |
| `budget_limit`   | `Capability.budget_limit`            |

Backward compatibility:

```rust
/// Convert legacy allowed_routes to capabilities
impl From<Vec<String>> for KeyCapabilities {
    fn from(routes: Vec<String>) -> Self {
        let capabilities = routes
            .into_iter()
            .map(|route| Capability {
                resource: format!("*:{}", route),
                max_tokens_per_request: None,
                max_requests_per_minute: None,
                budget_limit: None,
                time_window: None,
                conditions: BTreeMap::new(),
            })
            .collect();

        Self {
            capabilities,
            default_deny: true,
        }
    }
}
```

## Why Needed

- **Fine-grained control**: Beyond route-level permissions
- **Role-based access**: Frontend, batch, admin keys
- **Programmable keys**: Keys are data, not just boolean flags
- **Deterministic enforcement**: Capabilities hash enables cross-router verification

## Out of Scope

- OAuth2/JWT integration (future)
- Delegation/impersonation (future)
- Capability marketplace (future marketplace feature)

## Approval Criteria

- [ ] Capability structure defined with resource patterns
- [ ] Time window restrictions implemented
- [ ] Capability matching logic defined
- [ ] Backward compatibility with RFC-0903
- [ ] Capabilities hash for deterministic verification
