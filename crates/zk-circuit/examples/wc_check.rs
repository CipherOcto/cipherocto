//! Measurement / vector-provenance tool. Hex vectors read better
//! without digit separators, and the spec-mandated short names (a, b,
//! c, d, v, m) are load-bearing when reading against the BLAKE3 spec.

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

fn main() {
    let b = zk_circuit::bundled_casm_bytes().expect("compile");
    println!(
        "bundled_casm_bytes = {} bytes, words = {}",
        b.len(),
        b.len() / 32
    );
    let e = zk_circuit::bundled_executable_json().expect("exe");
    let v: serde_json::Value = serde_json::from_str(&e).expect("json");
    println!(
        "executable bytecode words = {}",
        v["program"]["bytecode"].as_array().unwrap().len()
    );
    println!(
        "hints = {}",
        v["program"]["hints"].as_array().unwrap().len()
    );
}
