//! CLI integration test for `octo-wallet ask publish` (RFC-0959 §CLI).
//!
//! Verifies end-to-end: shell out to the binary, drive a publish, and assert
//! the resulting signed Ask JSON deserializes through `AskSigned::verify`.
//!
//! Steps:
//! 1. Init a wallet seed (32 bytes) via `init --node-type self-host`.
//! 2. Invoke `ask publish` with canonical inputs.
//! 3. Parse the stdout JSON as `AskSigned`.
//! 4. Re-derive the asker DID from the seed → public key.
//! 5. `AskSigned::verify` with the derived public key passes.

use std::process::Command;

use assert_cmd::prelude::*;
use ed25519_dalek::VerifyingKey;
use quota_router_storage::ask::{AskSigned, AskSignedError};

const BIN: &str = env!("CARGO_BIN_EXE_octo-wallet");

fn octo_wallet() -> Command {
    Command::new(BIN)
}

#[test]
fn ask_publish_produces_signed_ask_that_verifies() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let seed_path = tmp.path().join("identity.seed");
    let node_type = "self-host";

    // 1. Init the wallet seed.
    octo_wallet()
        .args(["init", "--node-type", node_type, "--seed-out"])
        .arg(&seed_path)
        .assert()
        .success();

    // 2. Invoke `ask publish`.
    let output = octo_wallet()
        .args([
            "ask",
            "publish",
            "--node-type",
            node_type,
            "--model",
            "openai/gpt-4",
            "--axes",
            "input_tokens_per_1k:30,output_tokens_per_1k:60",
            "--ttl-unix",
            "1900000000",
            "--jurisdiction",
            "US-CA",
            "--nonce-hex",
            "8c3e6f4b2a1d9e7c5f8b3a6d9c2e5f8b",
            "--published-at-unix",
            "1700000000",
            "--seed",
        ])
        .arg(&seed_path)
        .output()
        .expect("invoke `ask publish`");

    assert!(
        output.status.success(),
        "ask publish failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");

    // 3. Parse AskSigned.
    let signed: AskSigned = serde_json::from_str(&stdout).expect("parse AskSigned JSON");

    // 4. Derive asker DID from the seed (matches `derive_asker_did` in the CLI).
    let seed_bytes = std::fs::read(&seed_path).expect("read seed");
    assert_eq!(seed_bytes.len(), 32);
    let mut seed_arr = [0u8; 32];
    seed_arr.copy_from_slice(&seed_bytes);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed_arr);
    let verifying_key: VerifyingKey = (&signing_key).into();

    // 5. Verify signature.
    let result = signed.verify(&verifying_key.to_bytes());
    assert!(
        result.is_ok(),
        "AskSigned::verify failed: {result:?} (expected: {:?})",
        AskSignedError::AskSignatureInvalid
    );
    // Sanity: ask_id is non-zero.
    assert!(
        signed.ask_id.iter().any(|b| *b != 0),
        "ask_id must be non-zero"
    );
    // Sanity: asker_did in payload matches the derived DID.
    let derived_did = format!("did:octo:b{}", hex::encode(verifying_key.to_bytes()));
    assert_eq!(
        signed.payload.asker_did, derived_did,
        "asker_did must match derived DID from seed"
    );
}

#[test]
fn ask_publish_rejects_oversize_nonce() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let seed_path = tmp.path().join("identity.seed");
    octo_wallet()
        .args(["init", "--node-type", "self-host", "--seed-out"])
        .arg(&seed_path)
        .assert()
        .success();

    // 64 hex chars = 32 bytes (RFC-0959 requires 16 bytes / 32 hex chars).
    let long_hex = "8c3e6f4b2a1d9e7c5f8b3a6d9c2e5f8b1a4d7c0e3f6b9a2d5c8e1f4b7a0d3c6f"; // 63 chars (odd)
    octo_wallet()
        .args([
            "ask",
            "publish",
            "--node-type",
            "self-host",
            "--model",
            "openai/gpt-4",
            "--axes",
            "input_tokens_per_1k:30",
            "--ttl-unix",
            "1900000000",
            "--jurisdiction",
            "US-CA",
            "--nonce-hex",
            long_hex,
            "--seed",
        ])
        .arg(&seed_path)
        .assert()
        .failure()
        .stderr(predicates::str::contains("nonce must be 16 bytes"));
}

#[test]
fn ask_publish_rejects_invalid_axes_format() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let seed_path = tmp.path().join("identity.seed");
    octo_wallet()
        .args(["init", "--node-type", "self-host", "--seed-out"])
        .arg(&seed_path)
        .assert()
        .success();

    // Missing colon in axes entry: "input_tokens_per_1k" should be "input_tokens_per_1k:30".
    octo_wallet()
        .args([
            "ask",
            "publish",
            "--node-type",
            "self-host",
            "--model",
            "openai/gpt-4",
            "--axes",
            "input_tokens_per_1k",
            "--ttl-unix",
            "1900000000",
            "--jurisdiction",
            "US-CA",
            "--nonce-hex",
            "8c3e6f4b2a1d9e7c5f8b3a6d9c2e5f8b",
            "--seed",
        ])
        .arg(&seed_path)
        .assert()
        .failure()
        .stderr(predicates::str::contains("expected `<axis>:<rate>`"));
}
