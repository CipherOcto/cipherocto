-- Migration v018: RFC-0903-D1 LiteLLM Persistence.
--
-- 5 tables (litellm_users + litellm_keys + scim_users + scim_groups +
-- scim_group_members) per RFC-0903-D1 §2.
--
-- Mission `0903-d1-litellm-persistence` (Session 5 deferred per
-- F-P5.2-3 RETAIN → implemented per claim-and-implement scope
-- inversion). Closes R2 finding on RFC-0903 amendment path:
-- persistent litellm persistence surfaces (litellm_users +
-- litellm_keys + scim_*) become first-class substrate tables
-- rather than in-memory ephemeral stores.
--
-- Stoolap fork constraints (per [[feedback_stoolap_persistence]] +
-- recon 2026-08-23):
--   1. Rejects `x'...'` hex literals — use `CAST(... AS TEXT)` instead.
--   2. Rejects `FOREIGN KEY ... REFERENCES` — drop FK; enforce via
--      application-layer lookup (`scim_user_lookup`, etc.).
--   3. Rejects partial UNIQUE INDEX with WHERE clause — drop WHERE
--      clauses; enforce one-active via application-layer lookup.
--   4. DQA(12) is the canonical amount-bearing column type per vault
--      v013/v014 + RFC-0959 v2.1. Stoolap fork accepts DQA(12) in DDL
--      and round-trips correctly when inserted via text literal
--      (e.g. `VALUES (..., '1.0', ...)`); Rust i64 parameter binding
--      silently zeros values — handlers MUST bind DQA(12) as String or
--      inline the decimal literal in the SQL string (matches vault
--      substrate pattern at `octo-vault/migrations/v013` + `v014`).
--   5. JSON column type is not native — store JSON as TEXT and parse
--      at the application layer.

-- ─────────────────────────────────────────────────────────────────────
-- Table 1: litellm_users (RFC-0903-D1 §2.1)
-- ─────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS litellm_users (
    user_id BLOB(16) NOT NULL PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL DEFAULT 'internal_user',
    max_budget DQA(12) NOT NULL DEFAULT 0,
    models TEXT,
    tpm_limit INTEGER,
    rpm_limit INTEGER,
    max_parallel_requests INTEGER,
    duration TEXT,
    budget_duration TEXT,
    metadata TEXT,
    permissions TEXT,
    created_at_unix INTEGER NOT NULL,
    updated_at_unix INTEGER NOT NULL,
    CHECK (length(user_id) = 16),
    CHECK (length(email) > 0),
    CHECK (max_budget >= 0)
);

-- ─────────────────────────────────────────────────────────────────────
-- Table 2: litellm_keys (RFC-0903-D1 §2.2)
-- ─────────────────────────────────────────────────────────────────────
-- Stoolap fork rejects FOREIGN KEY; we drop the FK to litellm_users
-- and enforce via application-layer lookup (`litellm_keys_user_idx`
-- + `litellm_user_lookup`). The index is non-partial per fork constraint.
CREATE TABLE IF NOT EXISTS litellm_keys (
    key_hash BLOB(32) NOT NULL PRIMARY KEY,
    user_id BLOB(16) NOT NULL,
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
    revoked_at_unix INTEGER,
    CHECK (length(key_hash) = 32),
    CHECK (length(user_id) = 16),
    CHECK (length(key_type) > 0)
);

CREATE INDEX IF NOT EXISTS litellm_keys_user_idx
    ON litellm_keys(user_id);

-- ─────────────────────────────────────────────────────────────────────
-- Table 3: scim_users (RFC-0903-D1 §2.3)
-- ─────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS scim_users (
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
    schemas TEXT NOT NULL DEFAULT '["urn:ietf:params:scim:schemas:core:2.0:User"]',
    meta_created_unix INTEGER NOT NULL,
    meta_last_modified_unix INTEGER NOT NULL,
    meta_version INTEGER NOT NULL DEFAULT 1,
    last_synced_at_unix INTEGER NOT NULL,
    CHECK (length(user_id) = 16),
    CHECK (length(external_id) > 0),
    CHECK (length(user_name) > 0),
    CHECK (active IN (0, 1))
);

CREATE INDEX IF NOT EXISTS scim_users_external_id_idx
    ON scim_users(external_id);

-- ─────────────────────────────────────────────────────────────────────
-- Table 4: scim_groups (RFC-0903-D1 §2.3)
-- ─────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS scim_groups (
    group_id BLOB(16) NOT NULL PRIMARY KEY,
    external_id TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    meta_created_unix INTEGER NOT NULL,
    meta_last_modified_unix INTEGER NOT NULL,
    CHECK (length(group_id) = 16),
    CHECK (length(external_id) > 0),
    CHECK (length(display_name) > 0)
);

-- ─────────────────────────────────────────────────────────────────────
-- Table 5: scim_group_members (RFC-0903-D1 §2.3)
-- ─────────────────────────────────────────────────────────────────────
-- Stoolap fork rejects FOREIGN KEY; we drop the FKs and enforce via
-- application-layer lookup. Composite PK preserves membership uniqueness.
CREATE TABLE IF NOT EXISTS scim_group_members (
    group_id BLOB(16) NOT NULL,
    user_id BLOB(16) NOT NULL,
    PRIMARY KEY (group_id, user_id),
    CHECK (length(group_id) = 16),
    CHECK (length(user_id) = 16)
);

CREATE INDEX IF NOT EXISTS scim_group_members_user_idx
    ON scim_group_members(user_id);

-- view removed per R2 review: cross-crate ownership ambiguity + phantom column references
-- (te.amount_dqa_micros and te.event_type do not exist in transfer_events;
-- vaults.owner_did is TEXT NOT NULL but litellm_users.user_id is BLOB(16)).
-- Spend aggregation belongs in `octo-vault-storage::aggregate_user_spend(user_id: &[u8; 16]) -> Dqa`
-- as an application-layer method (owner of vaults + transfer_events tables),
-- not a cipherocto-side SQL view. The cipherocto-side cross-crate link table
-- is now v019__litellm_user_vault_link.sql.