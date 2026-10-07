//! Measurement / vector-provenance tool. Hex vectors read better
//! without digit separators, and the spec-mandated short names (a, b,
//! c, d, v, m) are load-bearing when reading against the BLAKE3 spec.
//! Reference values for isolating the Cairo BLAKE3 compression function.

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
    // rot_r reference for the four BLAKE3 rotate amounts.
    for n in [16u32, 12, 8, 7] {
        let x: u32 = 0xB7E1_5162;
        let r = (x >> n) | (x << (32 - n));
        println!("rot_r({x:#010x}, {n}) = {r:#010x}");
    }

    // One G() invocation on the canonical initial state, round 0, column 0:
    //   a=0 b=4 c=8 d=12, m = m[0], m[1]
    // This is the first operation of the first round of BLAKE3(""), so any
    // deviation here is a deviation in G, the state layout, or SIGMA.
    let iv: [u32; 8] = [
        0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB,
        0x5BE0CD19,
    ];
    let mut v = [0u32; 16];
    v[0..8].copy_from_slice(&iv);
    // state[8..16] = IV[0..4], counter_low, counter_high, block_len, flags
    v[8..12].copy_from_slice(&iv[0..4]);
    v[12] = 0; // counter low
    v[13] = 0; // counter high
    v[14] = 0; // block_len
    v[15] = 0x0b; // CHUNK_START|CHUNK_END|ROOT

    let rot = |x: u32, n: u32| (x >> n) | (x << (32 - n));
    // message is the empty string -> all words zero
    let mx = 0u32;
    let my = 0u32;
    let (a, b, c, d) = (0usize, 4usize, 8usize, 12usize);

    let va1 = v[a].wrapping_add(v[b]).wrapping_add(mx);
    let vd1 = rot(v[d] ^ va1, 16);
    let vc1 = v[c].wrapping_add(vd1);
    let vb1 = rot(v[b] ^ vc1, 12);
    let va2 = va1.wrapping_add(vb1).wrapping_add(my);
    let vd2 = rot(vd1 ^ va2, 8);
    let vc2 = vc1.wrapping_add(vd2);
    let vb2 = rot(vb1 ^ vc2, 7);

    println!("G(0,4,8,12,0,0):");
    println!("  v0  = {:#010x}", va2);
    println!("  v4  = {:#010x}", vb2);
    println!("  v8  = {:#010x}", vc2);
    println!("  v12 = {:#010x}", vd2);
    println!();
    println!("state after 7 rounds:");
    full_rounds();
}
/// Full 7-round state for BLAKE3(""), printed so the Cairo round schedule can
/// be compared operation by operation.
#[allow(dead_code)]
fn full_rounds() {
    const SIGMA: [[usize; 16]; 7] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8],
        [3, 4, 10, 12, 13, 2, 7, 14, 6, 5, 9, 0, 11, 15, 8, 1],
        [10, 7, 12, 9, 14, 3, 13, 15, 4, 0, 11, 2, 5, 8, 1, 6],
        [12, 13, 9, 11, 15, 10, 14, 8, 7, 2, 5, 3, 0, 1, 6, 4],
        [9, 14, 11, 5, 8, 12, 15, 1, 13, 3, 0, 10, 2, 6, 4, 7],
        [11, 15, 5, 0, 1, 9, 8, 6, 14, 10, 2, 12, 3, 4, 7, 13],
    ];
    let iv: [u32; 8] = [
        0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB,
        0x5BE0CD19,
    ];
    let mut v = [0u32; 16];
    v[0..8].copy_from_slice(&iv);
    v[8..12].copy_from_slice(&iv[0..4]);
    v[12] = 0;
    v[13] = 0;
    v[14] = 0;
    v[15] = 0x0b;
    let rot = |x: u32, n: u32| (x >> n) | (x << (32 - n));
    let m = [0u32; 16];
    for r in 0..7 {
        for col in 0..8 {
            // BLAKE3's diagonals are NOT col+{4,8,12}; they are fixed pairs.
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
            let (a, b, c, d) = DIAG[col];
            let mx = m[SIGMA[r][2 * col]];
            let my = m[SIGMA[r][2 * col + 1]];
            let va1 = v[a].wrapping_add(v[b]).wrapping_add(mx);
            let vd1 = rot(v[d] ^ va1, 16);
            let vc1 = v[c].wrapping_add(vd1);
            let vb1 = rot(v[b] ^ vc1, 12);
            let va2 = va1.wrapping_add(vb1).wrapping_add(my);
            let vd2 = rot(vd1 ^ va2, 8);
            let vc2 = vc1.wrapping_add(vd2);
            let vb2 = rot(vb1 ^ vc2, 7);
            v[a] = va2;
            v[b] = vb2;
            v[c] = vc2;
            v[d] = vd2;
        }
    }
    for (i, w) in v.iter().enumerate() {
        println!("  v{i:<2} = {w:#010x}");
    }
    // Root output = state[i] ^ state[i+8].
    let mut out = [0u8; 32];
    for i in 0..8 {
        out[i * 4..i * 4 + 4].copy_from_slice(&(v[i] ^ v[i + 8]).to_le_bytes());
    }
    println!(
        "  root = {}",
        out.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
    println!("  crate blake3(\"\") = {}", blake3::hash(b"").to_hex());
}
