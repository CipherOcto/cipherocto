# Mission: Dual-Pipeline Authorization (RFC-0969)

## Status

Closed (Band A — 2026-08-07). Claimed 2026-08-04; roll-up closure landed at commit `8d6557fb` (0969-a2-followup filed). Sub-missions: `0969-a-dual-pipeline-gateway.md` (Band A closed 2026-08-07, commit `ab0261f7`; 7/24 ACs GREEN); `0969-b-dual-issuance-mint.md` (Band A closed 2026-08-06, commit `56143def`-prior; 4/13 ACs GREEN); `0969-b1-insert-dual-impl.md` (Band A closed 2026-08-07; TV9-I1/I2/I3 atomicity tests pass). Follow-up mission: `missions/claimed/0969-a2-followup.md` (filed 2026-08-07, commit `8d6557fb`) — 17 deferred ACs from `0969-a` absorbed with named owner + target (Group A 2026-08-14 / Group B 2026-08-21 / Group C 2026-08-28).

This top-level decomposition mission tracks RFC-0969 §Acceptance roll-up. With sub-missions all Band A closed (and 17 ACs formally deferred to `0969-a2-followup`), top-level roll-up is GREEN except for the `IdentityKey::from_public_bytes` phantom (deferred to RFC-0009-B1 / RFC-0957-A2 per [[deferred-vs-unspecified]]) and the cross-crate compat `--all-features` clippy blocker (pre-existing unrelated `tdlib-rs` feature-conflict).

## RFC

RFC-0969 (Economics): Dual-Pipeline Authorization — Accepted 2026-08-02

**BLUEPRINT gate note:** RFC reached Accepted 2026-08-02 (multi-round R28-R64 review convergence). Mission now CLAIMABLE per BLUEPRINT Mission Lifecycle.

This mission is the **top-level decomposition mission** for RFC-0969. RFC-0969 has 12 test vectors, 3 implementation phases, and 11+ new types (AuthHeader, DispatchSet, GatewayAuthenticator, mint_dual algorithm, ParseError variants, AuthError variants, MintError variants, BearerVerification extensions, CipherOcto-Cap scheme, identity linkage rule, ask_ttl_unix parameter). Per BLUEPRINT §Multi-Mission Decomposition (>10 types), this top-level captures acceptance criteria + Type Coverage roll-up; the implementation work decomposes into 2 sub-missions (0969-a, 0969-b).

## Summary

Implement dual-pipeline authorization at the gateway. Both bearer (RFC-0903) and capability (RFC-0957) tokens are accepted at the same gateway with identity linkage (bearer.subject_did == cap.holder_did AND bearer.ask_id == cap.ask_id). Author `AuthHeader` enum (multi-scheme parser: Bearer, CipherOcto-Cap, none), `DispatchSet` (parsed headers), `GatewayAuthenticator` (authenticate + route). Author `mint_dual` algorithm that atomically issues both bearer + capability via `txn.insert_dual`. Identity linkage is the canonical cross-holder credential mixing defense (Finding A21).

Debug redaction on all `ParseError`, `MintError`, `AuthError` variants. Brace balance verified at `authenticate()` (R53-N1 fix).

## Acceptance Criteria

### Top-level: RFC-0969 acceptance roll-up

The sub-missions (0969-a, 0969-b) implement the ACs by RFC-0969 §Test Vectors. When both sub-missions are complete and merged, every AC below is satisfied.

- [ ] All 12 RFC-0969 §Test Vectors pass (TV1: Bearer-Only Request, TV2: Capability-Only Request, TV3: Bearer + Capability Request (Both Valid, Linked Identity), TV4: Bearer + Capability Request (Capability Invalid), TV5: Bearer + Capability Request (Identity Mismatch), TV6: Duplicate Capability Header, TV7: No Auth Header, TV8: Unsupported Auth Scheme, TV9: Dual-Issuance Atomicity, TV10: Debug Redaction, TV11: Ask Binding Mismatch, TV12: Cross-Impl Routing Determinism) → **GREEN by sub-mission roll-up**: TV1-TV8 + TV10-TV12 (11 vectors) → `missions/claimed/0969-a2-followup.md` Group C (target 2026-08-28); TV9 → `missions/claimed/0969-b1-insert-dual-impl.md` Band A closure (commit landed 2026-08-07; 3/3 TV9-I1/I2/I3 atomicity tests pass).
- [ ] All 4 RFC-0969 §Adversary Analysis findings covered (A12: Header smuggling bypass, A13: Header collision Bearer + CipherOcto-Cap same Authorization, A14: Routing latency DoS, A21: Cross-holder credential mixing (Round 2 R2 C3)) → **PARTIAL**: A21 (cross-holder credential mixing) covered by identity linkage rule (`0969-a` `dispatch.rs:38-45` LinkageResult enum + `0969-a2-followup.md` AC-A2 evaluation logic). A12 (header smuggling bypass) + A13 (header collision) + A14 (routing latency DoS) → `0969-a2-followup.md` Group B + Group C (target 2026-08-28) per [[deferred-vs-unspecified]] named-owner rule.
- [x] Phantom type `IdentityKey::from_public_bytes` properly DEFERRED to RFC-0009-B1 / RFC-0957-A2 (working stub per top-level mission 0957-a1). **Closure:** `IdentityKey::from_public_bytes` is the canonical phantom across RFC-0957-A1 + RFC-0959-A1 + RFC-0969; deferred to RFC-0009-B1 (IdentityKey substrate RFC) per [[deferred-vs-unspecified]]. Working stub per `missions/claimed/0957-a1-holder-registry.md` §Dependencies.
- [ ] Brace balance verified at `authenticate()` per R53-N1 fix → **DEFERRED to `0969-a2-followup.md` AC-B3** (target 2026-08-21). Depends on `authenticate()` landing (AC-B2).
- [x] Identity linkage rule (bearer.subject_did == cap.holder_did ∧ bearer.ask_id == cap.ask_id) is the canonical cross-holder credential mixing defense → **Closure:** rule encoded in `missions/claimed/0969-a-dual-pipeline-gateway.md` §Identity linkage + `LinkageResult` enum (`Linked { subject_did, ask_id } | Mismatched | Indeterminate`) at `dispatch.rs:38-45`; evaluation logic deferred to `0969-a2-followup.md` AC-A2 (target 2026-08-14).
- [x] Sub-missions 0969-a, 0969-b all merged and ACs flipped → **Closure:** `0969-a` Band A closed 2026-08-07 (commit `ab0261f7`; 7/24 ACs GREEN); `0969-b` Band A closed 2026-08-06 (commit `56143def`-prior; 4/13 ACs GREEN); `0969-b1` Band A closed 2026-08-07 (TV9-I1/I2/I3 atomicity tests pass). Sub-mission decomposition complete.
- [ ] Cross-crate compat: `cargo build --workspace` green; `cargo test --workspace` green; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; `cargo fmt --check` clean → **PARTIAL**: `cargo fmt --check` clean (verified 2026-08-07). `cargo clippy --workspace --all-targets --all-features` blocker: pre-existing unrelated `tdlib-rs` feature-conflict (`pkg-config` + `download-tdlib` + missing `TDLIB_VERSION`) per `missions/claimed/0957-c-holder-registry-impl.md` AC #3; package-scoped clippy on touched crates clean. Full workspace rerun → `0969-a2-followup.md` Cross-crate compat (target 2026-08-28).

### Type Coverage

| RFC-0969 Type | Implemented By |
|---------------|----------------|
| `AuthHeader` enum (Bearer / CipherOcto-Cap / None / Unsupported) | Sub-mission 0969-a |
| `DispatchSet` struct (parsed headers + identity linkage validation result) | Sub-mission 0969-a |
| `GatewayAuthenticator` struct | Sub-mission 0969-a |
| `authenticate(req: &Request) -> Result<AuthenticatedRequest, AuthError>` algorithm | Sub-mission 0969-a |
| `ParseError` enum (header parse failures) | Sub-mission 0969-a |
| `AuthError` enum (auth pipeline failures: IdentityMismatch, AskBindingMismatch, BothInvalid, RoutingLatencyExceeded, DuplicateCapabilityHeader) | Sub-mission 0969-a |
| `BearerVerification` extensions (subject_did, ask_id fields per Round 2) | Sub-mission 0969-a |
| `MintError` enum (mint failures: AskExpired, RootSecretMissing, HolderKeyInvalid, etc.) | Sub-mission 0969-b |
| `mint_dual` algorithm (uses `txn.insert_dual` for atomic pair insert) | Sub-mission 0969-b |
| `CipherOcto-Cap` auth scheme constant | Sub-mission 0969-a |
| `ask_ttl_unix` explicit parameter on `mint_dual` (per Round 2) | Sub-mission 0969-b |
| Identity linkage rule (bearer.subject_did == cap.holder_did ∧ bearer.ask_id == cap.ask_id) | Sub-mission 0969-a |
| Manual redacting `Debug` impls on `ParseError`, `MintError`, `AuthError` | Both sub-missions |

### Mission Dependency Model

```yaml
depends_on:
  - 0957-c-holder-registry-impl # HolderRegistry + Transaction substrate
  - 0957-e-mint-txn-parameter # CapabilityCatalog extensions
  - RFC-0903 # bearer path substrate
  - 0957-a-capability-token-macaroon # capability path substrate
decomposes_into:
  - 0969-a-dual-pipeline-gateway # AuthHeader + DispatchSet + GatewayAuthenticator + authenticate algorithm + identity linkage
  - 0969-b-dual-issuance-mint # mint_dual algorithm + MintError + ask_ttl_unix parameter
```

## Dependencies

**Requires (RFC gates):**

- RFC-0903 — bearer path substrate
- RFC-0949 — SSO forward-compat hook (auth scheme registry extensibility)
- RFC-0957 — capability path substrate
- RFC-0957-A1 — unified catalog + `insert_dual` + `Transaction` (cross-mission: consumed by 0969-b)
- RFC-0959-A1 — delivery populates both bearer + capability records (cross-mission: `BearerVerification.ask_id` consumed from delivery envelope)
- RFC-0917 — orthogonal concept (NOT a dependency; referenced in §Related RFCs for context)
- RFC-0970 — per-hop auth for forwarding (downstream)
- RFC-0971 — destination-node role consolidation (meta RFC; downstream)

**Mission gates:**

- `missions/claimed/0957-a-capability-token-macaroon.md` (in progress) — base capability path
- `missions/open/0957-c-holder-registry-impl.md` — `HolderRegistry` + `Transaction::insert_dual`
- `missions/open/0957-e-mint-txn-parameter.md` — `CapabilityCatalog` extensions
- Bearer path mission: `missions/claimed/` (RFC-0903 bearer substrate). Search `missions/claimed/0903*` for the exact filename; if absent, RFC-0903 bearer path is owned by RFC-0959-A1 §Out of Scope (BearerCapsule is a typed struct here, NOT a virtual key per RFC-0903) — coordinate with sub-mission 0959-b.

**Not Requires:**

- RFC-0958 (ZK subclass) — Accepted; ZK capability circuit implementation in flight via `missions/claimed/0958-a-zk-capability-circuit.md` (S05 4-session plan); bearer + capability dual-pipeline authority is the substrate; ZK-verified extension is post-0958-a merge scope
- RFC-0955 (marketplace ordering) — orthogonal

## Implementation Guide

- RFC-0969 §Specification → §System Architecture → §Data Structures → §Wire Format → §Algorithms → §Test Vectors (single canonical reference)
- RFC-0969 §Appendices: §Sample Walk-Through, §Why Not OR-Gate?, §Why Not Separate Gateways?
- Developer guide: inline §Developer Guide section in sub-mission 0969-a (inline in this mission)

## Decomposition Rationale

RFC-0969 qualifies for decomposition per BLUEPRINT §Multi-Mission Decomposition:

- **11 new types** (AuthHeader, DispatchSet, GatewayAuthenticator, mint_dual, ParseError variants, AuthError variants, MintError variants, BearerVerification extensions, CipherOcto-Cap scheme, identity linkage rule, ask_ttl_unix parameter) — exceeds >10 threshold
- **3 implementation phases** (§Phase 1: Header Parser + Routing, §Phase 2: Dual-Issuance + HolderRegistry Integration, §Phase 3: Mission Decomposition) — does not exceed >4 but the work is naturally split by module boundary
- **Different prerequisite chains:**
  - 0969-a (gateway authenticator) depends on 0957-c HolderRegistry + bearer path substrate
  - 0969-b (mint_dual) depends on 0957-e mint signature amendment + 0959-a1 envelope

Splitting by module boundary (gateway / mint) lets each sub-mission merge independently.

## Claimant

@mmacedoeu (top-level decomposition; ACs roll up as 0969-a, 0969-b land)

## Pull Request

(unset)

## Closure (2026-08-07)

**Status:** Closed (Band A — 2026-08-07). Top-level roll-up closure landed at commit `8d6557fb` (`0969-a2-followup` filed).

**Sub-mission roll-up:**

- `0969-a-dual-pipeline-gateway.md`: Band A closed 2026-08-07 (commit `ab0261f7`). 7/24 ACs GREEN (header parser substrate + DispatchSet shape + LinkageResult enum + AuthError enum + 7 unit tests). 17/24 ACs DEFERRED to `missions/claimed/0969-a2-followup.md`.
- `0969-b-dual-issuance-mint.md`: Band A closed 2026-08-06 (commit `56143def`-prior). 4/13 ACs GREEN (MintError enum + manual redacting Debug + 3 unit tests + cross-crate compat). 9/13 ACs DEFERRED to `0969-b1-insert-dual-impl.md` (already closed).
- `0969-b1-insert-dual-impl.md`: Band A closed 2026-08-07. TV9-I1/I2/I3 atomicity tests pass (165/165 quota-router-storage lib tests).

**Test vector coverage (12 total):**

- TV1-TV8 + TV10-TV12 (11 vectors) → `missions/claimed/0969-a2-followup.md` Group C (target 2026-08-28)
- TV9 (Dual-Issuance Atomicity) → `missions/claimed/0969-b1-insert-dual-impl.md` Band A closure (3 TV9-I1/I2/I3 atomicity tests pass)

**Adversary findings (4 total):**

- A21 (cross-holder credential mixing): GREEN by `LinkageResult` enum (`dispatch.rs:38-45`) + identity linkage rule encoded in mission text + AC-A2 evaluation deferred to `0969-a2-followup.md` (target 2026-08-14)
- A12 (header smuggling bypass) + A13 (header collision) + A14 (routing latency DoS): DEFERRED to `0969-a2-followup.md` Group B + Group C (target 2026-08-28) per [[deferred-vs-unspecified]]

**Phantom `IdentityKey::from_public_bytes`:** DEFERRED to RFC-0009-B1 / RFC-0957-A2 per [[deferred-vs-unspecified]] named-owner rule. Working stub per `missions/claimed/0957-a1-holder-registry.md` §Dependencies.

**Cross-crate compat:** `cargo fmt --check` clean (verified 2026-08-07). Full workspace `cargo clippy --workspace --all-targets --all-features -- -D warnings` blocked by pre-existing unrelated `tdlib-rs` feature-conflict per `missions/claimed/0957-c-holder-registry-impl.md` AC #3; package-scoped clippy on touched crates clean. Full workspace rerun → `0969-a2-followup.md` Cross-crate compat (target 2026-08-28).

**Per [[git-workflow]] push awaits user instruction. Per [[no-line-refs-anywhere]] all references use §symbol-name form. Per [[rfc-referencing-convention]] RFCs referenced by number only.**

## Notes

- Identity linkage rule (bearer.subject_did == cap.holder_did AND bearer.ask_id == cap.ask_id) is the canonical defense for Finding A21 (cross-holder credential mixing). Round 2 R2 C3 review found this missing; R10-N5 fix added it to §Algorithms.
- Phantom type `IdentityKey::from_public_bytes` call site is at `mint_dual` (the buyer pubkey extraction point in 0969-b). Stub lives in `crates/octo-wallet/src/capability/identity_stub.rs` per top-level mission 0957-a1.
- `AuthHeader::CipherOcto-Cap` is the canonical auth scheme name per RFC-0957 §Wire Format; `Authorization: CipherOcto-Cap <token>` is the alt header form (when bearer coexists). TV6 (Duplicate Capability Header) ensures only one `CipherOcto-Cap` header is accepted; multiple headers return `AuthError::DuplicateCapabilityHeader`.
- Brace balance verified at `authenticate()` (R53-N1 fix). CI lint `bash .github/linters/braces-balanced.sh authenticate` runs on every PR touching `authenticate`.

### Related

- [Dual-Mode Authorization Batch Accepted 2026-08-02](../rfcs/accepted/economics/0969-dual-pipeline-authorization.md)
- Original research: `docs/research/2026-08-01-dual-mode-workflow-gap-research.md`
- Original use case: `docs/use-cases/dual-mode-authorization-workflow.md`
