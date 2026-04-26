# Mission: RFC-0904/0910 Cost Integration — Delegated compute_cost

## Status

Open

## RFC

RFC-0904 v1.29 (Draft): Real-Time Cost Tracking

**Note:** RFC-0904 is Draft, not Accepted. Mission created for planning and tracking purposes.

## Dependencies

- Mission: RFC-0910 Full Implementation (must complete first — `compute_cost` delegates to RFC-0910)

## Summary

Update RFC-0904 code to match v1.29 spec: `compute_cost` should delegate to RFC-0910's canonical implementation, and OCTO-W balance functions should align with the RFC spec.

## Acceptance Criteria

- [x] `CostError::Overflow` → `BudgetError::CostOverflow` conversion implemented (RFC doc, v1.29)
- [x] OCTO-W `deduct_octo_w` returns `Result<u64, StorageError>` (remaining balance) (RFC doc, v1.29)
- [ ] `compute_cost()` delegates to RFC-0910 canonical implementation (not own `saturating_mul/div`) — requires RFC-0910 implementation
- [ ] `octo_w_balances` table schema defined in storage — requires RFC-0910 implementation
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes with zero warnings
- [ ] `cargo test --lib` passes

## Implementation Notes

**File:** `crates/quota-router-core/src/keys/mod.rs`, `crates/quota-router-core/src/balance.rs`

**compute_cost delegation:**
```rust
pub fn compute_cost(...) -> Result<u64, BudgetError> {
    rfc0910::compute_cost(pricing, input_tokens, output_tokens)
        .map_err(|e| match e { CostError::Overflow { .. } => BudgetError::CostOverflow })
}
```

**OCTO-W balance DDL:**
```sql
CREATE TABLE octo_w_balances (
    key_id BLOB(16) PRIMARY KEY REFERENCES api_keys(key_id) ON DELETE CASCADE,
    balance INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);
```
