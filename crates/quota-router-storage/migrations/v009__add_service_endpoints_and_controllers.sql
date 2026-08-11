-- Mission 0010-f8-rich-did-storage: persist RFC-0010 v1.5
-- §ServiceEndpoint + §ControllerReference on the `did_registry` table
-- (mission 0871b-storage-backend landed v008 schema).
--
-- Encoding: borsh-serialized `Vec<ServiceEndpoint>` / `Vec<ControllerReference>`
-- as BLOB (matches `WireDid`/`RawDid` borsh pattern + the
-- `CapabilityBundleV2` borsh precedent from mission 0957-f-v2-bundle).
-- NULL = legacy v008 row (no rich fields populated); `resolve()` decodes
-- via `unwrap_or_default()` → empty `Vec`.
--
-- No backfill required: pre-v009 rows retain their (canonical_hash,
-- public_key, revoked, updated_at_unix_ms) tuple + NULL rich columns.
-- The resolver-node reconstructs `DidDocument` with empty rich vecs
-- for those rows.
--
-- CipherOcto-side migration per [[stoolap-general-purpose-db]]:
-- schema lives in cipherocto crate, NOT in the stoolap fork.

ALTER TABLE did_registry ADD COLUMN service_endpoints BLOB;
ALTER TABLE did_registry ADD COLUMN controllers BLOB;