//! `whatsapp_modern_client_hello` — Phase 7.J.6 (S6)
//!
//! Builds WA Web's modern `ClientHello` proto shape and prints the encoded
//! bytes. Does NOT open a WS connection — the goal is to verify the
//! protobuf structure matches the +102B gap from Chrome 150.
//!
//! WA modern ClientHello (Chrome 150, frame[2] = 363B):
//!     ephemeral          (32B)   tag=1
//!     r#static           (48B)   tag=2  AES-GCM(identity pub)
//!     payload            (~145B) tag=3  AES-GCM(signed cert)
//!     useExtended        (bool)  tag=4
//!     extendedCiphertext (~80B)  tag=5  AES-GCM(?, ECDH(ext_e, server_s))
//!     pqMode             (enum)  tag=9  WA_PQ=4
//!     extendedEphemeral  (32B)   tag=10
//!
//! wacore plain XX (frame[2] = ~261B):
//!     ephemeral          (32B)   only
//!
//! Run:
//!     cargo run -p octo-adapter-whatsapp --bin whatsapp_modern_client_hello --release
//!
//! Output: encoded proto bytes + per-field length breakdown + comparison vs
//! the expected Chrome 363B envelope.

use anyhow::Result;
use rand::{Rng, RngCore};

const CHROME_EXPECTED_FRAME2_LEN: usize = 363;
const WACORE_PLAIN_XX_FRAME2_LEN: usize = 261;

fn main() -> Result<()> {
    let mut rng = rand::thread_rng();

    // ---- Inputs (placeholders — real values from wacore IK + signed cert) ----
    let ephemeral_pub: [u8; 32] = random_bytes(&mut rng);
    let encrypted_static: [u8; 48] = random_bytes_arr(&mut rng); // 32B + 16B AES-GCM tag
    let encrypted_payload: Vec<u8> = (0..145).map(|_| rng.gen::<u8>()).collect();
    let extended_ciphertext: Vec<u8> = (0..80).map(|_| rng.gen::<u8>()).collect();
    let extended_ephemeral_pub: [u8; 32] = random_bytes(&mut rng);

    let pq_mode: i32 = 4; // WA_PQ

    // ---- Encode modern ClientHello (full proto) ----
    let proto_bytes = encode_client_hello(
        &ephemeral_pub,
        &encrypted_static,
        &encrypted_payload,
        true, // useExtended
        &extended_ciphertext,
        pq_mode,
        &extended_ephemeral_pub,
    );

    // ---- Encode wacore plain XX ClientHello (only ephemeral) ----
    let plain_xx_bytes = encode_client_hello(&ephemeral_pub, &[], &[], false, &[], 0, &[0u8; 32]);

    println!("== whatsapp_modern_client_hello ==");
    println!("modern ClientHello bytes : {}", proto_bytes.len());
    println!("  ephemeral              : 32B");
    println!("  encrypted static       : {}B ({}B cipher + 16B tag)", encrypted_static.len(), encrypted_static.len() - 16);
    println!("  encrypted payload      : {}B (signed cert)", encrypted_payload.len());
    println!("  useExtended            : true");
    println!("  extendedCiphertext     : {}B", extended_ciphertext.len());
    println!("  pqMode                 : WA_PQ ({pq_mode})");
    println!("  extendedEphemeral      : 32B");
    println!();
    println!("plain XX ClientHello     : {}B (only ephemeral)", plain_xx_bytes.len());
    println!();
    println!("Chrome 150 observed      : {CHROME_EXPECTED_FRAME2_LEN}B");
    // Suppress unused-const warnings
    let _ = WACORE_PLAIN_XX_FRAME2_LEN;
    let gap = if proto_bytes.len() > CHROME_EXPECTED_FRAME2_LEN {
        proto_bytes.len() - CHROME_EXPECTED_FRAME2_LEN
    } else {
        CHROME_EXPECTED_FRAME2_LEN - proto_bytes.len()
    };
    println!(
        "Gap to wacore            : proto={}B chrome_observed={}B diff={}B",
        proto_bytes.len(),
        CHROME_EXPECTED_FRAME2_LEN,
        gap
    );
    println!();
    println!("Encoded (modern) hex prefix : {}", hex::encode(&proto_bytes[..32.min(proto_bytes.len())]));
    println!("Encoded (plain XX) hex prefix: {}", hex::encode(&plain_xx_bytes));

    Ok(())
}

fn random_bytes(rng: &mut rand::rngs::ThreadRng) -> [u8; 32] {
    let mut b = [0u8; 32];
    rng.fill_bytes(&mut b);
    b
}

fn random_bytes_arr<const N: usize>(rng: &mut rand::rngs::ThreadRng) -> [u8; N] {
    let mut b = [0u8; N];
    rng.fill_bytes(&mut b);
    b
}

/// Encode a `HandshakeMessage::ClientHello` proto message as raw field-tag +
/// length + value bytes. Includes every optional field; fields with empty
/// data are skipped.
fn encode_client_hello(
    ephemeral: &[u8],
    encrypted_static: &[u8],
    payload: &[u8],
    use_extended: bool,
    extended_ct: &[u8],
    pq_mode: i32,
    extended_ephemeral_pub: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(363);
    // tag 1 (ephemeral), wire type 2
    if !ephemeral.is_empty() {
        out.push(0x0a);
        encode_varint(&mut out, ephemeral.len() as u64);
        out.extend_from_slice(ephemeral);
    }
    // tag 2 (r#static), wire type 2
    if !encrypted_static.is_empty() {
        out.push(0x12);
        encode_varint(&mut out, encrypted_static.len() as u64);
        out.extend_from_slice(encrypted_static);
    }
    // tag 3 (payload), wire type 2
    if !payload.is_empty() {
        out.push(0x1a);
        encode_varint(&mut out, payload.len() as u64);
        out.extend_from_slice(payload);
    }
    // tag 4 (useExtended), wire type 0
    if use_extended {
        out.push(0x20);
        out.push(0x01);
    }
    // tag 5 (extendedCiphertext), wire type 2
    if !extended_ct.is_empty() {
        out.push(0x2a);
        encode_varint(&mut out, extended_ct.len() as u64);
        out.extend_from_slice(extended_ct);
    }
    // tag 9 (pqMode), wire type 0
    if pq_mode != 0 {
        out.push(0x48);
        encode_varint(&mut out, pq_mode as u64);
    }
    // tag 10 (extendedEphemeral), wire type 2
    if !extended_ephemeral_pub.is_empty() && extended_ephemeral_pub != [0u8; 32] {
        out.push(0x52);
        encode_varint(&mut out, extended_ephemeral_pub.len() as u64);
        out.extend_from_slice(extended_ephemeral_pub);
    }
    out
}

fn encode_varint(out: &mut Vec<u8>, mut n: u64) {
    while n >= 0x80 {
        out.push((n as u8 & 0x7f) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}