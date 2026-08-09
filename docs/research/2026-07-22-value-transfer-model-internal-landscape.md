# Value Transfer Model — Internal Landscape

**Date:** 2026-07-22
**Status:** Research Phase 1 of N — internal scan (per user direction 2026-07-22)
**Author direction:** "pause quota-router side, keep deferred; do multiple researchs on current models other solutions use so we have something extra, a breakthrough on our own solution. Capability-based vaults looks interesting but we should do our own research."
**Next phases:** External research (UTXO / account / capability-vault models from other systems) — separate docs after this one.

---

## 0. Why this research exists

Cipherocto is a blockchain (master plan §2; not conventional — SQL + stoolap + RFC-0862 sync as propagation). It currently lacks two tables that every blockchain needs:

1. **A `transfers` table** — for moving value between DIDs (holders, providers, governance, treasuries)
2. **An `escrow` table** — for Step 6 of the 11-step exercise ("OCTO-W escrow pre-auth") which is currently a `blake3::hash(b"escrow/v1")` placeholder

RFC-0959 v1.0 (Accepted 2026-07-20) spec'd the **settlement receipt** (proof that a settlement completed). It did NOT spec the **value flow** that settlement triggers. Per master plan §7:

> On-chain settlement integration (waits for RFC-0959 v1.0 Accepted + on-chain binding per future RFC after RFC-0955 fiat ramp stabilizes)

So this is the gap. Before designing, scan what we already know.

---

## 1. What we already have

### 1.1 Whitepaper claims (docs/01-foundation/whitepaper/v0.1-draft.md)

| Token | Role | Source line |
|-------|------|-------------|
| OCTO-D / OCTO-A | AI Agents | L156 |
| OCTO-A | Compute (Providers) | L157 |
| OCTO-B | Bandwidth | L158 |
| OCTO-S / OCTO-H | Storage | L159 |
| OCTO-W | AI Access Quotas (resale market) | L160, L270 |
| OCTO-M | Marketing Reach | L161 |
| OCTO-O | Orchestration Services | L162 |
| OCTO | Governance (sovereign) | L163 |

Dual-stake model (L1921-): "Actors must stake" — both OCTO + a role token. Prevents role tourism.

**Quoted invariants:**
- "AI Access Quotas → AI Access Resale (OCTO-W)" — OCTO-W is the **resale unit** (L859)
- "Actors are bounded by: stake requirements, reputation penalties, permission scopes, audit trails, cryptographic accountability. Misbehaving agents lose economic viability." (L1390-)
- "Democracy of Contribution, not Democracy of Capital." (L1428) — voting power = stake + reputation + role participation + time, not tokens alone
- Bicameral governance: OCTO Holders Assembly + (other chamber)

**What's NOT specified:** how OCTO-W moves between holders and providers, how the escrow for Step 6 is held, what gates a transfer's validity beyond signature.

### 1.2 RFCs that touch value flow (existing claims + gaps)

| RFC | Status | Touches | Defines | Gap |
|-----|--------|---------|---------|-----|
| RFC-0904 | Accepted | Real-time cost tracking | Spend counter | No transfer log; per-key not per-DID; saturating-sub silent bug |
| RFC-0934 | Accepted | Budget + spend | `budgets` table, atomic check-and-increment | Same — counter, not ledger |
| RFC-0102 | Accepted | Wallet crypto | `struct Transfer { sender, receiver, token, amount }` sketch only | Mission acceptance criterion "Token transfer (OCTO-W)" listed L470 — never implemented |
| RFC-0957 | Accepted | Capability token (macaroon) | **Assumes escrow oracle exists** ("EscrowDischargeProvider checks buyer OCTO-W escrow balance ≥ AmountMax", L385) | Defines the *channel* but not the escrow itself — circular dependency |
| RFC-0959 | Accepted | Independent settlement chain | Settlement receipt + consumed_receipt_index | Receipt only — no value movement; explicitly notes "On-Chain Settlement Receiver" needs future RFC (L463) |
| RFC-0955 | Draft | Model Liquidity Layer (fiat ramp) | `TransferOwnership { transfers: Vec<OwnershipTransfer> }` sketch only | Fiat-ramp scope; defers to "future participants" |
| RFC-0958 | Accepted | ZK capability subclass | ZK proof binding for capability tokens | No value layer touched |
| RFC-0126 | Accepted | Deterministic serialization | Canonical field ordering for `balance: DQA` example | Substrate, not policy |
| RFC-0853 | Accepted | Overlay crypto | Section 6 references "mission key per (asker, model)" — implemented in `octo-wallet::key_hierarchy` (Phase E closed 2026-07-22 via `e139a898`) | Per-mission key, not value vault |
| RFC-0630 | Accepted | Proof-of-Inference consensus | "Execute transfers" referenced at L499 | Inference-reward distribution — assumes a transfer primitive |

**Critical circular dependency (RFC-0957 §discharge):**
> Channel provider evaluates its own predicate (escrow balance, revocation status, rate budget). (L392)

The escrow channel PROVIDER must know the escrow balance. But the escrow balance lives in a table that hasn't been spec'd yet. RFC-0957 assumes it; this research exists to fill it.

### 1.3 Code that exists today

| File | What it has | What's missing |
|------|-------------|----------------|
| `crates/quota-router-core/src/balance.rs` | In-memory `Balance { amount: u64 }` with `check`/`deduct`/`add`. **saturating_sub on deduct** (L27) — silently allows over-spend. | No persistence, no transfers, no multi-account |
| `crates/quota-router-core/src/schema.rs:182` | `octo_w_balances(key_id TEXT UNIQUE, balance INTEGER DEFAULT 0, updated_at INTEGER)` | Keyed by `key_id` not DID — entity = API key not agent; single-account debit; no transfers log; no escrow; no role tokens |
| `crates/quota-router-core/src/storage.rs:1062-1098` | `get_octo_w_balance`, `deduct_octo_w` impls (SQL: `UPDATE ... SET balance = balance - $2 WHERE balance >= $2`) | Atomic decrement, but no audit trail — who debited for what ask? No link to RFC-0959 settlement_hash |
| `crates/octo-wallet/src/capability/discharge.rs` | `Channel::Escrow` enum variant + "escrow" string ID; discharge macaroon for escrow channel | Channel-side plumbing only; no oracle impl; no underlying escrow table |
| `crates/quota-router-sm-engine/` (Phase D, just shipped) | `asks` + `consumed_receipt_index` tables; settlement state machine | Settlement state, not value state — `settle()` doesn't move OCTO-W, only records receipt |
| `crates/octo-wallet/src/key_hierarchy.rs` (Phase E, just shipped) | Per-(asker, model) mission keys | Crypto isolation, not value isolation |

### 1.4 The 11-step exercise (master plan §6)

Step 6 "OCTO-W escrow pre-auth" is currently `blake3::hash(b"escrow/v1")` — literally a string hash. No row gets written. **The exercise passes green but doesn't actually exercise an escrow.** This is the most concrete evidence that the table is missing.

### 1.5 docs/research/ scan results

No prior internal research on:
- account-model vs UTXO-model choice
- capability-based vaults
- MACI-style spending policies
- escrow patterns
- multi-token ledger designs

`docs/research/wallet-technology-research.md` mentions "Balance transfer ✅ Standard ERC-20" (L233) but as a one-line comparison cell, not a design study.

`docs/research/ai-quota-marketplace-research.md` L29: "Router ... routes prompts based on policy and balance" — assumes balance exists.

`docs/research/pricing-axes-research.md` and `docs/research/any-llm-vs-litellm-comparison.md` describe LiteLLM's per-key spend tracking (counter, not ledger) — they don't go deeper.

---

## 2. The gap matrix

| Need | Whitepaper | RFCs | Code | Status |
|------|-----------|------|------|--------|
| Per-DID OCTO-W account | implied (resale market) | RFC-0934 partial (per-key) | `octo_w_balances` per-key | **GAP** — no DID-keyed accounts |
| Value transfer between DIDs | implied (resale, settlement) | RFC-0102 sketch only | none | **GAP** — no `transfers` table |
| Escrow hold/release | implied (Step 6) | RFC-0957 channel assumes it | none | **GAP** — no escrow primitive |
| Multi-token (role tokens) | explicit (whitepaper §Token role table; §Dual-stake model) | RFC-0955 partial | none | **GAP** — OCTO-A/B/D/etc. not in any schema |
| Link transfer ↔ settlement_hash | implied | RFC-0959 spec'd cost but not movement | none | **GAP** — no FK between receipts and transfers |
| Capability-gated spend | NOT addressed | RFC-0957 channel layer exists | discharge.rs has the channel | **GAP** — no per-capability spending policy |
| Negative-balance defense | implied (misbehaving agents lose viability) | not addressed | `saturating_sub` L27 silently underflows | **BUG** |
| Audit trail | "audit trails" L1390 | not addressed | none | **GAP** — no per-transfer event log |
| Sync-propagation story | RFC-0862 sync | RFC-0862 sync | `stoolap-data-sync-via-cipherocto-network.md` | **READY** — sync layer can carry transfer rows |

---

## 3. Candidate shapes (from inside cipherocto)

Three plausible models derived from what we already have, not from external research:

### Option A — Account model (SQL `accounts` + `transfers`)

**Shape:**
```
accounts(did BLOB PRIMARY KEY, balance_micro INTEGER NOT NULL CHECK (>= 0),
         updated_at INTEGER NOT NULL)
transfers(transfer_id BLOB PRIMARY KEY, from_did BLOB NULL, to_did BLOB NULL,
          amount_micro INTEGER NOT NULL, kind TEXT NOT NULL,
          settlement_hash BLOB NULL, ask_id BLOB NULL, escrow_id BLOB NULL,
          timestamp INTEGER NOT NULL, signature BLOB NOT NULL)
```

Mint: `from_did=NULL`. Burn: `to_did=NULL`. Pre-auth: `kind='escrow_hold'` + `to_did=NULL` + escrow row holds the amount.

**Pros:** maps naturally to existing `octo_w_balances` shape (per-DID row + amount); CHECK constraint enforces non-negative; transfers table is the audit trail the whitepaper asks for; RFC-0862 sync just replicates rows.

**Cons:** hot row for popular DIDs (mitigated by per-token separate tables); `CHECK (>=0)` requires SQL-layer support — need to verify stoolap honors it.

### Option B — UTXO model (append-only `unspent_transfers`)

**Shape:**
```
unspent_transfers(utxo_id BLOB PRIMARY KEY, owner_did BLOB NOT NULL,
                  amount_micro INTEGER NOT NULL, kind TEXT NOT NULL,
                  spent_in_transfer_id BLOB NULL,
                  created_settlement_hash BLOB NULL, created_at INTEGER NOT NULL)
transfers(transfer_id BLOB PRIMARY KEY, inputs UTXO_LIST, outputs UTXO_LIST,
          timestamp INTEGER NOT NULL, signatures ...)
```

Mint creates unspent UTXO with `spent_in_transfer_id=NULL`. Transfer references input UTXOs + creates new output UTXOs.

**Pros:** append-only = RFC-0862 sync trivial; no balance row = no hot row; better privacy (no global balance view per DID); whitepaper says "infrastructure self-balances" — UTXO self-balances by construction.

**Cons:** Step 6 escrow doesn't fit cleanly (UTXO is spend-or-not, not hold); needs UTXO set + spent-set tracking per node; heavier than current code; doesn't match RFC-0957's "balance ≥ amount" discharge semantics.

### Option C — Append-only event log + recomputable balance

**Shape:**
```
transfer_events(seq INTEGER PRIMARY KEY, transfer_id BLOB, from_did BLOB NULL,
                to_did BLOB NULL, amount_micro INTEGER, kind TEXT,
                settlement_hash BLOB NULL, ask_id BLOB NULL, escrow_id BLOB NULL,
                timestamp INTEGER NOT NULL, signature BLOB NOT NULL)
```

No `accounts` table. Balance = `SUM(in) - SUM(out) - SUM(active escrow holds)` over events, recomputed on read or cached as a snapshot.

**Pros:** matches cipherocto's "everything is a log + sync" pattern perfectly (RFC-0862 is a log-shipping protocol); immutable history = whitepaper "audit trails" satisfied; canonical replay from genesis gives same balance everywhere.

**Cons:** every balance read = full event scan (mitigated by `idx_transfer_events_by_did` materialised index); needs a checkpoint/snapshot mechanism for performance; Step 6 escrow = holds appear as events with `kind='escrow_hold'`, release as `kind='escrow_release'` with FK.

### Option D — Capability-bound vault (the breakthrough candidate)

**Shape:** every spend requires a `CapabilityToken` (RFC-0957 macaroon) that includes a `SpendPolicy`:

```
struct SpendPolicy {
    max_per_epoch: MicroOCTO_W,
    max_per_tx: MicroOCTO_W,
    allowed_destinations: HashSet<DID>,   // optional allow-list
    denied_destinations: HashSet<DID>,   // optional deny-list
    requires_receipt_signature_by: Option<DID>,  // co-signer
    expires_at_unix: u64,
    can_hold_escrow: bool,
}
```

The holder signs a transfer; the verifier checks: (1) holder signature valid; (2) holder's balance ≥ amount; (3) capability token's SpendPolicy allows this transfer; (4) if `requires_receipt_signature_by` is set, the co-signer's RFC-0959 settlement receipt is attached.

**Pros:**
- Maps the whitepaper's "permission scopes" + "cryptographic accountability" (§Cryptographic Accountability) onto actual spend authority
- Maps RFC-0957's existing discharge-channel model — instead of "escrow oracle checked balance", the capability IS the escrow (it carries the policy)
- Lets a holder delegate spend to a sub-agent with a tighter policy (e.g., a daily limit) without moving actual tokens
- Naturally ties to Phase E mission keys (per-mission per-(asker, model) keys already exist) — a capability can be tied to a mission
- Step 6 escrow becomes "transfer with `kind='escrow_hold'` AND `capability_token.spend_policy.can_hold_escrow=true`" — escrow is policy-gated, not special-cased
- Direct match to user-flagged "capability-based vaults" direction

**Cons:**
- Requires extending RFC-0957 to include `SpendPolicy` (was previously only caveat-based attenuation)
- Needs a capability-verifier that runs at every spend site (router, governance, marketplace)
- SpendPolicy semantics need cross-impl test vectors (already a pattern from RFC-0957 TV files)
- Doesn't replace the underlying ledger model — D could compose with A, B, or C

---

## 4. Open questions that need external research

These I cannot answer from internal scan; need external research phases:

1. **Account vs UTXO vs event-log tradeoffs** — what do other "SQL-as-ledger" systems (not traditional L1s) actually use? Cosmos SDK? Diem/Aptos? Sui's object model? Sable (if it exists)?
2. **Capability-based spend authorities** — have any other systems shipped a production capability-bound vault? Ethereum ERC-7715 (wallet permissions)? Aztec's private spend notes? Starknet's account abstraction (any plugin implementing spend caps)?
3. **MACI (Minimal Anti-Collusion Infrastructure)** — relevant for the "Democracy of Contribution" (whitepaper L1428) voting power. Does MACI apply to spend authority too?
4. **Off-chain enforcement with on-chain dispute** — does any pattern combine capability-local enforcement with later on-chain challenge? Optimistic settlement + capability-attested pre-auth?
5. **Dual-stake slashing** — whitepaper says dual-stake (OCTO + role token). What slashing model? Account debit (forfeit from balance) vs UTXO burn (destroy specific stake UTXOs) vs role-token-revocation (separate from OCTO)?
6. **Fiat ramp (RFC-0955) interop** — when a fiat-on-ramp mints OCTO-W for a user, that mint is a `from_did=NULL, to_did=user` transfer. Does that need a different signature scheme than a normal transfer?
7. **Capability revocation latency** — capability is a macaroon, locally verifiable. But if a holder's capability is revoked, how does that propagate? Local-only check means a stale-capability spend could succeed before the verifier learns of revocation. Sync story?

---

## 5. Where the breakthrough candidate sits

Per user direction: "something related with capability-based vaults looks interesting but we should do our own research."

My hypothesis (to be tested by external research phases):

> **The breakthrough is not picking a ledger model — it's recognizing that for cipherocto's use case, the spend authority IS the capability token, and the underlying ledger is just a constraint oracle.**

Concretely:

- Step 6 escrow = `escrow_hold` transfer whose signature comes from a `CapabilityToken` with `can_hold_escrow=true`. The capability IS the escrow contract.
- Step 11 reputation delta = `kind='reward'` transfer whose destination DID is gated by the receiver's `CapabilityToken` (proof of capacity to perform work).
- Dual-stake = the actor's "compute capability" and "compute role-token capability" must both be valid and unexpired for them to receive a compute-reward transfer.
- On-chain dispute (future) = a transfer can be challenged within T seconds; the challenge must attach a `CapabilityToken` from a higher-authority (governance) that revokes the original.

If this holds, the "table for token and money transfers" (user's framing) is structurally simple — option A is sufficient — but the *policy engine* on top of it is where the breakthrough lives. The policy engine = RFC-0957 extended with `SpendPolicy` + RFC-0959 receipts as proof artifacts.

This is the hypothesis. External research phases will test it against:
- Starknet account abstraction (which already does capability-style spend limits at the contract level)
- Aztec private spend notes (capability-attested confidential transfers)
- Diem/Aptos wallet-level spend policies
- Sui's object-capability model (capabilities ARE the assets)
- MACI minimal anti-collusion (if spend authority needs anti-collusion)

---

## 6. Open research questions for next phases

| Phase | Topic | Output |
|-------|-------|--------|
| 2 | External ledger models — what "SQL-as-ledger" systems (Cosmos SDK, Diem/Aptos, Sui objects, Sable, etc.) actually ship | comparison matrix + lessons learned |
| 3 | External capability-based spend systems (Ethereum ERC-7715, Starknet AA plugins, Aztec notes, Sui owned-objects, MACI) | design synthesis; identify minimum viable SpendPolicy |
| 4 | Capability revocation + sync propagation patterns (how do systems handle stale capabilities?) | propose revocation model |
| 5 | Dual-stake slashing patterns (what models exist?) | propose slashing model |
| 6 | Combine 2-5 into a single RFC (RFC-0960 candidate) | draft RFC |

---

## 7. References

Internal:
- docs/01-foundation/whitepaper/v0.1-draft.md (§Token role table; §Dual-stake model)
- rfcs/accepted/economics/0904-real-time-cost-tracking.md (§counter model)
- rfcs/accepted/economics/0934-budget-management-spend-tracking.md (§per-key budget)
- rfcs/accepted/economics/0957-capability-token-format.md (§escrow oracle assumption)
- rfcs/accepted/economics/0959-ask-settlement-chain.md (§settlement receipt)
- rfcs/accepted/numeric/0102-wallet-cryptography.md (§Transfer struct sketch)
- rfcs/draft/economics/0955-model-liquidity-layer.md (§TransferOwnership sketch)
- crates/quota-router-core/src/balance.rs (§in-memory saturating_sub bug)
- crates/quota-router-core/src/schema.rs (§octo_w_balances table)
- crates/quota-router-core/src/storage.rs (§get/deduct impls)
- crates/octo-wallet/src/capability/discharge.rs (Channel::Escrow)
- crates/octo-wallet/src/key_hierarchy.rs (Phase E mission keys)

External (to be researched in subsequent phases):
- TBD — see §6 phase list

---

## 8. Status

This doc = internal landscape only. No external research cited. No RFC proposed. No code changed.

**Next action:** user picks which Phase (2-6) to tackle next, or gives the green light to do them in order.
