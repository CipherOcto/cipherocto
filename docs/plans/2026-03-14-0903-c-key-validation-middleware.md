# Mission 0903-c: Key Validation Middleware

> **For Claude:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** HTTP middleware to validate API keys from requests, extract key info, and reject unauthorized requests.

**Architecture:** Add key validation middleware to the quota-router-cli HTTP server that extracts the API key from headers/params, looks it up in storage, validates expiry/revoked status, and attaches key context to the request.

---

## Task 1: Add key validation middleware

**Files:**
- Create: `crates/quota-router-core/src/middleware.rs`
- Modify: `crates/quota-router-core/src/lib.rs`

**Step 1: Create middleware module**

```rust
// crates/quota-router-core/src/middleware.rs
use crate::keys::{ApiKey, KeyStorage, validate_key};
use crate::keys::KeyError;
use std::sync::Arc;

/// Middleware state containing key storage
pub struct KeyMiddleware<S: KeyStorage> {
    storage: Arc<S>,
}

impl<S: KeyStorage> KeyMiddleware<S> {
    pub fn new(storage: Arc<S>) -> Self {
        Self { storage }
    }

    /// Extract API key from request
    /// Supports: Authorization header (Bearer token), X-API-Key header, api_key query param
    pub fn extract_key_from_request(&self, request: &http::Request<()>) -> Result<Option<String>, KeyError> {
        // Check Authorization header
        if let Some(auth) = request.headers().get("authorization") {
            if let Ok(auth_str) = auth.to_str() {
                if auth_str.starts_with("Bearer ") {
                    return Ok(Some(auth_str[7..].to_string()));
                }
            }
        }

        // Check X-API-Key header
        if let Some(api_key) = request.headers().get("x-api-key") {
            return Ok(Some(api_key.to_str().unwrap_or("").to_string()));
        }

        // Check api_key query param (for compatibility)
        // Note: This requires parsing query params - simplified for now

        Ok(None)
    }

    /// Validate key and return ApiKey if valid
    pub fn validate_request_key(&self, key_string: &str) -> Result<ApiKey, KeyError> {
        use crate::keys::compute_key_hash;

        let key_hash = compute_key_hash(key_string);
        let key_prefix = key_string.chars().take(7).collect::<String>();

        let mut key = self.storage.lookup_by_hash(&key_hash)?
            .ok_or(KeyError::NotFound)?;

        // Set the key_prefix from the request
        key.key_prefix = key_prefix;

        // Validate expiry and revoked status
        validate_key(&key)?;

        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Add tests
}
```

**Step 2: Export from lib.rs**

Add to `crates/quota-router-core/src/lib.rs`:
```rust
pub mod middleware;
pub use middleware::KeyMiddleware;
```

**Step 3: Commit**

---

## Task 2: Integrate middleware with HTTP server

**Files:**
- Modify: `crates/quota-router-cli/src/main.rs` or relevant server module

**Step 1: Add middleware to server**

```rust
use quota_router_core::middleware::KeyMiddleware;

// Initialize middleware with storage
let key_middleware = KeyMiddleware::new(storage.clone());

// Add to request handling - pseudo-code
async fn handle_request(req, key_middleware) {
    // Extract and validate key
    let key_string = key_middleware.extract_key_from_request(&req)?
        .ok_or(KeyError::MissingKey)?;

    let api_key = key_middleware.validate_request_key(&key_string)?;

    // Attach key to request context
    // Continue to actual handler
}
```

**Step 2: Add tests**

**Step 3: Commit**
