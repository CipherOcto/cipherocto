-- Mission 0010-f2-registry-namespacing: multi-chain registry
-- namespacing (RFC-0010 §ChainId Namespace Extension).
--
-- Adds a `chain_id` BLOB column carrying the 17-byte canonical
-- encoding of the chain namespace (variant + 15-byte tag +
-- length per `ChainNamespace::canonical_bytes()`).
--
-- Stoolap fork does NOT parse `x'...'` hex literals in DEFAULT
-- clauses (per recon: parser rejects `x'...'` as column ref).
-- The column is added WITHOUT a DEFAULT clause; legacy
-- v008/v009/v010 rows are backfilled via UPDATE to the
-- CIPHEROCTO_MAINNET canonical bytes (17-byte hex encoded as a
-- string literal that the parser casts to BLOB).
--
-- The UPDATE is a no-op on re-run (rows already have chain_id
-- = MAINNET after first apply). Combined with the
-- `is_idempotent_already_applied` guard from mission
-- 0871b-storage-idempotent-alter-hardening, the entire v011
-- migration is retry-safe across mid-apply crashes.
--
-- Composite UNIQUE INDEX on `(chain_id, canonical_hash)` enforces
-- namespace isolation: the same 32-byte hash can be registered
-- independently on multiple chains. The v008 PK on
-- `(canonical_hash)` alone is preserved so single-chain lookups
-- still hit the PK index; multi-chain lookups go through the
-- composite UNIQUE INDEX via `register_in_chain` /
-- `resolve_in_chain`.

ALTER TABLE did_registry ADD COLUMN chain_id BLOB;

-- Backfill legacy rows with the CIPHEROCTO_MAINNET canonical
-- 17-byte prefix:
--   variant = 0x01 (Rfc)
--   tag     = 0xeb3071b5e113330c87630954e3cc08 (CIPHEROCTO_MAINNET_TAG, 15 bytes)
--   length  = 0x12 (18 chars for "cipherocto-mainnet")
-- Hex literal: 01eb3071b5e113330c87630954e3cc0812
-- (parsed as a 34-char string literal; the fork's parser
--  accepts string→BLOB coercion in CAST expressions — verified
--  in tests/stoolap_chain_namespace.rs.)
UPDATE did_registry SET chain_id = CAST('01eb3071b5e113330c87630954e3cc0812' AS BLOB)
    WHERE canonical_hash IS NOT NULL AND chain_id IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS did_registry_chain_hash_uidx
    ON did_registry (chain_id, canonical_hash);