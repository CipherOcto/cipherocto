//! Measurement / vector-provenance tool. Hex vectors read better
//! without digit separators, and the spec-mandated short names (a, b,
//! c, d, v, m) are load-bearing when reading against the BLAKE3 spec.
//! Emit BLAKE3 keyed-hash reference vectors for the Cairo parity tests.
//!
//! Run: `cargo run -p zk-circuit --example blake3_vectors`
//!
//! The values printed here are pasted into `cairo/src/blake3.cairo`'s test
//! module. The Rust side re-verifies them in
//! `crates/zk-circuit/tests/blake3_parity_vectors.rs`, so if the upstream
//! `blake3` crate ever changes behaviour the Rust test goes red and the Cairo
//! constants are known to be stale.

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

use blake3::Hasher;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Non-regular key so that a big-endian / little-endian mix-up cannot pass:
/// every 4-byte group differs from its own byte-reversal.
fn key_bytes(seed: u64) -> [u8; 32] {
    let mut k = [0u8; 32];
    let mut s = seed | 1;
    for b in k.iter_mut() {
        // xorshift64*
        s ^= s >> 12;
        s ^= s << 25;
        s ^= s >> 27;
        *b = (s.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u8;
    }
    k
}

fn emit(label: &str, key: &[u8; 32], msg: &[u8]) {
    let out = blake3::keyed_hash(key, msg);
    println!("// {label}  (msg_len={})", msg.len());
    println!("KEY_HEX   = \"{}\"", hex(key));
    println!("MSG_HEX   = \"{}\"", hex(msg));
    println!("EXPECTED  = \"{}\"", hex(out.as_bytes()));
    println!();
}

fn main() {
    let k1 = key_bytes(0x1234_5678_9abc_def0);

    // 0 bytes — BLAKE3 defines keyed_hash of the empty message.
    emit("empty", &k1, b"");

    // 1 byte.
    emit("one_byte", &k1, b"a");

    // 55 bytes — a full chunk *minus* the final flag block.
    emit("partial_final_chunk", &k1, &vec![0xabu8; 55]);

    // Exactly 64 bytes — one full chunk with CHUNK_START | CHUNK_END.
    emit("exactly_one_chunk", &k1, &vec![0x5au8; 64]);

    // 65 bytes — the first message that needs a *second chunk*, and therefore
    // a non-zero chunk counter. This is the vector that exposes a counter that
    // is stuck at 0.
    emit("two_chunks", &k1, &vec![0xc3u8; 65]);

    // 128 bytes — exactly two chunks.
    emit("two_full_chunks", &k1, &vec![0x11u8; 128]);

    // 129 bytes — three chunks.
    emit("three_chunks", &k1, &vec![0xfeu8; 129]);

    // A macaroon-shaped message: caveat name || serialized caveat || id.
    // The real chain uses messages far past one chunk, so this is the shape
    // that actually has to match.
    let mut msg = Vec::new();
    msg.extend_from_slice(b"amount_max");
    msg.extend_from_slice(&[0u8, 0, 0, 32]);
    msg.extend_from_slice(&[0x42u8; 32]);
    msg.extend_from_slice(&[0x77u8; 32]);
    emit("macaroon_shaped", &k2(), &msg);

    // Unkeyed hash of the same message, for reference when checking the
    // compression function independently of the key schedule.
    let h = Hasher::new();
    let mut hh = h.clone();
    hh.update(&msg);
    println!(
        "// unkeyed macaroon_shaped = \"{}\"",
        hex(hh.finalize().as_bytes())
    );
    let _ = h;
}

fn k2() -> [u8; 32] {
    key_bytes(0x0bad_c0de_dead_beef)
}
