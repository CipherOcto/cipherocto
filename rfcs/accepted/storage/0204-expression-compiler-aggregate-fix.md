# RFC-0204 (Storage): Expression Compiler Aggregate Function Resolution

## Status

Final (Phase 1, 2, and 4 Complete)

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
- [x] Add regression test: `tests/aggregate_in_transaction_test.rs` (6 tests)
- [x] Verify aggregation pushdown still works for eligible queries
- Status: Committed as `35cafe9`

### Phase 2: Route Aggregate Queries Through Executor (COMPLETED)

**Approach:** Path B - Give Transaction access to Executor for routing aggregate queries.

**Changes implemented:**

1. **`src/api/database.rs`:**
   - Changed `DatabaseInner::executor` from `Mutex<Executor>` to `Arc<Mutex<Executor>>`
   - Modified `begin_with_isolation()` to create new `Executor` for each transaction

2. **`src/api/transaction.rs`:**
   - Added `executor: Arc<Executor>` field to `Transaction` struct
   - Added `QueryClassification` detection for aggregate queries
   - Route aggregate queries to `Executor::execute_in_transaction()`

3. **`src/executor/mod.rs`:**
   - Added `pub(crate) fn execute_in_transaction()` method
   - Added WHERE clause to storage expression conversion
   - Added in-memory filtering fallback for complex WHERE
   - Made `aggregation` and `query_classification` modules public

**Test results:** All 6 regression tests pass after Phase 2.

Status: Committed as `dcb4f1c`

### Phase 3: Performance Validation (PENDING)

- [ ] Benchmark pushdown vs compilation for aggregate queries
- [ ] Ensure no regression in non-transaction path
- [ ] Integration test: COUNT/SUM/AVG/MIN/MAX in various contexts

**Mission created:** `missions/open/0204-a-aggregate-performance-validation.md`

### Phase 4: Parameter Resolution in WHERE Clause (COMPLETED)

**Bug:** When `execute_in_transaction` routes aggregate queries through `convert_where_to_storage_expr`, the WHERE clause's query parameters (e.g., `$1`) were not being resolved because `convert_where_to_storage_expr` created an empty `ExecutionContext` instead of using the one with actual parameters.

**Root cause:** At `src/executor/mod.rs:912-914`:
```rust
let value =
    crate::executor::expression::ExpressionEval::compile(&infix.right, &[])?  // empty columns
        .with_context(&crate::executor::context::ExecutionContext::new())  // empty ctx!
        .eval_slice(&crate::core::Row::new())?;
```

**Fix:** Pass `columns` and `ctx` through to `convert_where_to_storage_expr` so parameters resolve correctly:
```rust
let value =
    crate::executor::expression::ExpressionEval::compile(&infix.right, columns)?
        .with_context(ctx)
        .eval_slice(&crate::core::Row::new())?;
```

**Changes:**
- Added `columns: &[String]` and `ctx: &ExecutionContext` parameters to `convert_where_to_storage_expr`
- Updated recursive calls to pass these through
- Updated call site in `execute_in_transaction`

**Test results:** All 6 stoolap aggregate tests pass; quota-router-core tests `test_record_spend_ledger_populates_tokenizers` and `test_record_spend_ledger_provider_usage` now pass with parameterized queries.

Status: Committed as `1ca5d1a`

## Key Files to Modify

| File | Change |
|------|--------|
| `src/executor/expression/compiler.rs` | Add `get_aggregate()` fallback in `compile_function_call()` |
| `src/executor/expression/vm.rs` | Add `Op::CallAggregate` and aggregate execution logic |
| `tests/aggregate_in_transaction_test.rs` | New regression test |

## Test Vectors (RFC-0204)

These test cases are implemented in `tests/aggregate_in_transaction_test.rs`. After Phase 2, **all 6 tests pass**:

```sql
-- T1: SUM inside transaction (PASS)
BEGIN;
SELECT SUM(amount) FROM accounts WHERE user_id = 1 FOR UPDATE;
COMMIT;

-- T2: COUNT inside transaction (PASS)
BEGIN;
SELECT COUNT(*) FROM accounts WHERE user_id = 1 FOR UPDATE;
COMMIT;

-- T3: AVG inside transaction (PASS)
BEGIN;
SELECT AVG(amount) FROM accounts WHERE user_id = 1 FOR UPDATE;
COMMIT;

-- T4: MIN/MAX inside transaction (PASS)
BEGIN;
SELECT MIN(amount), MAX(amount) FROM accounts WHERE user_id = 1 FOR UPDATE;
COMMIT;

-- T5: Aggregates WITHOUT FOR UPDATE (PASS - uses pushdown path)
BEGIN;
SELECT SUM(amount) FROM accounts WHERE user_id = 1;
COMMIT;

-- T6: GROUP BY with aggregate inside transaction (PASS)
BEGIN;
SELECT user_id, SUM(amount) FROM accounts GROUP BY user_id FOR UPDATE;
COMMIT;
```

**Current behavior:** All 6 tests pass, routing through `Executor::execute_in_transaction()`.

**Test framework:** `tests/aggregate_in_transaction_test.rs` - 235 lines, 6 tests

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| Solution 1 (Partial) | Quick fix, minimal code | Aggregate still falls through to error |
| Solution 2 (Pushdown) | Preserves performance | Complex transaction restructuring |
| Solution 3 (Workaround) | No stoolap changes needed | Additional query overhead |
| Solution 1 Full | Complete fix | Requires VM changes + aggregate state machine |

## Future Work

- F1: Window function support for full SQL compliance
- F2: Multi-table aggregate JOIN support (requires JOIN executor in transactions)
- F3: Parallel aggregation for large datasets
- F4: Distributed transaction aggregate support
- F5: HAVING clause optimization (currently filtered in-memory, could be pushed to storage)

## Rationale

The expression compiler was written before aggregate function support was complete. The scalar-only lookup was an oversight — the aggregate registry exists and is properly populated (via `registry.register_aggregate::<SumFunction>()`), but the compiler never checks it. Adding the fallback is the minimal fix to enable the compiler to at least recognize aggregate function names, even if full execution isn't yet implemented.