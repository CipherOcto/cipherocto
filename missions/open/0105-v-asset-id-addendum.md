# Mission: 0105-v — RFC-0105 (asset_id addendum) v1.9 → v2.0: `asset_id_for` derivation rule + 109 TV

## Status

**OPEN 2026-08-17 (@mmacedoeu).** Filed per audit verdict 2026-08-17
(storage restructure hard-recommendation: S6g RFC-0105 amendment +
109 TV per audit verdict "S6g RFC-0105 (109 TV)" enumeration).
Closes RFC-0105 gap: spec carries DQA + DqaEncoding but does NOT
specify `asset_id` derivation rule for role-tokens used in §20.3
vault PK Model B.

## RFC

- Primary: RFC-0105 (asset_id addendum v1.9 → v2.0 — adds
  §Asset ID Derivation subsection + canonical mapping table +
  references canonical review §20.3.1 derivation)
- Co-RFC: RFC-0960 (chain-aware bump) — `asset_id` is part of
  vault PK per §20.3 Model B
- Co-RFC: RFC-0964 (Dqa bump) — names `asset_id: [u8; 32]` as opaque
  canonical (no change to type spec; deriver lives in RFC-0105)
- Co-RFC: RFC-0010 (32-byte addendum) — `ChainId::as_bytes` for
  `chain_id BLOB(32)` partition in vault PK

## Dependency edges

| From                                                                    | To                                                      | Why              | Layer direction |
| ----------------------------------------------------------------------- | ------------------------------------------------------- | ---------------- | --------------- |
| `rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md` (modify) | §Asset ID Derivation + v2.0 row + mapping table         | Spec coherence   | RFC → RFC       |
| `crates/octo-vault/tests/test_vectors.rs` (NEW)                         | 9 TV-D9 role-token fixtures + 100 TV-D10 DQA round-trip | Central registry | lib → test      |
| `crates/octo-determin/src/asset_id.rs` (NEW module)                     | `pub fn asset_id_for(role_token: &str) -> [u8; 32]`     | Canonical impl   | lib → lib       |
| `crates/octo-determin/src/lib.rs` (modify)                              | Re-export `asset_id_for`                                | Layer A          | lib → lib       |

No new cyclic edges. Layer A (octo-determin, frozen substrate)
gains `asset_id_for` function per §20.3.1 derivation.

## Problem

RFC-0105 v1.9 (last status 2026-03-19) covers:

- §Data Structures (DQA + DqaEncoding)
- §APIs/Interfaces
- §SQL Integration (DQA column type per Stoolap fork)
- §Arithmetic Algorithms (add, sub, mul, div, pow)
- §Canonical Representation
- §Deterministic Overflow Handling
- §Deterministic Rounding Mode
- §Reference Test Vectors (8 add + 3 mult + N div + ...)

It does NOT specify:

- `asset_id: [u8; 32]` derivation from `role_token: &str`
- The 9-role-token enumeration used in vault PK
- The `"cipherocto/asset/v1/"` domain-separation namespace

Per review §20.3.1:

> "RFC-0105 (asset_id addendum) specifies the derivation rule +
> canonical mapping table. RFC-0964 (Dqa bump) names the field as
> opaque (no change to type spec)."

Substrate has 9 role-tokens implied (OCTO-A, OCTO-B, OCTO-D,
OCTO-M, OCTO-N, OCTO-O, OCTO-S, OCTO-H, OCTO-W — Sovereign OCTO
excluded) but no canonical derivation function. Vault substrate in
`crates/octo-vault/migrations/v013__create_vaults.sql` references
`asset_id BLOB(32)` as "BLAKE3('cipherocto/asset/v1/' + role_token)
per §20.3.1" but §20.3.1 lives in the review doc, not in any RFC.
Spec gap = parallel-model risk.

109 TV = 9 TV-D9 role-token byte-exact fixtures + 100 TV-D10 DQA
round-trip fixtures.

## Acceptance Criteria

- AC-1: RFC-0105 §Version History v2.0 row added with:
  - Date: 2026-08-17
  - Author: @mmacedoeu
  - Change: "Asset ID addendum. §Asset ID Derivation subsection
    added: `asset_id_for(role_token: &str) -> [u8; 32]` =
    `BLAKE3("cipherocto/asset/v1/" + role_token)` per §20.3.1
    canonical anchor. 9 role-token enumeration table (OCTO-A, OCTO-B,
    OCTO-D, OCTO-M, OCTO-N, OCTO-O, OCTO-S, OCTO-H, OCTO-W — Sovereign
    OCTO excluded per review §1336 cross-section reconciliation).
    Domain-separation namespace: `cipherocto/asset/v1/` (future
    versions bump to `v2` etc.). Implementation lives in
    `octo-determin::asset_id::asset_id_for` (Layer A frozen
    substrate). 109 byte-exact TV (TV-D9 9 + TV-D10 100) in
    `crates/octo-vault/tests/test_vectors.rs` central registry per
    §8.10 + §18 + §24 per-RFC allocation."
- AC-2: RFC-0105 §Asset ID Derivation subsection added between §Data
  Structures (DqaEncoding section) and §APIs/Interfaces (or wherever
  spec structure dictates):
  - §Asset ID Derivation: `asset_id_for(role_token: &str) -> [u8; 32]`
    function shape (no type changes — input is `&str`, output is
    `[u8; 32]`).
  - BLAKE3 derivation: `BLAKE3("cipherocto/asset/v1/" + role_token)`.
  - Domain separation: future versions bump namespace string to
    `cipherocto/asset/v2/` etc. Cross-version asset ID collision
    impossible by namespace.
  - Canonical 9 role-token enumeration (per review §1336):
    - `OCTO-A` → AI Compute
    - `OCTO-B` → Bandwidth
    - `OCTO-D` → Developers
    - `OCTO-M` → Marketing
    - `OCTO-N` → Node Operators
    - `OCTO-O` → Orchestrator
    - `OCTO-S` → Storage
    - `OCTO-H` → Historical
    - `OCTO-W` → AI Wholesale
  - Note: Sovereign `OCTO` excluded from TV-D9 (separately handled
    per cross-layer capability-attestation path).
  - Function lived in Layer A `octo-determin::asset_id` per layer
    stability rules — frozen substrate, RFC-mandated.
- AC-3: RFC-0105 §Cross-RFC Amendment v1.9 note updated to §2.0
  addendum note: "v2.0 addendum: `asset_id_for(role_token: &str) ->
[u8; 32]` derivation rule + canonical 9-role-token enumeration.
  See RFC-0960 §Vault Substrate for vault PK use; RFC-0964 §3.2 for
  opaque `asset_id: [u8; 32]` type spec."
- AC-4: New module `crates/octo-determin/src/asset_id.rs`:
  ```rust
  //! Asset ID derivation per RFC-0105 §Asset ID Derivation.
  //!
  //! [`asset_id_for`] computes the canonical 32-byte asset ID
  //! from a role-token string per §20.3.1 derivation rule.

  /// Compute the canonical `asset_id` for a role-token string.
  ///
  /// Derivation: `BLAKE3("cipherocto/asset/v1/" + role_token)`.
  ///
  /// Domain-separated: namespace `cipherocto/asset/v1/` guarantees
  /// future-version (`v2`, etc.) asset ID collision impossibility.
  ///
  /// See RFC-0105 v2.0 §Asset ID Derivation + review §20.3.1.
  #[must_use]
  pub fn asset_id_for(role_token: &str) -> [u8; 32] {
      *blake3::hash(b"cipherocto/asset/v1/"
          .iter()
          .chain(role_token.as_bytes())
          .copied()
          .collect::<Vec<u8>>()
          .as_slice())
          .as_bytes()
  }
  ```
  - Use `blake3` crate (already in workspace per Cargo.lock)
  - Function is `#[must_use]` + pure + deterministic
  - Add `#[cfg(test)] mod tests` with 9 quick smoke tests for the
    9 role-tokens (matches TV-D9 byte sequences exactly)
- AC-5: `crates/octo-determin/src/lib.rs`:
  - Add `pub mod asset_id;` next to existing modules
  - Re-export `pub use asset_id::asset_id_for;`
  - Substrate-layer: this is the canonical public API
- AC-6: `crates/octo-determin/Cargo.toml`:
  - Verify `blake3` is in `[dependencies]` (likely yes per
    workspace; if not, add `blake3 = "1.x"` with rationale
    comment per layer model + crypto primitive)
  - No new transitive deps
- AC-7: 9 TV-D9 byte-exact role-token fixtures in
  `crates/octo-vault/tests/test_vectors.rs::tv_d9_asset_id`:
  - Each fixture: `{ role_token: &'static str, asset_id: [u8; 32] }`
  - 9 fixtures × exact 32-byte BLAKE3 sequences (computed at RFC
    acceptance time, frozen as canonical byte anchors per §8.10)
  - Helper: `fn assert_asset_id_derivation(f: &RoleTokenFixture) {
let computed = octo_determin::asset_id_for(f.role_token);
assert_eq!(computed, f.asset_id, "TV-D9 drift");
}`
  - Each fixture's `asset_id` byte sequence is part of the RFC
    canonical mapping table (commit-to-git anchors the byte sequences
    for cross-implementation verification)
- AC-8: 100 TV-D10 DQA round-trip byte-exact fixtures in
  `crates/octo-vault/tests/test_vectors.rs::tv_d10_dqa_round_trip`:
  - 100 distinct arithmetic combinations: addition, subtraction,
    multiplication, division across diverse scales (0, 1, 2, 3,
    5, 7, 12, 18) + diverse values (-10^18, 0, 10^-12, 10^0,
    10^12, 10^18) + diverse rounding boundaries
  - Each fixture: `{ input_a: Dqa, input_b: Dqa, op: enum, expected:
Dqa, expected_scale: u8 }`
  - Helper: `fn assert_dqa_round_trip(f: &DqaRoundTripFixture) {
let computed = match f.op { ... apply op ... };
assert_eq!(computed, f.expected, "TV-D10 drift");
assert_eq!(computed.scale(), f.expected_scale, "TV-D10 scale
  drift");
}`
  - 100 fixtures cover edge cases: scale-up overflow, scale-down
    truncation, division by zero (returns error), negative result,
    zero values, canonical representation rounding, scale alignment
    semantics
- AC-9: Existing TV in `crates/octo-vault/src/` (e.g., vaults.rs
  round-trip, transfer_events round-trip) re-validate that
  `asset_id_for(role_token)` re-derivation matches the column
  BLOB(32) byte-by-byte. Re-derivation is a substrate invariant.
- AC-10: Central registry: 9 + 100 = 109 NEW fixtures added to
  `crates/octo-vault/tests/test_vectors.rs`. Combined central
  registry count after this mission lands: 14 (RFC-0960 v3.0 108 +
  this 9 = 117, plus RFC-0105 TV-D10 100 brings to 217 — exceeds
  the §8.10 123 anchor). Wait — let me recount: §8.10 central-
  registry = 123 fixtures split as TV-D9 (9) + TV-D10 (100) +
  TV-V1 (10) + TV-C1 (4) = 123. TV-V1 10 are RFC-0960 §Vault
  Substrate (covered by 0960-v mission), NOT 108 (which is the
  9×3×4 matrix = 108 byte-exact vault_id derivation fixtures —
  108 ≠ 10; the 108 is per §24 per-RFC allocation for RFC-0960,
  the 10 in §8.10 is the canonical anchor count). RECONCILIATION:
  §8.10 holds the 123 canonical-byte sequences; §24 counts each
  distinct (role_token × chain × owner) combination as 1 TV
  fixture. The S6f mission 108 = §8.10 TV-V1 10 re-derived across
  the 9×3×4 matrix (each is a unique byte sequence per
  combination). When all lands, central registry holds 123
  canonical sequences + 98 (108 − 10) additional = 221 fixtures.
  This is documented in AC-10.
- AC-11: No regressions:
  - `cargo test -p octo-determin --lib`
  - `cargo test -p octo-vault --lib`
  - `cargo test -p octo-cap-macaroon --lib`
  - All existing tests pass byte-stable
- AC-12: clippy + fmt:
  - `cargo clippy --workspace --all-targets --features full -- -D warnings`
  - `cargo fmt --all -- --check`

## Cross-reference

- **Parent:** RFC-0105 v1.9 (last status 2026-03-19 — DQA substrate
  alone)
- **Co-RFC:** RFC-0960 v3.0 (chain-aware bump) — `asset_id` is
  part of vault PK Model B per §20.3; v3.0 mission FLAGGED this
  asset_id derivation gap (out of scope for 0960-v mission)
- **Co-RFC:** RFC-0964 (Dqa bump) — names `asset_id: [u8; 32]` as
  opaque canonical (no type change)
- **Co-RFC:** RFC-0010 v1.4 (32-byte addendum) — `ChainId::as_bytes`
  for `chain_id BLOB(32)` partition (parallel opaque-bytes type)
- **TV anchor:** §8.10 (central registry location) + §18 (canonical
  anchor registry) + §24 (per-RFC TV allocation: 109 = 9 + 100)
- **Review source:**
  `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §1335-1364 (canonical 9 role-token enumeration + asset_id_for
  derivation rule + Sovereign OCTO exclusion rationale)
- **Plan source:**
  `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 S6 row + §24 per-RFC TV table + §8.10 central-registry count
- **Sibling (per-audit-verdict):**
  - `missions/claimed/0862-c1-dqa-vault-bump-amendment.md` (LANDED
    2026-08-17) — Dqa + vault bump pattern
  - `missions/open/0862-c9-micro-octow-type-unification.md` (FILED
    2026-08-17) — closes audit-verdict Risk #1
  - `missions/open/0105-x-s4-deferred-codemod-sites.md` (FILED
    2026-08-17) — in-memory field type migration
  - `missions/open/0959-c1-wire-format-amendment.md` (FILED
    2026-08-17) — settlement wire format
  - `missions/open/0900-d-chain-aware-slash-ledger.md` (FILED
    2026-08-17) — S6d RFC-0900 chain-aware slash ledger
  - `missions/open/0960-vault-substrate-amendment.md` (FILED
    2026-08-17) — S6f RFC-0960 chain-aware vault substrate
- **Audit source:** 2026-08-17 audit verdict, S6g RFC-0105
  asset_id addendum + 109 TV per §24

## Critical files

- `rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md`
  (modify — §Version History v2.0 row + §Asset ID Derivation
  subsection + §Cross-RFC Amendment v2.0 note)
- `crates/octo-determin/src/asset_id.rs` (NEW — `asset_id_for`
  function)
- `crates/octo-determin/src/lib.rs` (modify — `pub mod asset_id;`
  - re-export)
- `crates/octo-determin/Cargo.toml` (modify if needed — `blake3`
  dep verified)
- `crates/octo-vault/tests/test_vectors.rs` (modify — add
  `tv_d9_asset_id` (9 fixtures) + `tv_d10_dqa_round_trip` (100
  fixtures))
- `rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md`
  §Reference Test Vectors (modify — add TV-D9 + TV-D10 sections;
  preserves existing add/mult/div reference vectors)

## Existing patterns reused

- `crates/octo-vault/src/lib.rs::vault_id_unchecked` — same
  BLAKE3 family derivation pattern (`BLAKE3("cipherocto/vault/v1/"
  - chain_id + owner_did + asset_id)`). `asset_id_for`shares
the`"cipherocto/asset/v1/"` domain-separation pattern (per
    review §1362).
- `blake3::hash(...)` (already used in octo-vault +
  octo-determin) — workspace dep.
- RFC-0105 §Reference Test Vectors section (existing add/mult/div
  tables) — TV-D10 100 fixtures EXTEND this section; preserves
  byte-exact invariants for existing 11+ vectors.
- Central registry §8.10 pattern from existing
  `crates/octo-vault/tests/test_vectors.rs` (S3 LANDED S6f
  mission) — same file gains new `tv_d9_asset_id` +
  `tv_d10_dqa_round_trip` modules.

## Risks

- **9 role-token enumeration stability** (MED): review §1336
  cross-section reconciliation pins the 9 specialized role-tokens
  (excludes Sovereign OCTO). If token-design doc (`docs/04-
tokenomics/token-design.md`) changes the enumeration, TV-D9
  byte sequences drift. Mitigation: anchor byte sequences at
  RFC-0105 v2.0 acceptance time; future enumeration changes
  require v2.1 bump.
- **100 TV-D10 coverage** (MED): need to ensure 100 fixtures
  genuinely cover edge cases (scale alignment, overflow,
  rounding boundaries) — not just 100 arbitrary combos.
  Mitigation: organize fixtures into 10 categories × 10 fixtures
  each (e.g., 10 addition, 10 multiplication, 10 division, 10
  overflow, 10 negative, 10 rounding-boundary, 10 scale-up, 10
  scale-down, 10 zero, 10 cross-scale-mix).
- **Layer A substrate addition** (LOW): `octo-determin` is
  Layer A frozen substrate. Per CLAUDE.md §Architectural
  Principles, Layer A is "RFC-frozen, semver-major only". Per
  RFC-0105 v2.0 amendment, `asset_id_for` addition is RFC-
  mandated; semver-major bump owed to `octo-determin` 1.x → 2.x
  if no previous minor allowed this. Mitigation: cite RFC
  amendment as the substrate-change driver in `Cargo.toml`
  version bump commit message.
- **blake3 version mismatch** (LOW): workspace likely uses
  `blake3 = "1.x"`. Verify `cargo tree -p octo-determin`
  matches the version used by `octo-vault`. Mitigation: pin
  via `[workspace.dependencies]` if already there.
- **9 vs 10 role-token reconciliation** (LOW): review §1336
  explicitly reconciles 9-fixture set vs 10-token superset;
  Sovereign OCTO excluded. If a future token is added (e.g.,
  OCTO-X), it expands TV-D9 to 10+ — separate bump owed.
- **Central registry growth** (LOW): §8.10 says 123 central-
  registry TV; after all 7 RFC amendments land, growth from
  expansion (9×3×4 matrix reconciliation per S6f) bumps to
  221+ fixtures. Central registry capacity not a concern at
  current scale; future fixture growth separated into
  additional test files (e.g., `test_vectors_v2.rs`) if file
  grows beyond comfortable limits.

## Version history

| Date       | Author     | Change                                                                                                                                                                                    |
| ---------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial filing per audit verdict 2026-08-17 (storage restructure hard-recommendation: S6g RFC-0105 asset_id addendum + 109 TV). Co-filed with `c9` + `x-mission` + `S6e` + `S6d` + `S6f`. |

## Out of scope

- Sovereign OCTO asset_id derivation (separate cross-layer
  capability-attestation path per review §1362)
- Other 0 (non-role-token) asset strings (e.g., `"team-budget"`,
  `"user-budget"` per review §1111-1112 — reserved for separate
  RFC owed)
- RFC-0010 v1.4 (32-byte addendum) ChainId::as_bytes derivation —
  separate mission per RFC-0010 chain_id amendment scope
- TV-D10 100 fixture ENUMERATION expansion (e.g., add 100 more
  boundary cases for v2.1) — separate follow-on mission if
  coverage gaps found
- Stoolap fork native DQA driver changes (substrate unchanged;
  v2.0 is canonical function + spec text + TV only)
- Asset cache layer / asset_id index (no proposed Layer C cache —
  OUT per RFC-0960 §Vault Substrate cross-asset-policy deferral)
- Cross-version namespace migration (e.g., `"cipherocto/asset/v2/"`)
  — future bump owed when spec version evolves
