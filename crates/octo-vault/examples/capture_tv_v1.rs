//! Capture TV-V1 vault_id vectors from the reference impl.
//!
//! Run: `cargo run -p octo-vault --example capture_tv_v1`
//!
//! Prints 10 deterministic scenarios: name / chain_string / owner_did /
//! role_token / expected_vault_id_hex. Copy the printed hex back into
//! `tests/test_vectors.rs` to update the central-registry lock.
//!
//! This binary is NOT for production use. It exists so the byte-exact
//! fixture in `tests/test_vectors.rs` can be regenerated whenever the
//! derivation inputs change (add a vector, get its hash; copy hex into
//! the fixture).

use octo_vault::{vault_id, AssetId, ChainId};

struct Vector {
    name: &'static str,
    chain_string: &'static str,
    owner_did: &'static str,
    role_token: &'static str,
}

const VECTORS: &[Vector] = &[
    Vector {
        name: "TV-V1-01_did_octo_test_alice_octo_w_testnet",
        chain_string: "cipherocto/testnet/v1",
        owner_did: "did:octo:test-alice",
        role_token: "OCTO_W",
    },
    Vector {
        name: "TV-V1-02_did_octo_test_bob_octo_w_testnet",
        chain_string: "cipherocto/testnet/v1",
        owner_did: "did:octo:test-bob",
        role_token: "OCTO_W",
    },
    Vector {
        name: "TV-V1-03_did_octo_test_alice_octo_a_testnet",
        chain_string: "cipherocto/testnet/v1",
        owner_did: "did:octo:test-alice",
        role_token: "OCTO_A",
    },
    Vector {
        name: "TV-V1-04_did_octo_test_alice_octo_w_mainnet",
        chain_string: "cipherocto/mainnet/v1",
        owner_did: "did:octo:test-alice",
        role_token: "OCTO_W",
    },
    Vector {
        name: "TV-V1-05_long_did_with_slash_octo_w_testnet",
        chain_string: "cipherocto/testnet/v1",
        owner_did: "did:octo:long-form-identifier-with-many-segments/and/nested/path",
        role_token: "OCTO_W",
    },
    Vector {
        name: "TV-V1-06_short_did_min_form_octo_w_testnet",
        chain_string: "cipherocto/testnet/v1",
        owner_did: "did:octo:x",
        role_token: "OCTO_W",
    },
    Vector {
        name: "TV-V1-07_unicode_did_octo_w_testnet",
        chain_string: "cipherocto/testnet/v1",
        owner_did: "did:octo:\u{00df}",
        role_token: "OCTO_W",
    },
    Vector {
        name: "TV-V1-08_empty_owner_did_sentinel",
        chain_string: "cipherocto/testnet/v1",
        owner_did: "",
        role_token: "OCTO_W",
    },
    Vector {
        name: "TV-V1-09_cross_chain_cross_asset_boundary",
        chain_string: "cipherocto/mainnet/v1",
        owner_did: "did:octo:test-bob",
        role_token: "OCTO_A",
    },
    Vector {
        name: "TV-V1-10_zero_vault_sentinel_mint",
        chain_string: "",
        owner_did: "",
        role_token: "",
    },
];

fn main() {
    for v in VECTORS {
        let chain_id = ChainId::derive(v.chain_string);
        let asset_id = AssetId::derive(v.role_token);
        let id = vault_id(chain_id, v.owner_did, asset_id);
        println!(
            "{}  chain='{}'  owner='{}'  asset='{}'  -> {}",
            v.name,
            v.chain_string,
            v.owner_did,
            v.role_token,
            hex::encode(id.as_bytes())
        );
    }
}
