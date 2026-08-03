# Mission: Wire Resolver + VerifyContext Extension (RFC-0957-A1 §Phase 2)

## Status

Open

## RFC

RFC-0957-A1 (Economics): Holder Registry + Catalog Storage (Amendment) — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0957-a1-holder-registry.md` (top-level decomposition mission)

## Summary

Implement RFC-0957-A1 §Phase 2: the wire format helper + VerifyContext extension. Author `compute_cap_root_hash_from_wire(s: &str) -> Result<[u8; 32], WireError>` that derives the BLAKE3 root hash from a 3-segment wire string (`base64url(macaroon) || "." || base64url(holder_sig) || "." || base64url(discharges_bag)`) without the `holder_did` parameter. Extend `VerifyContext` with a `holder_registry: Arc<dyn HolderRegistry>` slot so the verifier can resolve `holder_did` + `holder_pub` from the registry before calling `deserialize_wire(s, holder_did, holder_pub)`.

This sub-mission preserves wire format compatibility (G2: wire bytes byte-identical pre/post amendment). The helper is additive — the existing `deserialize_wire(s, holder_did, holder_pub)` is unchanged. `compute_cap_root_hash_from_wire` is the new lookup key path.

## Acceptance Criteria

### Wire format helper

- [ ] `crates/octo-wallet/src/capability/wire.rs` (EXTEND) — `compute_cap_root_hash_from_wire(s: &str) -> Result<[u8; 32], WireError>`. Parses 3-segment wire, decodes base64url-no-pad segments, computes BLAKE3 over the canonical concatenation per RFC-0957 §Algorithms + RFC-0853 §BLAKE3 keyed-hash mode.
- [ ] Same BLAKE3 derivation as `CapabilityToken::cap_root_hash()` (verified by property test: 100K random token + wire pairs, `compute_cap_root_hash_from_wire(&serialize_wire(&token))` equals `token.cap_root_hash()`).
- [ ] Existing `deserialize_wire(s, holder_did, holder_pub)` UNCHANGED. Wire bytes byte-identical pre/post amendment (TV7).
- [ ] Unit test: malformed wire (wrong segment count, bad base64url) returns `WireError::MalformedSegment`; no panic.

### VerifyContext extension

- [ ] `crates/octo-wallet/src/capability/verify.rs` (EXTEND) — `VerifyContext` struct gains `holder_registry: Arc<dyn HolderRegistry>` slot. Existing slots preserved.
- [ ] New constructor `VerifyContext::with_registry(clock: Arc<dyn Clock>, registry: Arc<dyn HolderRegistry>) -> Self` — replaces the prior 1-arg constructor as the canonical form.
- [ ] `verify_with_resolve(token_wire: &str) -> Result<VerifiedToken, VerifyError>` — high-level helper that calls `compute_cap_root_hash_from_wire`, looks up `HolderRecord` via the registry, extracts `holder_did` + `holder_pub`, then calls `deserialize_wire` + `Macaroon::verify` + holder-sig verify.
- [ ] Documented delta in RFC-0957-A1 §Appendices: "VerifyContext extension" appendix added at promotion time.

### Test vectors (RFC-0957-A1 §Test Vectors, this sub-mission owns TV5, TV7, TV15)

- [ ] TV5: Cross-Node Mint Verifiability — node A mints capability, syncs to node B (RFC-0862 gossip), node B's `VerifyContext::with_registry(...).verify_with_resolve(wire)` returns `Ok(VerifiedToken)` end-to-end.
- [ ] TV7: Wire Format Unchanged — `git diff` of 100 representative wire samples pre/post this sub-mission: zero byte difference. Property test: 10K random `(root_secret, holder, holder_did, initial_caveats)` tuples, serialize + parse round-trip, wire bytes stable.
- [ ] TV15: HopCapability Holder vs Audience — for a `HolderKind::HopCapability` record, `holder_did` (intermediate router) MUST differ from `audience_did` (destination). Unit test inserts a HopCapability record; `verify_with_resolve` confirms holder ≠ audience.

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace` green
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Dependencies

**Requires (RFC gates):**

- RFC-0853 — BLAKE3 keyed-hash primitive source
- RFC-0862 — gossip sync (consumed by TV5)

**Requires (mission gates):**

- `missions/open/0957-a1-holder-registry.md` (top-level)
- `missions/open/0957-c-holder-registry-impl.md` — `HolderRegistry` trait + `HolderRecord` MUST exist before this sub-mission compiles

```yaml
depends_on:
  - 0957-c-holder-registry-impl # HolderRegistry trait + HolderRecord
  - 0957-a-capability-token-macaroon # base Macaroon::verify + deserialize_wire
```

## Type Coverage

This sub-mission implements (per top-level Type Coverage table):

- `compute_cap_root_hash_from_wire` helper
- `VerifyContext::holder_registry` slot extension

## Location

- `crates/octo-wallet/src/capability/wire.rs` (EXTEND)
- `crates/octo-wallet/src/capability/verify.rs` (EXTEND)

## Claimant

@unclaimed

## Pull Request

(unset)

## Notes

- Wire format unchanged per RFC-0957-A1 §Out of Scope. The helper is purely additive — no change to `serialize_wire` or `deserialize_wire`.
- `VerifyContext::holder_registry` slot is shared with sub-mission 0969-a (gateway authenticator consumes the same slot via `Arc<dyn HolderRegistry>`).
- TV15 (HopCapability holder vs audience) is a cross-mission test — depends on `HolderRecord::from_hop_capability` constructor from sub-mission 0970-a. If 0970-a lands first, this test is straightforward. If 0970-a lands later, this test is `[ ]` until then.
