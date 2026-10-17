# Enterprise Migration Playbooks — Synthesis

**Date:** 2026-07-22
**Status:** Research Phase 5 of N — enterprise migration playbooks (per user direction 2026-07-22)
**Builds on:**
- `docs/research/2026-07-22-value-transfer-model-internal-landscape.md` (Phase 1)
- `docs/research/2026-07-22-grand-design-vaults-capabilities-reservations.md` (Phase 2)
- `docs/research/2026-07-22-external-capability-based-spend-systems.md` (Phase 3)
- `docs/research/2026-07-22-event-sourced-ledger-precedents.md` (Phase 4)
**Scope:** four production-tested enterprise data systems with lessons for cipherocto's grand design §12 (Consensus Sessions / compatibility layer):

| System | What it ships | What cipherocto learns |
|---|---|---|
| PostgreSQL logical replication | Publication/subscription; row filters; initial snapshot + incremental; transactional | The pattern for migrating enterprise data into cipherocto via logical decoding |
| ShardingSphere | Data sharding + distributed transaction + read/write splitting + data migration + query federation + data encryption | The middleware pattern — cipherocto can be a ShardingSphere-like "Database Plus" layer |
| Hibernate ORM | Object-relational mapping; session-per-request; transactions; JDBC under the hood | ORMs work unchanged if cipherocto ships a JDBC driver |
| PostgreSQL CREATE PROCEDURE | `LANGUAGE SQL`, `LANGUAGE plpgsql`, `LANGUAGE c`, `BEGIN ATOMIC` blocks, function overloading | Stored procedures survive if cipherocto adds `LANGUAGE CIPHERO_SQL` |

---

## 0. Why enterprise migration matters

Grand design §12 makes the case:

> "Every line of business logic, every ORM, every stored procedure, every framework is built around the session model. That is why ERP migrations to blockchain never happen."

Cipherocto's Consensus Session (§12.2-12.9) is the architectural answer. This phase provides the **concrete technical pattern**: what does an enterprise system actually need to migrate?

---

## 1. PostgreSQL logical replication — the migration primitive

### 1.1 What it ships

PostgreSQL ships production-grade logical replication:

```sql
-- On publisher
CREATE PUBLICATION my_pub FOR TABLE orders, customers;
ALTER TABLE orders REPLICA IDENTITY FULL;

-- On subscriber
CREATE SUBSCRIPTION my_sub CONNECTION 'host=publisher' PUBLICATION my_pub;
```

Replication flow:

1. **Initial snapshot** — copy the entire table state to subscriber
2. **Replication slot** — subscriber tracks which WAL segments it has consumed
3. **Logical decoding** — convert physical WAL changes into row-level `INSERT` / `UPDATE` / `DELETE` events
4. **Apply order** — subscriber applies changes in publisher's commit order, preserving transactional consistency
5. **Row filters** — `WHERE` clauses to subscribe to a subset of rows
6. **Column lists** — subscribe to a subset of columns (privacy!)

### 1.2 What cipherocto should adopt

| PostgreSQL primitive | Cipherocto analogue |
|---|---|
| `PUBLICATION` | `CIPHEROCTO_PUBLICATION` — declares which tables to expose |
| `SUBSCRIPTION` | `CIPHEROCTO_SUBSCRIPTION` — consumer registers, gets incremental events |
| `REPLICA IDENTITY FULL` | Vault/capability table must have full identity for updates to be replicated as logical events |
| Initial snapshot | CipherOcto node joins → receives full event log from genesis (Phase 4 §6.2) |
| Replication slot | Per-subscriber offset tracking |
| Row filters + column lists | Privacy via `visibility: Public | Confidential | Private` (Phase 4 §7) |

**Recommendation:** cipherocto ships a `logical_replication` mode that exposes `CIPHEROCTO_PUBLICATION` and `CIPHEROCTO_SUBSCRIPTION` SQL primitives. Enterprise migrations become:

```sql
-- In cipherocto, declare what to expose
CREATE CIPHEROCTO_PUBLICATION orders_pub FOR TABLE orders;

-- On the enterprise side, subscribe
CREATE CIPHEROCTO_SUBSCRIPTION cipher_sub 
    CONNECTION 'cipherocto://node1.cluster'
    PUBLICATION orders_pub;
```

The enterprise PostgreSQL becomes a `CIPHEROCTO_SUBSCRIPTION` consumer. Or vice versa — cipherocto consumes from enterprise PostgreSQL via logical replication.

### 1.3 The migration playbook — step by step

| Step | Action | Tool |
|---|---|---|
| 1 | Run `pg_dump --schema-only` on source | psql |
| 2 | Apply schema to cipherocto via SQL gateway | `CIPHERO_PUBLICATION` |
| 3 | Begin initial data copy | Snapshot mode |
| 4 | Subscribe to logical replication | `CIPHERO_SUBSCRIPTION` |
| 5 | Cut over read traffic | DNS / load balancer |
| 6 | Cut over write traffic | Disable old writes |
| 7 | Keep old database as warm backup | Replication slot preserves |
| 8 | After 30 days, decommission | Stop old database |

**This is the production-tested enterprise migration playbook, applicable to cipherocto directly.**

---

## 2. ShardingSphere — Database Plus pattern

### 2.1 What it ships

ShardingSphere is middleware that sits between applications and databases. It transforms any database into a distributed database by adding:

| Feature | What it does |
|---|---|
| Data sharding | Horizontal partitioning across multiple databases |
| Distributed transaction | XA + BASE transactions across shards |
| Read/write splitting | Route reads to replicas |
| Data migration | Move data between databases while serving traffic |
| Query federation | Query across heterogeneous databases |
| Data encryption | Transparent column-level encryption |

### 2.2 The "Database Plus" design philosophy

ShardingSphere's tagline:

> "Database Plus — building the standard and ecosystem on the upper layer of the heterogeneous database. It focuses on how to make full and reasonable use of the computing and storage capabilities of existing databases rather than creating a brand new database."

This is **exactly** cipherocto's positioning. cipherocto is a layer above existing databases (SQL backends: PostgreSQL, MySQL, SQLite, stoolap) that adds blockchain guarantees:

- Determinism (CONSENSUS_SAFE mode)
- Capability-based access (no passwords)
- Cryptographic audit trail (event log + signatures)
- Multi-party consensus (RFC-0862 sync)

**Cipherocto is ShardingSphere for blockchain semantics.**

### 2.3 What cipherocto should adopt from ShardingSphere

| ShardingSphere feature | Cipherocto analogue |
|---|---|
| Data sharding | Resource shards (grand design §10) |
| Distributed transaction | MultiSettlement (grand design §7) |
| Read/write splitting | Public vs private event reads |
| Data migration | Enterprise migration playbook (§1.3 above) |
| Query federation | Cross-shard queries with capability-checked projection |
| Data encryption | `Confidential` events (Phase 4 §7) |

### 2.4 The middleware vs native database question

ShardingSphere is middleware. Cipherocto is a "real" blockchain. The difference:

- ShardingSphere doesn't add cryptographic consensus
- Cipherocto adds consensus, but inherits all middleware features from ShardingSphere

**Recommendation:** cipherocto's `quota-router-core` (already exists) should expose a ShardingSphere-compatible JDBC driver. Enterprise applications can point to `jdbc:cipherocto://cluster` without changing ORM code.

---

## 3. Hibernate ORM — the session model cipherocto must preserve

### 3.1 What it ships

Hibernate is the dominant Java ORM. Programming model:

```java
Session session = sessionFactory.openSession();
session.beginTransaction();

User user = session.get(User.class, 42L);     // READ
user.setName("Alice");                       // MUTATE in memory
session.update(user);                        // WRITE (lazy)

Order order = new Order(user);
session.save(order);                         // INSERT (lazy)

session.getTransaction().commit();           // FLUSH all writes
session.close();                              // release session
```

Key abstractions:
- **Session** — scoped unit of work, identity map, change tracking
- **Lazy loading** — fetch on access, not upfront
- **Dirty checking** — auto-detect changes on flush
- **Cascade** — auto-persist related entities
- **Locking** — optimistic + pessimistic

### 3.2 The enterprise contract cipherocto must preserve

If cipherocto ships a JDBC driver, Hibernate applications should work unchanged:

| Hibernate concept | What cipherocto must provide |
|---|---|
| `Connection` | `jdbc:cipherocto://cluster` Connection |
| `Connection.commit()` | One signed WAL block (one signature for N SQL ops) |
| `Connection.rollback()` | Sign a no-op WAL block + discard session |
| `Connection.setAutoCommit(false)` | Begin ConsensusSession |
| Savepoint | Capability sub-scope |
| Read-only mode | Public-only events |
| Lock timeout | Capability expires_at |

The **single signature per WAL block** is the key insight: Hibernate batches N writes into one transaction. Cipherocto batches N writes into one signed consensus transaction. Identical programming model.

### 3.3 The Hibernate dialect

Hibernate ships **dialects** per database. Adding cipherocto means shipping a `CipherOctoDialect` that:

- Maps cipherocto types (DID, VaultID, CapabilityID, MicroOCTO_W) to JDBC types
- Generates cipherocto SQL extensions (`CIPHERO_FROM_CAPABILITY`, `CIPHERO_USING_LEDGER_EVENT`, etc.)
- Handles capability-checked UPDATEs via WHERE clause injection

**Recommendation:** phase 5+ deliverable: `cipherocto-hibernate-dialect` crate that ships a Hibernate dialect. Enables Java enterprise migration without code changes.

---

## 4. PostgreSQL CREATE PROCEDURE — stored procedures survive

### 4.1 What it ships

PostgreSQL supports multiple procedure languages:

```sql
CREATE PROCEDURE close_month()
LANGUAGE SQL
AS $$
    INSERT INTO monthly_summary
    SELECT date_trunc('month', created_at), SUM(amount)
    FROM transactions
    WHERE created_at < date_trunc('month', NOW())
    GROUP BY 1;
$$;

CREATE PROCEDURE audit_customer(customer_id BIGINT)
LANGUAGE plpgsql
AS $$
BEGIN
    -- can use loops, variables, control flow
    FOR r IN SELECT * FROM customers WHERE id = customer_id LOOP
        INSERT INTO audit_log VALUES (r.id, r.modified_at);
    END LOOP;
END;
$$;
```

### 4.2 The cipherocto extension

Grand design §12.4 says:

> ```text
> CREATE DETERMINISTIC PROCEDURE CloseMonth()
>     deterministic SQL only
>     deterministic functions only
>     deterministic ordering
>     deterministic timestamps
>     deterministic randomness
> ```

PostgreSQL's `LANGUAGE plpgsql` is **not** deterministic (loops, control flow, side effects, time, randomness). cipherocto needs a **constrained** language: `LANGUAGE CIPHERO_SQL`.

```sql
CREATE PROCEDURE close_month()
LANGUAGE CIPHERO_SQL          -- only deterministic SQL
DETERMINISTIC                  -- explicit declaration
AS $$
    INSERT INTO monthly_summary
    SELECT event_seq / 1000000 AS month_bucket, SUM(amount_micro)
    FROM transfer_events
    WHERE event_seq < $block_start_seq
    GROUP BY 1;
$$;
```

`CIPHERO_SQL` rejects:
- `NOW()`, `CURRENT_TIMESTAMP`, `CURRENT_TIME`
- `RANDOM()`, `GEN_RANDOM_UUID()`
- `RAISE` (no side effects)
- Loops, recursion
- `BEGIN` / `COMMIT` (one transaction only)
- `SELECT ... FOR UPDATE` (no external locks)
- `LISTEN` / `NOTIFY` (no IPC)
- File I/O, network access

`CIPHERO_SQL` allows:
- Pure SQL: SELECT, INSERT, UPDATE, DELETE (within capability scope)
- Deterministic functions: arithmetic, type casts, comparisons
- Deterministic ordering: explicit `ORDER BY` (mandatory for any `SELECT` returning >1 row)
- View references
- Constant CTEs

### 4.3 The deterministic flag

PostgreSQL doesn't enforce determinism. cipherocto must. Recommendation:

```sql
CREATE PROCEDURE foo() LANGUAGE CIPHERO_SQL DETERMINISTIC AS $$ ... $$;
--                                       ^^^^^^^^^^^^^^^^^^
--                                       enforced at parse time + verified at runtime
```

If the procedure body violates the deterministic subset, parse fails. If runtime behavior is non-deterministic (e.g., depends on clock), execute fails with `E_DETERMINISTIC_VIOLATION`.

### 4.4 What gets lost — the trade-off

Stored procedures that need `plpgsql` features (loops, time, error handling) **don't survive** in cipherocto. That's by design — consensus requires determinism.

But: **most enterprise stored procedures are pure SQL**. Reports, summaries, validations, refactors. These survive.

---

## 5. Compatibility Levels (concrete)

Grand design §12.8 defines four levels:

| Level | What works | Examples |
|---|---|---|
| 1. ANSI SQL | All ANSI SQL operators, types, joins | Reports, dashboards, simple CRUD |
| 2. PostgreSQL-compatible | + PostgreSQL extensions (JSONB, GIN, window functions) | Most enterprise apps |
| 3. Enterprise | + Oracle/SAP extensions (CONNECT BY, MATERIALIZE, hierarchical queries) | Migrations from legacy |
| 4. Deterministic Blockchain | + `CIPHERO_SQL`, capability-checked UPDATEs, event-sourced projections | New cipherocto-native apps |

### 5.1 The migration story per level

**Level 1 → 2:** Add PostgreSQL extensions (already supported by stoolap fork).

**Level 2 → 3:** Implement Oracle-style extensions (CONNECT BY → recursive CTE; MATERIALIZE → MATERIALIZED VIEW). Some work.

**Level 3 → 4:** Convert non-deterministic procedures to `CIPHERO_SQL`. Replace time-dependent logic with block-height-dependent logic. Replace `NOW()` with `event_seq`-based computation.

**Level 4 native:** Use cipherocto-specific primitives — capabilities, reservations, event-log reads, ZK proofs.

### 5.2 The onboarding story

| Enterprise user | Starting level | First migration | End state |
|---|---|---|---|
| PostgreSQL shop | Level 2 | Add cipherocto JDBC driver; no code change | Level 2 reading from cipherocto replicas |
| Oracle shop | Level 3 | Translate Oracle-specific SQL to PostgreSQL dialect; ship CIPHERO_SQL stubs | Level 3 with CIPHERO_SQL for new code |
| SAP / ERP shop | Level 3+ | SAP RFC adapter writes to cipherocto via JDBC | Level 4 with capability-gated writes |
| Web3 shop | Level 4 | Native | Level 4 |

---

## 6. The enterprise migration protocol (concrete)

```text
Enterprise Database (PostgreSQL/Oracle/MySQL)
              │
              │ CIPHERO_SUBSCRIPTION (logical replication, Phase 1)
              ▼
   CipherOcto ConsensusSession
              │
              │ JDBC driver (Phase 2)
              ▼
   Enterprise Application (Hibernate/Diesel/SQLAlchemy)
              │
              │ Application unchanged
              ▼
        End users
```

Three components ship:

| Component | What it does | Status |
|---|---|---|
| `cipherocto-jdbc` | JDBC driver exposing ConsensusSession | Phase 5 deliverable |
| `cipherocto-logical-repl` | `CIPHERO_PUBLICATION` + `CIPHERO_SUBSCRIPTION` SQL primitives | Phase 5 deliverable |
| `cipherocto-hibernate-dialect` | Hibernate dialect for JDBC driver | Phase 5+ deliverable |

---

## 7. Pitfalls the surveyed systems expose

### 7.1 Logical replication lag

PostgreSQL logical replication is **async**. Subscribers can lag seconds behind. Cipherocto must accept eventual consistency on reads.

**Recommendation:** `CIPHERO_SUBSCRIPTION` carries a `consistency_mode: Immediate | Bounded { lag_ms_max: u32 } | Eventually`. Application picks.

### 7.2 ShardingSphere's distributed transaction overhead

XA transactions are slow (2PC). BASE transactions require compensation logic. Cipherocto must not force every write into distributed transaction territory.

**Recommendation:** intra-shard writes are local (fast). Cross-shard writes use `MultiSettlement` (slow but atomic). Most writes should stay intra-shard.

### 7.3 Hibernate's session-per-request assumption

Hibernate assumes a session is bound to one HTTP request. Cipherocto ConsensusSessions can outlive requests (multi-block transactions).

**Recommendation:** `cipherocto-hibernate-dialect` should support both modes: short-lived (one block) and long-lived (multi-block with explicit checkpoints).

### 7.4 Stored procedure determinism enforcement

PostgreSQL doesn't enforce `DETERMINISTIC`. If the developer mis-declares, queries may diverge across nodes. Cipherocto MUST enforce.

**Recommendation:** `CIPHERO_SQL` procedures are verified at runtime via a test execution that compares results across nodes. Mismatch = procedure banned from consensus.

---

## 8. Updates to grand design §12 (Consensus Sessions)

Current §12 has 9 subsections. Phase 5 adds:

**§12.10 Compatibility Matrix (concrete)**

```text
Level 1: ANSI SQL
    + standard types, joins, aggregates
    + views, indexes, FK constraints
    + transactions, savepoints
    
Level 2: PostgreSQL-compatible
    + JSONB, GIN/GIST indexes
    + window functions, CTEs
    + generate_series, ARRAY types
    
Level 3: Enterprise
    + hierarchical queries (CONNECT BY → recursive CTE)
    + MATERIALIZED VIEW
    + PARTITION BY (range/list/hash)
    + CREATE PROCEDURE LANGUAGE SQL | CIPHERO_SQL
    
Level 4: Deterministic Blockchain (CONSENSUS_SAFE)
    + capability-checked UPDATEs (WHERE clause injected by capability)
    + event-log reads (SELECT FROM transfer_events)
    + CIPHERO_PUBLICATION / CIPHERO_SUBSCRIPTION
    + ZK proof outputs
    + deterministic mode enforced at runtime
```

**§12.11 Migration Tooling**

- `cipherocto-jdbc` — JDBC driver
- `cipherocto-logical-repl` — replication SQL primitives
- `cipherocto-hibernate-dialect` — Hibernate dialect
- `cipherocto-oracle-adapter` — Oracle-specific extensions to Level 3
- `cipherocto-sap-rfc` — SAP RFC adapter (Level 4 writes)

**§12.12 Onboarding Levels (updated)**

| Level | Toolchain required | Use case |
|---|---|---|
| 1 | Just JDBC | Reports, BI tools |
| 2 | JDBC + JSONB | Most enterprise apps |
| 3 | + Oracle/SAP adapter | Legacy migration |
| 4 | + Hibernate dialect + capability framework | Native cipherocto |

---

## 9. The minimal first deliverable

Per `cargo fmt workflow` + `always solve all issues` + Phase 5 being research not implementation, this doc is **research**, not RFC. The first implementation deliverable is:

1. **Phase 6: Deterministic SQL classification** — the `CIPHERO_SQL` language spec
2. **Phase 7: ConsensusSession object RFC** — the protocol-level design
3. **Phase 8: Resource shard routing RFC** — the scaling story
4. **Phase 9: RFC-0960 grand-design synthesis** — the umbrella RFC

Phases 6-9 are RFCs, not code. Once accepted, implementation can begin.

---

## 10. References

### External

- PostgreSQL logical replication: <https://www.postgresql.org/docs/current/logical-replication.html>
- PostgreSQL logical decoding: <https://www.postgresql.org/docs/current/logicaldecoding.html>
- PostgreSQL WAL: <https://www.postgresql.org/docs/current/wal.html>
- PostgreSQL CREATE PROCEDURE: <https://www.postgresql.org/docs/current/sql-createprocedure.html>
- ShardingSphere overview: <https://shardingsphere.apache.org/document/current/en/overview/>
- Hibernate user guide: <https://docs.jboss.org/hibernate/orm/current/userguide/html_single/Hibernate_User_Guide.html>
- Oracle GoldenGate: <https://docs.oracle.com/en/middleware/goldengate/> (model-knowledge supplement)

### Internal

- Grand design §12.1-12.9 (Consensus Sessions)
- RFC-0862 (sync as propagation)
- Phase 3 doc (capability-based spend systems)
- Phase 4 doc (event-sourced ledger precedents)

---

## 11. Status

This doc = Phase 5 of N research. Four enterprise systems surveyed (PostgreSQL logical replication, ShardingSphere, Hibernate, CREATE PROCEDURE). Concrete deliverables identified: `cipherocto-jdbc`, `cipherocto-logical-repl`, `cipherocto-hibernate-dialect`, `cipherocto-oracle-adapter`, `cipherocto-sap-rfc`. Compatibility Levels 1-4 enriched with concrete feature lists.

**Next action:** Phase 6 (Deterministic SQL classification) → Phase 7 (ConsensusSession RFC) → Phase 8 (Resource shard routing RFC) → Phase 9 (RFC-0960 grand-design synthesis). Recommend proceeding sequentially; all four are RFCs, not code.
