---
name: mission-0871e-force-relinquish-governance-status
description: Mission 0871e-force-relinquish-governance LANDED 2026-08-11. 8 end-to-end TV for WriterElectionForceRelinquish in octo-sync. Real bug fix: chain_id deployment-binding check was tautological (passed attestation.chain_id as both sides) — now passes configured deployment chain_id.
metadata:
  type: project
  originSessionId: c979a5ea-63a6-4b69-97ac-cd870c8a8f95
---

# Mission 0871e-force-relinquish-governance — Status (2026-08-11)

## What landed

End-to-end TV coverage for `WriterElectionForceRelinquish::force_relinquish_writer`
+ **real substrate bug fix**: chain_id deployment-binding check (RFC-0862 v1.3
R12 M23) was tautological in the previous code — the `force_relinquish_writer`
impl passed `&attestation.chain_id` as BOTH sides of the comparison, so the
check could never fire. A misconfigured operator set could replay attestations
across deployments without detection.

## Substrate changes (Layer B-substrate — `octo-sync`)

**`crates/octo-sync/src/substrate/raft_like.rs`:**
- `RaftLikeWriterElection` gains a `chain_id: ChainId` field (constructor
  arg 3).
- `RaftLikeWriterElection::new(node_id, cluster, chain_id)` — deployment
  chain_id is bound at construction; the same chain_id is used for the
  governance attestation deployment-binding check.
- `force_relinquish_writer` impl now passes `&self.chain_id` (the
  deployment-configured chain_id) to `verify_governance_attestation` instead
  of the attestation's own `chain_id` field. This makes the chain_id check
  fire when an operator set for deployment X tries to use an attestation
  minted for deployment Y.

**Constructor migration (16 sites)**
- 7 sites in `octo-sync/src/substrate/raft_like.rs::tests`
- 5 sites in `octo-sync/tests/cross_instance_tv.rs`
- 10 sites in `octo-sync/tests/governance_relinquish_tv.rs` (NEW)
- 1 site in `octo-sync/tests/property_tests.rs` (unchanged — no `RaftLikeWriterElection::new`)
- All call sites in tests use static `ChainId::new("cipherocto-test").expect("static literal")` per RFC-0010 v1.4 invariant.

## Test coverage (8 canonical TV per RFC-0862 v1.3 §Specification §Governance)

**New file: `octo-sync/tests/governance_relinquish_tv.rs`**
- TV-1 two_of_three_force_relinquish_clears_lease — happy path: 2 valid ed25519 sigs over `governance_signature_message`, lease cleared, 1 nonce record in WAL
- TV-2 wrong_chain_id_rejected — attestation chain_id = `"partner-mainnet"` vs configured `"cipherocto-test"` → `ChainIdMismatch`, lease stays
- TV-3 replayed_nonce_rejected — same nonce reused after first consume → `NonceReplayed`, lease stays
- TV-4 unauthorized_signer_rejected — 1 valid signer + 1 outsider signature → `UnauthorizedSigner`, lease stays
- TV-5 below_threshold_rejected — only 1 signature with threshold=2 → `InsufficientSignatures`
- TV-6 invalid_signature_rejected — 1 valid + 1 bit-flipped signature → `InvalidSignature`
- TV-7 duplicate_signer_rejected — same operator signs twice → `DuplicateSigner`
- TV-8 shard_key_mismatch_rejected — attestation shard_key ≠ caller shard_key → `ShardKeyMismatch`

Each TV exercises the full pipeline:
`verify_governance_attestation` (chain_id binding + threshold + signature checks) → `NonceTracker::consume` (WAL-anchored replay protection) → `Cluster::force_relinquish` (lease clear).

## Layer discipline (per [[cipherocto-design-principles]])

- `octo-sync` (Layer B-substrate) — `RaftLikeWriterElection` concrete impl + `Cluster` + `NonceTracker` + `OperatorSet` + `GovernanceAttestation`. The substrate gains a chain_id field; trait surface unchanged.
- `octo-ident` (Layer B) — UNCHANGED. `ChainId` substrate from mission `0010-f2-multi-chain-did-resolution` is consumed; no new trait or module added.
- `octo-identity-resolver-node` (Layer C) — UNCHANGED. The governance path lives below the resolver-node layer.

## Why the chain_id field is on the election impl, not the trait

`WriterElectionForceRelinquish::force_relinquish_writer` takes
`(shard_key, attestation, configured_operator_set, nonce_tracker)` — the
trait surface is layer-stable. Adding `chain_id` as a new trait arg would
be a breaking change. Instead, the chain_id is bound at the
concrete-impl construction boundary (`RaftLikeWriterElection::new`) — this
matches the `RaftLikeDidWriteCoordinator::new(..., chain_id, ...)` pattern
already established.

## Validation snapshot

| Check | Result |
|-------|--------|
| `cargo build -p octo-sync` | clean |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p octo-sync --all-targets --all-features -- -D warnings` | clean |
| `cargo test --lib -p octo-sync` | 225/225 pass (no regression) |
| `cargo test --test cross_instance_tv -p octo-sync` | 4/4 pass (no regression) |
| `cargo test --test governance_relinquish_tv -p octo-sync` | 8/8 pass |
| `cargo test --test property_tests -p octo-sync` | 7/7 pass (no regression) |
| `cargo test -p octo-sync` (full) | 245/245 pass |

## Implementation gotchas (for follow-on governance work)

- `NonceTracker::in_memory_for_test` is `#[cfg(test)]` — not visible from
  integration tests. Use `cluster.scan_nonce_records().len()` as the
  nonce-consume witness from external test crates.
- `governance_signature_message` takes `&ShardKey` (not `ShardKey`) — most
  test helpers pass owned `ShardKey`, so call site must add `&`.
- `Cluster` does NOT implement `WalAppender` (only `InMemoryWal` does).
  Use `InMemoryWal::new(cluster.clone())` as the durable backing for
  `NonceTracker` in tests.
- The ed25519 test signer seeding pattern: `SigningKey::from_bytes(&blake3_hash(&[byte]))` (BLAKE3 of `(domain || byte)`) produces deterministic, non-trivial 32-byte seeds. Avoid `SigningKey::from_bytes(&[byte; 32])` — those seeds are degenerate and cause ed25519-dalek to silently produce low-quality keys.
- The chain_id field on `RaftLikeWriterElection` is the deployment binding
  used by `force_relinquish_writer`. The same chain_id is also used as the
  expected value in `verify_governance_attestation` — there is exactly one
  place where deployment binding is enforced.

## Follow-on work

- **Snapshot+replay field (R16 H1)** — `Snapshot { elected_at_hlc, term, operator_set, writer_identity }` written on `force_relinquish_writer` success + on `relinquish_writer` flush success. Lands with the state-recovery substrate (RFC-0862 v1.3 §Replay Protocol) in a separate mission.
- **Byzantine fault tolerance (R16 H2)** — full BFT consensus (PBFT prepare/commit, quorum intersection proof, view-change protocol) deferred to RFC-0862 v2.0 + `crates/octo-coordinator-bft/` (Layer A).
- **Operator DKG ceremony** — RFC-0853 §F3 substrate provides the operator-set key-share ceremony; this mission assumes the ceremony has already run and an `OperatorSet` is configured at the election impl boundary.

## How to apply

- `RaftLikeWriterElection::new` now requires `chain_id` — if you see a build error about missing 3rd argument, the call site predates this mission and needs the deployment chain_id wired in.
- Any new `force_relinquish_writer` test must declare the configured chain_id explicitly; the check is `attestation.chain_id == configured chain_id`, not `attestation.chain_id == attestation.chain_id`.
- `MAX_GOVERNANCE_SIGNATURES = 32` (per RFC-0862 v1.3 R12 M23); tests should stay well below this bound.