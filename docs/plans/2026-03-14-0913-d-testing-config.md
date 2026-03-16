# Mission 0913-d: Testing & Configuration

> **For Claude:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add integration tests and configuration options for WAL pub/sub.

---

## Task 1: Add Configuration

**Files:**
- Modify: `crates/quota-router-core/src/config.rs`

**Step 1: Add WAL pub/sub config**

```rust
/// WAL Pub/Sub configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalPubSubConfig {
    /// Enable WAL pub/sub (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Polling interval in milliseconds (default: 50)
    #[serde(default = "default_wal_poll_interval")]
    pub poll_interval_ms: u64,
    /// WAL path for shared storage (optional)
    pub wal_path: Option<String>,
}
```

**Step 2: Test**

**Step 3: Commit**

---

## Task 2: Integration Tests

**Files:**
- Add tests to existing test modules

**Step 1: Add multi-process cache invalidation test**

**Step 2: Test idempotency via event_id**

**Step 3: Commit**
