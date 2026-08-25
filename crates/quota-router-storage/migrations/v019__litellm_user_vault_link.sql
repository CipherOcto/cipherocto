-- Migration v019: RFC-0903-D1 LiteLLM user→vault link bridge table.
--
-- Cross-crate bridge table: cipherocto litellm_users.user_id (BLOB(16))
-- ↔ octo-vault vaults.vault_id. Ownership of vaults lives in
-- octo-vault; this table is cipherocto's view of the link.
--
-- Mission `0903-d1-litellm-persistence` (R2 review follow-up). The
-- v018 view `litellm_users_spend` was removed per cross-crate
-- ownership ambiguity + phantom column references
-- (te.amount_dqa_micros / te.event_type not in transfer_events;
-- vaults.owner_did TEXT vs litellm_users.user_id BLOB(16) join
-- incompatibility). Spend aggregation now belongs in
-- `octo-vault-storage::aggregate_user_spend(user_id: &[u8; 16]) -> Dqa`
-- as an application-layer method; this bridge table supports the
-- lookup-before-aggregate pattern.
--
-- Stoolap fork constraints (per [[feedback_stoolap_persistence]] +
-- recon 2026-08-23):
--   1. Rejects `FOREIGN KEY ... REFERENCES` — drop FK; enforce via
--      application-layer lookup (`litellm_user_vault_link_lookup`).
--   2. Rejects partial UNIQUE INDEX with WHERE clause — drop WHERE
--      clauses; enforce one-active-link via application-layer lookup.
--   3. Bridge table is cipherocto-side only; octo-vault owns the
--      canonical vaults table. No DDL dependencies on octo-vault
--      migrations (decoupled substrate layers per RFC-0206).

-- ─────────────────────────────────────────────────────────────────────
-- Table: litellm_user_vault_link
-- ─────────────────────────────────────────────────────────────────────
-- Composite PK (user_id, vault_id) preserves that a user may hold
-- multiple vaults over time (e.g. rotated, revoked-then-rebound).
-- revoked_at_unix is NULL while the link is active; non-NULL marks
-- soft-revocation timestamp. Application layer enforces
-- one-active-link per user_id via lookup-before-insert.
CREATE TABLE IF NOT EXISTS litellm_user_vault_link (
    user_id BLOB(16) NOT NULL,
    vault_id BLOB(16) NOT NULL,
    linked_at_unix INTEGER NOT NULL,
    revoked_at_unix INTEGER,
    PRIMARY KEY (user_id, vault_id),
    CHECK (length(user_id) = 16),
    CHECK (length(vault_id) = 16)
);

CREATE INDEX IF NOT EXISTS litellm_user_vault_link_vault_idx
    ON litellm_user_vault_link(vault_id);
