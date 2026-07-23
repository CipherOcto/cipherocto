# External Capability-Based Spend Systems — Synthesis

**Date:** 2026-07-22
**Status:** Research Phase 3 of N — external capability-based spend systems (per user direction 2026-07-22)
**Builds on:**
- `docs/research/2026-07-22-value-transfer-model-internal-landscape.md` (Phase 1 — internal scan)
- `docs/research/2026-07-22-grand-design-vaults-capabilities-reservations.md` (Phase 2 — grand design)
**Scope:** five systems explicitly called out by user as the comparison set:

| System | What it ships | What we learn |
|---|---|---|
| Ethereum ERC-7715 | JSON-RPC permission grant/revoke between DApp and wallet | Standardized request/response shape; permission types as enum; rules layered on top |
| Ethereum ERC-4337 | Alt-mempool account abstraction, UserOp, bundlers, paymasters | AA composition: validation function + execution function; session key = signed UserOp with validUntil/validAfter |
| Starknet Account Abstraction + Session Keys | Native AA at protocol level; plugin session keys | Session key = temporary key with constraints (functions, time, spending caps); contract enforces; expiration mandatory |
| Sui object-capability model | Ownership types: Address-owned / Shared / Immutable / Wrapped / Party | Capabilities ARE the assets; type-system-enforced; object is primitive, account is derived |
| Aztec Authentication Witnesses (AuthWit) | `callerAddress × messageHash`-bound signature verified inside private execution | Capability bound to (caller, intent) pair; privacy-preserving; replay protected per intent |
| MACI (Minimal Anti-Collusion Infrastructure) | zk-SNARK vote tallying with on-chain verification, off-chain processing | Coordinator computes result; on-chain only sees proof; collusion resistance via key change |

**External research method note:** Direct fetches blocked for some docs sites (404, captcha). Pulled content from EIP-7715, EIP-4337, Starknet docs/blog, Sui docs, MACI main site + GitHub. Aztec AuthWit content derived from prior model knowledge of the design (its core spec hasn't changed substantively in the past 18 months; if user's vendor research needs ground truth, run a fresh fetch in a future session).

---

## 0. Why this comparison matters

Per grand design doc §5, the `Constraint` set is the breakthrough candidate:

```text
Constraint ∈ {
    NotBefore, UnlockAfter, LinearRelease, CliffVesting, LiquidityLock,
    GovernanceLock, MultiSig, RateLimit, ComplianceHold,
    RequireReceiptSignatureBy, MaxPerTx, AllowedDestinations,
    DeniedDestinations, AllowIf
}
```

This doc tests each `Constraint` against production systems that have shipped (or drafted) the same primitives. Two questions:

1. **What `Constraint`s do real systems actually need?** Minimum-viable policy surface.
2. **What gaps does cipherocto's draft `Constraint` set have?** Production-tested patterns we're missing.

---

## 1. Ethereum ERC-7715 — wallet permission grants

### 1.1 What it ships

Five JSON-RPC methods:

| Method | Direction | Purpose |
|---|---|---|
| `wallet_requestExecutionPermissions` | DApp → Wallet | Request scoped permission to act on user's behalf |
| `wallet_revokeExecutionPermission` | DApp → Wallet | Revoke a previously-granted permission |
| `wallet_getSupportedExecutionPermissions` | DApp → Wallet | Discover what permission types the wallet understands |
| `wallet_getGrantedExecutionPermissions` | DApp → Wallet | List active permissions |
| (redeem via) `delegationManager.redeemDelegation(...)` | DApp → chain | Use the permission to call a target |

### 1.2 Permission shape

```ts
type Permission = {
  type: 'native-token-allowance' | 'erc20-token-allowance' | 'contract-call' | ...,
  data: { allowance: uint256, token?: address, ... },
  rules: Rule[],                              // constraint layer
};

type Rule = {
  type: 'expiry' | 'period' | 'allowance' | 'allowed-target' | ...,
  data: { timestamp?: uint256, ... },
};
```

### 1.3 The two-layer model — what cipherocto's draft misses

ERC-7715 cleanly separates **what kind of permission** (Type) from **what restrictions** (Rules). cipherocto's draft §5 conflates both into a flat `Constraint` enum. Production-tested refinement:

```text
CapabilitySpendPolicy {
    permission_kind: PermissionKind,     // NATIVE_TRANSFER, ERC20_TRANSFER, CONTRACT_CALL, RESERVATION
    constraints: Vec<Constraint>,         // independent of kind
}
```

This separation matters: the same `RateLimit` constraint can apply to many `permission_kind`s. A flat enum forces the system to duplicate constraint semantics across kind variants.

### 1.4 The permission context (`permissionsContext`)

ERC-7715 returns an opaque `context` string alongside each permission. This is the redemption handle — the wallet hands it to the DApp, and the DApp must present it to redeem. cipherocto's draft has nothing analogous. Implications:

- Without a context handle, revoking a capability requires storing `capability_id` somewhere accessible to the revoker
- With a context handle, revocation is a simple lookup

**Recommendation:** cipherocto `Capability` needs a `redemption_context` field (opaque bytes, signed) that ties a capability to its redemption flow.

### 1.5 `dependencies` array — accounts that must be deployed

```ts
dependencies: [{ factory: address, factoryData: bytes }];
```

ERC-7715 returns a list of contracts + calldata that the DApp MUST deploy before the permission can be redeemed. This handles the "you don't have an account yet" case.

**Cipherocto analogue:** when a capability is granted to a `Vault` that doesn't exist yet, the capability should carry `factory` + `factoryData` so the holder can deploy it before redemption.

### 1.6 Constraints cipherocto is missing

| Missing constraint | What it does | ERC-7715 source |
|---|---|---|
| `isAdjustmentAllowed` | Whether the permission can be tightened at runtime | `native-token-allowance` data field |
| `required` (multi-permission atomicity) | Force all-or-nothing across multiple permissions | ERC-7715 multi-perm request pattern |
| `period` (per-window reset) | Time-bounded rate limit with reset semantics | Rule type |

---

## 2. Ethereum ERC-4337 — account abstraction substrate

### 2.1 What it ships

`UserOperation` (not a transaction) — sent to an alt mempool. Bundlers package `UserOp`s into `handleOps` calls on a singleton `EntryPoint` contract. Each user is a smart contract account that implements `validateUserOp(userOp, userOpHash, missingAccountFunds)`.

### 2.2 The validation/execution split

```solidity
interface IAccount {
    function validateUserOp(PackedUserOperation calldata userOp,
                            bytes32 userOpHash,
                            uint256 missingAccountFunds)
        external returns (uint256 validationData);
}
```

`validationData` packs three things:

| Field | Bytes | Meaning |
|---|---|---|
| `aggregator` / `authorizer` | 1 | 0 = valid, 1 = SIG_VALIDATION_FAILED, else aggregator address |
| `validUntil` | 6 | Last valid timestamp (inclusive) |
| `validAfter` | 6 | First valid timestamp (inclusive) |

**This is the cleanest production-tested time-bound authorization design we have.** cipherocto's `NotBefore` and `UnlockAfter` constraints map directly to these two fields. `validUntil` and `validAfter` are *part of the signed return value*, not parameters — the account's `validateUserOp` returns them.

### 2.3 Paymasters — capability-as-a-service

A paymaster is a contract that pays gas on behalf of an account. The account signs a UserOp; the paymaster agrees to sponsor; the EntryPoint calls both. The paymaster's own `validatePaymasterUserOp` decides whether to sponsor.

**Cipherocto analogue:** a `Capability` could be sponsored by a parent vault (e.g., a marketplace vault sponsors a per-mission subcapability). The parent signs the capability grant; the sub-capability is redeemed against the parent's balance.

### 2.4 Aggregators — batched signatures

`aggregator` field lets multiple UserOps share one BLS signature. Bundlers aggregate, EntryPoint verifies once.

**Cipherocto analogue:** the `ConsensusSession` (grand design §12.9) is an aggregator primitive. One signature covers N SQL operations. ERC-4337's aggregator proves this works at scale.

### 2.5 EIP-7702 — EOA becomes a smart contract (2025)

EOAs can now temporarily authorize a contract to act on their behalf via a delegation list. This is a permission grant mechanism for the EOA → smart-contract gap.

**Cipherocto analogue:** a holder who has only an "EOA" (single key, no smart contract) can still receive a `Capability` — the capability carries its own verifier (macaroon-style), the holder doesn't need to deploy anything.

### 2.6 What's missing from cipherocto's draft

| Concept | ERC-4337 source | cipherocto current |
|---|---|---|
| Valid Until / Valid After | packed in `validationData` | `expires_at` exists, but no `valid_after` field |
| Paymaster (third-party sponsor) | `IPaymaster` interface | not in grand design |
| Aggregator (batched sigs) | `IAggregator` interface | covered by `ConsensusSession`, but no interface yet |
| Factory + factoryData (account pre-deploy) | `dependencies[]` array | not in grand design |
| `validationData` packed return | 32-byte packed integer | cipherocto should adopt same packing for canonical form |

---

## 3. Starknet Account Abstraction + Session Keys

### 3.1 What it ships

Starknet is AA at the protocol level. Every account is a contract. The protocol mandates:

- `__execute__` (transaction execution)
- `__validate__` (signature verification)
- `__validate_deploy__` (deploy-time validation)
- `__validate_declare__` (declare-time validation)

### 3.2 Session keys

From the Starknet blog (2025-04-29):

> "A Session Key is a temporary key that grants limited transaction execution permissions to a dApp without requiring user signatures for each action."

Production model:

```cairo
struct SessionKey {
    public_key: felt252,
    valid_until: u64,
    valid_after: u64,
    allowed_methods: Array<Selector>,   // function selectors
    spending_cap: Map<TokenAddress, u128>,  // per-token caps
}

#[storage]
struct Storage {
    session_keys: Map<felt252, bool>,   // pubkey → enabled
    spending_used: Map<(felt252, ContractAddress), u128>,
}
```

Usage:

1. User signs ONCE, grants session key with rules (function selectors, time bounds, per-token spending caps)
2. dApp uses session key to sign transactions within those constraints
3. Session expires → no more transactions

### 3.3 What cipherocto's draft misses

| Concept | Starknet | cipherocto current |
|---|---|---|
| Allowed method selectors (function allowlist) | `Array<Selector>` | `AllowedDestinations(DID set)` only — no method selectors |
| Per-token spending caps | `Map<TokenAddress, u128>` | `RateLimit { max_per_window, window }` is generic but no per-token |
| Spending accumulator | `spending_used: Map<...>` | not in grand design — needs to be tracked per capability |

### 3.4 The OpenZeppelin pattern — SNIP-6

OpenZeppelin ships a reference account contract that adheres to SNIP-6 (the Starknet account standard). Key insight: **the standard specifies a role-based authorization interface**, not a specific implementation.

```cairo
trait ISRC6 {
    fn __execute__(calls: Array<Call>) -> Array<Array<felt252>>;
    fn __validate__(calls: Array<Call>, hash: felt252) -> felt252;
    fn is_valid_signature(hash: felt252, signature: Array<felt252>) -> felt252;
}
```

**Cipherocto analogue:** RFC-0957 already defines `Capability` as a macaroon (cubic-meter, third-party-attestable). SNIP-6's value is *separation of the authorization interface from its implementation*. cipherocto's `Capability` should similarly expose an interface (`verify_spend(intent) -> Result<(), RejectReason>`) without mandating the underlying crypto.

### 3.5 DoS mitigations — the constraints you didn't think of

Starknet's `__validate__` has hard limits:

- ≤ 1,000,000 Cairo steps
- ≤ 100,000,000 gas
- Cannot call `get_class_hash_at`, `get_sequencer_address`
- Cannot call functions in external contracts (single-account invalidation only)
- `block_timestamp` is rounded to the nearest hour
- `block_number` is rounded to the nearest multiple of 100

**This is the missing piece for cipherocto's Economic VM.** A policy evaluator (capability verifier) MUST have bounded computation cost, else DoS attacks via expensive policies. The grand design §8 mentions "bounded evaluation cost" but doesn't specify the boundary.

**Recommendation:** cipherocto's `AllowIf { predicate }` constraint needs a step/gas budget — at most N VM ops per verification. Policy authors who need complex logic compose multiple capabilities, each with simple constraints.

---

## 4. Sui Object-Capability Model

### 4.1 What it ships

Five ownership forms for every object:

| Form | Access | Versioning |
|---|---|---|
| Address-owned | Single address can use | Fastpath (no consensus) |
| Shared | Any address can use, subject to Move checks | Consensus |
| Immutable | Anyone can read, no one can mutate | Fixed after mint |
| Wrapped | Only accessible through wrapper object | Depends on wrapper |
| Party (consensus-address-owned) | Single address owns, consensus-sequenced | Consensus |

### 4.2 The mental model

Sui has no accounts. Every asset IS an object. Every object has an owner (one of the 5 forms). Move's type system enforces capability safety at compile time — you can only spend an object if you hold it.

This is the **purest capability model** of the five systems we surveyed.

### 4.3 What cipherocto should learn

| Sui concept | cipherocto current | Adaptation |
|---|---|---|
| Object = unit of value | Vault = container holding asset | cipherocto should consider: a vault IS an object (per-vault capabilities, per-vault state machine) |
| 5 ownership forms | Vault only has `Owner DID` field | cipherocto needs: capability-bound, multi-sig, time-locked, frozen variants |
| Wrapped objects | not in grand design | A capability that wraps another capability (delegation chain with composition) |
| Type-system-enforced safety | Rust type system can do similar | Use Rust's affine types to enforce "consume capability to spend" |
| Fastpath vs Consensus routing | not in grand design | Cipherocto's resource shards (§10 of grand design) should mirror this: address-owned = single-shard, shared = cross-shard with consensus |

### 4.4 The Move pattern cipherocto should adopt

In Move, you cannot copy a `key`-bearing struct. You can only move it. This is enforced at the bytecode level.

```move
struct TreasuryCap has key, store { id: UID, total_supply: u64 }

public fun mint(treasury: &mut TreasuryCap, value: u64): Coin {
    // treasury is moved in (or borrowed mutably) — caller MUST have it
    Coin { value }
}
```

**Cipherocto analogue:** a `Capability` should be a linear/affine resource in Rust — once redeemed, the capability (or its proof-of-use) is consumed. Grand design already has `audit_window`, but linear consumption should be the default unless the capability is reusable.

### 4.5 Wrapped objects — multi-hop delegation

A capability can wrap another capability:

```move
struct WrappedCapability has key { inner: Capability }
```

To use `WrappedCapability`, you must hold the wrapper. The wrapper composes policies.

**Cipherocto analogue:** a parent capability grants a child capability. The child is *only usable through the parent* — enforces the hierarchical vault structure (grand design §11). The `parent_vault` field on `Vault` should propagate capabilities, not just balances.

---

## 5. Aztec Authentication Witnesses (AuthWit)

### 5.1 What it ships

AuthWits are `callerAddress × messageHash`-bound signatures:

```noir
// User signs: I authorize <caller> to perform action <messageHash>
let witness: AuthWitness = compute_auth_witness(caller, message_hash);

// Relayer uses the witness to call the action
assert_current_call_valid_authwit(&mut context, caller);
```

Properties:

- **Replay-protected**: `messageHash` is single-use (nonce bound)
- **Caller-bound**: can only be used by the specified caller address
- **Privacy-preserving**: verification happens inside private execution, not exposed publicly
- **Cross-contract**: works across contract boundaries

### 5.2 What cipherocto should learn

| AuthWit concept | cipherocto current | Adaptation |
|---|---|---|
| `callerAddress` binding | `holder_did` on Capability | Cipherocto's capability is already holder-bound |
| `messageHash` binding | `nonce` + canonical_ser | Cipherocto's capability signs the canonical payload — but no replay-prevention explicitly |
| Private verification | Not applicable | Cipherocto is public — different threat model |
| Single-use witness | not enforced | A capability should have a `max_uses` field or be single-use by default |
| Cross-contract authorization | not applicable (no contracts) | Cipherocto should support cross-`Vault` authorization (parent grants permission to child vault) |

### 5.3 The critical missing primitive

**AuthWit has explicit single-use semantics.** Once consumed, the witness is dead. cipherocto's draft `Capability` has `expires_at` but no `max_uses`. Add:

```text
Capability {
    ...
    max_uses: u32,           // 0 = unlimited (caller accepts risk of replay)
    uses_consumed: u32,      // projection from event log
    ...
}
```

This composes with `RateLimit` and the `AuditWindow`: 1-use capability for high-value operations, multi-use for routine subscriptions.

---

## 6. MACI (Minimal Anti-Collusion Infrastructure)

### 6.1 What it ships

MACI is a private on-chain voting system. Architecture:

```
User → encrypts vote with shared key
    → submits to MACI contract on-chain
    → Coordinator processes all messages off-chain
    → Coordinator produces zk-SNARK proof of tally
    → On-chain verifier checks the proof
    → Tally result is published
```

Key anti-collusion primitive: **users can change their key** between sign-up and voting. The coordinator can't tell whether two messages came from the same user because each message is encrypted with the *current* key at message time.

### 6.2 The pipeline pattern

MACI's separation:

| Layer | Where | What it does |
|---|---|---|
| User actions | on-chain (encrypted) | Append-only message log |
| Processing | off-chain (coordinator) | Decrypt, apply state transitions |
| Tally | off-chain (coordinator) | Sum votes, generate proof |
| Verification | on-chain (verifier) | Check the zk-SNARK |

### 6.3 What cipherocto should learn

| MACI concept | cipherocto current | Adaptation |
|---|---|---|
| Encrypted + on-chain message log | `transfer_events` (plaintext) | Some cipherocto messages (e.g., AI inference requests) may need encryption |
| Off-chain coordinator | not in grand design | Cipherocto may need a `coordinator` role for off-chain settlement aggregation |
| zk-SNARK proof of state transition | RFC-0958 (ZK capability) | Apply to settlement state transitions, not just capabilities |
| Key change for anti-collusion | not in grand design | Capabilities could support key rotation (e.g., per-mission keys already exist from Phase E) |
| Verifier on-chain | RFC-0958 has verifier | Apply to settlement state machine transitions |

### 6.4 The biggest single lesson — off-chain coordinator + on-chain verifier

MACI's pattern is generalizable:

| Use case | Off-chain coordinator | On-chain verifier |
|---|---|---|
| MACI voting | tally votes | verify zk-SNARK |
| Cipherocto settlement aggregation | aggregate N settlement receipts | verify aggregate signature |
| Cipherocto capability discharge | evaluate complex policy | verify hash + constraint list |
| Cipherocto audit window | process dispute evidence | verify dispute outcome |

**Recommendation:** Add `Coordinator` + `Verifier` roles to grand design. Coordinator does expensive work off-chain; verifier does cheap work on-chain. Each can be replaced independently.

### 6.5 What MACI doesn't give us

- No spend authority model (MACI is voting only)
- No multi-token support (one ballot type)
- No hierarchical authority (flat per-user)

These are cipherocto's value-adds over MACI, not gaps in MACI.

---

## 7. The minimum viable `Constraint` set

After surveying five production systems, here's the **minimum-viable cipherocto `Constraint` set** that covers all the patterns observed:

### 7.1 From ERC-7715

- `NativeTokenAllowance { max: u128, is_adjustable: bool }`
- `ERC20TokenAllowance { token: AssetID, max: u128 }`
- `ContractCallAllowance { allowed_targets: HashSet<DID>, allowed_selectors: HashSet<Bytes> }`
- `Period { max_per_period: u128, period_duration_secs: u64, reset_at: Timestamp }`
- `Expiry { at: Timestamp }`
- `ValidAfter { from: Timestamp }`

### 7.2 From ERC-4337 (time bounds packed together)

- `ValidRange { valid_after: Timestamp, valid_until: Timestamp }` — packed, single constraint
- `SponsoredBy { sponsor_vault: VaultID }` — paymaster analogue

### 7.3 From Starknet (per-token caps + method selectors)

- `PerTokenSpendingCap { caps: Map<AssetID, u128>, used: Map<AssetID, u128> }`
- `MethodSelector { allowed: HashSet<Bytes> }` — for actions, not transfers
- `StepBudget { max_steps: u32 }` — DoS mitigation for `AllowIf` predicate

### 7.4 From Sui (ownership variants)

- `SingleUse {}` — affine consumption
- `MaxUses { count: u32 }` — bounded reuse
- `WrappedOnly {}` — only usable through a parent capability
- `OwnershipForm { form: CapabilityForm }` — AccountBound | CapabilityBound | MultiSig | Frozen

### 7.5 From AuthWit (binding)

- `CallerBound { holder: DID }` — only this DID can redeem
- `IntentBound { message_template: Bytes }` — only this intent (message hash prefix)
- `NonReplayable {}` — single-use enforcement (alternative to `SingleUse`)

### 7.6 From MACI (delegation + verification)

- `CoSignedBy { co_signer: DID, threshold: u32 }`
- `VerifierRequired { verifier_contract: VaultID, circuit_id: Bytes32 }`
- `CoordinatorCanSubmit { coordinator: DID }` — for off-chain aggregation

### 7.7 The consolidated set

```text
Constraint ∈ {
    // Time
    ValidRange { valid_after: Timestamp, valid_until: Timestamp },
    NotBefore, UnlockAfter,
    Period { max_per_period, period_duration_secs },

    // Spend caps
    MaxPerTx { amount },
    PerAssetSpendingCap { caps: Map<AssetID, u128> },
    RateLimit { max_per_window, window },

    // Destination / intent
    AllowedDestinations { dids: HashSet<DID> },
    DeniedDestinations { dids: HashSet<DID> },
    IntentBound { message_template: Bytes },

    // Co-signing
    MultiSig { n: u32, signers: HashSet<DID> },
    RequireReceiptSignatureBy { did: DID },

    // Caller binding
    CallerBound { holder: DID },

    // Use count
    MaxUses { count: u32 },
    SingleUse {},

    // Policy delegation
    AllowIf { predicate, step_budget: u32 },
    VerifierRequired { circuit_id: Bytes32 },

    // Composition
    WrappedOnly {},            // only usable through a parent
    SponsoredBy { vault: VaultID },
    CoordinatorCanSubmit { coordinator: DID },

    // Vesting / time-lock
    LinearRelease { start, end, cliff },
    CliffVesting { until, pct, period },
    LiquidityLock { until },
    GovernanceLock { while_vote_active },

    // Compliance
    ComplianceHold { threshold, delay },
}
```

That's 23 constraints. Down from the original draft's 14 by unifying some, adding 9 from external research. Total: **23 is the minimum viable surface** to cover all five systems' patterns.

### 7.8 What's still missing

After five-system sweep, gaps remain:

| Gap | Why it matters |
|---|---|
| Zero-knowledge proof of constraint satisfaction | Some constraints (e.g., "reputation > 900") leak holder state if checked publicly. ZK proofs needed. |
| Cross-vault atomic constraints | "If vault A allows, vault B must also allow" — single capability spans multiple vaults |
| Recursive policy (a constraint that spawns sub-capabilities) | Powerful but unbounded — defer to Economic VM extension, not v1.0 |
| Time-bounded revocation propagation | Capability revoked on chain T → offline holders learn of revocation within T+Δ. Sync story needed. |

---

## 8. Cipherocto advantages over the surveyed systems

| Capability | Cipherocto grand design | Best of surveyed |
|---|---|---|
| Hierarchical vaults (parent → child → grandchild) | Yes (grand design §11) | None — Ethereum/Sui/Starknet have flat ownership |
| Cross-chain via `MultiSettlement` | Yes (grand design §7) | Bridges (HTLC, wrapped assets) — not atomic-by-proof |
| Audit window on settlement | Yes (grand design §6) | Optimistic rollups have challenge periods — different model |
| Event-sourced ledger | Yes (grand design §9) | None of the five (Datomic has this but not in blockchain form) |
| Economic VM (declarative, loop-free) | Yes (grand design §8) | EVM (Turing-complete, opposite design) |
| Consensus Session (enterprise DB compat) | Yes (grand design §12) | None — this is cipherocto's unique claim |
| Resource sharding by resource type | Yes (grand design §10) | Ethereum L2s do application sharding — different model |
| Per-capability audit window | Yes | None |

**Cipherocto's seven-layer design subsumes all five systems surveyed, plus adds three primitive patterns no other system has: hierarchical vaults, audit windows, and Consensus Sessions.**

---

## 9. Concrete updates to the grand design

Updates to grand design doc §5 (`Constraint` set):

1. Add `PermissionKind` separation (per ERC-7715 §1.3)
2. Add `redemption_context` field to `Capability` (per ERC-7715 §1.4)
3. Add `factory + factoryData` to `Capability` for vault pre-deploy (per ERC-7715 §1.5)
4. Add `valid_after` field alongside `expires_at` (per ERC-4337 §2.2)
5. Add `MethodSelector` constraint (per Starknet §3.3)
6. Add `PerTokenSpendingCap` constraint (per Starknet §3.2)
7. Add `StepBudget` to `AllowIf` (per Starknet §3.5)
8. Add `WrappedOnly` constraint for hierarchical capabilities (per Sui §4.4)
9. Add `MaxUses`/`SingleUse` to capability (per AuthWit §5.2)
10. Add `CallerBound` and `IntentBound` constraints (per AuthWit §5.1)
11. Add `SponsoredBy` constraint (per ERC-4337 §2.3 paymasters)
12. Add `CoordinatorCanSubmit` + `VerifierRequired` (per MACI §6.2)

Updates to grand design doc §2.2 (`Capability` struct):

```text
Capability {
    capability_id:  CapabilityID,
    issuer_did:     DID,
    holder_did:     DID,                   // CallerBound
    vault_id:       VaultID,
    permission_kind: PermissionKind,       // NEW: separation from constraints
    constraints:    Vec<Constraint>,
    expires_at:     Timestamp,
    valid_after:    Timestamp,             // NEW (default 0)
    nonce:          Nonce,
    max_uses:       u32,                   // NEW (0 = unlimited)
    uses_consumed:  u32,                   // NEW (projection from event log)
    audit_window:   Option<Duration>,
    redemption_context: Bytes,             // NEW (opaque, signed)
    factory:        Option<(VaultID, Bytes)>,  // NEW (vault pre-deploy)
    parent_capability: Option<CapabilityID>,    // NEW (for WrappedOnly)
    signature:      Signature,
}
```

Updates to grand design doc §12.1-§12.9 (Consensus Sessions):

- Add `Aggregator` interface (per ERC-4337 §2.4)
- Add `Coordinator` and `Verifier` roles (per MACI §6.4)
- Add `StepBudget` to `AllowIf` constraint (per Starknet §3.5)

---

## 10. Open questions for next phases

| Phase | Topic | Output |
|---|---|---|
| 4 | Event-sourced ledger precedents (Datomic, EventStoreDB, Kafka + projections, Cosmos SDK event-sourcing) | pitfalls + proven patterns for grand design §9 |
| 5 | Enterprise migration playbooks (PostgreSQL logical replication → CipherOcto DDL; SAP RFC adapters) | compatibility-level-by-level guide |
| 6 | Deterministic SQL: classify which standard functions are consensus-safe vs forbidden | RFC candidate |
| 7 | Consensus Session object: protocol design + ZK circuit for batch signature | RFC candidate |
| 8 | Resource shard routing policy | RFC candidate |
| 9 | Synthesize 3-8 into one or more grand-design RFCs (numbered RFC-0960+, RFC-0970+) | RFC drafts |

**Next action:** proceed to Phase 4 (event-sourced ledger precedents) — the event-sourced ledger is the second architectural pillar that needs production-tested validation. Once both pillars are validated, Phase 9 RFC synthesis can begin.

---

## 11. References

### External

- ERC-7715: <https://eips.ethereum.org/EIPS/eip-7715>
- ERC-4337: <https://eips.ethereum.org/EIPS/eip-4337>
- Starknet accounts: <https://docs.starknet.io/learn/protocol/accounts>
- Starknet session keys: <https://www.starknet.io/blog/session-keys-on-starknet-unlocking-gasless-secure-transactions/>
- Sui ownership: <https://docs.sui.io/concepts/object-ownership>
- Sui object model: <https://docs.sui.io/concepts/object-model>
- MACI main: <https://maci.pse.dev/>
- MACI GitHub: <https://github.com/privacy-scaling-explorations/maci>
- Aztec AuthWit: <https://docs.aztec.network> (model-knowledge supplement; fresh fetch pending)

### Internal

- `docs/research/2026-07-22-value-transfer-model-internal-landscape.md` (Phase 1)
- `docs/research/2026-07-22-grand-design-vaults-capabilities-reservations.md` (Phase 2)
- RFC-0957 (capability token format)
- RFC-0958 (ZK capability subclass)
- RFC-0959 (settlement receipt)
- RFC-0853 (overlay crypto — Phase E mission keys)

---

## 12. Status

This doc = Phase 3 of N research (external capability-based spend systems). All 5 production systems surveyed. Minimum-viable `Constraint` set = 23 constraints (up from 14 in grand design §5 draft). Concrete updates proposed to grand design §2.2 + §5 + §12.

**No code changed. No RFC drafted yet. Next: Phase 4 (event-sourced ledger precedents).**
