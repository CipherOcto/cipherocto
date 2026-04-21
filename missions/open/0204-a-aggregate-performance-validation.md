# Mission: RFC-0204 Phase 3 - Aggregate Performance Validation

## Status

Open

## RFC

RFC-0204 (Storage): Expression Compiler Aggregate Function Resolution

## Dependencies

- Mission: 0202-e-bigint-decimal-integration-testing (Type system baseline)

## Blockers / Dependencies

None - Phase 3 is independent of other missions

## Acceptance Criteria

- [ ] Benchmark: Pushdown vs compilation latency comparison
- [ ] Benchmark: 1000 row aggregation < 10ms
- [ ] Test: Non-transaction aggregates still use pushdown path
- [ ] Test: Complex WHERE clause with aggregate inside transaction
- [ ] Test: HAVING clause with aggregate inside transaction
- [ ] Integration test: COUNT/SUM/AVG/MIN/MAX in various transaction contexts

## Description

Validate that Phase 2 implementation of aggregate function support inside MVCC transactions maintains performance and does not regress existing functionality.

## Technical Details

### Benchmark Requirements

| Metric | Target | Measurement |
|--------|--------|-------------|
| Pushdown latency (baseline) | <1ms | 1000 rows, no aggregate |
| Compilation path latency | <10ms | 1000 rows, SUM aggregate |
| Non-transaction pushdown | Unchanged | Compare before/after |

### Test Scenarios

1. **Simple aggregate with WHERE**
   ```sql
   SELECT SUM(amount) FROM accounts WHERE user_id = 1;
   ```

2. **Aggregate without WHERE**
   ```sql
   SELECT COUNT(*) FROM accounts;
   ```

3. **Multiple aggregates**
   ```sql
   SELECT MIN(a), MAX(a), AVG(a) FROM accounts WHERE user_id = 1;
   ```

4. **GROUP BY with aggregate**
   ```sql
   SELECT user_id, SUM(amount) FROM accounts GROUP BY user_id;
   ```

5. **HAVING with aggregate**
   ```sql
   SELECT user_id, SUM(amount) FROM accounts GROUP BY user_id HAVING SUM(amount) > 100;
   ```

### Implementation Notes

1. Use `criterion` crate for benchmarking
2. Compare aggregation pushdown vs `execute_in_transaction` path
3. Ensure WHERE clause conversion handles complex expressions

## Research References

- RFC-0204: `rfcs/draft/storage/0204-expression-compiler-aggregate-fix.md`
- Use Case: `docs/use-cases/stoolap-mvcc-transaction-aggregate-support.md`
- Research: `docs/research/stoolap-sum-aggregate-transaction-research.md`

## Claimant

<!-- Add your name when claiming -->

## Pull Request

<!-- PR number when submitted -->

---

**Mission Type:** Validation
**Priority:** Medium
**Phase:** RFC-0204 Phase 3