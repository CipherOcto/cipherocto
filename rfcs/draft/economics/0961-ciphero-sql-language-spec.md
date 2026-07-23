# RFC-0961 (Economics): CIPHERO_SQL — Deterministic SQL Dialect (v2.0 Strategic Reframe)

## Status

Draft v2.0

> **Note (v2.0 reframe):** Renamed from "Consensus-Safe SQL" to **Deterministic SQL** — the determinism is the property, the consensus is an implementation detail. CIPHERO_SQL is the dialect compiled by the Deterministic SQL Engine (RFC-0960 §1) into deterministic WAL entries. The dialect is the surface; the WAL is the protocol.

> **Note (v1.0):** Companion RFC to RFC-0960 §1 (Deterministic SQL Engine; v2.0 architecture). Defines the CIPHERO_SQL language: grammar, parse-time determinism checks, runtime determinism verification, allowed/forbidden constructors, the `DETERMINISTIC` flag semantics, and the relationship to PostgreSQL `CREATE PROCEDURE LANGUAGE` syntax. Builds on `docs/research/2026-07-22-enterprise-migration-playbooks.md` §4 (PostgreSQL CREATE PROCEDURE survey).

## Version History

| Version | Date | Author | Note |
|---------|------|--------|------|
| v1.0 | 2026-07-22 | @cipherocto + @mmacedoeu | Initial draft (as "Consensus-Safe SQL"). |
| v2.0 | 2026-07-23 | @cipherocto + @mmacedoeu | **Strategic reframe (R17+).** Renamed to Deterministic SQL Dialect. New §7 **Deterministic SQL Profile** (mandatory `ORDER BY` for `LIMIT`, `COLLATE "C"`, RFC-0104 DFP, `NULLS LAST` default, inclusive `BETWEEN`, deterministic `GROUP BY` order). Reframed as a layer of the Deterministic SQL Engine (RFC-0960 §1), not a standalone language. Bumped to v2.0 to signal breaking terminology change. |

## Dependencies

### Required RFCs

| RFC | Relationship | Reason |
|-----|--------------|--------|
| RFC-0960 | Required | Defines grand-design §1 (Deterministic SQL Engine) + §10 (Execution Envelopes); references this RFC for the deterministic SQL dialect (§7 Deterministic SQL Profile) |
| RFC-0126 | Required | Canonical serialization for SQL AST + procedure bytecode |
| RFC-0104 | Required | DFP encoding for all numeric expressions |
| RFC-0102 | Required | Wallet cryptography (capability holder signs the procedure invocation) |
| RFC-0862 | Required | Sync as propagation; the WAL-block-as-transaction model assumes deterministic replay |

### Companion RFCs (Draft)

| RFC | Relationship | Reason |
|-----|--------------|--------|
| RFC-0962 | Builds on | ExecutionEnvelope object protocol; one procedure invocation is one `sql_statement` entry |
| RFC-0964 | Builds on | Constraint encoding standard (CIPHERO_SQL is a constraint consumer when `AllowIf` references it) |
| RFC-0967 | Builds on | Policy Object Graph — procedures may be gated by a `policy_id` reference |

---

## 1. Motivation

### 1.1 The enterprise migration problem

`docs/research/2026-07-22-enterprise-migration-playbooks.md` §4.2 identifies the core problem:

> Most enterprise stored procedures are pure SQL (reports, summaries, validations, refactors). These survive. Stored procedures that need `plpgsql` features (loops, time, error handling) don't survive. That's by design — consensus requires determinism.

PostgreSQL doesn't enforce `DETERMINISTIC`. If the developer mis-declares, queries may diverge across nodes. CipherOcto **MUST** enforce at parse time + runtime.

### 1.2 What CIPHERO_SQL is not

CIPHERO_SQL is **not** a smart-contract language. It is **not** Turing-complete. It is **not** a procedure body language with loops, recursion, or arbitrary control flow.

CIPHERO_SQL is a **constrained deterministic SQL dialect** compiled by the **Deterministic SQL Engine** (RFC-0960 §1) into deterministic WAL entries. It runs inside an `ExecutionEnvelope` (RFC-0962 v2.0; renamed from `ConsensusSession`) and its result is bit-identical across every node that replays the same block. The dialect is the **surface**; the WAL is the **protocol**.

**v2.0 framing:** The determinism is the **property**. The consensus is an **implementation detail**. CIPHERO_SQL is "Deterministic SQL," not "Consensus-Safe SQL." Anything that is bit-identical across replays is deterministic, regardless of whether consensus happens to be in the loop.

### 1.3 What CIPHERO_SQL replaces

| PostgreSQL extension | CipherOcto replacement |
|---|---|
| `LANGUAGE plpgsql` | `LANGUAGE CIPHERO_SQL` (constrained subset) |
| `LANGUAGE c` | **forbidden** (no FFI in consensus) |
| `LANGUAGE internal` | **forbidden** (no internal access) |
| `LANGUAGE sql` (undeterministic functions allowed) | `LANGUAGE CIPHERO_SQL DETERMINISTIC` (enforced) |
| No declared determinism | Mandatory `DETERMINISTIC` or `NON_DETERMINISTIC` flag |

---

## 2. Grammar (informal)

```text
procedure_def := CREATE PROCEDURE proc_name ( [param_list] )
                 LANGUAGE CIPHERO_SQL
                 [DETERMINISTIC | NON_DETERMINISTIC]
                 AS 'proc_body'

param_list := (param [, param]*)?
param := param_name type

proc_body := stmt+

stmt := select_stmt
      | insert_stmt
      | update_stmt
      | delete_stmt
      | merge_stmt
      | cte_stmt
      | view_stmt

select_stmt := SELECT [DISTINCT] select_list
               FROM from_clause
               [WHERE where_clause]
               [GROUP BY group_by_clause]
               [HAVING having_clause]
               [ORDER BY order_by_clause]
               [LIMIT n]

insert_stmt := INSERT INTO table_name [(col_list)]
               [(VALUES (val_list) [, (val_list)]*)
                | SELECT ...]
```

### 2.1 Parser implementation

Parser is hand-written LALR(1) grammar; no parser generator. Lexer rejects reserved keywords in identifier positions. Parser is also a **determinism validator** — it walks the AST and rejects any node in the forbidden set (§4).

---

## 3. The `DETERMINISTIC` flag — two enforcement layers

### 3.1 Parse-time enforcement

`CREATE PROCEDURE ... DETERMINISTIC AS '...'` triggers a static analysis pass after parsing:

1. Walk AST.
2. For each function call, lookup against the **determinism registry** (RFC-0104 + §5 of this RFC).
3. For each statement, check forbidden constructors (§4).
4. If any forbidden constructor found: **parse fails** with `E_DETERMINISTIC_VIOLATION`.

If `NON_DETERMINISTIC` is declared, parse-time checks still run (forbidden constructors always rejected), but the registry lookup accepts both deterministic and non-deterministic functions. Non-deterministic procedures are explicitly tagged in the catalog and excluded from DETERMINISTIC mode.

### 3.2 Runtime verification

Before a `DETERMINISTIC` procedure is admitted to consensus:

1. Run the procedure on three independent nodes against the same input.
2. Compare byte-identical output (RFC-0126 canonical encoding).
3. If all three match: procedure accepted, `deterministic_verified_at` timestamp recorded.
4. If mismatch: procedure banned from consensus, alarm raised.

Re-verification runs periodically (default: every 10,000 invocations or every 30 days, whichever first).

---

## 4. Forbidden constructors

The following SQL syntax is **always rejected** by CIPHERO_SQL parser, regardless of `DETERMINISTIC` flag:

### 4.1 Time and randomness

| Construct | Reason |
|---|---|
| `NOW()` | Wall-clock time; non-deterministic across nodes |
| `CURRENT_TIMESTAMP` | Same as `NOW()` |
| `CURRENT_TIME` | Same as `NOW()` |
| `CURRENT_DATE` | Same as `NOW()` (date boundaries differ across timezones) |
| `LOCALTIMESTAMP` | Same as `NOW()` |
| `STATEMENT_TIMESTAMP()` | Same as `NOW()` (PostgreSQL statement-start time) |
| `TRANSACTION_TIMESTAMP()` | Same as `NOW()` (PostgreSQL transaction-start time) |
| `clock_timestamp()` | Same as `NOW()` (PostgreSQL actual current time) |
| `RANDOM()` | RNG; non-deterministic |
| `GEN_RANDOM_UUID()` | Same as `RANDOM()` |
| `UUID_GENERATE_V4()` | Same as `RANDOM()` |
| `SERIAL` / `BIGSERIAL` | Sequence advance depends on order of execution |
| `nextval('seq')` | Same as `SERIAL` |

**Replacement pattern:** use `event_seq` (current block's last event sequence number) or `block_height` instead of `NOW()`. See §6.2.

### 4.2 Side effects and IPC

| Construct | Reason |
|---|---|
| `RAISE NOTICE/EXCEPTION/INFO` | Side effect to client; non-replayable |
| `BEGIN` / `COMMIT` / `ROLLBACK` (inside procedure) | One procedure = one transaction |
| `START TRANSACTION` | Same |
| `SAVEPOINT` | Same |
| `LISTEN` / `NOTIFY` | IPC; not replayable |
| `UNLISTEN` / `UNLISTEN *` | Same |
| `VACUUM` / `ANALYZE` | Side effects on statistics |
| `REINDEX` | Side effects on indexes |
| `CLUSTER` | Same |
| `LOCK TABLE ... IN ... MODE` | External lock; not replayable |
| `SELECT ... FOR UPDATE` / `FOR SHARE` | External lock |
| `SET ... = ...` (session state) | Session state; not part of procedure body |
| `RESET ...` | Same |
| `SHOW ...` | Same |

### 4.3 Control flow (no loops, no branches)

| Construct | Reason |
|---|---|
| `IF ... THEN ... ELSE ... END IF` | Branching breaks deterministic evaluation cost |
| `CASE WHEN ... THEN ...` (top-level) | Same as `IF` |
| `LOOP` / `WHILE` / `FOR` (loop) | Loops are unbounded |
| `RETURN` (early return) | Breaks procedure shape |
| `EXIT` / `CONTINUE` | Loop control |
| `PERFORM` / `EXECUTE` (dynamic SQL) | Dynamic SQL is unparseable at create time |
| `DECLARE ... BEGIN ... END` (plpgsql block) | Implicit control flow |
| `RAISE` (any kind) | Side effect |
| `ASSERT` | Side effect |
| `EXCEPTION WHEN ...` | Error handling; non-deterministic |
| `CURSOR` / `FETCH` | Stateful traversal |

### 4.4 I/O and external access

| Construct | Reason |
|---|---|
| `COPY ... FROM ...` | File I/O |
| `COPY ... TO ...` | File I/O |
| `pg_read_file(...)` | File I/O |
| `pg_write_file(...)` | File I/O |
| `lo_import(...)` / `lo_export(...)` | Large-object I/O |
| `dblink(...)` | Network I/O |
| `pg_background_launch(...)` | Background job |
| Any function marked `VOLATILE` and not in the determinism registry | Unverified side effects |

### 4.5 DDL inside procedure body

`CREATE`, `ALTER`, `DROP` inside a procedure body: **forbidden**. Schema changes happen outside the consensus transaction (via `CIPHERO_PUBLICATION`, §6.4).

---

## 5. Allowed constructors

### 5.1 Pure SQL statements

| Statement | Notes |
|---|---|
| `SELECT` | WITH clause, CTEs, subqueries, joins, unions (all forms) |
| `INSERT ... VALUES` | Literal values or `SELECT` from another allowed statement |
| `INSERT ... SELECT` | Allowed |
| `UPDATE ... SET ... WHERE ...` | Allowed; WHERE clause must reference canonical inputs |
| `DELETE ... WHERE ...` | Allowed |
| `MERGE ... USING ... ON ... WHEN ...` | Allowed (since SQL:2003) |
| `TRUNCATE` | Forbidden (§4.5 — implicit DDL on catalogs) |

### 5.2 Deterministic functions (registry)

Functions are **deterministic** iff they:
1. Are pure (output depends only on inputs).
2. Are side-effect-free.
3. Have canonical encoding (RFC-0126).
4. Are time-independent.

Registry content (canonical set):

| Function class | Examples |
|---|---|
| Arithmetic | `+`, `-`, `*`, `/`, `%`, `abs`, `ceil`, `floor`, `round` (with explicit precision), `power`, `sqrt`, `exp`, `ln`, `log` |
| Comparison | `=`, `<>`, `<`, `>`, `<=`, `>=`, `between`, `is null`, `is not null` |
| Logical | `and`, `or`, `not`, `coalesce`, `nullif` |
| String | `length`, `substring`, `lower`, `upper`, `trim`, `ltrim`, `rtrim`, `replace`, `concat`, `position` |
| Type casts | `cast(x AS type)` — only between compatible types per DFP/RFC-0104 |
| Aggregates | `count`, `sum`, `avg`, `min`, `max` — with `GROUP BY` (deterministic only with explicit `ORDER BY` for `min`/`max` tie-breaking) |
| Window functions | `row_number`, `rank`, `dense_rank`, `lag`, `lead`, `first_value`, `last_value` — with explicit `ORDER BY` (mandatory) |
| Hash | `blake3(...)`, `hmac_blake3(key, msg)`, `ed25519_verify(pk, msg, sig)` (verify is deterministic; sign is not) |
| Encoding | `base64_encode`, `base64_decode`, `hex_encode`, `hex_decode` |
| Numeric scalar | All RFC-0104 DFP operations, RFC-0110 BIGINT, RFC-0111 DECIMAL, RFC-0113 DMAT |

### 5.3 Allowed but **non-deterministic** (requires `NON_DETERMINISTIC` flag)

These are explicitly tagged non-deterministic and excluded from DETERMINISTIC mode:

| Function | Why non-deterministic | CIPHERO_SQL replacement |
|---|---|---|
| `now()` | Wall clock | `event_seq` / `block_height` |
| `random()` | RNG | (none — VRF-derived from block seed, separate API) |
| `gen_random_uuid()` | RNG | (none — derive from `event_seq` if needed) |
| `current_setting(...)` | Session state | (none — bind as parameter) |
| `txid_current()` | Transaction ID | `event_id` |

### 5.4 Required explicit `ORDER BY`

Any `SELECT` returning more than one row **must** end with `ORDER BY`. Tie-breaking columns must be deterministic (e.g., `event_id`, not `random()`).

```sql
-- Allowed
SELECT account_id, SUM(amount_micro)
FROM transfer_events
WHERE event_seq < $block_start_seq
GROUP BY account_id
ORDER BY account_id;          -- deterministic tie-break

-- Forbidden
SELECT account_id, SUM(amount_micro)
FROM transfer_events
WHERE event_seq < $block_start_seq
GROUP BY account_id;
-- ^ parser rejects: result of aggregate is multi-row but no ORDER BY
```

---

## 6. Replacement patterns for enterprise migration

### 6.1 `plpgsql` → CIPHERO_SQL

| `plpgsql` pattern | CIPHERO_SQL replacement |
|---|---|
| `FOR r IN SELECT * FROM t LOOP ... END LOOP` | Single `UPDATE ... FROM (SELECT ...) AS r` or `MERGE` |
| `IF condition THEN ... END IF` | `WHERE` clause or `CASE` inside `SELECT` (allowed at expression level, not statement level) |
| Variable assignment | Subquery in `FROM` or `WITH` |
| `RETURNING ... INTO var` | Embed `RETURNING` clause directly in surrounding statement |
| `RAISE EXCEPTION` | Reject the entire transaction (return error) |

### 6.2 Time-dependent logic

```sql
-- plpgsql (forbidden in CIPHERO_SQL)
CREATE FUNCTION age_in_days(created_at TIMESTAMP) RETURNS INT AS $$
BEGIN
    RETURN EXTRACT(DAY FROM (NOW() - created_at));
END;
$$ LANGUAGE plpgsql;

-- CIPHERO_SQL (block-height-derived)
CREATE PROCEDURE close_month()
LANGUAGE CIPHERO_SQL DETERMINISTIC
AS $$
    INSERT INTO monthly_summary
    SELECT (event_seq / 1000000) AS month_bucket, SUM(amount_micro)
    FROM transfer_events
    WHERE event_seq < $block_start_seq
    GROUP BY 1
    ORDER BY 1;
$$;
```

`$block_start_seq` is bound by the ExecutionEnvelope protocol (RFC-0962 §4) to the current block's first event sequence.

### 6.3 Loops → set operations

```sql
-- plpgsql (forbidden)
CREATE PROCEDURE recompute_balances()
LANGUAGE plpgsql AS $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT account_id FROM accounts LOOP
        UPDATE accounts SET balance = (
            SELECT COALESCE(SUM(amount), 0) FROM ledger
            WHERE account_id = r.account_id
        ) WHERE account_id = r.account_id;
    END LOOP;
END;
$$;

-- CIPHERO_SQL
CREATE PROCEDURE recompute_balances()
LANGUAGE CIPHERO_SQL DETERMINISTIC
AS $$
    UPDATE accounts
    SET balance = agg.total
    FROM (
        SELECT account_id, COALESCE(SUM(amount), 0) AS total
        FROM ledger
        GROUP BY account_id
    ) AS agg
    WHERE accounts.account_id = agg.account_id;
$$;
```

### 6.4 Schema changes (DDL outside procedure)

DDL doesn't live inside a CIPHERO_SQL procedure. It uses the `CIPHERO_PUBLICATION` primitive:

```sql
-- Outside any procedure, in a DDL transaction:
CREATE CIPHERO_PUBLICATION orders_pub FOR TABLE orders;
```

Publication is the schema-level analog of a Kafka topic. Subscribers (§6.5) replay the publication.

### 6.5 Cross-database subscriptions

```sql
-- Subscribe enterprise database to cipherocto events
CREATE CIPHERO_SUBSCRIPTION cipher_sub
    CONNECTION 'cipherocto://node1.cluster'
    PUBLICATION orders_pub
    CONSISTENCY_MODE 'bounded'
    LAG_MS_MAX 1000;

-- Subscribe cipherocto to enterprise PostgreSQL (logical replication source)
CREATE CIPHERO_SUBSCRIPTION enterprise_orders_sub
    CONNECTION 'postgresql://enterprise.host/orders'
    PUBLICATION enterprise_orders_pub
    CONSISTENCY_MODE 'eventual';
```

---

## 7. Parse-time error codes

| Code | Meaning |
|---|---|
| `E_DETERMINISTIC_VIOLATION` | Procedure marked `DETERMINISTIC` but AST contains non-deterministic function |
| `E_FORBIDDEN_CONSTRUCTOR` | AST contains a §4 forbidden constructor |
| `E_MISSING_ORDER_BY` | SELECT returns >1 row but no `ORDER BY` |
| `E_VOLATILE_FUNCTION` | Function call marked `VOLATILE` and not in registry |
| `E_DDL_INSIDE_PROCEDURE` | DDL statement inside procedure body |
| `E_NON_DETERMINISTIC_IN_SAFE_MODE` | Procedure marked `NON_DETERMINISTIC` invoked in DETERMINISTIC mode |
| `E_RUNTIME_VERIFICATION_FAILED` | Three-node replay produced non-identical output |

---

## 8. DETERMINISTIC mode semantics

A `ExecutionEnvelope` (RFC-0962) declared `mode = DETERMINISTIC` rejects:
- Any `NON_DETERMINISTIC` procedure invocation.
- Any direct SQL using a non-deterministic function from §5.3.
- Any schema modification (DDL).
- Any cross-shard write (must use `MultiSettlement` per RFC-0960 §7).

A `ExecutionEnvelope` declared `mode = OFF_CHAIN_SAFE` (or similar) allows all of the above. Off-chain sessions do not enter consensus; they are local-only execution.

---

## 9. Catalog schema

Procedures live in a system catalog:

```sql
CREATE TABLE cipherocto_procedures (
    proc_id           BLOB PRIMARY KEY,          -- RFC-0126 canonical hash of proc_body
    proc_name         TEXT NOT NULL,
    schema_version    INT NOT NULL DEFAULT 1,
    language          TEXT NOT NULL DEFAULT 'CIPHERO_SQL',
    determinism       TEXT NOT NULL,             -- 'DETERMINISTIC' | 'NON_DETERMINISTIC'
    proc_body         BLOB NOT NULL,             -- RFC-0126 canonical encoding of AST
    param_types       BLOB NOT NULL,             -- RFC-0126 encoded type list
    deterministic_verified_at_unix BIGINT NULL,  -- runtime verification timestamp
    invocation_count  BIGINT NOT NULL DEFAULT 0,
    created_at_unix   BIGINT NOT NULL,
    created_by        BLOB NOT NULL,             -- DID of creator
    CONSTRAINT uq_proc_name UNIQUE (proc_name),
    CONSTRAINT ck_language CHECK (language = 'CIPHERO_SQL'),
    CONSTRAINT ck_determinism CHECK (determinism IN ('DETERMINISTIC', 'NON_DETERMINISTIC'))
);
```

`proc_id` is the canonical hash of the AST. Two procedures with identical ASTs (after normalization) share the same `proc_id`, enabling content-addressed deduplication.

---

## 10. Worked example: end-to-end enterprise migration

### 10.1 Original Oracle procedure

```sql
CREATE OR REPLACE PROCEDURE close_month
AS
    v_total NUMBER;
BEGIN
    SELECT SUM(amount) INTO v_total
    FROM transactions
    WHERE TRUNC(created_at, 'MM') = TRUNC(SYSDATE, 'MM') - 1;
    
    INSERT INTO monthly_summary (month, total)
    VALUES (TRUNC(SYSDATE, 'MM') - 1, v_total);
    
    COMMIT;
END;
/
```

### 10.2 Translated CIPHERO_SQL

```sql
CREATE PROCEDURE close_month()
LANGUAGE CIPHERO_SQL DETERMINISTIC
AS $$
    INSERT INTO monthly_summary (month, total)
    SELECT
        (event_seq / 1000000) - 1 AS month_bucket,
        SUM(amount_micro)
    FROM transfer_events
    WHERE event_seq < $block_start_seq
      AND event_seq >= ($block_start_seq - 1000000)
    GROUP BY 1
    ORDER BY 1;
$$;
```

### 10.3 Deployment

1. `CREATE PROCEDURE` parsed → AST built → `DETERMINISTIC` registry walked → all calls pass → parse succeeds.
2. Procedure committed to catalog with `proc_id = blake3(ast_canonical_bytes)`.
3. Three-node runtime verification runs the procedure against synthetic input → all match → `deterministic_verified_at_unix` recorded.
4. Procedure now available for `ExecutionEnvelope` invocation (RFC-0962).
5. Application invokes via JDBC: `Connection.prepareCall("{call close_month()}")` — translates to `ExecutionEnvelope` with one `sql_statement` entry.

---

## 7. Deterministic SQL Profile (v2.0 NEW)

The forbidden-constructors list (§4) prevents obviously non-deterministic operations. The **Deterministic SQL Profile** goes further: it pins down behavioral rules so that any two compliant implementations produce bit-identical results across all replay paths. These rules apply to **all** CIPHERO_SQL statements, in **all** modes (`DETERMINISTIC`, `NON_DETERMINISTIC`, `OFF_CHAIN`, `AUDIT_ONLY`).

### 7.1 `ORDER BY` requirement for `LIMIT` / `OFFSET`

```sql
-- Forbidden: row order is implementation-defined
SELECT * FROM orders LIMIT 10;

-- Required: explicit ordering before LIMIT
SELECT * FROM orders ORDER BY order_id LIMIT 10;
```

A query that uses `LIMIT` or `OFFSET` without an `ORDER BY` clause is **rejected at parse time** with `E_DETERMINISTIC_PROFILE_NO_ORDER_BY`. Two implementations might otherwise return different "first 10" rows.

### 7.2 String comparison uses `COLLATE "C"` only

```sql
-- Forbidden: locale-dependent ordering
SELECT * FROM customers WHERE name < 'Müller';

-- Required: explicit C collation
SELECT * FROM customers WHERE name < 'Müller' COLLATE "C";
```

Locale-dependent comparisons produce different sort orders on different locales (e.g., `é` vs `e`). CIPHERO_SQL accepts only the C (binary) collation. The default collation for any `VARCHAR` column is `COLLATE "C"`; declarations of other collations are rejected.

### 7.3 Floating-point uses RFC-0104 DFP encoding

All numeric computation uses the **Deterministic Floating-Point** encoding from RFC-0104. Naive IEEE-754 floating-point is forbidden:

- **Forbidden:** `SELECT SUM(price * quantity) FROM orders;` where `price` is `DOUBLE PRECISION`.
- **Required:** `price` and `quantity` columns are RFC-0104 DFP-typed; `SUM` operates on DFP values; result is bit-identical across implementations.

`DOUBLE PRECISION`, `REAL`, `FLOAT` types are **rejected** at `CREATE TABLE` time. `DECIMAL` (RFC-0104 DFP) is the only numeric type.

### 7.4 `NULL` ordering — `NULLS LAST` default

```sql
-- Default: NULLS LAST
SELECT * FROM employees ORDER BY salary DESC;
-- NULLs appear at the end, after all non-NULL salaries.

-- Explicit: NULLS FIRST or NULLS LAST allowed
SELECT * FROM employees ORDER BY salary DESC NULLS FIRST;
```

Default null ordering is `NULLS LAST` for `ASC`, `NULLS FIRST` for `DESC`. Explicit `NULLS FIRST` / `NULLS LAST` clauses are required if the application needs a different behavior — implicit reliance on a specific null position is rejected.

### 7.5 `BETWEEN` is inclusive on both ends

```sql
-- Range: 100 <= salary <= 200 (both inclusive)
SELECT * FROM employees WHERE salary BETWEEN 100 AND 200;
```

CIPHERO_SQL `BETWEEN x AND y` is inclusive on both ends: `x <= value <= y`. This matches SQL standard but is reinforced here because some dialects (e.g., T-SQL legacy) had different semantics historically.

### 7.6 Aggregates are deterministic given deterministic input

`SUM`, `AVG`, `MIN`, `MAX`, `COUNT`, `COUNT(DISTINCT ...)` are deterministic provided:

- Input rows are ordered deterministically (§7.1).
- Numeric inputs are RFC-0104 DFP (§7.3).
- String inputs are `COLLATE "C"` (§7.2).

`STDDEV`, `VARIANCE` are also deterministic (DFP-encoded sum-of-squares). `MEDIAN` requires a deterministic ordering of input rows and produces the lower median on even-count input (no averaging of the two middles — averaging is forbidden because it can be non-deterministic under DFP rounding choices).

### 7.7 `GROUP BY` produces deterministic row order

```sql
-- GROUP BY column set forms a deterministic ordering key.
SELECT region, SUM(amount) FROM sales GROUP BY region ORDER BY region;
```

The implicit row order produced by `GROUP BY` is deterministic: rows are sorted by the `GROUP BY` columns (lexicographic, `COLLATE "C"`, `NULLS LAST`). A `SELECT ... GROUP BY ...` without an explicit `ORDER BY` returns rows in this deterministic order.

### 7.8 Forbidden `SELECT *` in stored procedures

```sql
-- Forbidden: column list is implementation-dependent after ALTER TABLE ADD COLUMN
CREATE PROCEDURE get_orders() LANGUAGE CIPHERO_SQL DETERMINISTIC AS '
    SELECT * FROM orders;
';

-- Required: explicit column list
CREATE PROCEDURE get_orders() LANGUAGE CIPHERO_SQL DETERMINISTIC AS '
    SELECT order_id, customer_id, amount FROM orders;
';
```

`SELECT *` is **forbidden** in stored procedures because adding a column to the table (`ALTER TABLE orders ADD COLUMN foo TEXT`) would change the procedure's output, breaking replay determinism for any prior procedure invocation. Ad-hoc queries (not in procedures) MAY use `SELECT *` because they are not replayed across nodes.

### 7.9 Determinism registry for functions

CIPHERO_SQL ships with a **determinism registry** listing every allowed function and its determinism class. Examples:

| Function | Determinism | Notes |
|---|---|---|
| `length(s)` | Deterministic | Pure |
| `lower(s)` | Deterministic | Locale-independent (C) |
| `abs(x)` | Deterministic | DFP |
| `current_block_height()` | Deterministic per block | All nodes see same block height in same WAL segment |
| `now()` | **Forbidden** | Wall-clock; non-deterministic |
| `random()` | **Forbidden** | Non-deterministic |

A function not in the registry is rejected at procedure-creation time. Operators may add custom functions to the registry by submitting a CIPHERO_SQL extension RFC.

### 7.10 Enforcement layers

The Deterministic SQL Profile is enforced at three layers:

1. **Parse-time** (§3.1) — static analysis rejects violations.
2. **Compile-time** — the engine refuses to lower a non-conformant AST to WAL entries.
3. **Replay-time** — every node replays the WAL and verifies the output matches the recorded `expected_post_state_hash`. Mismatch = `E_REPLAY_MISMATCH`.

The three layers are cumulative: a violation may pass parse-time and be caught at compile or replay. The strictest layer that catches it is the one that fires.

---

## 11. Open questions for RFC-0962

The following questions are deferred to RFC-0962 (ExecutionEnvelope protocol):

1. How is `$block_start_seq` bound at procedure invocation time?
2. How does the `ExecutionEnvelope` sign the procedure invocation?
3. What is the wire format for the canonical AST over the network?
4. How are runtime verification failures handled in production (alarm routing, procedure quarantine)?

---

## 12. Out of scope

- **Triggers.** Triggers are forbidden in CIPHERO_SQL because they introduce non-deterministic ordering. CipherOcto triggers, if needed, are a separate RFC.
- **Views with non-deterministic functions.** Materialized views can only reference DETERMINISTIC procedures.
- **Cross-shard DDL.** Publication/subscription model covers this in a future RFC.
- **Full plpgsql compatibility layer.** Out of scope; explicit migration to CIPHERO_SQL is the migration path.

---

## 13. Status

This RFC = CIPHERO_SQL **Deterministic SQL Dialect** companion to RFC-0960 §1 (Deterministic SQL Engine). Status: **Draft v2.0** (strategic reframe; "Consensus-Safe SQL" → "Deterministic SQL"). Awaiting review and promotion to Accepted. Once Accepted, the `cipherocto-sql` crate can implement the parser + registry + runtime verification.

Companion RFCs in flight:
- RFC-0962 (ExecutionEnvelope object protocol) — Draft v2.0 (2026-07-23)
- RFC-0964 (Constraint encoding) — Draft v1.1 (CIPHERO_SQL is a consumer when `AllowIf` references procedures)
- RFC-0967 (Policy Object Graph) — Draft v1.0 (2026-07-23, NEW; procedures may be gated by `policy_id`)

**v2.0 strategic positioning:** CIPHERO_SQL is the deterministic SQL dialect that compiles to deterministic WAL entries (RFC-0960 §1.1). The dialect is the surface language enterprise developers use (Hibernate → ORM → JDBC → CIPHERO_SQL → deterministic SQL Engine → WAL). The WAL is the protocol; consensus is one possible certifier.
