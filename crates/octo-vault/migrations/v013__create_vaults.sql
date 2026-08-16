-- v013__create_vaults.sql (plan §B.3 / stream B.3; review §20.3 Model B)
--
-- Canonical vault_id derivation (per §8.10 TV-V1):
--   vault_id = BLAKE3("cipherocto/vault/v1/" + chain_id + owner_did + asset_id)
-- where chain_id = BLAKE3("cipherocto/chain/v1/" + chain_string) per §20.3.2
-- and asset_id  = BLAKE3("cipherocto/asset/v1/" + role_token)     per §20.3.1
--
-- PK shape per RFC-0960 (chain-aware bump) + §20.3: composite (chain_id, owner_did, asset_id)
-- VaultHierarchy dropped (phantom type, see §20.10); no parent_vault_id.
--
-- Naming convention: bare table names (Stoolap fork parser does not support
-- schema-qualified identifiers at S3 stage). Crate ownership is recorded in
-- the Cargo.toml path + substrate tracker table.
--
-- Stoolap fork parser constraint: NO inline `--` comments mid-statement —
-- parser fails with "expected column name" at the comment start. Comments
-- must be on their own lines, or omitted. Substrate-level documentation
-- lives in this header block.

CREATE TABLE IF NOT EXISTS vaults (
    vault_id        BLOB(32) NOT NULL,
    chain_id        BLOB(32) NOT NULL,
    owner_did       TEXT     NOT NULL,
    asset_id        BLOB(32) NOT NULL,
    balance         DQA(12)  NOT NULL,
    policy          BLOB     NOT NULL,
    state           TEXT     NOT NULL,
    created_at_unix BIGINT   NOT NULL,
    metadata        BLOB     NOT NULL,
    PRIMARY KEY (chain_id, owner_did, asset_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS vaults_vault_id_idx ON vaults(vault_id);
