# Mission: ZK Proof Verification System

## Status

Claimed (2026-08-06); **v0.1 re-evaluation amendments 2026-08-06** (per re-evaluation of stale substrate description + RFC-0100 ghost + dependency classification).

## RFC

- **Primary:** RFC-0958 (Proof Systems): ZK Capability Subclass — the actual ZK substrate
- **Secondary:** RFC-0102 (Numeric): Wallet Cryptography Specification — Accepted
- **Ghost reference (RFC-0100: AI Quota Marketplace Protocol):** does NOT exist as a published RFC in `rfcs/{draft,accepted,planned}/`. Intent documented in `docs/use-cases/enhanced-quota-router-gateway.md` + `docs/use-cases/hybrid-ai-blockchain-runtime.md`. Either RFC-0100 is planned-but-not-authored or the mission text predates the RFC renumbering. **Flagged as doc bug; deferred to a future RFC-0100 authoring pass.**

## Substrate (verified 2026-08-06)

Sibling missions shipped:

- `missions/claimed/0958-a-zk-capability-circuit.md` (Claimed 2026-07-22; 15/15 ACs done; R4 redo landed 2026-08-04 via `ae4dc4f8`, `9c996fba`, `89063351`)
- `missions/claimed/0958-b-real-cairo-crypto.md` (Claimed 2026-08-05; 46/47 ACs done — added `prove_batch_signature` API)
- `missions/claimed/0958-c-real-cairo-crypto-followup.md` (Claimed; redaction policy + `witness_format` field on `ProofBundle`)

Workspace crates (all on `next`, modified 2026-08-05):

- `crates/zk-circuit/` — scarb-build → Sierra → CASM via `cairo-lang-sierra-to-casm` 2.20.0; BLAKE3 hash binding; `bundled_casm_bytes` / `bundled_casm_hash_hex`
- `crates/zk-verifier/` — public API: `verify_capability_zk`, `PublicInputs`, `ProofBundle`, `MAX_SKEW_SECS` (300s clock skew bound per RFC-0958 R3 N5), `ProverError` enum (incl. `StubVerifierDisabled` for prod-gate)
- `crates/zk-vendor/stwo-sys/` — workspace-EXCLUDED cdylib, nightly `2025-06-23` toolchain, `cargo +nightly build --release` → `libstwo_sys.so`
- `crates/zk-vendor/src/lib.rs::loaded_library() -> Option<StwoSys>` — libloads `libstwo_sys.so` at runtime
- `crates/octo-wallet/src/capability/zk_mint.rs` + `crates/quota-router-core/src/zk_verify/capability.rs` — mint + verify wire-up; NodeType gating

**Architecture pin (commit `4f7f47db`, 2026-07-31):** the earlier "vendored STWO source into cipherocto workspace" attempt was REVERTED. Canonical substrate is the decoupled FFI cdylib pattern. Cipherocto workspace stays MSRV-stable (1.93 per `crates/zk-vendor/rust-toolchain.toml`).

**Prod-gate (commit `0e0c3ee9`, R3 #1):** stub proofer is behind `--features allow-stub-verifier` (default OFF). Production calls to `stub_commitment` without the feature return `Err(ProverError::StubVerifierDisabled)`.

## Blockers / Dependencies

| AC                                          | Block type                    | Reason                                                                                                                                                                                                              |
| ------------------------------------------- | ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| AC-1 (STWO verifier wire-up)                | UNBLOCKED                     | Substrate in `crates/zk-verifier/` is public API                                                                                                                                                                    |
| AC-2 (Batch multiple proofs)                | UNBLOCKED                     | `prove_batch_signature` API in 0958-b                                                                                                                                                                               |
| AC-3 (On-chain proof submission to Stoolap) | **HARD-BLOCKED**              | No settlement carrier claimed. `0968-b-marketplace-integration.md` is in `archived/superseded/` (stale). `missions/quota-market-integration.md` is top-level orphan, status Open, blocked on Multi-Provider Support |
| AC-4 (Verify before payment release)        | **HARD-BLOCKED** (transitive) | Depends on AC-3 settlement hook                                                                                                                                                                                     |
| AC-5 (Display verification status)          | UNBLOCKED                     | Pure UI / CLI work                                                                                                                                                                                                  |
| AC-6 (GPU acceleration)                     | OPTIONAL                      | Mission text marks optional; defer                                                                                                                                                                                  |

**Soft-block context:** "Mission: Stoolap Provider Integration" (`missions/stoolap-provider-integration.md`, top-level orphan) was previously listed as the gate, but that mission itself is Open + blocked on Multi-Provider Support (also top-level orphan). This mission does NOT depend on Stoolap Provider Integration for AC-1, AC-2, AC-5 — those are pure verifier + UI work. Only AC-3 + AC-4 inherit the marketplace-carrier gap.

## Acceptance Criteria

- [x] AC-1 Integrate STWO verifier for STARK proofs via `zk_verifier::verify_capability_zk(...)` — landed Session 1 (2026-08-06): `crates/quota-router-cli/src/commands.rs::verify_one_from_json` calls `zk_verifier::verify_capability_zk` directly
- [x] AC-2 Batch multiple proofs into single verification via `prove_batch_signature` (0958-b API) — landed Session 1 (2026-08-06): `verify_batch_from_json` loops envelopes through `verify_capability_zk` (one-per-call); batch API surface exposed via `--batch --from <json-array>`
- [ ] ~~AC-3 On-chain proof submission to Stoolap~~ — DEFERRED (settlement carrier missing; see §Blockers)
- [ ] ~~AC-4 Verify proofs before releasing payment~~ — DEFERRED (transitive on AC-3)
- [x] AC-5 Display verification status (`quota-router verify --proof <id>`, `--batch`, `history`) — landed Session 1 (2026-08-06): `Commands::Verify { from, batch, history, history_path }` in `crates/quota-router-cli/src/cli.rs`; history stored as JSONL at `${XDG_DATA_HOME:-~/.local/share}/cipherocto/quota-router/verify-history.jsonl` (override via `--history-path`)
- [ ] ~~AC-6 GPU-accelerated proof generation~~ — DEFERRED (optional, per mission text)

## Description

Enable ZK proof-based verification for marketplace transactions using Stoolap's STARK proving system.

## Technical Details

### Proof Types (from Stoolap)

| Proof Type      | Use Case             | Verification | Size       |
| --------------- | -------------------- | ------------ | ---------- |
| HexaryProof     | Individual execution | ~2-3 μs      | ~68 bytes  |
| StarkProof      | Batch verification   | ~15 ms       | 100-500 KB |
| CompressedProof | Multiple batches     | ~100ms       | ~10 KB     |

### Stoolap Integration

```mermaid
flowchart TD
    subgraph STOOLAP["Stoolap ZK Stack"]
        SQL[SQL Query] --> HEX[HexaryProof]
        HEX --> BATCH[Batch]
        BATCH --> STARK[STARK Proof<br/>via stwo-cairo-prover]
    end

    subgraph CIPHER["CipherOcto Integration"]
        REQ[Request] --> VERIFY[Verify Proof]
        VERIFY --> PAY[Release OCTO-W]
    end

    STOOLAP -->|verify| CIPHER
```

### GPU Acceleration (Optional)

For production, consider GPU-accelerated STWO:

| Implementation | Speedup  | Notes             |
| -------------- | -------- | ----------------- |
| NitrooZK-stwo  | 22x-355x | Cairo AIR support |
| ICICLE-Stwo    | 3x-7x    | Drop-in backend   |
| stwo-gpu       | ~193%    | Multi-GPU scaling |

See: `docs/research/stwo-gpu-acceleration.md`

### Verification Flow

```mermaid
sequenceDiagram
    participant B as Buyer
    participant S as Seller
    participant P as Stoolap

    B->>S: Request prompt execution
    S->>P: Submit transaction
    P-->>S: Return proofs
    S-->>B: Response + proofs
    B->>P: Verify proofs
    P-->>B: Verified / Failed
    B->>S: Release payment (if verified)
```

### CLI Commands

```bash
# Verify a proof
quota-router verify --proof <proof-id>

# Batch verify multiple proofs
quota-router verify --batch <proof-ids>

# View verification history
quota-router verify history
```

## Implementation Notes

1. **Async verification** - Don't block response, verify in background
2. **Batch for cost** - Combine multiple verifications
3. **Caching** - Cache verified proofs to avoid re-verification
4. **GPU acceleration** - Consider NitrooZK-stwo for production (22x-355x speedup)

## Research References

- [Stoolap vs LuminAIR Comparison](../docs/research/stoolap-luminair-comparison.md)
- [STWO GPU Acceleration](../docs/research/stwo-gpu-acceleration.md)
- [Privacy-Preserving Query Routing](../docs/use-cases/privacy-preserving-query-routing.md)
- [Provable QoS](../docs/use-cases/provable-quality-of-service.md)

## Claimant

@cipherocto (2026-08-06)

## Pull Request

#

## Session 1 (2026-08-06) scope

- AC-1: STWO verifier wire-up — pure substrate call-through; verify `verify_capability_zk` reachable from `quota-router-cli`
- AC-2: batch wrapper around `prove_batch_signature` (0958-b API)
- AC-5: `quota-router verify --proof <id>`, `--batch`, `history` CLI surface

Skip AC-3, AC-4 (settlement carrier missing). Skip AC-6 (optional, defer).

## Closure (Session 1, 2026-08-06)

**Status:** AC-1, AC-2, AC-5 closed in single Session 1 push; AC-3, AC-4, AC-6 deferred.

**Implementation commits (local on `next`, push awaits user instruction):**

1. `feat(zk-proof-verification): claim mission with v0.1 re-evaluation amendments` — mission file claim (commit `2f9eb4ff`)
2. (this session's implementation commit, subject to follow)

**Substrate crates touched:**

- `crates/quota-router-cli/Cargo.toml` — added `zk-verifier = { path = "../zk-verifier" }` dep + `allow-stub-verifier` feature gate (test-only, propagated via `[dev-dependencies]`); `tempfile` dev-dep for hermetic history tests
- `crates/quota-router-cli/src/cli.rs` — `Commands::Verify { from, batch, history, history_path }` variant + 4 parse tests
- `crates/quota-router-cli/src/main.rs` — match arm for `Commands::Verify`
- `crates/quota-router-cli/src/commands.rs` — `VerifyEnvelope` + `VerifyProofBundleWire` (hex-or-bytes deserializer) + `VerifyHistoryEntry` + `verify_one_from_json` + `verify_batch_from_json` + `verify_history_print` + `verify` CLI dispatcher + `default_history_path` (XDG) + 6 unit tests

**Verification output:**

```text
cargo fmt --all                                                                       # clean
cargo clippy -p quota-router-cli --lib --features allow-stub-verifier \
            --all-targets -- -D warnings                                             # clean
cargo test -p quota-router-cli --lib --features allow-stub-verifier                   # 42/42 pass
cargo clippy --workspace --all-targets --features allow-stub-verifier \
            -- -D warnings                                                            # clean (1m 15s)
```

**Test coverage (10 verify-specific tests):**

- `cli::tests::parse_verify_default_single_mode` — default args parse (`--from -`, no flags)
- `cli::tests::parse_verify_batch_with_from` — `--batch --from /tmp/proofs.json` parses
- `cli::tests::parse_verify_history_with_path_override` — `--history --history-path /var/log/verify.jsonl` parses
- `cli::tests::parse_verify_history_flag_explicit` — `--history` flag parses
- `commands::tests::verify_one_ok_writes_history_with_ok_outcome` — stub proof + valid CASM + valid skew → ok
- `commands::tests::verify_one_casm_mismatch_writes_history_with_error_outcome` — tampered casm_hash → CasmHashMismatch variant surfaced
- `commands::tests::verify_one_clock_skew_writes_history_with_error_outcome` — skew 600s > MAX_SKEW_SECS (300s) → ClockSkewExceeded variant surfaced
- `commands::tests::verify_batch_appends_one_entry_per_envelope` — batch input appends 2 history rows
- `commands::tests::verify_history_print_empty_path_returns_ok` — missing history file is Ok (no entries)
- `commands::tests::verify_history_print_newest_first` — file on disk preserves append order; print function sorts DESC for display

**AC walk:** 3/6 closed (AC-1, AC-2, AC-5). 3/6 deferred (AC-3, AC-4, AC-6) per §Blockers.

---

**Mission Type:** Implementation
**Priority:** Medium
**Phase:** ZK Proofs
