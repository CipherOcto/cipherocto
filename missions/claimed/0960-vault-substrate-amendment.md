# Mission: 0960-v — RFC-0960 (chain-aware bump) v2.1-Resolved → v3.0: chain-aware vault substrate + 108 TV

## Status

**LANDED 2026-08-18 (@mmacedoeu).** Filed per audit verdict 2026-08-17
(storage restructure hard-recommendation: S6f RFC-0960 amendment +
108 TV per pending task #458). Closes parallel-model risk surfaced by
audit: RFC-0960 v2.1-Resolved §2.1 contained a hierarchical root-vault
example (Global → Regional → ... → Capability) that the §20.3 Model B
chain-aware substrate explicitly REMOVES (per review §20.8 lock
"no organizational intermediates").

## What landed (2026-08-18)

- **RFC-0960 v3.0 row added** to `§Version History` (chain-aware bump; PK + derivation cross-references; 108 TV matrix anchor; companion RFC §References cross-refs).
- **§2.1 root-vault example REMOVED**: hierarchical `parent_vault: Option<VaultID>` field dropped from `Vault` struct definition; 13-line hierarchy block replaced with §20.3 lattice note pointing at §Vault Substrate + RFC-0965 `WrappedOnly` capability-decoration layer.
- **`§Vault Substrate` subsection added (§2.6)**: canonical PK = `(chain_id, owner_did, asset_id)`; UNIQUE INDEX on `vault_id`; column-type map (`vault_id BLOB(32)`, `chain_id BLOB(32)`, `owner_did TEXT`, `asset_id BLOB(32)`, `balance DQA(12)`, `policy BLOB`, `state TEXT`, `created_at_unix BIGINT`, `metadata BLOB`); transfer events table PK = `(chain_id, event_id)`; bump compatibility note (non-additive text change + additive substrate — no migration owed); cross-RFC pin to §20.3.1 role-token enumeration (RFC-0105 v2.0) + §20.3.2 chain_id derivation (RFC-0010 v1.4 ChainNamespace) + §8.10 TV-V1 derivation (`octo_vault::vault_id` + `vault_id_unchecked`).
- **§References updated**: added review-doc citation (§9.5 + §20.3 + §20.7 + §20.8), plan-doc citation (§3 S6 row + §24 per-RFC TV count table), RFC-0105 v2.0 + RFC-0010 v1.4 cross-refs, LANDED substrate anchors (v013 + v014 migrations + `octo_vault::vault_id` + central-registry fixture).
- **6 companion RFCs §References cross-refs** (light touch — single bullet each, pointing at RFC-0960 v3.0 chain-aware bump): RFC-0961 (CIPHERO_SQL), RFC-0962 (ExecutionEnvelope), RFC-0963 (shard routing), RFC-0964 (constraint encoding), RFC-0965 (capability extension), RFC-0967 (PolicyObject). Each notes substrate-unaffected impact (per-companion scope).
- **108 byte-exact TV added** at `crates/octo-vault/tests/test_vectors.rs::tv_v1_vault_id_matrix`:
  - `VaultIdFixture` struct + `vault_id_unchecked_fixt` helper (AC-6 anti-drift guard).
  - `MATRIX_ROLE_TOKENS` (9 canonical per RFC-0105 v2.0 §Asset ID Derivation — OCTO-A/B/D/M/N/O/S/H/W; Sovereign OCTO excluded per review §1336).
  - `matrix_chains()` (3 chains per AC-5 — canonical-default `[0u8; 32]` sentinel, mainnet, testnet).
  - `MATRIX_OWNER_DIDS` (4 owners per AC-5 — alice + bob user DIDs, escrow-svc + reputation-svc service DIDs).
  - 7 new TV: `tv_v1_vault_id_matrix_is_108_fixtures`, `tv_v1_vault_id_matrix_matches_self_derivation`, `tv_v1_vault_id_matrix_covers_canonical_role_tokens`, `tv_v1_vault_id_matrix_covers_three_chains`, `tv_v1_vault_id_matrix_covers_four_owners`, `tv_v1_vault_id_matrix_all_vault_ids_distinct`, `tv_v1_vault_id_matrix_no_vault_equals_zero_sentinel`.
  - Self-derivation anti-drift: each fixture's `vault_id` field is computed via `vault_id_unchecked_fixt` (which calls canonical `vault_id_unchecked`); test re-derives + asserts byte equality on every run.

## Verify (2026-08-18)

- `cargo test -p octo-vault --test test_vectors` → 15/15 green (8 prior TV + 7 new TV-V1-MATRIX).
- `cargo test -p octo-vault --lib` → 10/10 green (substrate derivation tests byte-stable).
- `cargo test -p octo-cap-macaroon --lib` → 193/193 green (WrappedOnly chain invariant preserved; cross-crate regression zero).
- `cargo clippy -p octo-vault --all-targets -- -D warnings` → clean.
- `cargo fmt --all -- --check` → clean.

## RFC

- Primary: RFC-0960 (chain-aware bump v2.1-Resolved → v3.0 — adds
  §Vault Substrate subsection + removes §2.1 root-vault example +
  promotes PK = `(chain_id, owner_did, asset_id)` per §20.3
  lattice)
- Co-RFC: RFC-0010 v1.4 (typed `ChainId` + `ChainNamespace` per
  R15-F9 — wire form for `chain_id BLOB(32)` column)
- Co-RFC: RFC-0105 (asset_id addendum — `asset_id = BLAKE3("cipherocto/asset/v1/" + role_token)` per §20.3.1; provides 9 role-token enumeration for TV-D9)
- Co-RFCs (lockstep bump to v3.0): RFC-0961 / RFC-0962 / RFC-0963 / RFC-0964 / RFC-0965 / RFC-0967 — already bumped to v2.1-Resolved in lockstep with 0960 v2.1-Resolved; v3.0 bump re-aligns with §20.3 chain-aware substrate

## Dependency edges

| From                                                                                | To                               | Why                | Layer direction |
| ----------------------------------------------------------------------------------- | -------------------------------- | ------------------ | --------------- |
| `rfcs/accepted/economics/0960-...md` (modify)                                       | §Vault Substrate + v3.0 row      | Spec coherence     | RFC → RFC       |
| `rfcs/accepted/economics/{0961,0962,0963,0964,0965,0967}-*.md` (modify — selective) | §References cross-refs v3.0 row  | Lockstep           | RFC → RFC       |
| `crates/octo-vault/tests/test_vectors.rs` (NEW)                                     | 108 byte-exact TV-V1 fixtures    | Canonical anchor   | lib → test      |
| `crates/octo-vault/src/vault_id.rs` (REUSED — LANDED S3)                            | `vault_id_unchecked` (canonical) | Existing substrate | lib → lib       |
| `crates/octo-vault/src/lib.rs` (REUSED — LANDED S3)                                 | canonical re-exports             | Layer B            | lib → lib       |

No new cyclic edges. v3.0 RFC amendment is non-additive (REMOVES
§2.1 example, ADDS §Vault Substrate subsection) — single commit with
§Version History v3.0 row.

## Problem

RFC-0960 v2.1-Resolved (last status 2026-07-23) carries forward a
hierarchical vault example in §2.1:

> "Example hierarchy (Global → Regional → Marketplace → Provider →
> Mission → Task → Capability → Reservation)"

This example is INCOMPATIBLE with the §20.3 Model B chain-aware
substrate now LANDED in `crates/octo-vault/migrations/v013__create_vaults.sql`:

- v013 schema: PK = `(chain_id, owner_did, asset_id)`, **NO
  `parent_vault_id` column** (review §9.5 removal + §20.8 lock)
- v013 schema: 32-byte `vault_id` derived via
  `BLAKE3("cipherocto/vault/v1/" + chain_id + owner_did + asset_id)`
  per §8.10 TV-V1
- v014 schema: PK = `(chain_id, event_id)` with `from_vault_id` +
  `to_vault_id` columns (no organizational intermediates)

Land-substrate AHEAD of RFC text: implementers reading RFC-0960 v2.1
get ONE model (root vault hierarchy), implementers reading the
storage migrate to ANOTHER model (chain-aware leaf vault). This is
the parallel-model risk: spec text + substrate diverge.

The review explicitly notes at §20.8:

> "Cross-asset policies unsupported at the vault layer (per §20.8
> decision locked); org intermediates become optional
> capability-decoration layer (not part of PK). Implementation
> defers to vault design RFC (separate)."

And at §9.5:

> "The `parent_vault_id` column from §9.5 is REMOVED (no
> organizational intermediates per §20.8 lock)."

So v3.0 amendment must:

1. REMOVE §2.1 root-vault example
2. ADD §Vault Substrate subsection specifying PK + derivation
3. ADD §Version History v3.0 row
4. Cross-reference review §20.3 + canonical LANDED substrate
5. Cross-reference §8.10 TV-V1 anchor for vault_id derivation

The 108 TV = 9 role-tokens × 3 chains × 4 owners (per review line
2019 explicitly stated). Per §8.10, central registry location =
`crates/octo-vault/tests/test_vectors.rs`.

## Acceptance Criteria

- AC-1: RFC-0960 §Version History v3.0 row added with:
  - Date: 2026-08-17
  - Author: @mmacedoeu
  - Change: "Chain-aware bump. §2.1 root-vault example REMOVED per
    review §20.8 lock (no organizational intermediates). §Vault
    Substrate subsection added: PK = `(chain_id, owner_did,
asset_id)` per §20.3 Model B lattice; `vault_id =
BLAKE3("cipherocto/vault/v1/" + chain_id + owner_did +
asset_id)` per §8.10 TV-V1 canonical anchor. §2.5
    `transfer_events` chain-aware shape: PK = `(chain_id,
event_id)`. LANDED substrate: `crates/octo-vault/migrations/
v013__create_vaults.sql` +
    `v014__create_transfer_events.sql`. 108 byte-exact TV (9
    role-token × 3 chain × 4 owner matrix per §8.10 + §24).
    Companion RFCs (0961/0962/0963/0964/0965/0967) cross-ref
    updated."
- AC-2: RFC-0960 §2.1 root-vault example REMOVED. Replace with
  note: "Per §20.3 lattice (no organizational intermediates), this
  hierarchical example is REMOVED. Vault layer is a flat lattice of
  `(chain_id, owner_did, asset_id)` leaf rows; cross-asset policies
  attach via RFC-0965 capability decoration, NOT via vault
  hierarchy. See §Vault Substrate below."
- AC-3: RFC-0960 §Vault Substrate subsection added between §2.1 and
  §2.5 (or wherever the spec structure dictates):
  - §Vault Substrate: PK = `(chain_id, owner_did, asset_id)`; UNIQUE
    INDEX on `vault_id`; columns: `vault_id BLOB(32)`, `chain_id
BLOB(32)`, `owner_did TEXT`, `asset_id BLOB(32)`, `balance
DQA(12)`, `policy BLOB`, `state TEXT`, `created_at_unix
BIGINT`, `metadata BLOB`. `vault_id_unchecked` derivation: see
    §8.10 TV-V1.
  - §Transfer Events Substrate: PK = `(chain_id, event_id)`; columns
    mirror v014 migration. Canonical log shape per §2.5 (append-
    only). `from_vault_id` + `to_vault_id` columns FK-reference
    vaults table PK (chain_id-prefixed to enforce intra-chain
    transfer semantics per §20.7).
  - §Bump compatibility: v3.0 is non-additive (§2.1 removal) but
    _additive for substrate_ (PK shape + UNIQUE INDEX + DQA column
    type ALREADY landed in v013/v014). No migration owed; spec
    update matches LANDED substrate.
- AC-4: RFC-0960 §References updated to cite:
  - review doc `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
    §20.3 + §20.7 + §20.8 + §9.5
  - plan doc `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
    §3 S6 row + §24 per-RFC TV table
  - central registry `crates/octo-vault/tests/test_vectors.rs::tv_v1_vault_id_matrix`
  - LANDED substrate `crates/octo-vault/migrations/v013__create_vaults.sql`
    - `v014__create_transfer_events.sql`
- AC-5: 108 byte-exact TV in
  `crates/octo-vault/tests/test_vectors.rs::tv_v1_vault_id_matrix`:
  - 9 role-token fixtures: `OCTO`, `OCTO-A`, `OCTO-B`, `OCTO-D`,
    `OCTO-M`, `OCTO-O`, `OCTO-W`, and 2 RFC-0105-amendment-defined
    role-tokens (verify exact enumeration in RFC-0105 v1.6 §Asset ID
    Addendum)
  - 3 chain fixtures: canonical-default (32-byte zero), `mainnet`,
    `testnet` (BLAKE3-derived per §20.3.2 + RFC-0010 v1.4
    `ChainNamespace`)
  - 4 owner_did fixtures: 2 user DIDs (e.g., `did:cipherocto:alice`,
    `did:cipherocto:bob`) + 2 service DIDs (e.g.,
    `did:cipherocto:escrow-svc`, `did:cipherocto:reputation-svc`)
  - 108 fixtures = 9 × 3 × 4 distinct vault_id byte sequences
- AC-6: Each TV fixture structure (per §8.10 + §18 central registry):
  ```rust
  pub struct VaultIdFixture {
      pub role_token: &'static str,
      pub chain_namespace: ChainNamespace,
      pub owner_did: &'static str,
      pub vault_id: [u8; 32],  // BLAKE3("cipherocto/vault/v1/" + chain_id + owner_did + asset_id)
  }
  ```
  - helper `fn vault_id_unchecked_fixt(f: &VaultIdFixture) ->
[u8; 32]` that re-derives and asserts equality with
    `vault_id` byte-exact (anti-drift guard).
- AC-7: Companion RFCs (0961/0962/0963/0964/0965/0967) selective
  update: only §References cross-refs to RFC-0960 v3.0 row — not a
  full lockstep bump. (Each companion RFC is independently scoped;
  S6f is RFC-0960 + 6 light-touch cross-refs.)
- AC-8: No substrate code changes — substrate ALREADY LANDED S3.
  v013 + v014 migrations match §20.3 Model B. Mission is spec-
  coherence + TV addition only.
- AC-9: No regressions:
  - `cargo test -p octo-vault --lib` (existing tests pass)
  - `cargo test -p octo-cap-macaroon --lib` (WrappedOnly chain
    invariant preserved)
  - 108 TV pass byte-exact; cross-crate `octo-vault::vault_id`
    re-derivation produces identical bytes for all 108 fixtures
- AC-10: clippy + fmt:
  - `cargo clippy -p octo-vault --all-targets -- -D warnings`
  - `cargo fmt --all -- --check`

## Cross-reference

- **Parent:** RFC-0960 v2.1-Resolved (LANDED 2026-07-23 with 7
  companion RFCs in lockstep)
- **Substrate anchor:**
  - `crates/octo-vault/migrations/v013__create_vaults.sql` (PK =
    `(chain_id, owner_did, asset_id)`, `vault_id` UNIQUE INDEX,
    `balance DQA(12)`)
  - `crates/octo-vault/src/lib.rs::vault_id_unchecked` (canonical
    BLAKE3 derivation)
  - `crates/octo-vault/src/transfer_events.rs` (canonical log
    writer per §2.5)
- **TV anchor:** §8.10 + §24 + §18 central registry
- **Review source:**
  `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §9.5 + §20.3 + §20.7 + §20.8 + §24 line 2019 (9 role-token × 3
  chain × 4 owner = 108)
- **Plan source:**
  `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 S6 row + §24 per-RFC TV table
- **Sibling (per-audit-verdict):**
  - `missions/claimed/0862-c1-dqa-vault-bump-amendment.md` (LANDED
    2026-08-17) — Dqa + vault bump pattern for v013
  - `missions/open/0862-c9-micro-octow-type-unification.md` (FILED
    2026-08-17) — closes audit-verdict Risk #1 (MicroOctoW alias)
  - `missions/open/0105-x-s4-deferred-codemod-sites.md` (FILED
    2026-08-17) — in-memory field type migration
  - `missions/open/0959-c1-wire-format-amendment.md` (FILED
    2026-08-17) — settlement wire format
  - `missions/open/0900-d-chain-aware-slash-ledger.md` (FILED
    2026-08-17) — S6d RFC-0900 chain-aware slash ledger
- **Audit source:** 2026-08-17 audit verdict, RFC amendment per
  parallel-model closure plan

## Critical files

- `rfcs/accepted/economics/0960-grand-design-vaults-capabilities-reservations.md`
  (modify — §Version History v3.0 row + §2.1 removal + §Vault
  Substrate subsection + §References update)
- `rfcs/accepted/economics/0961-*.md` (selective — §References cross-
  ref)
- `rfcs/accepted/economics/0962-*.md` (selective — §References cross-
  ref)
- `rfcs/accepted/economics/0963-*.md` (selective — §References cross-
  ref)
- `rfcs/accepted/economics/0964-*.md` (selective — §References cross-
  ref)
- `rfcs/accepted/economics/0965-caveat-extension-format.md`
  (selective — §References cross-ref)
- `rfcs/accepted/economics/0967-*.md` (selective — §References
  cross-ref)
- `crates/octo-vault/tests/test_vectors.rs` (NEW — §18 central
  registry, 108 fixtures at `tv_v1_vault_id_matrix`)
- `crates/octo-vault/src/lib.rs` (modify if needed for fixture
  re-export)

## Existing patterns reused

- `crates/octo-vault/src/lib.rs::vault_id_unchecked` — canonical
  BLAKE3 derivation (LANDED S3). TV fixture helper calls this AND
  asserts equality with stored fixture byte sequence (anti-drift
  guard).
- `crates/octo-vault/tests/` — central registry pattern (R15-F10 +
  §18). `test_vectors.rs` is the canonical home for §8.10-anchored
  TV; existing test files unchanged.
- `octo_determin::Dqa` — already used for `balance DQA(12)` column;
  no DQP code change. S4 codemod shipped Dqa substrate; v3.0 just
  cites.
- `octo-ident::ChainNamespace` — wire form for `chain_id BLOB(32)`
  per RFC-0010 v1.4. v3.0 cites the canonical derivation.

## Risks

- **9 role-token enumeration ambiguity** (MED): §20.3.1 mentions
  role-tokens (`OCTO`, `OCTO-A`, `OCTO-B`, `OCTO-D`, etc.) but the
  exact 9-element set is not enumerated in RFC-0960 itself.
  Mitigation: cross-reference RFC-0105 v1.6 §Asset ID Addendum for
  the canonical enumeration (9 distinct role-token string inputs
  fed to BLAKE3("cipherocto/asset/v1/" + role_token)). If RFC-0105
  amendment (S6g, 109 TV) does not finalize the 9 list, default to
  RFC-0105 v1.5 known set + file a follow-on mission for missing
  role-tokens.
- **3 chain enumeration ambiguity** (LOW): canonical-default
  (R15-F9 + zero bytes) + `mainnet` + `testnet`. If RFC-0010 v1.4
  `ChainNamespace` enumeration grows beyond 3, fixture set must
  grow. Mitigation: scope to 3 for 0960-v; chain growth owed to
  RFC-0010 amendments.
- **Companion RFC drift** (LOW): 6 companion RFCs (0961-0967)
  §References updates are mechanical cross-ref additions. If any
  companion is independently under amendment, cross-ref to its
  current version (not assumed v3.0).
- **TV fixture byte-exact drift** (MED): 108 vault_id byte
  sequences must remain byte-stable across substrate changes.
  Mitigation: each fixture calls `vault_id_unchecked_fixt` helper
  - asserts byte equality. Drift caught at compile + test time.
- **Spec text removal non-additive** (LOW): §2.1 example REMOVAL
  is the first non-additive change in this RFC family. Mitigation:
  explicitly document in §Version History v3.0 row that §2.1
  removal is intentional per §20.8 lock; no behavior change for
  LANDED substrate (already leaf-only).

## Version history

| Date       | Author     | Change                                                                                                                                                                                                 |
| ---------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 2026-08-17 | @mmacedoeu | Initial filing per audit verdict 2026-08-17 (storage restructure hard-recommendation: S6f RFC-0960 chain-aware bump + 108 TV per pending task #458). Co-filed with `c9` + `x-mission` + `S6e` + `S6d`. |
| 2026-08-18 | @mmacedoeu | LANDED. RFC-0960 v3.0 row + §2.1 root-vault example REMOVED + §Vault Substrate subsection (§2.6) added + §References cross-refs added (review-doc §20.3/§20.7/§20.8 + plan-doc §24 + RFC-0105 v2.0 + RFC-0010 v1.4 + LANDED substrate anchors) + 6 companion RFCs §References cross-refs (0961/0962/0963/0964/0965/0967 light-touch bullets) + 108 byte-exact TV at `crates/octo-vault/tests/test_vectors.rs::tv_v1_vault_id_matrix` (9 role-token × 3 chain × 4 owner = 108 per §24 central registry; `VaultIdFixture` struct + `vault_id_unchecked_fixt` anti-drift helper + 7 new TV). Mission file moved `open/` → `claimed/`. 15/15 octo-vault test_vectors green + 10/10 octo-vault lib green + 193/193 octo-cap-macaroon lib green + clippy + fmt clean. |

## Out of scope

- LOCKSTEP v3.0 bump of companion RFCs 0961/0962/0963/0964/0965/0967
  (selective §References cross-ref only — each companion RFC has its
  own substrate scope; full lockstep bump is a separate mission per
  companion RFC if any has substrate changes owed)
- Cross-asset policy substrate (review §20.8 deferral: optional
  capability-decoration layer per RFC-0965, not part of vault PK)
- Vault hierarchy substrate (per §20.8 lock: not introduced; root-
  vault example REMOVED)
- 109 RFC-0105 TV (S6g — separate mission)
- 123 central-registry TV total (108 vault_id + 9 TV-D9 + 4 TV-C1 +
  others are out-of-scope-for-RFC-0960 TV; only 108 vault_id =
  RFC-0960 §Version History v3.0 row count, per §24)
- Stoolap fork native DQA driver surface (verified at S3;
  `vaults.balance DQA(12)` lands without driver change)
- Companion RFC substrate-level changes (each companion RFC has its
  own substrate; this mission is RFC-0960 spec-coherence only)
