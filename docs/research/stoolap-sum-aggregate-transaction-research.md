# Research: stoolap SUM Aggregate Inside Transactions

## Executive Summary

Investigating why `SUM` aggregate function returns "Function not found: SUM" when executed inside a stoolap MVCC transaction, while the same query succeeds outside transaction context. Root cause identified: the expression compiler (`compiler.rs`) only checks the scalar function registry (`get_scalar`) when resolving function calls, never checking the aggregate function registry (`get_aggregate`). The transaction context changes the query execution path, bypassing the aggregation pushdown optimization that directly calls `table.sum_column(idx)`.

## Problem Statement

In quota-router's `record_spend_ledger`, when calculating remaining budget via `SELECT SUM(cost_amount) FROM spend_ledger WHERE key_id = $1`, stoolap (CipherOcto fork) returns:

```
Compile error: Function not found: SUM
```

This occurs **only inside an MVCC transaction**. The same query outside transaction context succeeds because it routes through the aggregation pushdown path which calls `table.sum_column(idx)` directly (bypassing expression compilation).

## Research Scope

### Included
- stoolap expression compiler function resolution path
- Aggregate vs scalar function registry separation
- MVCC transaction query execution path differences
- Aggregation pushdown optimization mechanism

### Excluded
- Fix implementation (deferred to stoolap core)
- Performance benchmarking of workarounds

## Findings

### 1. Function Registry Architecture

stoolap separates scalar and aggregate functions into two distinct registries:

**`registry.rs`** — `get_aggregate(name)` and `get_scalar(name)` are separate methods:

```rust
pub fn get_aggregate(&self, name: &str) -> Option<Arc<dyn AggregateFunction>> {
    self.aggregates.get(name).cloned()
}

pub fn get_scalar(&self, name: &str) -> Option<Arc<dyn ScalarFunction>> {
    self.scalars.get(name).cloned()
}
```

Both are properly registered at initialization:
- `registry.register_aggregate::<SumFunction>()`
- `registry.register_scalar::<SomeScalar>()`

### 2. SUM Implementation (`aggregate/sum.rs`)

The `SumFunction` is fully implemented:
- Handles `Integer`, `BigInt`, `Decimal`, `NonDetFloat` states
- Returns `Value::Integer` for integer sums, `Value::Float` for non-integer results
- Properly registered via macro: `registry.register_aggregate::<SumFunction>()`

### 3. Root Cause: Compiler Only Checks Scalars

**`compiler.rs:1576`** — The expression compiler resolves function calls like `SUM(col)` by checking ONLY the scalar registry:

```rust
// Get function from registry
if let Some(scalar_func) = self.ctx.functions.get_scalar(&func_name) {
    // ... compile scalar function
} else {
    // Check if it's an aggregate being referenced post-aggregation
    // This would be handled via LoadAggregateResult in a real implementation
    Err(CompileError::FunctionNotFound(func_name.to_string()))
}
```

**Key observation:** There is NO fallback to `get_aggregate()`. When `get_scalar("SUM")` returns `None` (because SUM is an aggregate, not a scalar), the compiler immediately returns `FunctionNotFound`.

### 4. Why Outside-Transaction Works: Aggregation Pushdown

**`aggregation.rs:5011`** — `try_aggregation_pushdown()`

When queries execute **without** a transaction, stoolap can use aggregation pushdown:

```rust
"SUM" => {
    let col_idx = col_index_map.get(&agg.column_lower).copied();
    if let Some(idx) = col_idx {
        if let Some((sum, count)) = table.sum_column(idx) {
            // Directly computes sum from table storage
            result_values.push(Value::Integer(sum as i64));
        }
    }
}
```

This path **bypasses expression compilation entirely** — it calls `table.sum_column(idx)` directly on the table storage engine.

### 5. Why Inside-Transaction Fails

**Transaction execution path** differs:

1. Inside MVCC transaction → `Executor::execute_with_transaction()`
2. Transaction context changes query classification
3. Aggregation pushdown eligibility check may fail (e.g., `classification.has_where = true` due to `FOR UPDATE` lock)
4. Falls through to normal compilation path → `compile_function_call()` → `get_scalar("SUM")` → `None` → `FunctionNotFound`

The `FOR UPDATE` clause in `record_spend_ledger`'s budget check query likely triggers `has_where = true`, disqualifying the query from pushdown.

### 6. Query Execution Path Difference

| Context | Path | Result |
|---------|------|--------|
| Non-transaction | `query` → `try_aggregation_pushdown` → `table.sum_column()` | Success |
| MVCC transaction + pushdown eligible | `query` → `try_aggregation_pushdown` → `table.sum_column()` | Success |
| MVCC transaction + pushdown ineligible | `query` → `begin_transaction` → compile → `compile_function_call` → `get_scalar("SUM")` → `None` | **FunctionNotFound** |

The `FOR UPDATE` clause (for row locking) likely disqualifies pushdown, forcing the compilation path which doesn't check aggregates.

## Recommendations

### For stoolap Core (Root Fix)

The compiler should check aggregates when scalar lookup fails:

```rust
// In compiler.rs compile_function_call()
if let Some(scalar_func) = self.ctx.functions.get_scalar(&func_name) {
    // ... existing scalar handling
} else if let Some(agg_func) = self.ctx.functions.get_aggregate(&func_name) {
    // TODO: Handle aggregate function compilation
    Err(CompileError::FunctionNotFound(func_name.to_string()))
} else {
    Err(CompileError::FunctionNotFound(func_name.to_string()))
}
```

Note: Full fix requires aggregate compilation support (not just scalar dispatch like `Op::CallScalar`).

### Workaround for quota-router

**Option A (Current):** Validate via middleware path (`test_record_spend`) which exercises `process_response` → `record_spend_ledger` outside the problematic transaction context.

**Option B:** Avoid SUM inside transactions by computing budget client-side:
- Instead of `SELECT SUM(...) WHERE key_id = $1 FOR UPDATE`, use separate key lookup + separate spend lookup
- Adds one additional query but avoids transaction aggregate issue

**Option C:** Document as known stoolap limitation; track fix in stoolap issue tracker.

## Next Steps

- [ ] File stoolap issue: "Expression compiler doesn't check aggregate registry"
- [ ] Evaluate Option B (workaround) viability for production
- [ ] Monitor stoolap fork for aggregate compilation support

## Related Files

| File | Relevance |
|------|-----------|
| `stoolap/src/executor/expression/compiler.rs:1576` | Root cause — scalar-only lookup |
| `stoolap/src/functions/registry.rs` | Aggregate/scalar registry separation |
| `stoolap/src/functions/aggregate/sum.rs` | SUM implementation (complete) |
| `stoolap/src/executor/aggregation.rs:5011` | Pushdown path that bypasses compilation |
| `quota-router-core/src/storage.rs` | `record_spend_ledger` triggering the issue |