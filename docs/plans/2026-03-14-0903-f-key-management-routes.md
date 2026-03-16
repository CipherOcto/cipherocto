# Mission 0903-f: Key Management Routes

> **For Claude:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add key management HTTP API routes to quota-router-cli.

**Background:**
- Proxy server exists in quota-router-core/src/proxy.rs
- Key storage already implemented in 0903-a/b
- Middleware for validation in 0903-c

---

## Task 1: Add key management routes to CLI

**Files:**
- Modify: `crates/quota-router-core/src/proxy.rs`

**Step 1: Add key management routes**

Add routes for key CRUD operations to the proxy server.

```rust
// Add to proxy.rs - key management endpoints
async fn handle_key_management(req: Request<()>, key_storage: &StoolapKeyStorage) -> Result<Response<()>, Infallible> {
    match (req.method(), req.uri().path()) {
        // POST /api/keys - create key
        (&Method::POST, "/api/keys") => {
            // Parse request body, create key
        }
        // GET /api/keys - list keys
        (&Method::GET, "/api/keys") => {
            // List all keys or filter by team
        }
        // PUT /api/keys/:id - update key
        (&Method::PUT, path) if path.starts_with("/api/keys/") => {
            // Update key
        }
        // POST /api/keys/:id/revoke - revoke key
        (&Method::POST, path) if path.contains("/revoke") => {
            // Revoke key
        }
        // DELETE /api/keys/:id - delete key
        (&Method::DELETE, path) if path.starts_with("/api/keys/") => {
            // Delete key
        }
        _ => NOT_FOUND
    }
}
```

**Step 2: Test**

**Step 3: Commit**

---

## Task 2: Integration tests

**Files:**
- Add tests for key management routes

**Step 1: Add integration tests**

**Step 2: Commit**
