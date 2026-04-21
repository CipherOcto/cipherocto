# RFC-0204 (Storage): Expression Compiler Aggregate Function Resolution

## Status

Draft

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

Fix the stoolap expression compiler to check the aggregate function registry when resolving function calls, enabling `SUM`, `COUNT`, `AVG`, `MIN`, `MAX` and other aggregate functions to work correctly inside MVCC transactions. Currently, `compile_function_call()` in `compiler.rs` only checks `get_scalar()` — aggregate functions are never found, causing "Function not found" errors for valid SQL aggregates executed in transaction context.

## Dependencies

**Requires:**

- RFC-0200: Production Vector-SQL Storage Engine (MVCC transactions)
- RFC-0202: Stoolap BIGINT/DECIMAL Core Types (type system baseline)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Aggregate functions work inside transactions | `SELECT SUM(col) FROM t WHERE key = $1 FOR UPDATE` succeeds |
| G2 | Zero regression for non-aggregate functions | Existing scalar functions unchanged |
| G3 | Preserve aggregation pushdown optimization | Pushdown path still used when eligible |
| G4 | Minimal compiler complexity | Single lookup additional branch |

## Motivation

When executing `SELECT SUM(cost_amount) FROM spend_ledger WHERE key_id = $1 FOR UPDATE` inside an MVCC transaction in quota-router's `record_spend_ledger`, stoolap returns:

```
Compile error: Unsupported expression: aggregate function 'SUM' is not supported in this context (use SQL aggregation path)
```

The same query outside transaction context succeeds because it routes through the aggregation pushdown optimization (`try_aggregation_pushdown()`) which calls `table.sum_column(idx)` directly, bypassing expression compilation entirely.

### Root Cause

In `src/executor/expression/compiler.rs:1576` (BEFORE fix):

```rust
// Get function from registry
if let Some(scalar_func) = self.ctx.functions.get_scalar(&func_name) {
    // ... scalar function handling
} else {
    // Check if it's an aggregate being referenced post-aggregation
    Err(CompileError::FunctionNotFound(func_name.to_string()))
}
```

**Problem:** There was no fallback to `get_aggregate()`. When `get_scalar("SUM")` returns `None`, the compiler immediately returned `FunctionNotFound` without checking if the function existed as an aggregate.

### After Fix

The compiler now checks aggregates when scalar lookup fails, providing a clearer error:

```rust
} else if self.ctx.functions.get_aggregate(&func_name).is_some() {
    Err(CompileError::UnsupportedExpression(format!(
        "aggregate function '{}' is not supported in this context (use SQL aggregation path)",
        func_name
    )))
} else {
    Err(CompileError::FunctionNotFound(func_name.to_string()))
}
```

**Key insight:** This is NOT a bug in the compiler per se — aggregate functions are simply not supported in scalar expression compilation context. The transaction SELECT path routes aggregate queries to scalar expression compilation, which is the wrong path. The real fix requires routing aggregate queries to `execute_select_with_aggregation()` instead.

### Why Outside-Transaction Works

`try_aggregation_pushdown()` in `aggregation.rs:5011` handles simple aggregation queries by:
1. Checking eligibility (no WHERE, no GROUP BY, no HAVING, no window functions)
2. For eligible queries, computing aggregates directly via `table.sum_column(idx)`
3. **Bypassing expression compilation entirely**

### Why Inside-Transaction Fails

MVCC transaction execution uses a different path:
1. `Transaction::execute()` → parses SQL → executes via `project_columns()` → `compile_expression()`
2. The `compile_expression()` path uses `CompileContext::with_global_registry()` which only compiles scalar expressions
3. Aggregate functions require the full `execute_select_with_aggregation()` path, not scalar expression compilation
4. The query fails at compilation stage with `UnsupportedExpression` (after fix) or `FunctionNotFound` (before fix)

**Note:** The `FOR UPDATE` clause mentioned in earlier analysis is not the primary cause — even simple aggregate queries without `FOR UPDATE` fail because the transaction SELECT path routes ALL queries (not just those with WHERE) through scalar expression compilation.

## Specification

### Solution 1: Better Aggregate Detection (APPLIED)

The expression compiler now checks the aggregate registry when scalar lookup fails, providing a clearer error message indicating the function IS recognized but not supported in scalar context:

```rust
// In compiler.rs compile_function_call() (APPLIED)
} else if self.ctx.functions.get_aggregate(&func_name).is_some() {
    Err(CompileError::UnsupportedExpression(format!(
        "aggregate function '{}' is not supported in this context (use SQL aggregation path)",
        func_name
    )))
} else {
    Err(CompileError::FunctionNotFound(func_name.to_string()))
}
```

**Limitation:** This is a diagnostic improvement — aggregate functions are still not supported in scalar expression compilation. The transaction SELECT path still routes them incorrectly.

### Solution 2: Enhance Aggregation Pushdown for Transactions

Extend `try_aggregation_pushdown()` to handle queries with `FOR UPDATE` by:
1. Computing aggregate before acquiring row locks
2. Validating within transaction using the pre-computed value

**Complexity:** High — requires restructuring transaction execution order.

### Solution 3: Workaround in quota-router

Avoid SUM inside transactions by computing budget client-side:
- Use separate key lookup + separate spend lookup instead of `SELECT SUM(...) WHERE key_id = $1 FOR UPDATE`
- Adds one additional query but avoids the compiler bug

**Status:** Currently in use as a documented limitation (Mission 0909-i).

## Implementation Phases

### Phase 1: Better Aggregate Detection (COMPLETED)

- [x] Modify `compiler.rs` to check aggregate registry when scalar lookup fails (APPLIED)
- [x] Add regression test: `tests/aggregate_in_transaction_test.rs` (6 tests, all fail as expected)
- [ ] Verify aggregation pushdown still works for eligible queries

### Phase 2: Full Aggregate Compilation Support (IN PROGRESS)

**Investigation Findings (2026-04-21):**

The original plan was to route transaction SELECT with aggregates to `execute_select_with_aggregation()`, which is `pub(crate)` on `Executor`. Investigation revealed a structural issue:

1. **`execute_select_with_aggregation()` is an Executor method:**
   - It's `impl Executor { pub(crate) fn execute_select_with_aggregation(...) }`
   - Uses `self.function_registry` internally for `get_aggregate()`
   - Requires an Executor instance to call

2. **Transaction path has no Executor access:**
   - `Transaction` wraps a `Box<dyn StorageTransaction>` directly
   - No `Executor` instance is available in transaction context
   - The storage transaction is accessed via `tx.get_table()`, not through Executor

3. **Failed approach: Making `execute_select_with_aggregation` standalone**
   ```rust
   // Tried: pub fn execute_select_with_aggregation(...) // instead of pub(crate)
   // Error: uses self.function_registry.get_aggregate() which requires Executor
   ```

4. **Possible paths forward:**
   - **Path A**: Extract standalone aggregation functions that take `&FunctionRegistry` directly
     - Extract `parse_aggregations()`, `execute_global_aggregation()`, etc. as free functions
     - Transaction code creates a temporary `FunctionRegistry` or uses global default
   - **Path B**: Give Transaction access to an Executor (even a minimal one)
     - Transaction could hold an `Arc<FunctionRegistry>` directly
     - Create lightweight aggregator that uses the registry
   - **Path C**: Restructure transaction SELECT to route through Executor
     - Have `Transaction::execute()` call `Executor::execute_transaction()` for SELECT
     - Executor already has all the machinery for aggregation

**Current recommendation:** Path C is cleanest — restructuring transaction SELECT to route through Executor would align the transaction path with the non-transaction path, which already works correctly.

- [ ] Extract standalone aggregation functions (Path A) OR restructure transaction to use Executor (Path C)
- [ ] Add aggregate state machine if implementing Path A
- [ ] Integration test: COUNT/SUM/AVG/MIN/MAX in various contexts

### Phase 3: Performance Validation (PENDING)

- [ ] Benchmark pushdown vs compilation for aggregate queries
- [ ] Ensure no regression in non-transaction path

## Key Files to Modify

| File | Change |
|------|--------|
| `src/executor/expression/compiler.rs` | Add `get_aggregate()` fallback in `compile_function_call()` |
| `src/executor/expression/vm.rs` | Add `Op::CallAggregate` and aggregate execution logic |
| `tests/aggregate_in_transaction_test.rs` | New regression test |

## Test Vectors (RFC-0204)

These test cases are implemented in `tests/aggregate_in_transaction_test.rs`. They currently **fail as expected** — the error message is now more informative: `Unsupported expression: aggregate function 'X' is not supported in this context (use SQL aggregation path)`.

```sql
-- T1: SUM inside transaction (fails with better error after fix)
BEGIN;
SELECT SUM(amount) FROM accounts WHERE user_id = 1 FOR UPDATE;
COMMIT;

-- T2: COUNT inside transaction (fails with better error after fix)
BEGIN;
SELECT COUNT(*) FROM accounts WHERE user_id = 1 FOR UPDATE;
COMMIT;

-- T3: AVG inside transaction (fails with better error after fix)
BEGIN;
SELECT AVG(amount) FROM accounts WHERE user_id = 1 FOR UPDATE;
COMMIT;

-- T4: MIN/MAX inside transaction (fails with better error after fix)
BEGIN;
SELECT MIN(amount), MAX(amount) FROM accounts WHERE user_id = 1 FOR UPDATE;
COMMIT;

-- T5: Aggregates WITHOUT FOR UPDATE (also fails — issue is transaction path, not FOR UPDATE)
BEGIN;
SELECT SUM(amount) FROM accounts WHERE user_id = 1;
COMMIT;

-- T6: GROUP BY with aggregate inside transaction (fails with better error after fix)
BEGIN;
SELECT user_id, SUM(amount) FROM accounts GROUP BY user_id FOR UPDATE;
COMMIT;
```

**Current behavior:** All 6 tests fail with `UnsupportedExpression` (after fix) indicating aggregates are not supported in scalar expression context.

**Expected after Phase 2:** All 6 tests pass.
```

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| Solution 1 (Partial) | Quick fix, minimal code | Aggregate still falls through to error |
| Solution 2 (Pushdown) | Preserves performance | Complex transaction restructuring |
| Solution 3 (Workaround) | No stoolap changes needed | Additional query overhead |
| Solution 1 Full | Complete fix | Requires VM changes + aggregate state machine |

## Future Work

- F1: Implement `Op::CallAggregate` for full aggregate compilation
- F2: Add aggregate window function support
- F3: Optimize aggregate pushdown for complex queries in transactions

## Rationale

The expression compiler was written before aggregate function support was complete. The scalar-only lookup was an oversight — the aggregate registry exists and is properly populated (via `registry.register_aggregate::<SumFunction>()`), but the compiler never checks it. Adding the fallback is the minimal fix to enable the compiler to at least recognize aggregate function names, even if full execution isn't yet implemented.