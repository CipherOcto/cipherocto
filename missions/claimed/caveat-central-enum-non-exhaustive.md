# caveat-central-enum-non-exhaustive — mark `Caveat` with `#[non_exhaustive]` (Layer A)

**Status:** claimed (2026-08-27)
**Substrate:** RFC-0965 (Caveat type catalogue) + `cipherocto-design-principles` §Extension over enumeration
**Parent:** R3 review follow-on (L4 CRITICAL #1 — central-enum upgrade-hostile)
**Depends on:**

- RFC-0965 — Caveat type catalogue (the substrate that owns the enum)
- Mission `producer-wrapper-consumer-wiring.md` (parallel) — downstream consumers that exhaustively match MUST be migrated in same commit. **Cycle ordering rule:** `caveat-central-enum-non-exhaustive` lands at the SAME commit as the producer-wrapper-consumer-wiring call-site rewrites — splitting them across commits leaves the workspace non-compiling because the missing wildcard arms (from this mission) must accompany the new `Caveat::Payment` discriminant (from the parallel mission). Both are committed together; no soft depends_on cycle exists at the git level, only at the planning level.

## Motivation

`crates/octo-cap-macaroon/src/caveat/mod.rs` declares `pub enum Caveat { ... }` (27 variants as of substrate). The `Caveat` enum is a Layer A central enum — every new caveat type becomes a central edit + cross-crate review. Per `cipherocto-design-principles` §Extension over enumeration, central enums are upgrade-hostile. The mitigation: mark the enum `#[non_exhaustive]` so downstream crates cannot exhaustively `match` it; new variants land via RFC + additive `match _ =>` arms.

The existing escape hatch `Caveat::Raw(RawCaveat)` already provides a forward-compat path for unknown types at the wire level, but does NOT prevent the enum from being a closed set at the substrate type level.

## Scope

Mark `pub enum Caveat` with `#[non_exhaustive]` at the `Caveat` decl in `crates/octo-cap-macaroon/src/caveat/mod.rs`. Audit all downstream exhaustive matches and add wildcard arms.

### Sub-steps

1. **Apply `#[non_exhaustive]` attribute** — single-line attribute insertion above the existing `#[derive(...)]` block on `pub enum Caveat`. No variant changes.

2. **Audit exhaustive `match` sites** — `grep -rn "match .*Caveat" crates/ agents/ use-cases/` returns every site that pattern-matches the enum. Each MUST add a wildcard arm OR convert to `if let` per the upgrade-friendly pattern.

3. **Migrate consumers** — known sites (verified by grep at landing time):
   - `crates/octo-cap-macaroon/src/caveat/validate.rs` — caveat validation; must add `_ => Err(ValidationError::UnsupportedCaveat)` arm
   - `crates/octo-cap-macaroon/src/caveat/attenuate.rs` — attenuation dispatch; wildcard returns `Ok(Caveat::Raw(_))` passthrough
   - Any external consumer in `octo-wallet`, `octo-wallet-node`, `octo-marketplace`, `octo-policy`, `octo-vault`, `quota-router-*` that exhaustively matches the enum

4. **Test vector migration** — any test that constructs `Caveat::Variant` via struct expression continues to work (struct expression is unaffected by `#[non_exhaustive]`). Tests that exhaustively `match` MUST add wildcard arms.

5. **RFC-0965 update** — bump Version History with `v1.2` (or appropriate) noting `Caveat` is now `#[non_exhaustive]`; the change is backward-compatible at the wire level (Serde tag-based encoding unaffected) and only affects substrate pattern matching.

## Out of Scope

- Renaming any Caveat variant
- Removing the `Raw` escape hatch (kept as defense-in-depth; even with `#[non_exhaustive]`, unknown wire-form caveats still deserialize via `Raw`)
- Adding new Caveat variants (separate RFC per variant)
- Migration of `CaveatName` (the central enum in `crates/octo-cap-macaroon/src/caveat/mod.rs`) — also a central enum but narrower scope; tracked at a separate mission if needed

## Test Vectors

- TV-CE-1: `#[non_exhaustive]` attribute present on `pub enum Caveat` (`grep -A1 "pub enum Caveat" crates/octo-cap-macaroon/src/caveat/mod.rs` returns `#[non_exhaustive]`)
- TV-CE-2: Downstream exhaustive `match` without wildcard fails to compile (regression guard: temporarily add a `match x { Caveat::AmountMax(_) => ... }` in a test crate → `cargo build` returns non-exhaustive-patterns error)
- TV-CE-3: Downstream exhaustive `match` with wildcard compiles cleanly (`match x { Caveat::AmountMax(_) => ..., _ => ... }` → `cargo build` green)
- TV-CE-4: Wire-form Serde round-trip still works: `serde_json::to_string(&Caveat::AmountMax(...))` → JSON → `serde_json::from_str` returns the same variant (no wire-format break)
- TV-CE-5: `Caveat::Raw(RawCaveat { ... })` continue to construct via struct expression (struct expression allowed by `#[non_exhaustive]` for known variants)

## Layer direction (per `cipherocto-design-principles`)

- `octo-cap-macaroon` (Layer A, RFC-frozen) — `Caveat` enum attribute change. **Layer A means this is a semver-MAJOR change** per the §Layer stability table. Justification: `#[non_exhaustive]` is the canonical mechanism for forward-compatable central enums; the alternative (leaving the enum exhaustive) is more brittle (every new variant = central edit across all consumers).
- All downstream crates (Layer B / C / D / E) — exhaustive `match` sites require wildcard arms. **Non-breaking** for downstream crates that already use wildcard arms; **breaking** for exhaustive matches (regression test TV-CE-2).

## Validation

```bash
# Pre-merge
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features full -- -D warnings

# Grep gates
grep -rn "match .*Caveat\b" crates/ agents/ use-cases/
# Each match must end in a wildcard arm (manual review)

grep -B1 "^pub enum Caveat" crates/octo-cap-macaroon/src/caveat/mod.rs
# Expect: `#[non_exhaustive]` directly preceding `pub enum Caveat`

# Wire-form regression
cargo test --workspace --lib caveat::  # all caveat tests pass
```

## Backward compat

- **Wire form (Serde):** UNCHANGED. `#[serde(tag = "type", content = "value")]` is independent of `#[non_exhaustive]`. Encoded payloads round-trip identically.
- **Substrate pattern matching:** BREAKING for exhaustive `match` arms without wildcards. Justified per `cipherocto-design-principles` §Extension over enumeration.
- **Substrate struct expression:** UNCHANGED. `Caveat::AmountMax(...)` still constructs via struct expression.

**Semver impact:** Layer A semver-MAJOR (enforced per `cipherocto-design-principles` §Layer stability). Cross-crate review required for this bump.

## Risk

- HIGH: Layer A semver-MAJOR churn — every consumer crate (Layer B / C / D / E) carrying an exhaustive `match` on `Caveat` must add a wildcard arm; blast radius spans the entire workspace. Mitigation: cross-crate review per `cipherocto-design-principles` §Layer stability; TV-CE-2 regression guard catches un-migrated exhaustive matches at compile time.
- MEDIUM: external-consumer exhaustive-match breakage — out-of-tree consumers (third-party wallet plugins, marketplace integrations) that exhaustively match `Caveat` will fail to compile. Mitigation: broadcast semver-MAJOR via RFC-0965 VH + CHANGELOG; TV-CE-2 regression guard surfaces breakage in CI.
- LOW: wildcard-arm proliferation — every consumer invents its own `_ => ...` arm, drift risk across crates. Mitigation: if pattern repeats, land shared `Caveat::passthrough(&Caveat) -> Caveat` helper in `octo_cap_macaroon` (out of scope for this mission; tracked separately).

## Cross-references

- `cipherocto-design-principles` §Extension over enumeration (typed-discriminator + Raw escape hatch)
- `cipherocto-design-principles` §Layer stability table — Layer A = RFC-frozen, semver-major only
- RFC-0965 — Caveat type catalogue (substrate owner)
- RFC-0965 §5 PermissionKind Co-Bound Caveat — inline location of `Caveat::AssetBinding` variant
- R3 review L4 CRITICAL #1 — finding source

## Claimant

@mmacedoeu

## Pull Request

#