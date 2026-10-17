//! Measurement / vector-provenance tool. Hex vectors read better
//! without digit separators, and the spec-mandated short names (a, b,
//! c, d, v, m) are load-bearing when reading against the BLAKE3 spec.
//! Determine the exact block/flag structure `blake3::keyed_hash` uses for a
//! message that spans more than one 64-byte block.

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

const IV: [u32; 8] = [
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
];

const SIGMA: [[usize; 16]; 7] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8],
    [3, 4, 10, 12, 13, 2, 7, 14, 6, 5, 9, 0, 11, 15, 8, 1],
    [10, 7, 12, 9, 14, 3, 13, 15, 4, 0, 11, 2, 5, 8, 1, 6],
    [12, 13, 9, 11, 15, 10, 14, 8, 7, 2, 5, 3, 0, 1, 6, 4],
    [9, 14, 11, 5, 8, 12, 15, 1, 13, 3, 0, 10, 2, 6, 4, 7],
    [11, 15, 5, 0, 1, 9, 8, 6, 14, 10, 2, 12, 3, 4, 7, 13],
];

const CHUNK_START: u32 = 1 << 0;
const CHUNK_END: u32 = 1 << 1;
const ROOT: u32 = 1 << 3;
const KEYED_HASH: u32 = 1 << 4;

const DIAG: [(usize, usize, usize, usize); 8] = [
    (0, 4, 8, 12),
    (1, 5, 9, 13),
    (2, 6, 10, 14),
    (3, 7, 11, 15),
    (0, 5, 10, 15),
    (1, 6, 11, 12),
    (2, 7, 8, 13),
    (3, 4, 9, 14),
];

#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
fn compress(
    cv: &[u32; 8],
    block: &[u8; 64],
    counter: u64,
    block_len: u32,
    flags: u32,
) -> [u32; 16] {
    let mut m = [0u32; 16];
    for (i, w) in m.iter_mut().enumerate() {
        let o = i * 4;
        *w = u32::from_le_bytes([block[o], block[o + 1], block[o + 2], block[o + 3]]);
    }
    let mut s = [0u32; 16];
    s[..8].copy_from_slice(cv);
    s[8..12].copy_from_slice(&IV[..4]);
    s[12] = counter as u32;
    s[13] = (counter >> 32) as u32;
    s[14] = block_len;
    s[15] = flags;

    for r in 0..7 {
        let sch = &SIGMA[r];
        for col in 0..8 {
            let (a, b, c, d) = DIAG[col];
            let (x, y) = (m[sch[2 * col]], m[sch[2 * col + 1]]);
            s[a] = s[a].wrapping_add(s[b]).wrapping_add(x);
            s[d] = (s[d] ^ s[a]).rotate_right(16);
            s[c] = s[c].wrapping_add(s[d]);
            s[b] = (s[b] ^ s[c]).rotate_right(12);
            s[a] = s[a].wrapping_add(s[b]).wrapping_add(y);
            s[d] = (s[d] ^ s[a]).rotate_right(8);
            s[c] = s[c].wrapping_add(s[d]);
            s[b] = (s[b] ^ s[c]).rotate_right(7);
        }
    }

    let mut out = [0u32; 16];
    for i in 0..8 {
        out[i] = s[i] ^ s[i + 8];
        out[i + 8] = s[i + 8] ^ cv[i];
    }
    out
}

/// Single-chunk keyed hash: every block shares counter 0; CHUNK_START on the
/// first, CHUNK_END|ROOT|KEYED_HASH on the last.
fn keyed_single_chunk(key: &[u8; 32], msg: &[u8]) -> [u8; 32] {
    let mut cv = [0u32; 8];
    for i in 0..8 {
        cv[i] = u32::from_le_bytes([key[i * 4], key[i * 4 + 1], key[i * 4 + 2], key[i * 4 + 3]]);
    }
    let mut offset = 0usize;
    loop {
        let remaining = msg.len() - offset;
        let block_len = remaining.min(64);
        let is_first = offset == 0;
        let is_last = offset + block_len == msg.len();

        let mut block = [0u8; 64];
        block[..block_len].copy_from_slice(&msg[offset..offset + block_len]);

        let mut flags = KEYED_HASH;
        if is_first {
            flags |= CHUNK_START;
        }
        if is_last {
            flags |= CHUNK_END | ROOT;
        }

        let out = compress(&cv, &block, 0, block_len as u32, flags);
        if std::env::var("TRACE_CV").is_ok() {
            println!("    block at offset {offset} block_len {block_len} flags {flags:#04x}");
            println!("      cv after = {:08x?}", &out[..8]);
        }
        cv.copy_from_slice(&out[..8]);
        if is_last {
            let mut res = [0u8; 32];
            for i in 0..8 {
                res[i * 4..i * 4 + 4].copy_from_slice(&out[i].to_le_bytes());
            }
            return res;
        }
        offset += block_len;
    }
}

fn main() {
    // Consecutive xorshift64* bytes from one evolving state — must match
    // `key_bytes()` in the blake3_vectors example and `test_key()` in Cairo.
    let mut key = [0u8; 32];
    let mut s: u64 = 0x1234_5678_9abc_def0 | 1;
    for b in key.iter_mut() {
        s ^= s >> 12;
        s ^= s << 25;
        s ^= s >> 27;
        *b = (s.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u8;
    }
    println!(
        "key = {}",
        key.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );

    // The macaroon-shaped vector from the Cairo test: key k2 and a
    // name || u32be(32) || 32*0x42 || 32*0x77 message.
    {
        let mut k2 = [0u8; 32];
        let mut s2: u64 = 0x0bad_c0de_dead_beef | 1;
        for b in k2.iter_mut() {
            s2 ^= s2 >> 12;
            s2 ^= s2 << 25;
            s2 ^= s2 >> 27;
            *b = (s2.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u8;
        }
        println!(
            "k2 = {}",
            k2.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
        let mut m = Vec::new();
        m.extend_from_slice(b"amount_max");
        m.extend_from_slice(&[0u8, 0, 0, 32]);
        m.extend_from_slice(&[0x42u8; 32]);
        m.extend_from_slice(&[0x77u8; 32]);
        if std::env::var("TRACE_CV").is_ok() {
            println!("  macaroon trace:");
        }
        let mine = keyed_single_chunk(&k2, &m);
        let theirs = blake3::keyed_hash(&k2, &m);
        println!(
            "macaroon_shaped len={} mine={} crate={} {}",
            m.len(),
            mine.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            theirs.to_hex(),
            if mine == *theirs.as_bytes() {
                "MATCH"
            } else {
                "DIFFER"
            }
        );
    }

    for len in [0usize, 1, 55, 64, 65, 78, 128, 129] {
        let msg = vec![0x11u8; len];
        let mine = keyed_single_chunk(&key, &msg);
        let theirs = blake3::keyed_hash(&key, &msg);
        let hx = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        println!(
            "len={len:<4} mine={} crate={} {}",
            hx(&mine),
            theirs.to_hex(),
            if mine == *theirs.as_bytes() {
                "MATCH"
            } else {
                "DIFFER"
            }
        );
    }
}
