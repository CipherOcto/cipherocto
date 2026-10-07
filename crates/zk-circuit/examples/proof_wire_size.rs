//! Measurement / vector-provenance tool. Hex vectors read better
//! without digit separators, and the spec-mandated short names (a, b,
//! c, d, v, m) are load-bearing when reading against the BLAKE3 spec.
//! How large does a real STARK proof get in the capability wire format?
//!
//! `ProofBundle` derives `Serialize` with `stark_proof: Vec<u8>`, and serde_json
//! encodes `Vec<u8>` as a JSON *array of numbers* — not base64. This measures
//! the resulting size against the v2 wire limits
//! (`MAX_SEGMENT_BYTES` = 1 MiB, `MAX_WIRE_TOTAL` = 2 MiB).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::explicit_into_iter_loop,
    clippy::explicit_iter_loop,
    clippy::format_collect,
    clippy::manual_rotate,
    clippy::many_single_char_names,
    clippy::needless_range_loop,
    clippy::uninlined_format_args,
    clippy::unreadable_literal,
    clippy::useless_vec
)]

const PROOF_BYTES: usize = 6_289_495; // the proof stwo_verify accepted

fn main() {
    let stark_proof: Vec<u8> = vec![0xABu8; PROOF_BYTES];
    let bundle = serde_json::json!({
        "stark_proof": stark_proof,
        "casm_hash": vec![0u8; 32],
        "public_inputs": { "ask_id": vec![0u8; 32] },
    });
    let json = serde_json::to_vec(&bundle).expect("serialize");
    let b64_len = json.len().div_ceil(3) * 4;

    println!("proof bytes (raw)        : {PROOF_BYTES}");
    println!("proof_bundle JSON bytes  : {}", json.len());
    println!("base64url of that JSON   : {b64_len}");
    println!("MAX_SEGMENT_BYTES        : {}", 1024 * 1024);
    println!(
        "over per-segment cap by  : {:.1}x",
        b64_len as f64 / (1024.0 * 1024.0)
    );
    println!(
        "over MAX_WIRE_TOTAL by   : {:.1}x",
        b64_len as f64 / (2.0 * 1024.0 * 1024.0)
    );
}
