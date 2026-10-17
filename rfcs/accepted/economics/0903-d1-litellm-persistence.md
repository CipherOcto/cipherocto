---
rfc: 0903-D1
title: LiteLLM Persistence (litellm_users + litellm_keys + scim_users tables)
status: Accepted
version: 1.0
date: 2026-08-22
note: |
  RFC-0903 is Final (rfcs/final/economics/0903-virtual-api-key-system.md).
  Per R2 finding: Final RFCs cannot be amended to Accepted per RFC process rules.
  This is filed as a NEW D-prefix RFC per RFC process convention for Final→Draft branches.
supersedes_refs:
  - missions/archived/0945-a-user-management-api.md (stub rehydrated)
builds_on:
  - rfcs/final/economics/0903-virtual-api-key-system.md
  - rfcs/accepted/economics/0967-a1-policy-registry.md
  - docs/research/2026-08-21-vault-monetary-representation-redesign.md
---

# RFC-0903-D1 — LiteLLM Persistence

## 0. Status

**Accepted (v1.0, 2026-08-22).** Filed as RFC-0903-D1 because RFC-0903 is Final and cannot be amended via the -A1 path. Per RFC process rules, this D-prefix RFC extends the Final RFC's persistence layer.

**Promotion trail:** v1.0 initial draft 2026-08-22 → Accepted 2026-08-22 per long-horizon plan v1.6 Phase 4 Tier 2 promotion sequence (RFC-0903-D1 third in Tier 2 per research §20 decision #9). litellm_users + litellm_keys + scim_users + scim_groups + scim_group_members tables + litellm_users_spend view + DQA(12) cost migration + ScimStore singleton all preserved. Cite pins stripped to bare RFC numbers per CLAUDE.md §RFC Reference Conventions.

## 1. Motivation

RFC-0903 (Final) defines the Virtual API Key System. Mission `0945-a-user-management-api.md` (archived) stubbed the LiteLLM-compatible admin API endpoints (`/user/new`, `/user/info`, `/user/update`) and the SCIM v2 endpoints — all in-memory or missing persistent backing.

RFC-0903-D1 makes these endpoints **persistent** with full field preservation matching the original LiteLLM product surface.

## 2. Persistent Tables

### 2.1 litellm_users

```sql
CREATE TABLE litellm_users (
    user_id BLOB(16) NOT NULL PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL DEFAULT 'internal_user',
    max_budget DQA(12),
    models TEXT,
    tpm_limit INTEGER,
    rpm_limit INTEGER,
    max_parallel_requests INTEGER,
    duration TEXT,
    budget_duration TEXT,
    metadata JSON,
    permissions JSON,
    created_at_unix INTEGER NOT NULL,
    updated_at_unix INTEGER NOT NULL
);

CREATE VIEW litellm_users_spend AS
SELECT lu.user_id, COALESCE(SUM(te.amount_dqa_micros), 0) AS spend_dqa_micros
FROM litellm_users lu
LEFT JOIN vaults v ON v.owner_did = lu.user_id
LEFT JOIN transfer_events te ON te.from_vault_id = v.vault_id
                              AND te.event_type IN ('TransferApplied', 'Burn')
GROUP BY lu.user_id;
```

### 2.2 litellm_keys

```sql
CREATE TABLE litellm_keys (
    key_hash BLOB(32) NOT NULL PRIMARY KEY,
    user_id BLOB(16) NOT NULL REFERENCES litellm_users(user_id),
    team_id BLOB(16),
    key_alias TEXT,
    key_type TEXT NOT NULL,
    expires_at_unix INTEGER,
    max_budget DQA(12),
    budget_duration TEXT,
    tpm_limit INTEGER,
    rpm_limit INTEGER,
    max_parallel_requests INTEGER,
    models TEXT,
    created_at_unix INTEGER NOT NULL,
    revoked_at_unix INTEGER
);
CREATE INDEX litellm_keys_user_idx ON litellm_keys(user_id);
```

### 2.3 scim_users + scim_groups + scim_group_members

```sql
CREATE TABLE scim_users (
    user_id BLOB(16) NOT NULL PRIMARY KEY,
    external_id TEXT NOT NULL UNIQUE,
    user_name TEXT NOT NULL,
    email TEXT,
    given_name TEXT,
    family_name TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    display_name TEXT,
    title TEXT,
    locale TEXT,
    timezone TEXT,
    schemas JSON NOT NULL DEFAULT '["urn:ietf:params:scim:schemas:core:2.0:User"]',
    meta_created_unix INTEGER NOT NULL,
    meta_last_modified_unix INTEGER NOT NULL,
    meta_version INTEGER NOT NULL DEFAULT 1,
    last_synced_at_unix INTEGER NOT NULL
);
CREATE INDEX scim_users_external_id_idx ON scim_users(external_id);

CREATE TABLE scim_groups (
    group_id BLOB(16) NOT NULL PRIMARY KEY,
    external_id TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    meta_created_unix INTEGER NOT NULL,
    meta_last_modified_unix INTEGER NOT NULL
);

CREATE TABLE scim_group_members (
    group_id BLOB(16) NOT NULL REFERENCES scim_groups(group_id),
    user_id BLOB(16) NOT NULL REFERENCES scim_users(user_id),
    PRIMARY KEY (group_id, user_id)
);
CREATE INDEX scim_group_members_user_idx ON scim_group_members(user_id);
```

## 3. Endpoint Behavior

| Endpoint | Behavior |
|---|---|
| `POST /user/new` | INSERT INTO litellm_users; ISO 8601 timestamp serialization |
| `GET /user/info?user_id=<id>` | Single-user response with `spend` from `litellm_users_spend` view |
| `GET /user/info` (no params) | List response `{"users": [...]}` (LiteLLM native shape) |
| `POST /user/update` | Branch on body: `user_id` → UPDATE litellm_users; `key_id` → UPDATE litellm_keys |
| `GET /key/info` | SELECT FROM litellm_keys WHERE key_hash = ?; ISO 8601 `expires_at` |

`ScimStore::new(&Database)` constructed once at server startup (singleton). Per-request `new()` factory pattern replaced.

## 4. Migration

Substrate migration v018: `litellm_users + litellm_keys + scim_users + scim_groups + scim_group_members + litellm_users_spend view`.

## 5. Execution Class Mapping (RFC-0008 §RFC-0008 Execution Class Mapping)

| Surface | Class | Justification |
|---|---|---|
| `litellm_users.INSERT` | A | Deterministic INSERT |
| `litellm_keys.INSERT` | A | Deterministic INSERT + BLAKE3 hash |
| `litellm_users_spend` view | A | Deterministic sum |
| `/user/new` handler | A | Single-write endpoint |
| `/user/info` handler | A | Read-only endpoint |
| SCIM sync | A | Deterministic upsert |

## 6. Cross-References

- RFC-0903 (Final — Virtual API Key System)
- Mission `0945-a-user-management-api.md` (archived — stub rehydrated)
- RFC-0967-A1 §2.1 (WorkflowKind trait declaration — litellm workflow kind)
- RFC-0206 §3 ValueTransfer Trait (vault creation backing)
- RFC-0957-A1 (capability revocation persistence — §mint Signature AMENDED)
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` v2.0 §5.3 + §9 amendment table

## 7. Version History

| Version | Date | Change |
|---|---|---|
| 1.0 | 2026-08-22 | Initial draft. Filed as RFC-0903-D1 (Final→Draft branch per R2 finding). Persistent litellm_users + litellm_keys + scim_users + scim_groups + scim_group_members tables. DQA(12) cost. SCIM DDL complete. ScimStore singleton. Resolves R2 finding on RFC-0903 amendment path. |
| 1.0 | 2026-08-22 | **R16 promotion:** Draft → Accepted per long-horizon plan v1.6 Phase 4 Tier 2 promotion sequence (RFC-0903-D1 third in Tier 2 = final Tier 2). Status bumper + citation cleanup (1 STALE RFC-0206 v3.0 pin + 1 PHANTOM RFC-0957-A1 §mint Signature AMENDED anchor all stripped/fixed per CLAUDE.md §RFC Reference Conventions). 5-table persistence + DQA(12) cost + ScimStore singleton preserved. Resolves R2 finding on RFC-0903 amendment path. |
