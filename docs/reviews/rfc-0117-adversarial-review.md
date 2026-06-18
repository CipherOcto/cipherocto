# Adversarial Review: RFC-0117 — Deterministic Execution Context (DETERMINISTIC VIEW)

**Reviewer:** Code Review Agent
**Date:** 2026-04-02
**Document:** `rfcs/draft/numeric/0117-deterministic-execution-context.md`
**Version Reviewed:** 1.0 (Draft)
**Cross-referenced Against:** RFC-0104 v1.17, RFC-0105 v2.14, RFC-0124 v1.7

---

## Summary

| Severity  | Count  |
| --------- | ------ |
| CRITICAL  | 1      |
| HIGH      | 10     |
| MEDIUM    | 8      |
| LOW       | 4      |
| **Total** | **23** |

---

## What Was Done Well

- **Type enforcement tables** (Allowed Types, CAST Rules, Implicit Promotion) are internally consistent — no contradictions found.
- **Planner flow** is clearly laid out with explicit error conditions.
- **Error taxonomy** covers main error cases with template messages for consistent diagnostics.
- **View composition rules** properly handle deterministic-to-deterministic, deterministic-to-regular, and JOIN scenarios.
- **Defense-in-depth** at the VM level is well-motivated and explained clearly.
- **Cross-RFC references** correctly identify dependencies on RFC-0104, RFC-0105, RFC-0124.

---

## CRITICAL Findings

### C1. Literal `1.5` CAST Requirement Contradicts Literal Parsing Rules

**Location:** §Implicit Promotion in Deterministic Context — Rationale paragraph

The Rationale text states:

> `dfp_col * 2` is allowed; `dfp_col * 1.5` requires `CAST(1.5 AS DFP)`

This directly contradicts the Literal Parsing table (§Literal Parsing in Deterministic Context), which shows that `3.14` (a decimal literal) is parsed as **DFP** in deterministic context. If `0.1` and `3.14` are parsed as DFP, then `1.5` is also parsed as DFP and `dfp_col * 1.5` should work without CAST.

The text appears to be a copy from RFC-0104's rationale (which makes sense for non-deterministic contexts where literals are FLOAT), but RFC-0117's own literal parsing rules make it incorrect.

**Fix:** Replace the rationale sentence with:

> `dfp_col * 2` is allowed; `dfp_col * 1.5` is also allowed (decimal literal parsed as DFP in deterministic context).

---

## HIGH Findings

### H1. No Specification for Subqueries

**Location:** §Planner Integration

The Planner Flow does not mention subqueries. A deterministic view might contain:

```sql
CREATE DETERMINISTIC VIEW v_sub AS
SELECT price * (SELECT rate FROM config WHERE id = 1) AS adjusted
FROM trades;
```

The subquery could reference FLOAT columns. No rule defines whether type enforcement extends recursively into subqueries.

**Fix:** Add to Planner Flow Step 2: "Recursively apply deterministic type enforcement to all subqueries. All columns referenced in subqueries must comply with deterministic type rules."

### H2. No Specification for CTEs (WITH clauses)

**Location:** §Specification (missing)

CTEs are common in SQL views:

```sql
CREATE DETERMINISTIC VIEW v_cte AS
WITH base AS (SELECT price, qty FROM trades WHERE price > 0)
SELECT price * qty AS total FROM base;
```

The RFC does not specify whether CTEs within a DETERMINISTIC VIEW are supported, nor whether the `deterministic_mode` flag propagates into CTE definitions.

**Fix:** Add section: "CTEs within DETERMINISTIC VIEWs are supported. The `deterministic_mode` flag propagates into all CTE definitions. All columns referenced in CTEs must comply with deterministic type rules."

### H3. No Specification for UNION / UNION ALL

**Location:** §Specification (missing)

```sql
CREATE DETERMINISTIC VIEW v_union AS
SELECT dfp_price FROM trades
UNION ALL
SELECT float_price FROM trades;
```

Type enforcement across UNION arms is undefined.

**Fix:** Add rule: "UNION/UNION ALL is permitted. Each arm is independently subject to deterministic type enforcement. All columns in each arm must comply with deterministic type rules."

### H4. No Specification for Aggregates (GROUP BY, SUM, AVG, etc.)

**Location:** §Specification (missing)

```sql
CREATE DETERMINISTIC VIEW v_agg AS
SELECT SUM(price) AS total, AVG(quantity) AS avg_qty
FROM trades
GROUP BY category;
```

Aggregates on deterministic types produce deterministic results, but the spec doesn't say so. Additionally, `AVG` involves division, which in DQA uses `dqa_div` with RoundHalfEven — this must be explicitly noted.

**Fix:** Add section: "Aggregate functions (SUM, AVG, MIN, MAX, COUNT) are permitted on deterministic types. Aggregates on DQA columns use DQA arithmetic (RFC-0105). Aggregates on DFP columns are lowered to DQA via RFC-0124 before aggregation. Non-deterministic aggregates (e.g., APPROX_COUNT_DISTINCT) are forbidden."

### H5. No Specification for CASE/WHEN

**Location:** §Specification (missing)

```sql
CREATE DETERMINISTIC VIEW v_case AS
SELECT CASE WHEN price > 100 THEN price * 1.1 ELSE price END
FROM trades;
```

CASE/WHEN is a common SQL construct. The spec does not address whether all branches must produce deterministic-compatible types.

**Fix:** Add rule: "CASE/WHEN is permitted. All WHEN branches and the ELSE branch must produce values of a deterministic-compatible type. If any branch produces a non-deterministic type, ERROR 41001 is raised."

### H6. No Specification for ORDER BY / LIMIT in View Definition

**Location:** §Specification (missing)

```sql
CREATE DETERMINISTIC VIEW v_sorted AS
SELECT price * quantity AS total FROM trades ORDER BY total DESC LIMIT 10;
```

ORDER BY with LIMIT in a view definition raises questions about determinism: if the ordering column has ties, different nodes may return different rows. The spec does not address this.

**Fix:** Add rule: "ORDER BY in DETERMINISTIC VIEWs must include a tiebreaker column (unique key) to guarantee deterministic row ordering across nodes. If no tiebreaker is present and LIMIT/OFFSET is used, the planner should emit a warning. If no tiebreaker is present and ORDER BY + LIMIT is used without a unique key, ERROR 41006 (Non-deterministic ordering) is raised."

### H7. No Specification for Non-Deterministic Function Whitelist/Blacklist

**Location:** §Test Vectors — T20

T20 tests `RANDOM()` rejection, but the RFC does not define which functions are non-deterministic. Other candidates:

- `NOW()`, `CURRENT_TIMESTAMP` — time-dependent
- `UUID()` — random
- `ROW_NUMBER()` without ORDER BY — order-dependent
- `USER()`, `SESSION_USER()` — session-dependent

**Fix:** Add a "Non-Deterministic Function Classification" section:

- Forbidden: `RANDOM()`, `NOW()`, `CURRENT_TIMESTAMP`, `UUID()`, `USER()`, `SESSION_USER()`
- Allowed: `ABS()`, `CEIL()`, `FLOOR()`, `ROUND()`, `SUM()`, `AVG()`, `MIN()`, `MAX()`, `COUNT()`
- Conditionally allowed: `ROW_NUMBER()`, `RANK()`, `DENSE_RANK()` (require ORDER BY with unique tiebreaker)
- User-defined functions: forbidden unless explicitly marked deterministic-safe

### H8. No Specification for ALTER TABLE After DETERMINISTIC VIEW Creation

**Location:** §VM Integration — defense-in-depth note

The defense-in-depth section mentions "a view's underlying table was altered after view creation" as a scenario the VM runtime check catches. But the spec provides no mechanism for this:

```sql
CREATE TABLE t (price DFP NOT NULL);
CREATE DETERMINISTIC VIEW v AS SELECT price * 2 FROM t;
ALTER TABLE t MODIFY COLUMN price FLOAT;
-- View 'v' was validated at creation time. Does it still work?
```

**Fix:** Add schema versioning rules:

- DETERMINISTIC VIEWs record the schema version of referenced tables at creation time.
- If a referenced table's schema changes (ALTER TABLE), the view is marked invalid.
- At query time, if the view's recorded schema version does not match the current table, raise error.
- `SHOW VIEWS` should mark invalid views with `[D!]` or similar.

### H9. Implicit Promotion Table Missing BIGINT Entries

**Location:** §Implicit Promotion in Deterministic Context table

The table has generic "INT" entries but the Type Enforcement Rules say BIGINT is "promoted to DFP/DQA per type rules." BIGINT is a distinct type from INTEGER in most SQL systems and has different promotion behavior (potential overflow when converting to i64 for DQA).

**Fix:** Add explicit entries:

| Left Type | Right Type | Result Type | Behavior                                                        |
| --------- | ---------- | ----------- | --------------------------------------------------------------- |
| BIGINT    | DFP        | DFP         | BIGINT promoted via `Dfp::from_i128()` (or RFC-0136 BigInt→DFP) |
| BIGINT    | DQA        | DQA         | BIGINT promoted via RFC-0131 `bigint_to_dqa` — TRAP if overflow |
| DFP       | BIGINT     | DFP         | BIGINT promoted via `Dfp::from_i128()`                          |
| DQA       | BIGINT     | DQA         | BIGINT promoted via RFC-0131                                    |

### H10. OR REPLACE Allows Changing Deterministic Status Without Validation Note

**Location:** §DDL Statements, Test Vector T15

T15 shows replacing a regular view with a deterministic one:

```sql
CREATE VIEW v15 AS SELECT float_col FROM some_table;
CREATE OR REPLACE DETERMINISTIC VIEW v15 AS SELECT int_col FROM some_table;
```

The reverse case is not specified:

```sql
CREATE DETERMINISTIC VIEW v AS SELECT dfp_col FROM t;
CREATE OR REPLACE VIEW v AS SELECT float_col FROM t;
-- This silently downgrades a deterministic view to a regular view.
-- Any downstream views referencing v may now get non-deterministic results.
```

**Fix:** Add rule: "OR REPLACE may change the deterministic status of a view. If a DETERMINISTIC VIEW is replaced with a non-deterministic view, any DETERMINISTIC VIEWs that reference it become invalid and must be re-validated."

---

## MEDIUM Findings

### M1. DESCRIBE VIEW Grammar Not Defined

**Location:** §DDL Statements — SHOW / DESCRIBE

The spec shows `DESCRIBE VIEW v_portfolio` output but provides no BNF production. The BNF section only covers `create_view_stmt`.

**Fix:** Add grammar productions:

```
describe_view_stmt ::= DESCRIBE VIEW view_name
show_views_stmt    ::= SHOW VIEWS
```

### M2. Test Vector T14 Uses Bare SELECT Without FROM

**Location:** §Test Vectors — T14

```sql
CREATE DETERMINISTIC VIEW v14 AS SELECT 1;
```

No FROM clause. The spec does not define whether `SELECT literal` without FROM is valid in deterministic views.

**Fix:** Document that `SELECT ...` without FROM is valid in Stoolap SQL (literal `1` parsed as DFP in deterministic context). Or add `FROM dual` if Stoolap requires FROM.

### M3. Missing Test: FLOAT Column Exists But Not Referenced

**Location:** §Test Vectors

T19 tests WHERE clause enforcement, but no test verifies that an unreferenced FLOAT column in the table does NOT cause an error:

```sql
CREATE TABLE t (price DFP NOT NULL, rate FLOAT);
CREATE DETERMINISTIC VIEW v AS SELECT price FROM t;
-- Should succeed: rate exists but is not referenced
```

**Fix:** Add test vector T21 for this case.

### M4. Missing Test: GROUP BY Deterministic Enforcement

**Location:** §Test Vectors

No test vector covers GROUP BY in a deterministic view.

**Fix:** Add test vector:

```sql
CREATE TABLE t (category VARCHAR, price DFP NOT NULL);
CREATE DETERMINISTIC VIEW v AS
SELECT category, SUM(price) AS total FROM t GROUP BY category;
-- Expected: Success.
```

### M5. Missing Test: View References Non-Existent Column

**Location:** §Test Vectors

No test covers the case where a column referenced by a deterministic view is dropped from the underlying table. Related to H8 (schema changes).

**Fix:** Add test vector:

```sql
CREATE TABLE t (price DFP NOT NULL);
CREATE DETERMINISTIC VIEW v AS SELECT price FROM t;
ALTER TABLE t DROP COLUMN price;
SELECT * FROM v; -- Expected: Error (view invalid, schema changed)
```

### M6. Missing Test: CTE in Deterministic View

**Location:** §Test Vectors

No test vector covers CTEs (addressed by H2).

**Fix:** Add test vector once CTE support is specified.

### M7. Missing Test: UNION in Deterministic View

**Location:** §Test Vectors

No test vector covers UNION (addressed by H3).

**Fix:** Add test vector once UNION support is specified.

### M8. No Specification for NULL Handling in Deterministic Context

**Location:** §Type Enforcement Rules

The table says NULL is "Allowed" and "Propagates through deterministic ops." But NULL introduces non-determinism in some SQL contexts:

- `NULL = NULL` → NULL (not TRUE) — well-defined
- `NULL OR TRUE` → TRUE — well-defined
- `SUM(column_with_nulls)` — NULLs excluded, deterministic
- `COUNT(*)` vs `COUNT(col)` — different NULL handling

While most NULL handling is standard SQL, it's worth noting that NULL does not affect determinism guarantees.

**Fix:** Add note: "NULL values in deterministic views follow standard SQL three-valued logic. NULL propagation is deterministic and does not affect the bit-identical guarantee. All aggregate functions exclude NULLs per SQL standard."

---

## LOW Findings

### L1. Test Vectors Not Mapped to Implementation Checklist

**Location:** §Implementation Checklist

Checklist says "All 20 test vectors passing" but T-IDs are not mapped to checklist items.

**Fix:** No spec change needed. Implementation can use T1-T20 IDs as test names.

### L2. No Compilation Time Bound

**Location:** §Gas Model

The gas model says "Type check (per expression): 0" gas. CREATE DETERMINISTIC VIEW may involve scanning large schemas. No bound on compilation time for deeply nested views.

**Fix:** Add note: "CREATE DETERMINISTIC VIEW compilation time is bounded by the expression tree size limit per Stoolap engine limits."

### L3. Mermaid Diagram Missing Caption

**Location:** §Compiler Integration — Flag Propagation

The Mermaid flowchart has no descriptive caption.

**Fix:** Add: `_Figure 1: Deterministic flag propagation from SQL to VM_`

### L4. `stl_views.created_at` Uses TIMESTAMP

**Location:** §Catalog Storage

`created_at TIMESTAMP` — TIMESTAMP may be non-deterministic across nodes (depends on node clock). For a deterministic system catalog, consider using a consensus-derived value.

**Fix:** No change needed for the RFC. Catalog timestamps are metadata, not consensus values. Note this as implementation guidance.

---

## Resolved Open Questions Assessment

The original planned RFC had 4 open questions. This review assesses their resolution:

| #   | Question         | Resolution                                          | Assessment                                        |
| --- | ---------------- | --------------------------------------------------- | ------------------------------------------------- |
| 1   | Implicit CAST    | INT→DFP allowed; DFP↔DQA requires explicit CAST     | **Resolved** (but rationale text has bug — C1)    |
| 2   | View composition | DETERMINISTIC VIEWs can reference each other        | **Resolved**                                      |
| 3   | JOIN scope       | Allowed with type enforcement on referenced columns | **Resolved**                                      |
| 4   | SHOW/DESCRIBE    | [D] marker and Type indicator                       | **Partially resolved** — grammar not defined (M1) |

---

## Priority Recommendations

### Must Fix Before Draft Acceptance

1. **C1** — Fix literal rationale contradiction
2. **H1-H5** — Add subquery, CTE, UNION, aggregate, CASE/WHEN specifications
3. **H7** — Define non-deterministic function classification
4. **H8** — Define schema change handling (ALTER TABLE after view creation)

### Should Fix Before Implementation

5. **H6** — Define ORDER BY + LIMIT determinism rules
6. **H9** — Add BIGINT promotion entries
7. **H10** — Define OR REPLACE deterministic status change rules
8. **M1** — Add DESCRIBE/SHOW grammar productions

### Nice to Have

9. **M2-M7** — Add missing test vectors (6 new tests needed)
10. **M8** — Add NULL handling note
11. **L1-L4** — Low-priority documentation fixes
