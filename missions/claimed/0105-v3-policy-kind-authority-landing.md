---
name: 0105-v3-policy-kind-authority-landing
description: Land `policy_kind_authority` substrate table per RFC-0105 §3 Authority-to-Issue + RFC-0967-A1 §2.5. Migration v017 adds `policy_kind_authority(kind_uuid BLOB(16) PK, required_signer_did BLOB(32), authority_kind TEXT CHECK IN ('octo_treasury' | 'corp_admin'))` to substrate schema. Substrate `register_policy` enforces transactional wrapper + FK + authority check. Per RFC-0206 §4 Layer B additive-only rule, migration is additive (no destructive changes to existing `policy_objects` table).
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-23T00:00:00.000Z
  v: "1.0"
  depends_on:
    - 0206-001-substrate-newtype
    - 0206-009-adapter-crates
    - RFC-0105
    - RFC-0967-A1
    - RFC-0010
status: OPEN
---

# Mission `0105-v3-policy-kind-authority-landing` v1.0 — OPEN 2026-08-23

## Context

RFC-0105 (economics amendment Authority-to-Issue table) cites `policy_kind_authority` as the substrate table where namespace issuers register their signing authority before any `policy_registry` INSERT lands. RFC-0967-A1 v1.9.2 §2.5 specifies the table schema (with R5 fixes for transactional wrapper + FK + authority check enforcement). RFC-0010 §4 documents the authority registration flow for sovereign + private namespace operators. Substrate currently declares only `TABLE_POLICY_OBJECTS = "policy_objects"` at `crates/octo-policy-storage/src/lib.rs`; `policy_kind_authority` + `policy_registry` tables do NOT exist on disk.

RFC-0105 (economics amendment) also notes: "RFC-defined substrate-pending landing via mission `vault-chain-metadata` per research doc §16 + RFC-0206 §Layer B additive-only rule". This mission captures the `policy_kind_authority` table migration portion of that scope (the broader chain_metadata bridge is split into a separate mission per single-concern discipline).

## Scope

### Step 1: Migration v017 — `policy_kind_authority` table

Create `crates/octo-policy-storage/migrations/v017__create_policy_kind_authority.sql` per RFC-0206 §4 Layer B additive-only rule (no `DROP` / no destructive `ALTER` of existing `policy_objects` table; per-crate migrations dir layout matches existing pattern `crates/octo-vault/migrations/v013__create_vaults.sql` + `crates/quota-router-storage/migrations/v016__settlement_chain_vault.sql`):

```sql
-- Migration v017: policy_kind_authority substrate table
-- Per RFC-0967-A1 §2.5 + RFC-0105 §3 Authority-to-Issue
-- Per RFC-0206 §4 Layer B additive-only rule (additive; no destructive change)

CREATE TABLE policy_kind_authority (
    kind_uuid BLOB(16) NOT NULL PRIMARY KEY,
    required_signer_did BLOB(32) NOT NULL,
    authority_kind TEXT NOT NULL CHECK (
        authority_kind IN ('octo_treasury', 'corp_admin')
    )
);

-- Registry bootstrap seed (per RFC-0967-A1 §2.5 item 5)
-- kind_uuid NULL bytes reserved for system / bootstrap authority
INSERT INTO policy_kind_authority (kind_uuid, required_signer_did, authority_kind)
VALUES
    (zeroblob(16), zeroblob(32), 'octo_treasury'),  -- bootstrap; required_signer_did replaced at onboarding
    (zeroblob(16), zeroblob(32), 'corp_admin');     -- bootstrap; required_signer_did replaced at onboarding
```

Migration runner applies v017 sequentially after v016 within the migration transaction; consistent with existing migration pattern (e.g., v013 `create_vaults.sql`).

### Step 2: Substrate `register_policy` enforcement

Update `crates/octo-policy-storage/src/lib.rs` to expose `register_policy` per RFC-0967-A1 §2.5 enforcement rules:

```rust
pub async fn register_policy(
    &self,
    kind_uuid: [u8; 16],
    body: Vec<u8>,
    registered_by_did: [u8; 32],
) -> Result<PolicyHash, PolicyRegistryError> {
    let mut tx = self.db.begin_tx().await?;

    // 1. Verify authority check (item 3)
    let required_signer = tx.query_row(
        "SELECT required_signer_did FROM policy_kind_authority WHERE kind_uuid = ?1",
        [kind_uuid.as_slice()],
    ).await?
     .ok_or(PolicyRegistryError::UnknownKindUuid(kind_uuid))?;

    if registered_by_did != required_signer {
        return Err(PolicyRegistryError::UnauthorizedRegistrar {
            kind_uuid,
            required: required_signer,
            provided: registered_by_did,
        });
    }

    // 2. INSERT into policy_objects (RFC-0105 §3 row 1/2 write path)
    let policy_hash = blake3::hash(&body).into();
    tx.execute(
        "INSERT INTO policy_objects (policy_hash, kind_uuid, body, execution_class, registered_at_unix, registered_by_did) \
         VALUES (?1, ?2, ?3, 'A', ?4, ?5)",
        params![&policy_hash, &kind_uuid, &body, now_unix(), &registered_by_did],
    ).await?;

    tx.commit().await?;
    Ok(policy_hash)
}
```

Add to error enum:

```rust
#[derive(Debug, thiserror::Error)]
pub enum PolicyRegistryError {
    #[error("policy_kind_authority row missing for kind_uuid {0:?}")]
    UnknownKindUuid([u8; 16]),
    #[error("required_signer_did {required:?} != provided {provided:?} for kind_uuid {kind_uuid:?}")]
    UnauthorizedRegistrar { kind_uuid: [u8; 16], required: [u8; 32], provided: [u8; 32] },
    #[error("policy_registry insert failed")]
    PolicyInsertFailed(#[source] std::io::Error),
}
```

### Step 3: RFC-0105 §3 row 1 (Sovereign) + row 2 (Private) registration

Two `policy_kind_authority` rows seeded via separate onboarding transactions (NOT the migration tx — separate to avoid embedding live DIDs in migration history):

1. **Row 1 — Sovereign `OCTO-*`**:
   - `kind_uuid` = UUIDv5 namespace: `cipherocto/policy-kind/v1/octopus-treasury/sovereign-octo`
   - `required_signer_did` = octo treasury DID (canonical anchor per `crates/octo-ident/src/identity.rs::TREASURY_DID`)
   - `authority_kind` = `'octo_treasury'`
   - Surface in substrate `register_policy` per RFC-0105 §3 Sovereign row.

2. **Row 2 — Private `PRIVATE-*`**:
   - `kind_uuid` = UUIDv5 namespace: `cipherocto/policy-kind/v1/corp-chain-admin/private-namespace`
   - `required_signer_did` = corporate chain operator DID (placeholder; runtime provision)
   - `authority_kind` = `'corp_admin'`
   - Surface in substrate `register_policy` per RFC-0105 §3 Private row.

### Step 4: Doctests + documentation

`crates/octo-policy-storage/src/lib.rs` `register_policy` doctring — cite RFC-0967-A1 §2.5 + RFC-0105 §3 + RFC-0010 §4 Authority Registration Flow.

## Acceptance Criterion

- `crates/octo-policy-storage/migrations/v017__create_policy_kind_authority.sql` exists + applies green
- `cargo run -p octo-storage-core --bin migrate apply --to v017` exits 0
- `policy_kind_authority` table exists post-migration (verified via `sqlite3 ... ".schema policy_kind_authority"`)
- 2 bootstrap rows seeded with `zeroblob(16)` kind_uuid + `zeroblob(32)` signer_did + `'octo_treasury' | 'corp_admin'` (live DIDs inserted via separate onboarding tx, NOT migration tx)
- `crates/octo-policy-storage/src/lib.rs::register_policy` enforces:
  - Transactional wrapper (all-or-nothing — body INSERT + authority check in single tx)
  - Authority check failure returns `Err(PolicyRegistryError::UnauthorizedRegistrar)` (NOT a panic; substrate fails-closed)
  - FK-style lookup of `policy_kind_authority.required_signer_did` BEFORE policy INSERT
- `PolicyRegistryError` enum has `UnknownKindUuid` + `UnauthorizedRegistrar` variants
- `register_policy` doctring cites RFC-0967-A1 §2.5 + RFC-0105 §3 + RFC-0010 §4
- AC gate: `rg '^CREATE TABLE policy_kind_authority' crates/octo-policy-storage/migrations/v017*` → 1 hit
- AC gate: `rg 'register_policy' crates/octo-policy-storage/src/lib.rs` → ≥1 hit (function def)
- AC gate: `rg 'PolicyRegistryError::UnauthorizedRegistrar' crates/octo-policy-storage/src/` → ≥1 hit (error variant used)
- `cargo build --workspace --all-targets` green
- `cargo test --workspace --lib` green
- `cargo clippy --workspace --all-targets --features full -- -D warnings` green (per `quota-router-core-feature-mutex`)
- `cargo fmt --all -- --check` green
- Per RFC-0206 §4: NO destructive migration to existing tables (`policy_objects` schema unchanged)

## Files / Artifacts

- New: `crates/octo-policy-storage/migrations/v017__create_policy_kind_authority.sql`
- Edit: `crates/octo-policy-storage/src/lib.rs` (register_policy function + PolicyRegistryError enum)
- Edit: `crates/octo-policy-storage/Cargo.toml` (no new deps; uses existing thiserror + blake3 + octo_determin)
- Doctring cite refs (no file edits; substrate doctring carries the §-anchors)

## Cross-references

- RFC-0105 §3 Authority-to-Issue table
- RFC-0967-A1 v1.9.2 §2.5 `policy_kind_authority` schema + enforcement
- RFC-0010 §4 Authority Registration Flow (sovereign + private issuers)
- RFC-0206 §4 Layer B additive-only rule (migration ownership)
- Research doc §16 vault-XXX cross-references (the "mission `vault-chain-metadata`" reference decomposed into: this mission owns `policy_kind_authority`; sibling mission owns `chain_metadata` bridge)
- Mission `0206-001-substrate-newtype` (parent — substrate `Database` newtype + migration runner)
- Mission `0206-009-adapter-crates` (parent — adapter crate creation)
- Mission `0105-v3-private-namespace-rollout` (sibling — substrate test vector coverage)

## Out of scope

- `policy_registry` table migration (deferred to separate mission per single-concern; siblings to this; shares migration v017 sequence or separate v018)
- `chain_metadata` table bridge (owned by separate mission per research doc §16)
- Corporate chain operator onboarding flow (owned by RFC-0010 §4 follow-on missions)
- `kind_uuid_registry` 30-UUIDv5 namespace seeding (per RFC-0967-A1 §2.6; deferred to separate RFC-0967-A1 substrate landing mission)
- Live DID provisioning for treasury + corp_admin signers (separate onboarding flow; out of substrate migration scope)

## Dependencies

- `0206-001-substrate-newtype` (substrate `Database` newtype + migration runner)
- `0206-009-adapter-crates` (adapter crate creation)
- RFC-0105 v3.4 (canonical Accepted — defines Authority-to-Issue table)
- RFC-0967-A1 v1.9.2 (canonical Accepted — defines substrate table schema + enforcement)
- RFC-0010 v1.9.2 (canonical Accepted — defines Authority Registration Flow)

## Version History

| Version | Date       | Change                                                                                                                                                                                                                       |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-23 | Initial filing per cross-RFC harmonization close-out RFC-0105 v3.4 mission audit. Substrate landing of `policy_kind_authority` table per RFC-0967-A1 §2.5 + RFC-0010 §4; migration v017 additive per RFC-0206 §Layer B rule. |
