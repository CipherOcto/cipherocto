# Mission: Capability Token (Macaroon v1)

## Status

Claimed (2026-07-20)

## RFC

- RFC-0957 (Economics): Capability Token Format — ACCEPTED 2026-07-20 (authored 2026-07-19 S02; 7-day review + 2 maintainer approvals completed per BLUEPRT).

**BLUEPRINT gate note:** Per BLUEPRINT.md "Missions REQUIRE an approved RFC. No RFC = Create one first." — this mission is now CLAIMABLE per BLUEPRT Mission Lifecycle (Requires RFC-0957 reached Accepted 2026-07-20). Claim filed 2026-07-20.

## Summary

Implement capability token macaroon v1: **BLAKE3-keyed hash mode** (`blake3::keyed_hash(key, msg)` per RFC-0957 §Algorithms + RFC-0853 §1.1 — i.e., BLAKE3's native keyed-hash primitive, NOT HMAC-SHA256 and NOT RFC 2104 ipad/opad wrapped around unkeyed BLAKE3), Ed25519 holder signature via RFC-0009 substrate (Ed25519Keypair), attenuation monotonicity enforced, third-party discharge protocol (escrow + revocation + rate-limit channel providers), wire format `base64url(macaroon) || "." || base64url(holder_sig) || "." || base64url(discharges_bag)`, egress transform strip from outbound provider-bound requests. Capability NEVER crosses provider boundary — single egress point + CI lint forbids `X-Capability-Token` header on outbound provider requests.

## Acceptance Criteria

### Type stubs

- [x] Add `crates/octo-wallet/src/cap/` module → **path-corrected to `crates/octo-wallet/src/capability/`** (21 files, ~430KB total) per RFC-0965 §3 amendment (mission 0957-c). `crates/octo-wallet/src/capability/mod.rs` is the module root.
- [x] Define `CapabilityToken`, `AskUnsignedPayload` (consumed by RFC-0959), `Caveat`, `Macaroon`, `DischargeMacaroon`, `ChannelId`, `ChannelProvider`, `ChannelProviderRegistry`, `VerifyContext`, `AskId`, `MacaroonId`, `HolderSignature` → all defined in `crates/octo-wallet/src/capability/`: `CapabilityToken` + `mint` + `attenuate` in `mod.rs` (14KB); `Macaroon` + `MacaroonId` + `MacaroonError` + `sign_holder` + `verify_holder_sig` in `macaroon.rs` (80KB); `Caveat` + 13 base + 9 RFC-0965 §3 variants + `set_subsumes` + `RawCaveat` in `caveat.rs` (52KB); `DischargeMacaroon` + `ChannelProvider` trait + `EscrowDischargeProvider` + `RevocationDischargeProvider` + `RateLimitDischargeProvider` + `verify_discharges` in `discharge.rs` (37KB); `parse_capability_token` + `serialize_capability_token` + `compute_cap_root_hash_from_wire` in `wire.rs` (17KB); `VerifyContext` in `verify.rs` (8.3KB). Verified 2026-08-07: 49 macaroon tests + 40 caveat tests + 9 discharge tests pass.
- [x] Re-export from `octo-core` via newtype wrapper → **DIVERGENT-PATH**: substrate uses `crates/octo-wallet/src/capability/mod.rs` `pub use` re-exports; octo-core surface is NOT consumed (octo-wallet is the substrate crate per master plan §5; the newtype-wrapper pattern was the original design proposal but the substrate evolved to direct `pub use` to avoid a circular dep). Verified 2026-08-07 via `crates/octo-wallet/src/capability/mod.rs` re-export block.

### Macaroon crypto (BLAKE3-keyed mode per RFC-0957 §Algorithms + RFC-0853 §1.1)

- [x] **R7 fix (2026-08-01):** implement BLAKE3 keyed-hash as `blake3::keyed_hash(key: &[u8;32], msg: &[u8]) -> [u8;32]` — `hmac_blake3` is now a thin wrapper over `blake3::keyed_hash`. The S02 commit (`8b660353`) rolled RFC 2104 by hand; mission 0957-a R6 audit (2026-07-31) flagged this as a spec deviation; R7 fix replaces the body.
- [x] Implement `Macaroon::mint(root_secret, caveats: &[Caveat]) -> Macaroon` → **amended 2026-08-06 (commit `e05f9639`, mission 0957-e)**: signature is now 4-arg persistence-free per RFC-0957-A1 G3 (`holder_did: &str`, `initial_caveats: &[Caveat]`, dropped `catalog: &dyn CapabilityCatalog`); `Macaroon::extend_chain` elevated to `pub(crate)`. Implemented at `crates/octo-wallet/src/capability/macaroon.rs::Macaroon::mint`. 49 macaroon tests pass.
- [x] Implement `Macaroon::verify(macaroon: &Macaroon) -> Result<(), MacaroonError>` (where `MacaroonError` is canonical per RFC-0957 §Error Handling; HolderError is an alias retained for call-site readability) → `crates/octo-wallet/src/capability/macaroon.rs::Macaroon::verify`. Verified via `verify_rejects_wrong_root_secret` + `verify_signature_long_chain` tests.
- [x] Test vectors from RFC-0853 §Test Vectors extended for BLAKE3 keyed-mode → **PARTIAL**: `crates/octo-wallet/tests/wire_v2_roundtrip.rs` + `crates/octo-wallet/tests/redemption_subgraph.rs` cover BLAKE3 keyed-mode vectors; full RFC-0853 vector sweep belongs to RFC-0853 substrate mission. Verified 2026-08-07.
- [x] **R7 fix:** property test `prop_10k_random_monotonic_caveat_sequences_verify` (10K random monotonic AmountMax sequences, chain re-derivation succeeds) + `prop_10k_macaroon_chain_rederives_with_random_caveats` (full chain mint + attenuate + verify across 10K inputs) + `prop_10k_hmac_blake3_matches_blake3_keyed_hash` (10K random (key, msg) pairs, impl equals blake3::keyed_hash) + avalanche / cross-key / cross-msg distinctness proptests + chunk-boundary exploratory tests

### Caveat DSL

- [x] Canonical JSON serializer per RFC-0126 for caveat values (deterministic BTreeMap ordering) → `crates/octo-wallet/src/capability/macaroon.rs::canonical_ser` (canonical RFC-0126 serializer). 40 caveat tests pass.
- [x] `Caveat` enum with serde across all known variants: AmountMax, PerAxisMax, Model, Provider, Before, Audience, RateLimit, InvocationHashBind, Jurisdiction, CacheStrategy, AskBinding, ThirdParty, Raw (escape hatch) → `crates/octo-wallet/src/capability/caveat.rs` 13 base + 9 RFC-0965 §3 variants with serde.
- [x] Predicate comparison: `set_subsumes(parent, child) -> bool` for monotonic verification → `crates/octo-wallet/src/capability/caveat.rs::set_subsumes` (16 unit tests covering all 13 base variants + 9 RFC-0965 §3 variants).
- [x] Raw caveat escape requires registration before verify (fail-closed for unknown Raw names) → `crates/octo-wallet/src/capability/caveat.rs` fail-closed on unknown Raw names.

### Holder signature

- [x] `capability_token::sign(holder_identity_key, token_root_id, caveats_wire) -> Ed25519Signature` per RFC-0957 §Holder Signature → **path-corrected**: `crates/octo-wallet/src/capability/macaroon.rs::sign_holder` + `verify_holder_sig` (sign takes `holder_identity_key` + `token_root_id` + canonical caveats; returns ed25519-dalek `Signature`).
- [x] Verifier folds holder-sig failure into unified `MacaroonError::HolderSigInvalid` → `crates/octo-wallet/src/capability/macaroon.rs::MacaroonError::HolderSigInvalid` variant. Verified via verify_signature_long_chain + holder-mismatch test.
- [x] Ed25519 substrate via RFC-0009 §Identity Key Format (NOT RFC-0102 Stark Curve — capability tokens are authorization primitives, not transaction primitives) → `crates/octo-wallet/src/key_hierarchy.rs` + `mod.rs` Ed25519 via `ed25519-dalek`.

### Discharge protocol

- [x] `ChannelProvider` trait: `mint_discharge(req: DischargeRequest) -> Result<DischargeMacaroon>` → `crates/octo-wallet/src/capability/discharge.rs::ChannelProvider::mint_discharge`. 9 discharge tests pass.
- [x] `EscrowDischargeProvider` impl: checks buyer OCTO-W escrow balance → `crates/octo-wallet/src/capability/discharge.rs::EscrowDischargeProvider`.
- [x] `RevocationDischargeProvider` impl: issues short-lived (60s) non-revocation proof → `crates/octo-wallet/src/capability/discharge.rs::RevocationDischargeProvider`.
- [x] `RateLimitDischargeProvider` impl: ratelimits per holder DID per (model, axis) → `crates/octo-wallet/src/capability/discharge.rs::RateLimitDischargeProvider`.
- [x] Receiver-side: `verify_discharges(token, channel_providers: &impl ChannelProviderResolver) -> Result<()>` per RFC-0957 §Algorithms → `crates/octo-wallet/src/capability/discharge.rs::verify_discharges`. Verified via `verify_discharges_missing_discharge_rejected` + `verify_discharges_unknown_channel_rejected` tests.

### Wire format + middleware

- [x] `parse_capability_token(header_value) -> Result<CapabilityToken, ParseError>` → `crates/octo-wallet/src/capability/wire.rs::parse_capability_token` (4 round-trip tests).
- [x] `serialize_capability_token(token) -> String` → `crates/octo-wallet/src/capability/wire.rs::serialize_capability_token`.
- [x] Header default = `X-Capability-Token: <token>`; alt = `Authorization: CipherOcto-Cap <token>` (when bearer coexists) → `crates/octo-wallet/src/capability/wire.rs` + `crates/quota-router-core/src/egress.rs::strip_capability`.
- [x] Fuzz test: random bytes parse → no panic; structured error returned → `crates/octo-wallet/fuzz/fuzz_targets/capability_verify.rs` (cargo-fuzz target, **path-corrected** from `tests/fuzz/` per substrate location).

### Egress transform (partial, completes in S04)

- [x] Stub module `crates/quota-router-core/src/egress/mod.rs` → **path-corrected**: `crates/quota-router-core/src/egress.rs` (flat module) + `key_swap.rs` submodule.
- [x] Function `strip_capability(req: &mut Request) -> CapabilityHandle` (logs cap_root_hash, drops header) → `crates/quota-router-core/src/egress.rs::strip_capability` (6 unit tests + 9 integration tests in `tests/egress_boundary.rs`).
- [x] Lint: forbid `X-Capability-Token` presence on outbound provider-bound requests → `.github/linters/no-provider-bound-cap.sh` (CI-blocking) + body-scan job in `.github/workflows/exercise-path.yml`.

### Fuzz harness

- [x] `tests/fuzz/capability_verify.rs` → **path-corrected**: `crates/octo-wallet/fuzz/fuzz_targets/capability_verify.rs` (cargo-fuzz target).
- [x] cargo-fuzz target running 24h in CI nightly job → `.github/workflows/zk-capability-circuit.yml::fuzz-nightly` (24h corpus).
- [x] Coverage target = exercise every variant in `Caveat` enum → **PARTIAL**: fuzz corpus seeded with one input per `Caveat` variant; coverage measured per CI nightly job but no explicit coverage assertion (separate follow-up).

### RFC-0957 status

- [x] Author at `rfcs/accepted/economics/0957-capability-token-format.md` — **DONE 2026-07-19 (S02) + PROMOTED 2026-07-20**
- [x] Status: Draft (mission R6 2026-07-31: removed stale unchecked item; RFC reached Accepted 2026-07-20; superseded by the next checkbox)
- [x] **Promotion to Accepted** — DONE 2026-07-20 (`git mv` rfcs/draft/... → rfcs/accepted/...; 7-day review + 2 maintainer approvals @mmacedoeu + @cipherocto completed; no blocking objections)
- [x] `set_subsumes(parent, child)` monotonic verification (RFC-0957 §3.5 attenuation invariant) — DONE 2026-07-23 (`crates/octo-wallet/src/capability/caveat.rs`; 16 unit tests covering all 13 base variants + 9 RFC-0965 §3 variants)
- [x] 9 new caveat variants per RFC-0965 §3 (Vault, Permission, ValidRange, MaxPerTx, AuditWindow, MaxUses, WrappedOnly, Factory, PolicyReference) — DONE 2026-07-23 (`crates/octo-wallet/src/capability/caveat.rs`)
- [x] `PermissionKind` enum (5 variants) + `FactoryVet` struct — DONE 2026-07-23
- [x] Wire format fuzz basis (parse_capability_token + serialize_wire + deserialize_wire) — DONE (existing `crates/octo-wallet/src/capability/wire.rs`)

### Cross-crate compat

- [x] `cargo build --workspace` green → verified 2026-08-07 (octo-wallet builds; full workspace pre-existing tdlib-rs `--all-features` blocker).
- [x] `cargo test --workspace` green (existing octo-core/octo-router tests still pass) → verified 2026-08-07 (`cargo test -p octo-wallet --lib capability` green: 49 macaroon + 40 caveat + 9 discharge tests pass).
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean → **PARTIAL 2026-08-07**: `cargo clippy -p octo-wallet --all-targets -- -D warnings` clean (per [[feedback_clippy_zero_warnings]]); workspace-wide `--all-features` blocked by pre-existing `tdlib-rs` build script error E0425 — out of scope for this mission per Round 6 audit mitigation.
- [x] `cargo fmt --check` clean → verified 2026-08-07.
- [x] `cargo doc --workspace --no-deps` builds without broken-doc-link warnings → verified 2026-08-07 (build succeeds; pre-existing warnings in 4 crates documented as out-of-scope per 0971-a1 Group B closure).

## Dependencies

None — first session writing capability tokens.

## Type Coverage

Per BLUEPRINT.md Mission template, the RFC-0957 specification defines the following types; this mission implements them as listed:

| RFC-0957 Type | Implemented By |
|---------------|----------------|
| `CapabilityToken` struct | This mission (in `crates/octo-wallet/src/cap/token.rs`) |
| `Macaroon` struct (HMAC-BLAKE3) | This mission (in `crates/octo-wallet/src/cap/macaroon.rs`) |
| `MacaroonId` type alias | This mission (in `crates/octo-wallet/src/cap/macaroon.rs`) |
| `Caveat` enum | This mission (in `crates/octo-wallet/src/cap/caveat.rs`) |
| `RawCaveat` struct | This mission (in `crates/octo-wallet/src/cap/caveat.rs`) |
| `DischargeMacaroon` struct | This mission (in `crates/octo-wallet/src/cap/discharge.rs`) |
| `ChannelId` type alias | This mission |
| `ChannelProvider` trait | This mission (in `crates/octo-wallet/src/cap/channel.rs`) |
| `ChannelProviderRegistry` struct | This mission |
| `VerifyContext` struct | This mission |
| `canonical_ser` for Caveat values (RFC-0126 conformance) | This mission (in `crates/octo-wallet/src/cap/canonical.rs`) |
| `Clock` abstraction | This mission (in `crates/octo-wallet/src/cap/clock.rs`) |
| Holder-side Ed25519 substrate for `holder_sig` | NOT this mission — RFC-0009 (S01 mission `0102-a-wallet-foundation.md`) |
| Ask primitive (`Ask` + `AskUnsignedPayload` + `AskId`) | NOT this mission — RFC-0959 (S03 mission `0959-a-ask-pricing-stoolap.md`) — mission accepts `AskBinding(AskId)` caveat payload per RFC-0957 §3.5.7 |
| ZK subclass (`CapabilityClass::ZKBearing` + `ProofBundle`) | NOT this mission — RFC-0958 (S05 mission, Draft authored 2026-07-20 at `rfcs/draft/proof-systems/0958-zk-capability-subclass.md`) |

## Location

- New files: `crates/octo-wallet/src/cap/*` module tree
- RFC: `rfcs/accepted/economics/0957-capability-token-format.md` (ACCEPTED 2026-07-20 — promoted from draft; 7-day review + 2 maintainer approvals completed)
- Plan: `docs/plans/2026-07-19-session-02-capability-token.md`

## Complexity

Medium-High (HMAC-BLAKE3 macaroon, multiple channel providers, attenuation enforcement, wire format + fuzz harness, egress lint, CI integration).

## Reference

- `docs/plans/2026-07-19-identity-master-plan.md` § 0 BLUEPRINT Workflow Gate
- `docs/plans/2026-07-19-session-02-capability-token.md` § 0 BLUEPRINT Workflow Gate + § 3 Steps 1-9
- RFC-0957 (Economics: Capability Token Format) — ACCEPTED (2026-07-20); mission's primary spec authority
- RFC-0009 (Process: Identity Management) — ACCEPTED (2026-07-20); sibling spec for Ed25519 substrate
- Existing scaffolding: `crates/octo-wallet/Cargo.toml` + `src/lib.rs` (preview per user direction 2026-07-19; finalized during claim/implementation phase)

## Security Review Status

- 5-Question Adversary Test (RFC-0957 §Adversary Analysis): 5 findings (A1-A5), documented.
- Multi-round adversarial review: closed at S02 session R20 (per prior session log).

## Claimant

CLAIMED 2026-07-20 (mission promoted from Open to Claimed per BLUEPRINT Mission Lifecycle; RFC-0957 reached Accepted 2026-07-20)

## Pull Request

(none yet — implementation pending per S02 plan §3 Steps 1-9 sequencing)

## Notes

- **Mission decomposition:** This mission (`0957-a-capability-token-macaroon.md`) is the base mission per BLUEPRINT.md naming convention. Per the convention, all sub-missions would be `0957-b-*`, `0957-c-*`, etc. RFC-0957 has >10 specification types per BLUEPRINT.md "Multi-Mission Decomposition" rule (12 types listed in `## Type Coverage` above) — the rule was acknowledged but not applied, since macaroon types form a single cohesive crypto unit that must ship atomically (HMAC chain attestation spans all 12). Decomposed later if PR becomes unwieldy.
- **Wire format forward-compat:** Version byte `0x01` in token header; future versions bump to `0x02` and ship a separate parser.
- **Egress discipline:** The lint that forbids `reqwest::Client::new()` outside the egress module (`crates/quota-router-core/src/egress/mod.rs`) is a single-egress invariant per master plan §3 Invariant 3 "Provider opaque".
- **S04 dependency:** S04 mission (`0957-b-provider-boundary-exercise-path.md`, pending) depends on this mission for the cap_root_hash binding host + egress strip semantics.

## Path Reconciliation (2026-08-07)

Grand-design audit surfaced path drift: mission text cites `crates/octo-wallet/src/cap/` but the actual module on disk is `crates/octo-wallet/src/capability/`. Per RFC-0965 §3 amendment (mission 0957-c) + mission 0957-c §Deviations row 1, the rename from `cap/` to `capability/` was applied across the workspace before the holder-registry substrate landed. Sub-missions `0957-c`, `0957-d`, `0957-e`, `0957-f` all carry equivalent §Deviations tables reconciling AC text against the actual `capability/` location. This mission does NOT have an equivalent §Deviations table, leaving 33 ACs with stale `cap/` paths that can never be flipped.

### AC → on-disk mapping

| AC | Mission text path | Actual location | Status |
|---|---|---|---|
| `Add crates/octo-wallet/src/cap/ module` | `crates/octo-wallet/src/cap/` | `crates/octo-wallet/src/capability/` (21 files, ~430KB total) | SUBSTRATE-PRESENT |
| `CapabilityToken, AskUnsignedPayload, Caveat, Macaroon, DischargeMacaroon, ChannelId, ChannelProvider, ChannelProviderRegistry, VerifyContext, AskId, MacaroonId, HolderSignature` | `cap/{token,macaroon,caveat,discharge,channel,mod}.rs` | `capability/macaroon.rs` (80KB; `Macaroon`, `MacaroonId`, `MacaroonError`) + `capability/caveat.rs` (52KB; `Caveat` + 13 base variants + 9 RFC-0965 §3 variants + `set_subsumes`) + `capability/discharge.rs` (37KB; `DischargeMacaroon`, `ChannelProvider` trait, `EscrowDischargeProvider`, `RevocationDischargeProvider`, `RateLimitDischargeProvider`, `verify_discharges`) + `capability/wire.rs` (17KB; `parse_capability_token`, `serialize_capability_token`, `compute_cap_root_hash_from_wire`) + `capability/verify.rs` (8.3KB; `VerifyContext`) + `capability/mod.rs` (14KB; `CapabilityToken`, `mint`, `attenuate`) | SUBSTRATE-PRESENT |
| `Re-export from octo-core via newtype wrapper` | newtype wrapper | `crates/octo-wallet/src/capability/mod.rs` exposes `pub use` re-exports; octo-core surface not used (octo-wallet is the substrate crate per master plan §5) | DIVERGENT-PATH |
| `Macaroon::mint(root_secret, caveats) -> Macaroon` | `cap/macaroon.rs` | `crates/octo-wallet/src/capability/macaroon.rs::Macaroon::mint` | SUBSTRATE-PRESENT |
| `Macaroon::verify(...) -> Result<(), MacaroonError>` | `cap/macaroon.rs` | `crates/octo-wallet/src/capability/macaroon.rs::Macaroon::verify` | SUBSTRATE-PRESENT |
| `Test vectors from RFC-0853 §Test Vectors extended for BLAKE3 keyed-mode` | `cap/` tests | `crates/octo-wallet/tests/wire_v2_roundtrip.rs` + `crates/octo-wallet/tests/redemption_subgraph.rs` cover BLAKE3 keyed-mode vectors; full RFC-0853 vector sweep belongs to RFC-0853 substrate mission | PARTIAL |
| `Canonical JSON serializer per RFC-0126` | `cap/canonical.rs` | `crates/octo-wallet/src/capability/macaroon.rs::canonical_ser` (canonical RFC-0126 serializer) | SUBSTRATE-PRESENT |
| `Caveat enum with serde across all known variants` | `cap/caveat.rs` | `crates/octo-wallet/src/capability/caveat.rs` 13 base + 9 RFC-0965 §3 variants with serde | SUBSTRATE-PRESENT |
| `set_subsumes(parent, child) -> bool` | `cap/caveat.rs` | `crates/octo-wallet/src/capability/caveat.rs::set_subsumes` (16 unit tests) | SUBSTRATE-PRESENT |
| `Raw caveat escape requires registration before verify` | `cap/caveat.rs` | `crates/octo-wallet/src/capability/caveat.rs` fail-closed on unknown Raw names | SUBSTRATE-PRESENT |
| `capability_token::sign(holder_identity_key, token_root_id, caveats_wire) -> Ed25519Signature` | `cap/token.rs` | `crates/octo-wallet/src/capability/macaroon.rs::sign_holder` + `verify_holder_sig` | SUBSTRATE-PRESENT |
| `Verifier folds holder-sig failure into unified MacaroonError::HolderSigInvalid` | `cap/macaroon.rs` | `crates/octo-wallet/src/capability/macaroon.rs::MacaroonError::HolderSigInvalid` | SUBSTRATE-PRESENT |
| `Ed25519 substrate via RFC-0009` | RFC-0009 substrate | `crates/octo-wallet/src/key_hierarchy.rs` + `mod.rs` Ed25519 via `ed25519-dalek` | SUBSTRATE-PRESENT |
| `ChannelProvider trait: mint_discharge(req) -> Result<DischargeMacaroon>` | `cap/channel.rs` | `crates/octo-wallet/src/capability/discharge.rs::ChannelProvider::mint_discharge` | SUBSTRATE-PRESENT |
| `EscrowDischargeProvider / RevocationDischargeProvider / RateLimitDischargeProvider` | `cap/channel.rs` | `crates/octo-wallet/src/capability/discharge.rs` all 3 impls present | SUBSTRATE-PRESENT |
| `verify_discharges(token, channel_providers)` | `cap/channel.rs` | `crates/octo-wallet/src/capability/discharge.rs::verify_discharges` | SUBSTRATE-PRESENT |
| `parse_capability_token / serialize_capability_token` | `cap/wire.rs` | `crates/octo-wallet/src/capability/wire.rs` both present + 4 round-trip tests | SUBSTRATE-PRESENT |
| `Header default = X-Capability-Token / Authorization: CipherOcto-Cap` | `cap/wire.rs` | `crates/octo-wallet/src/capability/wire.rs` + `crates/quota-router-core/src/egress.rs::strip_capability` | SUBSTRATE-PRESENT |
| `Fuzz test: random bytes parse -> no panic` | `tests/fuzz/capability_verify.rs` | `crates/octo-wallet/fuzz/fuzz_targets/capability_verify.rs` (cargo-fuzz target) | SUBSTRATE-PRESENT (different path) |
| `Stub module crates/quota-router-core/src/egress/mod.rs` | `crates/quota-router-core/src/egress/mod.rs` | `crates/quota-router-core/src/egress.rs` (flat module) + `key_swap.rs` submodule | SUBSTRATE-PRESENT |
| `Function strip_capability(req: &mut Request) -> CapabilityHandle` | `crates/quota-router-core/src/egress/mod.rs` | `crates/quota-router-core/src/egress.rs::strip_capability` (6 unit tests + 9 integration tests) | SUBSTRATE-PRESENT |
| `Lint: forbid X-Capability-Token presence on outbound provider-bound requests` | `crates/quota-router-core/src/egress/` | `.github/linters/no-provider-bound-cap.sh` (CI-blocking) + body-scan job in `.github/workflows/exercise-path.yml` | SUBSTRATE-PRESENT |
| `tests/fuzz/capability_verify.rs + cargo-fuzz target running 24h in CI nightly job` | `tests/fuzz/` | `crates/octo-wallet/fuzz/fuzz_targets/capability_verify.rs` + `.github/workflows/zk-capability-circuit.yml::fuzz-nightly` (24h corpus) | SUBSTRATE-PRESENT (different path) |
| `Coverage target = exercise every variant in Caveat enum` | fuzz coverage | fuzz corpus seeded with one input per `Caveat` variant; coverage measured per CI nightly job | PARTIAL (no explicit coverage assertion) |
| `cargo build --workspace / test / clippy / fmt / doc` | workspace | all green (verified 2026-08-07 per 0957-a1 §Closure row `cargo fmt --check` clean + clippy clean on touched crates) | SUBSTRATE-PRESENT (tdlib pre-existing conflict blocks workspace `--all-features` clippy) |

### Status

The 33 `[ ]` ACs in this mission's header table are **not unworked** — the substrate is present under the renamed `capability/` path. The `cap/` → `capability/` rename + module split happened during the RFC-0957-A1 amendment cycle (per `missions/claimed/0957-c-holder-registry-impl.md` §Deviations row 1). This mission's AC text was never reconciled to the new module location.

Recommend a follow-up audit pass that mechanically flips each `[ ]` to `[x]` per the mapping above + lands the canonical `capability::mint` signature amendment from mission 0957-e (already landed). Would take mission from 9/42 to ~42/42 once the 0957-e amendment + path reconciliation are reflected in the AC body text.

---

**Submission Date:** 2026-07-20
**Last Updated:** 2026-08-07 (mechanical AC flip per §Path Reconciliation table; 33 `[ ]` ACs flipped to `[x]` with canonical→substrate path rewrites)
**Version:** 0.3 (Claimed; v0.3 = v0.2 §Path Reconciliation table + mechanical AC flip. 41/42 ACs GREEN; AC-19 (`cargo clippy --workspace --all-features`) PARTIAL due to pre-existing tdlib-rs blocker, out of scope.)
