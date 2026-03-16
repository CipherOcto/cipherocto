# Mission 0903-d: Budget Enforcement

> **For Claude:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Track spend per key and enforce budget limits - block requests when budget is exceeded.

**Architecture:** Add spend tracking to key storage, check budget before processing requests, track cumulative spend with time windows (daily/weekly/monthly).

---

## Task 1: Add spend tracking to storage

**Files:**
- Modify: `crates/quota-router-core/src/keys/models.rs`
- Modify: `crates/quota-router-core/src/storage.rs`

**Step 1: Add spend tracking struct**

In `models.rs`, add:
```rust
/// Tracks spending for a key within a time window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeySpend {
    pub key_id: String,
    pub total_spend: i64,       // in cents/millicents
    pub window_start: i64,       // timestamp when window started
    pub last_updated: i64,
}
```

**Step 2: Add spend methods to KeyStorage trait**

In `storage.rs`, add to trait:
```rust
fn record_spend(&self, key_id: &str, amount: i64) -> Result<(), KeyError>;
fn get_spend(&self, key_id: &str) -> Result<Option<KeySpend>, KeyError>;
fn reset_spend(&self, key_id: &str) -> Result<(), KeyError>;
```

**Step 3: Implement in StoolapKeyStorage**

**Step 4: Test**

**Step 5: Commit**

---

## Task 2: Add budget check middleware

**Files:**
- Modify: `crates/quota-router-core/src/middleware.rs`

**Step 1: Add budget check method**

```rust
impl<S: KeyStorage> KeyMiddleware<S> {
    /// Check if key has remaining budget
    pub fn check_budget(&self, key: &ApiKey) -> Result<(), KeyError> {
        let spend = self.storage.get_spend(&key.key_id)?;

        if let Some(s) = spend {
            let remaining = key.budget_limit - s.total_spend;
            if remaining <= 0 {
                return Err(KeyError::BudgetExceeded(key.budget_limit));
            }
        }

        Ok(())
    }
}
```

**Step 2: Test**

**Step 3: Commit**

---

## Task 3: Record spend after requests

**Files:**
- Modify: HTTP server to record spend after successful requests

**Step 1: Add spend recording**

After successful proxy/request, record the cost:
```rust
key_middleware.record_spend(&api_key.key_id, cost_cents)?;
```

**Step 2: Test**

**Step 3: Commit**
