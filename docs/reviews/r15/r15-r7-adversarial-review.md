# R15 R7 Adversarial Review

## Scope
Re-review of `crates/octo-network/src/{mon,dc,gossip,dom,dgp,dps,gdp,dot,ocrypt,orr,porelay,common}/`
after R6 fixes. Focus on: remaining empty-input DoS in
gossip/replay/attestation topics, rejoin cooldown DoS, and
adapter-related trust assumptions.

## Findings & Fixes

### R7-1 (LOW) — `attest_topic` accepts empty `domain_id`
**Issue**: `format!("/dot/admin/{}/{}", domain_id, platform.as_str())`
with empty `domain_id` produces the malformed topic
`"/dot/admin//whatsapp"`.
**Fix**: assert + test `topic_rejects_empty`.

### R7-2 (LOW) — `RejoinCooldown::check_and_record` accepts empty `peer_id`
**Issue**: Empty `peer_id` would key the cooldown map on `""`,
causing all anonymous rejoin attempts to rate-limit each other.
**Fix**: Reject empty `peer_id` with new `RejoinError::InvalidPeerId`
variant. Test `cooldown_rejects_empty_peer_id`.

## Other Areas Investigated (No Issues)

- `dgp/dedup.rs`, `dgp/incremental.rs`, `dgp/anti_entropy.rs` — clean
  (BTreeMap, FIFO eviction, time-window logic correct).
- `dps/verifier.rs`, `dps/suite.rs` — clean (typed enums, BLAKE3
  hashing, no overflow risk).
- `dot/adapters/registry.rs` — `unsafe` block is gated by operator-
  controlled `plugin_dirs`; threat model requires trusted plugin
  directory, so no fix needed.
- `dot/route.rs`, `drs/scoring.rs`, `drs/trust.rs` — clean (saturating
  arithmetic throughout).
- `dom/admission.rs` — clean (Ed25519 verify returns Err for invalid
  signature; type system enforces 64-byte length).
- `porelay/score.rs`, `gdp/discovery.rs`, `ocrypt/session.rs` — clean.

## Test Results
- octo-network: 1083 passed (up from 1081 at R6; +2 in R7)

## Files Changed
- `crates/octo-network/src/dc/admin_attest.rs`
- `crates/octo-network/src/dc/rejoin.rs`
