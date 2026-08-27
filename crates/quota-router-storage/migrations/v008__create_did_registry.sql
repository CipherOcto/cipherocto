-- Mission 0871b-storage-backend: persistent DidRegistry substrate
-- (RFC-0010 §Storage Extension §StoolapDidRegistry).
--
-- Schema: per-canonical-hash DID document keyed for upsert + revoke
-- + list. The resolver-node wires `register`/`resolve`/`revoke`/`list`
-- via the `DidRegistry` trait in `octo-ident` (Layer B).
--
-- Concurrency: row-level locking via stoolap's per-statement
-- transaction. Concurrent register/resolve/revoke on the same
-- `canonical_hash` serialize so torn writes are impossible
-- (atomic per-key guarantee — matches `spend_ledger` v007 pattern).
--
-- Cipherocto-side migration per [[stoolap-general-purpose-db]]:
-- schema lives in cipherocto crate, NOT in the stoolap fork.

CREATE TABLE IF NOT EXISTS did_registry (
    canonical_hash BLOB NOT NULL,
    public_key BLOB NOT NULL,
    revoked INTEGER NOT NULL DEFAULT 0,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY (canonical_hash)
);

CREATE INDEX IF NOT EXISTS did_registry_updated_at_idx
    ON did_registry (updated_at_unix_ms);
