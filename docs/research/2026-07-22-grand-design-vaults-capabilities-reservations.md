# Grand Design — Vaults, Capabilities, Reservations, Execution Envelopes (over a Deterministic WAL)

**Date:** 2026-07-22 (original); 2026-07-23 (v2.0 strategic reframe)
**Status:** Research Phase 2 of N — grand design synthesis (per user-supplied conversation 2026-07-22). **v2.0 strategic reframe applied 2026-07-23**: architecture inverted to make the Deterministic WAL the primary protocol primitive (RFC-0960 §1.1); `ConsensusSession` renamed to `ExecutionEnvelope` (RFC-0962 v2.0); "Consensus-Safe SQL" renamed to "Deterministic SQL" (RFC-0961 v2.0); five new database-ergonomic primitives added (Time Travel, Materialized Views, Event Store, Git-branches, Cost Model); Policy Object separation introduced (RFC-0967).
**Authors direction:** "pause quota-router side; capability-based vaults looks interesting; do our own research; this is strategically the most important part — the economic operating system for AI."
**Builds on:** `docs/research/2026-07-22-value-transfer-model-internal-landscape.md` (Phase 1 — internal scan)

---

## 0. The v2.0 inversion (strategic reframe, 2026-07-23)

> **Run your existing enterprise application unchanged while replacing the trust model underneath it.**

The architectural inversion that the rest of this document develops was sound for v1.0 but framed the stack too blockchain-centric. The v2.0 reframe (RFC-0960 §1, RFC-0962 v2.0, RFC-0961 v2.0, RFC-0967 v1.0) elevates the **Deterministic WAL** to the primary protocol primitive and pushes SQL, Sessions/Envelopes, Consensus, and ZK behind it as projections.

```text
Application
        │
        ▼
JDBC / SQL / ORM
        │
        ▼
Deterministic SQL Engine (RFC-0961 v2.0)
        │
        ▼
Deterministic WAL ← PRIMARY PROTOCOL OBJECT (RFC-0960 §1.1)
        │
        ├────────► Replication (OctoSync)
        ├────────► Consensus (certifies WAL segments)
        ├────────► ZK Proof Generation
        ├────────► Time Travel (RFC-0960 §14)
        ├────────► Materialized Views (RFC-0960 §15)
        ├────────► Event Stream / CQRS (RFC-0960 §16)
        ├────────► Git-style branches / merge (RFC-0960 §17)
        └────────► Resource Accounting (Cost Model, RFC-0960 §18)
```

Enterprise migration mapping (the "JDBC stays, trust model changes" story):

| Stays the same | Replaced |
|---|---|
| JDBC, SQL, stored procedures, ORM | Password → Capability (with `PolicyReference` caveat, RFC-0967) |
| Views, triggers, schemas, migrations | Replication → Consensus-certified WAL |
| Transactions | Audit → Immutable WAL-derived audit log |
| Reports | Time Travel → `AS OF block_height` queries (RFC-0960 §14) |
| Branch isolation (DB-level) | Branch isolation → Git-style WAL branches (RFC-0960 §17) |

This is the difference between "SQL on blockchain" and "**deterministic database with cryptographic trust**." The former is a smart-contract platform; the latter is the default migration target for enterprise systems over the next decade.

The remainder of this research document develops the v1.0 framing. The v2.0 inversion is layered on top of the same primitives — Vaults, Capabilities, Reservations, Execution Envelopes — and re-interprets them as projections of the WAL.

---

## 0. The original inversion (v1.0, 2026-07-22)

The mistake almost every blockchain makes: start from **money**.

```
Alice → transfer → Bob
```

CipherOcto's primary purpose is not currency. It is **delegating expensive computation**. The first-class object should not be a balance. The first-class object should be **authorization to consume scarce resources**.

User asks GPT. ↓ Router chooses provider. ↓ Provider spends GPU. ↓ Settlement occurs. ↓ Provider gets paid. Money is a consequence; the real asset is **permission to consume network resources.**

This inverts the standard chain:

| Asset-centric (most chains) | Constraint-centric (CipherOcto) |
|---|---|
| Asset | Resource |
| Account | Vault |
| Transfer | Policy |
| Balance | Capability |
| Tx | Reservation |
| Block | Settlement |
| State | Ledger |

---

## 1. The seven layers

1. **Resources** — compute, bandwidth, storage, inference, governance. Scarce physical or logical capacity.
2. **Assets** — OCTO, OCTO-A, OCTO-B, OCTO-S, OCTO-W. Accounting representation of resources.
3. **Vaults** — programmable containers that hold assets, scoped by Owner DID.
4. **Capabilities** — delegated rights to spend or reserve assets under explicit constraints (max, time, destination, purpose).
5. **Reservations** — temporary commitments (escrow/pre-auth) that bind a capability to an intended operation.
6. **Settlement** — cryptographically verified completion that consumes reservations and emits transfers.
7. **Ledger** — immutable event log (vault created, capability granted, reservation created, settlement completed, transfer applied). Balances are projections, not state.

---

## 2. Primitives

### 2.1 Vault

```text
Vault {
    vault_id:        VaultID,
    owner_did:       DID,
    token:           AssetID,         // OCTO, OCTO-W, OCTO-A, ...
    policy:          VaultPolicy,     // inherited by children
    current_state:   VaultState,      // Active | Frozen | Retired
    parent_vault:    Option<VaultID>, // for hierarchical vaults
    created_at:      Timestamp,
    metadata:        Metadata,
}
```

Semantic vault types — not just balances:

- Provider Vault
- Marketplace Vault
- Escrow Vault
- Treasury Vault
- Mission Vault
- Node Vault
- DAO Vault
- Liquidity Vault
- Compliance Vault
- Regional Vault

### 2.2 Capability

```text
Capability {     // extends RFC-0957 macaroon
    capability_id:  CapabilityID,
    issuer_did:     DID,
    holder_did:     DID,
    vault_id:       VaultID,
    constraints:    Vec<Constraint>,
    expires_at:     Timestamp,
    nonce:          Nonce,
    audit_window:   Option<Duration>,  // dispute period
    signature:      Signature,         // issuer sig over canonical form
}
```

Examples:

```text
May spend
  up to 50 OCTO-W
  until 2027-01-01
  only for Claude Sonnet
  only through Marketplace
  only for Mission X
  with audit window 24h
```

### 2.3 Reservation

```text
Reservation {
    reservation_id:  ReservationID,
    vault_id:        VaultID,
    capability_id:   CapabilityID,
    resources:       ResourceSpec,     // compute, tokens, bytes, ...
    amount:          MicroOCTO_W,
    expires_at:      Timestamp,
    state:           ReservationState,
    settlement_ref:  Option<SettlementID>,
}

ReservationState ∈ {
    Reserved,     // pre-auth holds the amount
    Executing,    // provider is working
    Settled,      // proof attached, awaiting audit window
    Auditable,    // inside dispute window
    Released,     // amount moved; terminal
    Expired,      // no settlement arrived before deadline
    Cancelled,    // explicit cancel by holder
    Frozen,       // dispute in progress
}
```

Reservations are first-class blockchain objects. Step 6 of the 11-step exercise becomes a real `Reservation` row, not a hash of a string.

### 2.4 Settlement

```text
Settlement {
    settlement_id:   SettlementID,
    reservation_id:  ReservationID,
    proof:           Proof,            // RFC-0959 settlement_hash + signature
    transfers:       Vec<Transfer>,    // emitted by this settlement
    timestamp:       Timestamp,
}
```

Settlement consumes reservations. Transfers are a consequence of settlement, never the primitive.

### 2.5 Transfer (consequence, not primitive)

```text
Transfer {
    transfer_id:    TransferID,
    settlement_id:  SettlementID,
    from_vault:     Option<VaultID>,  // None = mint
    to_vault:       Option<VaultID>,    // None = burn
    amount:         MicroOCTO_W,
    kind:           TransferKind,
    timestamp:      Timestamp,
}
```

---

## 3. Capability Spending Graph

Ethereum mixes ownership and spending. CipherOcto separates them.

```text
Alice
  ├── Mission A        20 OCTO-W   capability
  ├── Claude            50 OCTO-W   capability
  ├── GPT               10 OCTO-W   capability
  └── Daily Budget     100 OCTO-W   capability
```

Each node is a capability. Not a balance. Each capability carries its own constraints.

Owner never spends directly. Capabilities spend.

This is exactly how RFC-0957 macaroons work. The value layer mirrors the authorization layer. **One philosophy, two domains.**

---

## 4. Resource-Native Economy

Stop thinking tokens. Think resources.

```text
GPU seconds  → Compute Units      → OCTO-A
Bytes        → Bandwidth Units    → OCTO-B
GB-days      → Storage Units      → OCTO-S
Model tokens → Inference Units    → OCTO-W
```

The blockchain stores **resource consumption**, not arbitrary money movement. Money is the accounting layer; resources are the ground truth.

Each resource type has its own:

- Vault(s)
- Market
- Reservation shape
- Pricing oracle
- Settlement policy
- Shard (see §10)

---

## 5. Constraints as policy modules

Every classical token feature becomes a reusable `Constraint`. No new smart contracts. No new token types.

```text
Constraint ∈ {
    NotBefore(timestamp),
    UnlockAfter(block_height),
    LinearRelease { start, end, cliff },
    CliffVesting { until, pct, period },
    LiquidityLock { until },
    GovernanceLock { while_vote_active },
    MultiSig { n, signers },
    RateLimit { max_per_window, window },
    ComplianceHold { threshold, delay },
    RequireReceiptSignatureBy(did),
    MaxPerTx(amount),
    AllowedDestinations(set),
    DeniedDestinations(set),
    AllowIf { predicate },    // see §8 Economic VM
}
```

Reuse table:

| Need | Constraint | Today |
|---|---|---|
| Time lock | `NotBefore` / `UnlockAfter` | Bitcoin CLTV, Ethereum timelock contract |
| Vesting | `LinearRelease` | Sablier, OpenZeppelin VestingWallet |
| Cliff | `CliffVesting` | OpenZeppelin VestingWallet |
| Liquidity lock | `LiquidityLock` | Unicrypt, Team.Finance |
| Governance lock | `GovernanceLock` | Compound Governor, OpenZeppelin Timelock |
| Compliance hold | `ComplianceHold` | Manual multisig wallets |
| Multi-sig | `MultiSig` | Gnosis Safe |
| Rate limit | `RateLimit` | EIP-7702 daily limits, ERC-7715 wallet permissions |
| AI spend cap | `RateLimit` (per-window token counts) | not native anywhere |

One primitive, every feature.

---

## 6. Audit Window — extended settlement state machine

Replace binary Settled with a multi-state transition:

```text
Reserved
  ↓
Executing
  ↓
Settled         ← proof attached, transfers drafted
  ↓
Auditable       ← inside dispute window
  ↓
Released        ← terminal; transfers applied
  │
  └─→ Frozen     ← dispute filed
        ↓
      Dispute
        ↓
      Rollback  or  Uphold
```

If fraud is discovered inside the audit window:

```text
Settled → Frozen → Dispute → Rollback (transfers never applied)
```

This is hard on account-based ledgers. Natural on event-sourced ledgers where settlement is already a first-class state machine.

The `audit_window` field on `Capability` and `Reservation` controls the dispute period. 0 = no audit window = instant release (high trust). 24h default for AI marketplace settlements. 7d default for treasury vaults.

---

## 7. Atomic Swaps and Cross-Chain

Don't think bridges. Think multi-settlement.

```text
MultiSettlement {
    id,
    participants: [
        { chain: "Ethereum",  reservation: R_eth,  proof: HTLC_preimage },
        { chain: "Bitcoin",   reservation: R_btc,  proof: witness },
        { chain: "CipherOcto", reservation: R_octo, proof: settlement_hash },
    ],
    completion: AllRequired,
}
```

Completion requires every proof. All or nothing. No bridge contract, no wrapped asset, no custodian.

Cross-chain capability:

```text
Capability {
    ...
    secured_by: CrossChainBacking::BitcoinHTLC { ... }
    // OR
    secured_by: CrossChainBacking::EthereumProof { ... }
}
```

A capability can carry proof that its authority is itself backed by an external chain's lock. The CipherOcto capability IS the cross-chain primitive — not a bridge, but a delegation backed by an external proof.

---

## 8. Economic Virtual Machine (not EVM)

A **declarative, deterministic, loop-free** policy language. Not Turing-complete. Not a smart-contract platform.

```text
ALLOW
  spend up_to 50 OCTO-W
  IF
    time > cliff
    AND reputation > 900
    AND remaining_budget > cost
    AND gpu_available
    AND price <= oracle_price * 1.05
    AND counterparty in allowlist
```

Properties:

- No loops, no recursion, no arbitrary storage
- Deterministic (provable by construction; ZK-friendly)
- Compiles to RFC-0126 canonical_ser
- Verifier is a small state machine — cheap to run in router, gateway, or ZK circuit
- Bounded evaluation cost → no DoS via expensive policies

This is the missing piece: not a programming language for arbitrary logic, but a **policy language for economic constraints**. Every constraint in §5 is a builtin; complex AND/OR/NOT compositions are user-authored.

The EVM name clash is intentional: Economic VM, not Ethereum VM.

---

## 9. Event-Sourced Ledger (no mutable balances)

Reject mutable balance rows as canonical state. Use an append-only event log:

```text
VaultCreated
CapabilityGranted
CapabilityAttenuated
CapabilityExpired
CapabilityRevoked
ReservationCreated
ReservationUpdated        // state transitions
SettlementCompleted
TransferApplied
DisputeOpened
DisputeResolved
VaultFrozen
VaultRetired
```

Balances are **projections** computed from events. Like modern CQRS systems.

Advantages:

- Perfect audit (every state change is an event)
- Deterministic replay (genesis to head recomputes identical projections)
- ZK-friendly (event log is a Merkle chain; prove balance ∈ [a,b] without revealing events)
- Sync-friendly (RFC-0862 + OctoSync ship event batches; no UPDATE conflicts)
- Rollback-friendly (revert last N events during dispute resolution)

This is structurally a CRDT: each vault's balance is `+ins - outs` over events, replicated and projected independently on each node.

The `octo_w_balances` table that exists today is a cache (projection) of the event log, not the source of truth. The Phase 1 finding ("saturating_sub on `Balance::deduct`") is the bug you get when the cache pretends to be the source.

---

## 10. Horizontal scalability — resource sharding

Most blockchains scale transactions. CipherOcto should scale **resources**.

```text
GPU shard           Storage shard         Bandwidth shard
Inference shard     Governance shard      Settlement shard
```

Each shard only processes reservations and settlements relevant to its resource. The ledger is horizontally partitioned by resource type, provider, market, mission, or geography.

Concretely:

- Each resource type has its own `events_<resource>` table partition
- Each shard publishes its own Merkle root
- Cross-shard settlements are `MultiSettlement` (see §7) — atomic across shards via proof composition
- Shard routing by capability `vault.token`

Result: the bottleneck is no longer "how many token transfers can the chain process," but "how many independent resource commitments can the network coordinate in parallel."

---

## 11. Hierarchical Vaults

```text
Global Treasury
  └─ Regional Treasury (Americas)
       └─ Marketplace Vault (US-East)
            └─ Provider Vault (OpenAI shadow)
                 └─ Mission Vault (gpt-4-eu-prod)
                      └─ Task Vault (batch-2026-07-22)
                           └─ Capability (Claude, 50 OCTO-W, daily)
                                └─ Reservation (req-12345)
                                     └─ Settlement (proof-abc)
```

Each layer inherits policy. A child vault can never violate the parent (capability-security lattice).

Examples:

- Treasury cap = sum across children ≤ parent limit
- Compliance threshold cascades downward
- Vesting cliff at root propagates to descendants

This is closer to capability security (KeyKOS, E, Capsicum) than to banking.

---

## 12. The compatibility layer — Consensus Sessions

The billion-dollar opportunity: **preserve the enterprise programming model, replace only the trust model.**

### 12.1 The problem

Enterprise applications assume:

```
Login → Session → Many operations → Logout
```

Blockchain forces:

```
Key → Sign → One transaction → Forget everything
```

The mental models are opposite. Every line of business logic, every ORM, every stored procedure, every framework is built around the session model. That is why ERP migrations to blockchain never happen.

### 12.2 The proposal

Login still exists. But it creates a **Capability Session** instead of a server session.

```text
Login
  ↓
OIDC | LDAP | Kerberos | SAML | OAuth
  ↓
Identity Verified
  ↓
Capability minted  (per RFC-0957)
  ↓
Capability Session  (in-memory, ephemeral)
  ↓
SQL Statements     (execute under capability)
```

SQL executes under capabilities instead of passwords.

### 12.3 Deterministic SQL Engine

Stoolap already gives us DDL, indexes, views, foreign keys, window functions, joins. Add a `CONSENSUS_SAFE` mode that forbids:

```text
NOW()         RAND()        HTTP()       FILE()
current_time  uuid_random  net.http     file.read
```

… and allows everything else. Deterministic by construction. Matches the numeric RFCs (RFC-0102, RFC-0104, RFC-0110, RFC-0126, RFC-0127).

### 12.4 Stored Procedures survive

```text
CREATE DETERMINISTIC PROCEDURE CloseMonth()
    deterministic SQL only
    deterministic functions only
    deterministic ordering
    deterministic timestamps
    deterministic randomness
```

The procedure itself survives. No Solidity rewrite.

### 12.5 ORMs and JDBC work

```text
Hibernate        Entity Framework     Diesel
SQLAlchemy       Django ORM           Prisma
```

… keep working because the SQL endpoint still exists. Add a `jdbc:cipherocto://cluster` driver that wraps a `Connection` over a `Capability Session` and signs the WAL.

### 12.6 WAL as Transaction

Instead of signing each `UPDATE inventory`, sign the **WAL block**:

```text
LSN 1000
  → 100 SQL operations
  → Hash
  → One signature
  → Consensus
```

One signature for 100 statements. Application keeps session semantics. Consensus only sees immutable WAL segments (RFC-0862 + OctoSync).

### 12.7 Identity Translation Gateway

```text
Legacy Identity
  │
  ├── LDAP            → Capability
  ├── Active Directory → Capability
  ├── Kerberos        → Capability
  ├── SAML            → Capability
  ├── OAuth / OIDC    → Capability
  └── mTLS / SSH key  → Capability
```

Or, even better, emulate **services** not users:

```text
SAP         → Capability
Oracle ERP  → Capability
CRM         → Capability
Warehouse    → Capability
```

Systems become first-class actors. Exactly how enterprise systems actually work.

### 12.8 Compatibility Levels

```text
Level 1  ANSI SQL
Level 2  PostgreSQL-compatible
Level 3  Enterprise (Oracle/SAP extensions)
Level 4  Deterministic Blockchain (CONSENSUS_SAFE)
```

Migrate incrementally. Each level is a superset of the previous.

### 12.9 Consensus Session object

```text
ConsensusSession {
    session_id:        SessionID,
    capability:        CapabilityID,
    sql_statements:    Vec<CanonicalSQL>,    // deterministic_replay list
    stored_procs:      Vec<ProcInvocation>,
    ddl_changes:       Vec<DDLOperation>,
    wal_segment_hash:  Hash,                 // RFC-0862 segment commitment
    signature:         Signature,            // capability holder signs
    timestamp:         Timestamp,
}
```

One signed session object in the ledger. Internally many SQL ops. Externally indistinguishable from a regular database session to the application.

---

## 13. The Resource Graph

Every economic object is a node. Every relationship is a cryptographically linked edge. Every state transition is deterministic.

```text
Resource
   ↓
Vault
   ↓
Policy (constraints)
   ↓
Capability
   ↓
Reservation
   ↓
Settlement
   ↓
Transfer
   ↓
Ledger Event
```

Append-only. ZK-friendly. Sync-friendly. Replay-deterministic.

This graph is the new primitive. Not a UTXO set. Not an account state. A DAG of immutable economic events, each node carrying its own authority proof (the capability signature that produced it).

---

## 14. Strategic positioning

Stop calling CipherOcto "another blockchain with AI features."

The architecture here is closer to a **deterministic resource coordination network**:

- Blockchain = consensus substrate
- Primary abstraction = lifecycle of scarce resources
- Tokens = accounting representation of resources

This naturally accommodates:

- Time locks, vesting, lockups, liquidity locks → reusable constraints
- Atomic swaps, cross-chain → multi-settlement protocols
- Audit windows, disputes, delayed release → settlement state machine
- Massive horizontal scalability → resource shards + append-only events

The bottleneck shifts from "transfers per second" to "independent resource commitments per second" — a much better fit for decentralized AI infrastructure.

---

## 15. Open research questions for next phases

| Phase | Topic | Output |
|---|---|---|
| 3 | External capability-based spend systems (Ethereum ERC-7715, Starknet AA plugins, Aztec notes, Sui owned-objects, MACI) | design synthesis; minimum viable `Constraint` set |
| 4 | Event-sourced ledger precedents (Datomic, EventStoreDB, Kafka + projections, Cosmos SDK event-sourcing) | pitfalls + proven patterns |
| 5 | Enterprise migration playbooks (PostgreSQL logical replication → CipherOcto DDL; SAP RFC adapters) | compatibility-level-by-level guide |
| 6 | Deterministic SQL: classify which standard functions are consensus-safe vs forbidden | RFC candidate |
| 7 | Consensus Session object: protocol design + ZK circuit for batch signature | RFC candidate |
| 8 | Resource shard routing policy | RFC candidate |
| 9 | Synthesize 3-8 into one or more grand-design RFCs (numbered RFC-0960+, RFC-0970+) | RFC drafts |

---

## 16. Status

This doc = grand-design synthesis from user-supplied conversation. No code changed. No RFC drafted yet.

**Next action:** user picks Phase 3-9 (or all sequentially). Phase 3 (external capability-based spend systems) is the natural next step — tests the `Constraint` set against production systems, identifies the minimum-viable policy surface.
