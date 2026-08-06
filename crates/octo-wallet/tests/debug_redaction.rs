//! Debug redaction contract tests (mission 0957-b R3 carryover #15).
//!
//! Asserts that `Debug` output for security-sensitive octo-wallet structs
//! does NOT contain raw secret bytes. Per the user's explicit constraint:
//! "octo-wallet is security sensitive, Debug should not leak in full
//! security related data".
//!
//! Strategy: construct each sensitive struct with a marker byte pattern
//! `0xCC` that is NOT used anywhere else in production code. Run
//! `format!("{:?}", x)`. Assert the marker is absent (would mean secret
//! material was dumped).
//!
//! Markers:
//! - Identity seed: `0xCC 0xCC 0xCC ... 0xCC` (32 bytes)
//! - Holder signature: `0xDD 0xDD ... 0xDD` (64 bytes — fed into the
//!   signer so the signature bytes are derived deterministically)
//! - Root secret: `0xEE 0xEE ... 0xEE` (32 bytes)
//! - Macaroon chain HMACs: produced by minting; we can't seed them, but
//!   we assert `chain_len` shows up instead of the raw bytes.
//!
//! The test name itself documents which struct is being checked; if the
//! test fails, the failure message identifies the leak.

use std::collections::{HashMap, HashSet};

use octo_ident::test_helpers::sample_did;
use octo_wallet::capability::{
    macaroon::{CapabilityCatalog, Macaroon},
    zk_mint::{CapabilityClass, PrivateWitness, ProofBundle},
    CapabilityToken, Caveat,
};
use octo_wallet::hsm::{InMemorySigner, LedgerSigner};
use octo_wallet::identity::IdentityKey;
use octo_wallet::key_hierarchy::{KeyHierarchy, MissionId};
use octo_wallet::keystore::{CipherParams, Crypto, KdfParams, KeystoreFile};
use octo_wallet::mpc::KeyShare;
use octo_wallet::vault::VaultFile;

/// Test-only no-op catalog (the production `InMemoryCatalog` is gated
/// to in-crate tests via `#[cfg(test)]`). This minimal stand-in
/// satisfies `CapabilityCatalog` for the integration test below.
#[derive(Default, Clone, Debug)]
struct TestCatalog {
    by_id: HashMap<[u8; 32], Macaroon>,
    raw_names: HashSet<String>,
}

impl CapabilityCatalog for TestCatalog {
    fn get(&self, id: &[u8; 32]) -> Option<&Macaroon> {
        self.by_id.get(id)
    }
    fn is_raw_name_registered(&self, name: &str) -> bool {
        self.raw_names.contains(name)
    }
}

/// Marker byte for "this seed was the identity seed". The marker MUST
/// NOT appear in any Debug output below; if it does, a redacted field
/// is leaking the secret to logs / panic messages.
const SEED_MARKER: [u8; 32] = [
    0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
    0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
];

const ROOT_SECRET_MARKER: [u8; 32] = [
    0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE,
    0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE,
];

const SHARE_MARKER: [u8; 32] = [
    0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77,
    0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77,
];

/// Asserts `haystack` does NOT contain the hex-encoded marker bytes.
/// Helper for readable failure messages.
fn assert_no_marker(test_name: &str, debug_output: &str) {
    // SEED_MARKER hex = 64 chars of "cc". Check the longer unambiguous
    // pattern of 16 repeated bytes ("cccc") which would never appear in
    // real Debug output that doesn't leak the secret.
    let marker_hex = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    assert!(
        !debug_output.contains(marker_hex),
        "DEBUG LEAK in {test_name}: hex marker `{marker_hex}` (64 chars) found in Debug output.\n\
         Debug output:\n{debug_output}"
    );
}

fn assert_no_marker_77(test_name: &str, debug_output: &str) {
    let marker_hex = "7777777777777777777777777777777777777777777777777777777777777777";
    assert!(
        !debug_output.contains(marker_hex),
        "DEBUG LEAK in {test_name}: hex marker `{marker_hex}` (64 chars) found in Debug output.\n\
         Debug output:\n{debug_output}"
    );
}

#[test]
fn identity_key_debug_does_not_leak_seed() {
    let k = IdentityKey::from_seed(SEED_MARKER);
    let s = format!("{k:?}");
    assert_no_marker("IdentityKey", &s);
    // Tripwire: the manual Debug impl prints `public_key` as a hex string
    // field; a regression to `#[derive(Debug)]` would render the
    // `IdentityKey(SigningKey(...))` tuple form (no `public_key` field
    // name, and the SigningKey inner Debug surfaces the secret scalar).
    // Asserting `public_key` is present catches that regression.
    assert!(
        s.contains("public_key"),
        "IdentityKey Debug must include the redacted `public_key` field; got: {s}"
    );
}

#[test]
fn capability_key_debug_does_not_leak_bytes() {
    // CapabilityKey has no public constructor — use derive_capability_key
    // with a deterministic (identity, audience, channel) tuple to get one.
    let identity = IdentityKey::from_seed(SEED_MARKER);
    let audience: octo_wallet::identity::AudienceId = sample_did(241).parse().unwrap();
    let channel: octo_wallet::identity::ChannelId = "channel-redaction".parse().unwrap();
    let k = octo_wallet::identity::derive_capability_key(&identity, &audience, &channel).unwrap();
    let s = format!("{k:?}");
    assert_no_marker("CapabilityKey", &s);
    // Tripwire: the manual Debug uses the literal `[REDACTED]` for the
    // 32-byte key field. A `#[derive(Debug)]` regression would render
    // `[u8; 32]` as `[204, 204, ...]` and omit the magic string.
    assert!(
        s.contains("[REDACTED]"),
        "CapabilityKey Debug must include the `[REDACTED]` marker; got: {s}"
    );
}

#[test]
fn in_memory_signer_debug_does_not_leak_seed() {
    let pk = IdentityKey::from_seed(SEED_MARKER).public_key_bytes();
    let s = InMemorySigner::new(SEED_MARKER, pk);
    let out = format!("{s:?}");
    assert_no_marker("InMemorySigner", &out);
    // Tripwire: manual Debug prints `[REDACTED]` for `seed_bytes`.
    // Derived Debug would print `[u8; 32]` as decimal `[204, 204, ...]`.
    assert!(
        out.contains("[REDACTED]"),
        "InMemorySigner Debug must include the `[REDACTED]` marker for seed_bytes; got: {out}"
    );
}

#[test]
fn ledger_signer_debug_does_not_leak_seed() {
    let pk = IdentityKey::from_seed(SEED_MARKER).public_key_bytes();
    let s = LedgerSigner::new(SEED_MARKER, pk);
    let out = format!("{s:?}");
    assert_no_marker("LedgerSigner", &out);
    // Tripwire: LedgerSigner Debug delegates to the inner InMemorySigner
    // which prints `[REDACTED]`. A `#[derive(Debug)]` regression on
    // LedgerSigner would print the inner field as a struct literal with
    // the seed bytes as decimal — `[REDACTED]` would be absent.
    assert!(
        out.contains("[REDACTED]"),
        "LedgerSigner Debug must propagate the inner `[REDACTED]` marker; got: {out}"
    );
}

#[test]
fn key_hierarchy_debug_does_not_leak_seed() {
    let h = KeyHierarchy::new(SEED_MARKER);
    let out = format!("{h:?}");
    assert_no_marker("KeyHierarchy", &out);
    // Tripwire: manual Debug prints `[REDACTED 32 bytes]` for the
    // `identity_seed` field. A `#[derive(Debug)]` regression would print
    // `[u8; 32]` as decimal `[204, 204, ...]` and omit the magic string.
    assert!(
        out.contains("[REDACTED 32 bytes]"),
        "KeyHierarchy Debug must include the `[REDACTED 32 bytes]` marker for identity_seed; got: {out}"
    );

    // Also verify derived key Debug does not leak the seed (KeyHierarchy
    // derives `identity_seed` only, but a downstream user might Debug a
    // MissionKey or similar — exercise the derive path).
    let m = MissionId {
        asker_did: sample_did(169).clone(),
        model: "openai/gpt-4".to_owned(),
    };
    let k = h.derive_mission_key(&m).unwrap();
    let k_dbg = format!("{k:?}");
    // MissionKey holds [u8; 32] derived from SEED_MARKER via blake3 derive_key
    // — the derived bytes won't match the marker. Tripwire: the manual
    // MissionKey Debug prints `[REDACTED 32 bytes]`. A regression to
    // `#[derive(Debug)]` would print `[u8; 32]` as decimal and omit the
    // magic string.
    assert!(
        k_dbg.contains("[REDACTED 32 bytes]"),
        "MissionKey Debug must include the `[REDACTED 32 bytes]` marker; got: {k_dbg}"
    );
}

#[test]
fn macaroon_debug_does_not_leak_chain_or_root_secret_hash() {
    let m = Macaroon::mint(&ROOT_SECRET_MARKER).unwrap();
    let out = format!("{m:?}");
    // Tripwire: the manual Macaroon Debug prints `chain_len` (a count,
    // not the chain bytes) and the literal `[REDACTED 32 bytes]` /
    // `[REDACTED 16 bytes]` markers for the secret-derived fields. A
    // `#[derive(Debug)]` regression would print the chain as
    // `chain: [[204, 204, ...], ...]` (decimal array of [u8; 32] values)
    // and the literal `chain_len` field name would be absent. The
    // redaction-marker strings would also be absent.
    assert!(
        out.contains("chain_len"),
        "Macaroon Debug must include chain_len (redaction marker); got: {out}"
    );
    assert!(
        out.contains("[REDACTED 32 bytes]"),
        "Macaroon Debug must include `[REDACTED 32 bytes]` for root_secret_hash / id; got: {out}"
    );
    assert!(
        out.contains("[REDACTED 16 bytes]"),
        "Macaroon Debug must include `[REDACTED 16 bytes]` for root_id; got: {out}"
    );
    // Note: the hex marker assertion was previously here as documentation,
    // but `ROOT_SECRET_MARKER` is the 32-byte root secret, while
    // `macaroon.root_secret_hash` is `BLAKE3(ROOT_SECRET_MARKER)` —
    // distinct bytes. The chain HMACs are also BLAKE3-derived from the
    // root secret. The marker is irrelevant to Macaroon (the redacted
    // fields carry derived bytes, not the raw secret). The redaction
    // marker strings above are the real tripwires.
}

#[test]
fn capability_token_debug_does_not_leak_holder_sig_or_chain() {
    let holder = IdentityKey::from_seed(SEED_MARKER);
    let caveats = [Caveat::Model("gpt-4".to_owned())];
    let token =
        CapabilityToken::mint(&ROOT_SECRET_MARKER, &holder, &sample_did(192), &caveats).unwrap();
    let out = format!("{token:?}");
    assert_no_marker("CapabilityToken (holder seed)", &out);
    // holder_sig is derived from holder.sign(...); it does NOT contain
    // the SEED_MARKER bytes directly, but the redacted form must NOT
    // include the literal hex of holder_sig.to_bytes() either. We assert
    // the explicit redaction marker is present.
    assert!(
        out.contains("[REDACTED 64 bytes]"),
        "CapabilityToken Debug must include '[REDACTED 64 bytes]' (holder_sig redaction marker); got: {out}"
    );
    assert!(
        out.contains("discharges_count"),
        "CapabilityToken Debug must include discharges_count; got: {out}"
    );
}

#[test]
fn private_witness_debug_does_not_leak_root_secret_or_holder_sig() {
    let holder = IdentityKey::from_seed(SEED_MARKER);
    let token = CapabilityToken::mint(&ROOT_SECRET_MARKER, &holder, &sample_did(192), &[]).unwrap();
    let witness = PrivateWitness {
        cap_root_secret: ROOT_SECRET_MARKER,
        holder_sig: token.holder_sig,
        caveats_full: vec![],
        discharges_full: vec![],
        inference_trace: None,
    };
    let out = format!("{witness:?}");
    // The cap_root_secret IS ROOT_SECRET_MARKER (32 bytes of 0xEE) — it
    // would appear as "eeeeeeee..." in the leaked Debug. Assert absent.
    let root_hex = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    assert!(
        !out.contains(root_hex),
        "DEBUG LEAK in PrivateWitness: root_secret hex `{root_hex}` found in Debug output.\n\
         Debug output:\n{out}"
    );
    assert!(
        out.contains("[REDACTED 32 bytes]"),
        "PrivateWitness Debug must include '[REDACTED 32 bytes]'; got: {out}"
    );
    assert!(
        out.contains("[REDACTED 64 bytes]"),
        "PrivateWitness Debug must include '[REDACTED 64 bytes]' (holder_sig); got: {out}"
    );
}

#[test]
fn proof_bundle_debug_does_not_leak_stark_proof_bytes() {
    // Use a distinctive byte pattern (0x99) for the stark_proof that
    // does NOT appear in any other field — the test asserts the 0x99
    // pattern (hex "99...99") is absent from Debug output. The casm_hash
    // and public_inputs fields use a separate pattern (0xAB) so that
    // even if stark_proof leaked, the test would not pass on casm_hash
    // alone.
    let bundle = ProofBundle {
        stark_proof: vec![0x99; 1024],
        public_inputs: octo_wallet::capability::zk_mint::PublicInputs {
            ask_id: [0xAB; 32],
            axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
            cap_root_hash: ROOT_SECRET_MARKER,
            invocation_hash: [0xAB; 32],
            holder_did: sample_did(192).clone(),
            current_unix_time: 1_700_000_000,
            output_hash: None,
            provider_slot_id: "openai-prod".to_owned(),
        },
        casm_hash: [0xAB; 32],
        casm_version: 1,
        security_bits: 128,
        witness_format: zk_vendor::prover_input::WitnessFormat::BytesFallback,
    };
    let out = format!("{bundle:?}");
    // stark_proof was 1024 bytes of 0x99 — would appear as
    // "9999...99" if leaked. Assert absent.
    let proof_marker = "9999999999999999999999999999999999999999999999999999999999999999";
    assert!(
        !out.contains(proof_marker),
        "DEBUG LEAK in ProofBundle: stark_proof hex `{proof_marker}` found in Debug output.\n\
         Debug output:\n{out}"
    );
    // Should include the size + casm_hash hex + casm_version.
    assert!(
        out.contains("stark_proof_size_bytes"),
        "ProofBundle Debug must include stark_proof_size_bytes; got: {out}"
    );
    // Class enum should still Debug fine.
    let x = CapabilityClass::V1;
    let _ = format!("{x:?}");
}

#[test]
fn key_share_debug_does_not_leak_payload() {
    let share = KeyShare {
        x: 1,
        y: SHARE_MARKER,
    };
    let out = format!("{share:?}");
    assert_no_marker_77("KeyShare", &out);
    assert!(
        out.contains("[REDACTED 32 bytes]"),
        "KeyShare Debug must include '[REDACTED 32 bytes]'; got: {out}"
    );
}

#[test]
fn keystore_file_debug_redacts_crypto_envelope() {
    // Build a KeystoreFile whose `ciphertext`/`mac`/`salt`/`nonce`
    // contain a known marker pattern. We can only set the fields
    // directly here since StarkliCompat::export uses random salts +
    // nonces. The Debug redaction applies regardless of how the file
    // was constructed.
    let mut ciphertext_marker = String::new();
    for _ in 0..64 {
        ciphertext_marker.push_str("ab");
    }
    let file = KeystoreFile {
        version: 1,
        crypto: Crypto {
            cipher: "chacha20-poly1305".to_owned(),
            ciphertext: ciphertext_marker.clone(),
            cipherparams: CipherParams {
                nonce: ciphertext_marker.clone(),
            },
            kdf: "argon2id".to_owned(),
            kdfparams: KdfParams {
                salt: ciphertext_marker.clone(),
                time_cost: 3,
                memory_cost: 65536,
                parallelism: 4,
                output_len: 32,
            },
            mac: ciphertext_marker.clone(),
        },
        public_key: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".to_owned(),
        cipher_name: None,
    };
    let out = format!("{file:?}");
    // The marker hex is 128 chars of "ab" repeated — must NOT appear.
    let marker = "ababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababab";
    assert!(
        !out.contains(marker),
        "DEBUG LEAK in KeystoreFile: ciphertext/salt/nonce/mac hex marker found in Debug output.\n\
         Debug output:\n{out}"
    );
    assert!(
        out.contains("[REDACTED — encrypted seed blob + MAC + KDF params]"),
        "KeystoreFile Debug must include redaction marker; got: {out}"
    );
}

#[test]
fn vault_file_debug_redacts_envelope() {
    let file = VaultFile {
        version: 1,
        salt: "abababababababababababababababababababababababababab".to_owned(),
        nonce: "ababababababababababababab".to_owned(),
        ciphertext: vec![0xAB; 64],
    };
    let out = format!("{file:?}");
    let marker = "ababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababab";
    assert!(
        !out.contains(marker),
        "DEBUG LEAK in VaultFile: envelope marker found in Debug output.\n\
         Debug output:\n{out}"
    );
    assert!(
        out.contains("ciphertext_size_bytes"),
        "VaultFile Debug must include ciphertext_size_bytes; got: {out}"
    );
}

#[test]
fn debug_works_end_to_end_with_no_panic() {
    // Smoke: build a full CapabilityToken (the most security-sensitive
    // struct in the wallet) and assert Debug formatting doesn't panic.
    // This catches any field-ordering / generic-bound errors in the
    // manual Debug impl.
    let holder = IdentityKey::from_seed(SEED_MARKER);
    let caveats = [
        Caveat::Model("gpt-4".to_owned()),
        Caveat::Before(1_700_000_000),
    ];
    let token =
        CapabilityToken::mint(&ROOT_SECRET_MARKER, &holder, &sample_did(3), &caveats).unwrap();
    let _ = format!("{token:?}");
    let _ = format!("{token:#?}");

    // Exercise an attenuated token too.
    let catalog = TestCatalog::default();
    let next = token
        .attenuate_with_signer(Caveat::Model("gpt-3.5-turbo".to_owned()), &holder, &catalog)
        .unwrap();
    let _ = format!("{next:?}");
}
