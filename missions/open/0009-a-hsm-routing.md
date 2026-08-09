# Mission: 0009-a — Wallet HSM Routing (RFC-0009 HsmAdapter Gap Closure)

## Status

Open (2026-08-08). RFC-0009 amendment adds the HSM routing requirement; this mission implements the gap closure.

## RFC

RFC-0009 (Process): Identity Management

**BLUEPRINT gate note:** RFC-0009 is Accepted. Mission 0009-a implements the HsmAdapter Integration mandate.

This mission closes the wallet-side HSM routing gap surfaced by the 2026-08-08 specialized node protocol research (`docs/research/2026-08-08-specialized-node-protocol-research.md`). `IdentityKey::sign` in `crates/octo-wallet/src/identity.rs` currently calls `ed25519_dalek::SigningKey::from_bytes(...).sign(...)` directly — bypassing the `HsmAdapter` trait in `crates/octo-wallet/src/hsm.rs`. Hardware wallets (`LedgerSigner`) cannot sign capability tokens today.

## Summary

Refactor `IdentityKey` (and every signing call site in `octo-wallet`) to route through `Arc<dyn HsmAdapter>` rather than direct `ed25519_dalek::SigningKey` access. Production parity preserved via `InMemorySigner` default impl. After the refactor, a Ledger device can sign capability mint + Ask payloads + capability attenuation end-to-end without host-side seed exposure.

## Acceptance Criteria

### Top-level: HSM routing closure

- [ ] `crates/octo-wallet/src/identity.rs::IdentityKey` holds `signer: Arc<dyn HsmAdapter>` (CHANGED: was `SigningKey` directly)
- [ ] `IdentityKey::sign(msg: &[u8]) -> Result<Signature, WalletError>` delegates to `self.signer.sign(msg)` and returns the wrapped `Signature` (CHANGED: was `self.0.sign(msg)` directly)
- [ ] `IdentityKey::generate()` constructs `IdentityKey` with `signer: Arc::new(InMemorySigner::new(seed_bytes, public_key))` (default impl preserved for MVP)
- [ ] `IdentityKey::from_seed(seed: [u8; 32]) -> Self` constructs with same default `InMemorySigner`
- [ ] `HolderSignError` / `WalletError::Hsm(HsmError)` error variant exists for HSM failure propagation
- [ ] Every signing call site in `octo-wallet` updates to use `Result` return type:
  - `crates/octo-wallet/src/identity.rs::sign` callers
  - `crates/octo-wallet/src/capability/macaroon.rs` (holder signature on mint)
  - `crates/octo-wallet/src/capability/zk_mint.rs` (mint_with_zk signature)
  - `crates/octo-wallet/src/capability/discharge.rs` (discharge macaroon signatures)
  - `crates/octo-wallet/src/capability/redemption.rs` (redemption signatures)
- [ ] Test fixture: `InMemorySigner` produces byte-identical signatures to pre-refactor `ed25519_dalek::SigningKey::sign` for the same input (determinism parity check)
- [ ] Test fixture: `LedgerSigner` smoke test (existing test in `crates/octo-wallet/src/hsm/tests/ledger_signer_smoke.rs`) continues to pass; production LedgerSigner (real APDU) deferred to separate mission
- [ ] `cargo test -p octo-wallet --lib` green
- [ ] `cargo test -p octo-wallet --test eleven_step_zk` green
- [ ] `cargo test -p octo-wallet --test capability_zk_acceptance` green
- [ ] `cargo clippy -p octo-wallet --all-targets -- -D warnings` clean (per `[[feedback_clippy_zero_warnings]]`)
- [ ] `cargo fmt --check` clean (per `[[cargo-fmt-workflow]]`)

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace --lib` green
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` green

### Adversary coverage (per RFC-0009 A9 + A10)

- [ ] A9 (host-side seed exfiltration): `IdentityKey::seed_bytes()` is REMOVED from public API. Seed exists only inside `InMemorySigner` (MVP) or hardware secure element (production). Unit test: pre-refactor `IdentityKey::seed_bytes()` does NOT compile.
- [ ] A10 (malicious host signs for user): `LedgerSigner::sign` requires explicit on-device confirmation per `HsmError::UserRejected`. Hardware wallet test fixture: simulated reject returns `HsmError::UserRejected`, propagates as `WalletError::Hsm(HsmError::UserRejected)`.

## Dependencies

**Requires:**

- RFC-0009 — HSM routing requirement
- RFC-0853 §F2 — HSM substrate specification
- `crates/octo-wallet/src/hsm.rs` — `HsmAdapter` trait + `InMemorySigner` + `LedgerSigner` impls (existing)

**Mission gates:**

- RFC-0009 amendment (committed 2026-08-08; this mission)

**Not Requires:**

- Production LedgerSigner (APDU over USB HID) — tracked under RFC-0850 §F2 future work; no mission filed yet (will file under `missions/open/` when production APDU work starts; not a phantom pointer per `[[no-phantom-mission-pointers]]` because the substrate is documented in RFC-0850)
- Per-extension crate extraction (RFC-0957; separate missions `0957-ext-*`)

## Implementation Guide

- `crates/octo-wallet/src/hsm.rs` — `HsmAdapter` trait already defined; no changes
- `crates/octo-wallet/src/identity.rs` — refactor `IdentityKey` struct; update `sign` + `generate` + `from_seed`
- `crates/octo-wallet/src/capability/*.rs` — update signing call sites to use `Result` propagation
- Test parity: existing tests must continue to pass with byte-identical signatures from `InMemorySigner` (determinism)

## Decomposition Rationale

RFC-0009 HSM routing is multi-file (identity.rs + 4 capability files + tests). Below the BLUEPRINT §Multi-Mission Decomposition threshold (>10 types, >4 phases, different prerequisite chains). Single mission.

## Claimant

@unassigned (per `[[feedback_initiation_user_only]]` — user initiates the claim)

## Pull Request

(unset)

## Notes

- This mission is the wallet-side complement to the 2026-08-08 specialized node protocol research + RFC-0871 §Implementation Phase 2.
- Mission `0010-d-wallet-audience-validation.md` is the OTHER gap closure (AudienceId DID validation). These two missions together close the wallet-side foundational gaps surfaced by the audit. Both are independent of RFC-0871 acceptance — claimable today.
- Per `[[deferred-vs-unspecified]]` named-owner rule: this mission has a concrete scope (RFC-0009 HsmAdapter Integration). No further deferral.
- Production LedgerSigner (real APDU over USB HID) is tracked under RFC-0850 §F2 future work. No mission file exists today; a new mission will be filed under `missions/open/` (slug TBD at work-start; not pre-named to avoid phantom pointer per `[[no-phantom-mission-pointers]]`) when production APDU work begins.

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-08 | Mission filed. RFC-0009 amendment adds HsmAdapter Integration requirement; mission captures gap closure scope. Cross-references RFC-0871 §Implementation Phase 2 + RFC-0009 §HsmAdapter Integration. |

Last Updated: 2026-08-08
Version: 0.1