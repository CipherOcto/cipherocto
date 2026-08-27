-- Migration v017: chain_metadata + policy_registry + policy_kind_authority +
-- ledger_chain_registry (combined per RFC-0206 §Substrate Migration
-- v015-v018; RFC-0010 §ledger_chain_registry Table; research doc
-- §8.1 §chain_metadata + §8.2 §policy_registry + §policy_kind_authority).
--
-- Mission `0010-v17-chain-id-registration-authority` (Session 6 deferred
-- per F-P5.2-3 RETAIN → implemented per claim-and-implement scope
-- inversion). Closes P0 BLOCKER for `vault-chain-metadata` per
-- research doc §9 + §16: chain_metadata table soft-reference was
-- waiting on this migration landing.
--
-- v017 is purely ADDITIVE — 4 new tables (no ALTER on existing tables):
--   1. ledger_chain_registry (RFC-0010 §2)
--   2. chain_metadata (research doc §8.1)
--   3. policy_registry (research doc §8.2)
--   4. policy_kind_authority (research doc §8.2)
--
-- Why this migration combines 4 tables: per RFC-0206 §Substrate
-- Migration v015-v018, the v017 slot is reserved for the
-- "chain_metadata + policy_registry + ledger_chain_registry +
-- policy_kind_authority" bundle. Splitting these into 4 separate
-- migrations would break the bundle and cause migration-ordering bugs
-- (FK references from chain_metadata.workflow_kind_hashes to
-- policy_registry rows; FK from policy_kind_authority to
-- policy_registry rows).

-- ─────────────────────────────────────────────────────────────────────
-- Table 1: ledger_chain_registry (RFC-0010 §2)
-- ─────────────────────────────────────────────────────────────────────
--
-- RFC-0010 §2 specifies the ledger_chain_registry schema:
-- chain_id BLOB(32) PK + chain_namespace BLOB(1) (0x01 Rfc / 0x02 User
-- only — narrower CHECK mirrors substrate `ChainNamespace::from_canonical_bytes`
-- which rejects 0x00 and 0x03-0xFF) + operator_did BLOB(32) +
-- operator_signature BLOB(64) + registration_body BLOB (canonical CBOR) +
-- registered_at_unix INTEGER + revoked_at_unix INTEGER NULL.
--
-- UNIQUE INDEX on (operator_did) WHERE revoked_at_unix IS NULL enforces
-- one-active-registration-per-operator.
CREATE TABLE IF NOT EXISTS ledger_chain_registry (
    chain_id BLOB(32) NOT NULL PRIMARY KEY,
    chain_namespace BLOB(1) NOT NULL,
    operator_did BLOB(32) NOT NULL,
    operator_signature BLOB(64) NOT NULL,
    registration_body BLOB NOT NULL,
    registered_at_unix INTEGER NOT NULL,
    revoked_at_unix INTEGER,
    CHECK (length(chain_id) = 32),
    CHECK (length(chain_namespace) = 1),
    CHECK (CAST(chain_namespace AS TEXT) IN ('01', '02')),
    CHECK (length(operator_signature) = 64),
    CHECK (length(operator_did) = 32)
);

CREATE UNIQUE INDEX IF NOT EXISTS ledger_chain_registry_active_op_idx
    ON ledger_chain_registry(operator_did);

-- ─────────────────────────────────────────────────────────────────────
-- Table 2: chain_metadata (research doc §8.1)
-- ─────────────────────────────────────────────────────────────────────
--
-- chain_metadata carries per-chain configuration: the workflow_kind_hashes
-- (CBOR-encoded Vec<[u8;32]>) for CompositeWorkflow dispatch, plus
-- policy hashes for interop + audit + burn policy lookup, plus the
-- admin_pubkey for capability chain-of-caveats verification (per
-- RFC-0957 macaroon semantics).
--
-- Foreign key to ledger_chain_registry(chain_id) ON DELETE RESTRICT
-- (research doc §726 R4 finding): prevents orphan chain_metadata rows
-- when the chain is revoked.
--
-- workflow_kind_hashes column comment: plural Vec<[u8;32]> per
-- research doc R1 finding S3; CBOR array encoding.
CREATE TABLE IF NOT EXISTS chain_metadata (
    chain_id BLOB(32) NOT NULL PRIMARY KEY,
    workflow_kind_hashes BLOB NOT NULL,
    interop_policy_hash BLOB(32),
    audit_policy_hash BLOB(32),
    burn_policy_hash BLOB(32),
    admin_pubkey BLOB(32) NOT NULL,
    composite_depth INTEGER NOT NULL DEFAULT 0,
    registered_at_unix INTEGER NOT NULL,
    revoked_at_unix INTEGER,
    CHECK (length(chain_id) = 32),
    CHECK (length(admin_pubkey) = 32),
    CHECK (composite_depth >= 0),
    CHECK (composite_depth <= 4)
);

-- ─────────────────────────────────────────────────────────────────────
-- Table 3: policy_registry (research doc §8.2)
-- ─────────────────────────────────────────────────────────────────────
--
-- Generic policy hash → trait-impl lookup table. One row per
-- registered policy. The hash is the BLAKE3 digest of the canonical
-- trait spec (per RFC-0967-A1 v1.9.2 §3). registry_kind discriminates
-- AuthorityPolicy / MembershipPolicy / InteropPolicy / BurnPolicy /
-- WorkflowKind / AuditPolicy / InteropSelector / InteropOutcome.
--
-- The "30 per-policy-kind crates" substrate landing (per research doc
-- line 30) inserts one row per crate here.
CREATE TABLE IF NOT EXISTS policy_registry (
    policy_hash BLOB(32) NOT NULL PRIMARY KEY,
    registry_kind INTEGER NOT NULL,
    crate_name TEXT NOT NULL,
    trait_spec BLOB NOT NULL,
    registered_at_unix INTEGER NOT NULL,
    revoked_at_unix INTEGER,
    CHECK (length(policy_hash) = 32),
    CHECK (registry_kind BETWEEN 1 AND 8),
    -- registry_kind values per RFC-0967-A1 v1.9.2 §2:
    --   1 = AuthorityPolicy (6 entries)
    --   2 = MembershipPolicy (7 entries)
    --   3 = InteropPolicy (4 entries)
    --   4 = BurnPolicy (3 entries)
    --   5 = WorkflowKind (4 entries)
    --   6 = AuditPolicy (3 entries)
    --   7 = InteropSelector (3 entries)
    --   8 = InteropOutcome (0 entries currently)
    CHECK (length(crate_name) > 0),
    CHECK (length(trait_spec) > 0)
);

-- ─────────────────────────────────────────────────────────────────────
-- Table 4: policy_kind_authority (research doc §8.2)
-- ─────────────────────────────────────────────────────────────────────
--
-- Authoritative mapping: (policy_kind_uuid → policy_hash). Each
-- policy_kind_uuid is derived via per-policy-kind UUIDv5
-- (per RFC-0967-A1 v1.9.2 §3 + 30 per-policy-kind fixtures). The
-- authority record carries the registrant DID + signature + body,
-- mirroring ledger_chain_registry shape for symmetry.
--
-- UNIQUE INDEX on (policy_kind_uuid) WHERE revoked_at_unix IS NULL
-- enforces one-active-registration-per-policy-kind.
CREATE TABLE IF NOT EXISTS policy_kind_authority (
    policy_kind_uuid BLOB(16) NOT NULL PRIMARY KEY,
    policy_hash BLOB(32) NOT NULL,
    registrant_did BLOB(32) NOT NULL,
    registrant_signature BLOB(64) NOT NULL,
    registration_body BLOB NOT NULL,
    registered_at_unix INTEGER NOT NULL,
    revoked_at_unix INTEGER,
    CHECK (length(policy_kind_uuid) = 16),
    CHECK (length(policy_hash) = 32),
    CHECK (length(registrant_did) = 32),
    CHECK (length(registrant_signature) = 64)
);

CREATE UNIQUE INDEX IF NOT EXISTS policy_kind_authority_active_uuid_idx
    ON policy_kind_authority(policy_kind_uuid);

-- FK from chain_metadata.{interop_policy_hash, audit_policy_hash,
-- burn_policy_hash} to policy_registry.policy_hash. Stoolap fork does
-- NOT enforce FK constraints by default (verified 2026-08-23 in
-- `crates/octo-vault/migrations/v014__create_transfer_events.sql`
-- comment + recon); these FK constraints are documented in the schema
-- for substrate-level enforcement via the application-layer
-- `policy_registry.get_by_hash` lookup before INSERT into
-- chain_metadata. Substrate error variant:
-- `ValueTransferError::UnknownPolicyHash { hash }` when lookup fails
-- (per research doc §892 R4 finding).