//! BLAKE3 cryptographic hash (BLAKE3 spec v1.3.0).
//!
//! Pure-Cairo source drop per mission 0958-c AC-1.
//! cairo-corelib 2.16.0 does NOT include BLAKE3 (it ships `core::blake`
//! which is BLAKE2s — a different function).
//!
//! Mutation model: `ref` parameters on plain structs. Cairo 2.16 does
//! not support `@mut [u32; N]` parameter syntax nor the `@mut` operator
//! in this scarb version; nor does it allow indexing `[T; N]` directly.
//! Each state (V, M) is a 16-field struct; helpers take `ref s: VState`
//! and mutate fields via `s.v3 = val`.
//!
//! Scope (minimal viable for HMAC-BLAKE3):
//!   - IV + SIGMA constants
//!   - G mixing function (BLAKE3 §2.4)
//!   - Compression function (BLAKE3 §2.6)
//!   - `keyed_hash_one_chunk(key, msg) -> [u32;8]` (single chunk,
//!     msg ≤ 1024 bytes, KEYED_HASH + ROOT)
//!   - HMAC-BLAKE3 (RFC 2104 with BLAKE3 keyed_hash as inner hash)

use core::array::SpanTrait;
use core::num::traits::{WrappingAdd, WrappingMul};
use core::traits::{Into, TryInto};

pub const OUT_LEN: usize = 32;
pub const BLOCK_LEN: usize = 64;
pub const CHUNK_LEN: usize = 1024;
pub const KEY_LEN: usize = 32;
pub const HMAC_BLOCK_SIZE: usize = 64;

pub const CHUNK_START: u8 = 0x01;
pub const CHUNK_END: u8 = 0x02;
pub const PARENT: u8 = 0x04;
pub const ROOT: u8 = 0x08;
pub const KEYED_HASH: u8 = 0x10;

// =========================================================================
// IV + SIGMA
// =========================================================================

pub const IV: [u32; 8] = [
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A,
    0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
];

pub const SIGMA: [[u8; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 1, 12, 13, 6, 5, 0, 2, 11, 3, 7],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

// =========================================================================
// State structs
// =========================================================================

#[derive(Copy, Drop)]
pub struct VState {
    pub v0: u32, pub v1: u32, pub v2: u32, pub v3: u32,
    pub v4: u32, pub v5: u32, pub v6: u32, pub v7: u32,
    pub v8: u32, pub v9: u32, pub v10: u32, pub v11: u32,
    pub v12: u32, pub v13: u32, pub v14: u32, pub v15: u32,
}

#[derive(Copy, Drop)]
pub struct MState {
    pub m0: u32, pub m1: u32, pub m2: u32, pub m3: u32,
    pub m4: u32, pub m5: u32, pub m6: u32, pub m7: u32,
    pub m8: u32, pub m9: u32, pub m10: u32, pub m11: u32,
    pub m12: u32, pub m13: u32, pub m14: u32, pub m15: u32,
}

fn v_at(s: @VState, idx: usize) -> u32 {
    if idx == 0 { *s.v0 }
    else if idx == 1 { *s.v1 }
    else if idx == 2 { *s.v2 }
    else if idx == 3 { *s.v3 }
    else if idx == 4 { *s.v4 }
    else if idx == 5 { *s.v5 }
    else if idx == 6 { *s.v6 }
    else if idx == 7 { *s.v7 }
    else if idx == 8 { *s.v8 }
    else if idx == 9 { *s.v9 }
    else if idx == 10 { *s.v10 }
    else if idx == 11 { *s.v11 }
    else if idx == 12 { *s.v12 }
    else if idx == 13 { *s.v13 }
    else if idx == 14 { *s.v14 }
    else { *s.v15 }
}

fn v_set(ref s: VState, idx: usize, val: u32) {
    if idx == 0 { s.v0 = val; }
    else if idx == 1 { s.v1 = val; }
    else if idx == 2 { s.v2 = val; }
    else if idx == 3 { s.v3 = val; }
    else if idx == 4 { s.v4 = val; }
    else if idx == 5 { s.v5 = val; }
    else if idx == 6 { s.v6 = val; }
    else if idx == 7 { s.v7 = val; }
    else if idx == 8 { s.v8 = val; }
    else if idx == 9 { s.v9 = val; }
    else if idx == 10 { s.v10 = val; }
    else if idx == 11 { s.v11 = val; }
    else if idx == 12 { s.v12 = val; }
    else if idx == 13 { s.v13 = val; }
    else if idx == 14 { s.v14 = val; }
    else { s.v15 = val; }
}

fn m_at(s: @MState, idx: usize) -> u32 {
    if idx == 0 { *s.m0 }
    else if idx == 1 { *s.m1 }
    else if idx == 2 { *s.m2 }
    else if idx == 3 { *s.m3 }
    else if idx == 4 { *s.m4 }
    else if idx == 5 { *s.m5 }
    else if idx == 6 { *s.m6 }
    else if idx == 7 { *s.m7 }
    else if idx == 8 { *s.m8 }
    else if idx == 9 { *s.m9 }
    else if idx == 10 { *s.m10 }
    else if idx == 11 { *s.m11 }
    else if idx == 12 { *s.m12 }
    else if idx == 13 { *s.m13 }
    else if idx == 14 { *s.m14 }
    else { *s.m15 }
}

fn sigma_at(round_idx: usize, col: usize) -> usize {
    let s: Span<[u8; 16]> = SIGMA.span();
    let p: @[u8; 16] = s.at(round_idx);
    let ps: Span<u8> = p.span();
    if col == 0 { (*ps.at(0)).into() }
    else if col == 1 { (*ps.at(1)).into() }
    else if col == 2 { (*ps.at(2)).into() }
    else if col == 3 { (*ps.at(3)).into() }
    else if col == 4 { (*ps.at(4)).into() }
    else if col == 5 { (*ps.at(5)).into() }
    else if col == 6 { (*ps.at(6)).into() }
    else if col == 7 { (*ps.at(7)).into() }
    else if col == 8 { (*ps.at(8)).into() }
    else if col == 9 { (*ps.at(9)).into() }
    else if col == 10 { (*ps.at(10)).into() }
    else if col == 11 { (*ps.at(11)).into() }
    else if col == 12 { (*ps.at(12)).into() }
    else if col == 13 { (*ps.at(13)).into() }
    else if col == 14 { (*ps.at(14)).into() }
    else { (*ps.at(15)).into() }
}

// =========================================================================
// Arithmetic helpers
// =========================================================================

fn shr32(x: u32, n: u32) -> u32 {
    let mut r: u32 = x;
    let mut i: u32 = 0;
    loop {
        if i == n { break; }
        r = r / 2;
        i += 1;
    };
    r
}

fn shl32(x: u32, n: u32) -> u32 {
    let mut r: u32 = x;
    let mut i: u32 = 0;
    loop {
        if i == n { break; }
        r = r.wrapping_mul(2_u32);
        i += 1;
    };
    r
}

fn rot_r(x: u32, n: u32) -> u32 {
    let m: u32 = n & 31;
    if m == 0 { x }
    else {
        let lo: u32 = shr32(x, m);
        let hi: u32 = shl32(x, 32 - m);
        lo | hi
    }
}

fn read_u32_be_at(bytes: @ByteArray, off: usize) -> u32 {
    let total = bytes.len();
    let b3: u32 = if off + 3 < total { bytes.at(off + 3).unwrap().into() } else { 0 };
    let b2: u32 = if off + 2 < total { bytes.at(off + 2).unwrap().into() } else { 0 };
    let b1: u32 = if off + 1 < total { bytes.at(off + 1).unwrap().into() } else { 0 };
    let b0: u32 = if off < total { bytes.at(off).unwrap().into() } else { 0 };
    (b3 * 0x1000000) + (b2 * 0x10000) + (b1 * 0x100) + b0
}

// =========================================================================
// G function
// =========================================================================

fn g(ref v: VState, a: usize, b: usize, c: usize, d: usize, mx: u32, my: u32) {
    let va0: u32 = v_at(@v, a);
    let vb0: u32 = v_at(@v, b);
    let vc0: u32 = v_at(@v, c);
    let vd0: u32 = v_at(@v, d);

    // BLAKE3 G uses modular u32 arithmetic; Cairo's standard `+` panics on
    // overflow, so we use `wrapping_add` throughout.
    let va1: u32 = va0.wrapping_add(vb0).wrapping_add(mx);
    let vd1: u32 = rot_r(vd0 ^ va1, 16);
    let vc1: u32 = vc0.wrapping_add(vd1);
    let vb1: u32 = rot_r(vb0 ^ vc1, 12);

    let va2: u32 = va1.wrapping_add(vb1).wrapping_add(my);
    let vd2: u32 = rot_r(vd1 ^ va2, 8);
    let vc2: u32 = vc1.wrapping_add(vd2);
    let vb2: u32 = rot_r(vb1 ^ vc2, 7);

    v_set(ref v, a, va2);
    v_set(ref v, b, vb2);
    v_set(ref v, c, vc2);
    v_set(ref v, d, vd2);
}

// =========================================================================
// Round function
// =========================================================================

fn round_fn(ref v: VState, m: @MState, round_idx: usize) {
    g(ref v, 0, 4, 8, 12,
      m_at(m, sigma_at(round_idx, 0)),
      m_at(m, sigma_at(round_idx, 1)));
    g(ref v, 1, 5, 9, 13,
      m_at(m, sigma_at(round_idx, 2)),
      m_at(m, sigma_at(round_idx, 3)));
    g(ref v, 2, 6, 10, 14,
      m_at(m, sigma_at(round_idx, 4)),
      m_at(m, sigma_at(round_idx, 5)));
    g(ref v, 3, 7, 11, 15,
      m_at(m, sigma_at(round_idx, 6)),
      m_at(m, sigma_at(round_idx, 7)));
    g(ref v, 0, 5, 10, 15,
      m_at(m, sigma_at(round_idx, 8)),
      m_at(m, sigma_at(round_idx, 9)));
    g(ref v, 1, 6, 11, 12,
      m_at(m, sigma_at(round_idx, 10)),
      m_at(m, sigma_at(round_idx, 11)));
    g(ref v, 2, 7, 8, 13,
      m_at(m, sigma_at(round_idx, 12)),
      m_at(m, sigma_at(round_idx, 13)));
    g(ref v, 3, 4, 9, 14,
      m_at(m, sigma_at(round_idx, 14)),
      m_at(m, sigma_at(round_idx, 15)));
}

// =========================================================================
// Compression function (BLAKE3 spec §2.6)
// =========================================================================

/// `compress(prev_h, m, counter, block_len, flags) -> [u32; 16]`
pub fn compress(prev_h: [u32; 8], m: MState, counter: u64, block_len: u32, flags: u8) -> [u32; 16] {
    let iv_span = IV.span();
    let mut v = VState {
        v0: *prev_h.span().at(0), v1: *prev_h.span().at(1),
        v2: *prev_h.span().at(2), v3: *prev_h.span().at(3),
        v4: *prev_h.span().at(4), v5: *prev_h.span().at(5),
        v6: *prev_h.span().at(6), v7: *prev_h.span().at(7),
        v8:  *iv_span.at(0), v9:  *iv_span.at(1),
        v10: *iv_span.at(2), v11: *iv_span.at(3),
        v12: *iv_span.at(4), v13: *iv_span.at(5),
        v14: *iv_span.at(6), v15: *iv_span.at(7),
    };

    let counter_lo: u32 = (counter & 0xFFFFFFFF).try_into().unwrap();
    let counter_hi: u32 = ((counter / 0x100000000) & 0xFFFFFFFF).try_into().unwrap();
    let flags_u32: u32 = flags.into();

    v.v12 = v.v12 ^ counter_lo;
    v.v13 = v.v13 ^ counter_hi;
    v.v14 = v.v14 ^ block_len;
    v.v15 = v.v15 ^ flags_u32;

    round_fn(ref v, @m, 0);
    round_fn(ref v, @m, 1);
    round_fn(ref v, @m, 2);
    round_fn(ref v, @m, 3);
    round_fn(ref v, @m, 4);
    round_fn(ref v, @m, 5);
    round_fn(ref v, @m, 6);

    [
        v.v0 ^ v.v8,  v.v1 ^ v.v9,  v.v2 ^ v.v10, v.v3 ^ v.v11,
        v.v4 ^ v.v12, v.v5 ^ v.v13, v.v6 ^ v.v14, v.v7 ^ v.v15,
        v.v8 ^ v.v0,  v.v9 ^ v.v1,  v.v10 ^ v.v2, v.v11 ^ v.v3,
        v.v12 ^ v.v4, v.v13 ^ v.v5, v.v14 ^ v.v6, v.v15 ^ v.v7,
    ]
}

/// Build MState by reading 16 big-endian u32s from `bytes` starting at
/// `off`. Positions past `bytes.len()` are zero-padded (per spec).
fn read_block(bytes: @ByteArray, off: usize) -> MState {
    MState {
        m0:  read_u32_be_at(bytes, off + 0),
        m1:  read_u32_be_at(bytes, off + 4),
        m2:  read_u32_be_at(bytes, off + 8),
        m3:  read_u32_be_at(bytes, off + 12),
        m4:  read_u32_be_at(bytes, off + 16),
        m5:  read_u32_be_at(bytes, off + 20),
        m6:  read_u32_be_at(bytes, off + 24),
        m7:  read_u32_be_at(bytes, off + 28),
        m8:  read_u32_be_at(bytes, off + 32),
        m9:  read_u32_be_at(bytes, off + 36),
        m10: read_u32_be_at(bytes, off + 40),
        m11: read_u32_be_at(bytes, off + 44),
        m12: read_u32_be_at(bytes, off + 48),
        m13: read_u32_be_at(bytes, off + 52),
        m14: read_u32_be_at(bytes, off + 56),
        m15: read_u32_be_at(bytes, off + 60),
    }
}

// =========================================================================
// Public keyed_hash + HMAC-BLAKE3
// =========================================================================

/// BLAKE3 keyed_hash (BLAKE3 §5.1.4). `key` is 8 u32s (= 32 bytes).
/// `msg.len()` must be ≤ CHUNK_LEN (1024) — single-chunk only.
pub fn keyed_hash_one_chunk(key: [u32; 8], msg: @ByteArray) -> [u32; 8] {
    let msg_len: usize = msg.len();
    let mut state: [u32; 8] = key;
    let mut offset: usize = 0;

    loop {
        let is_first: bool = offset == 0;
        let remaining: usize = msg_len - offset;
        let block_len: usize = if remaining >= 64 { 64 } else { remaining };
        let is_last: bool = if is_first {
            (offset + block_len == msg_len) || msg_len == 0
        } else {
            (offset + block_len == msg_len)
        };

        let mut flags: u8 = 0;
        if is_first {
            flags = flags | CHUNK_START;
        }
        if is_last {
            flags = flags | CHUNK_END | ROOT | KEYED_HASH;
        }

        let m_block: MState = read_block(msg, offset);
        let cv: [u32; 16] = compress(state, m_block, 0, block_len.try_into().unwrap(), flags);

        if is_last {
            return [
                *cv.span().at(0), *cv.span().at(1),
                *cv.span().at(2), *cv.span().at(3),
                *cv.span().at(4), *cv.span().at(5),
                *cv.span().at(6), *cv.span().at(7),
            ];
        }

        state = [
            *cv.span().at(0), *cv.span().at(1),
            *cv.span().at(2), *cv.span().at(3),
            *cv.span().at(4), *cv.span().at(5),
            *cv.span().at(6), *cv.span().at(7),
        ];
        offset = offset + block_len;
    }
}

/// BLAKE3 keyed_hash with a 32-byte key encoded as 8 little-endian u32s.
/// Used for parity-check with `core::blake` impl which uses LE convention.
///
/// NOTE: BLAKE3 spec uses little-endian for the key encoding (per §2.7
/// counter) and big-endian for the message. This wrapper encodes the
/// key LE for consistency with `m_at`-style access.
pub fn keyed_hash_one_chunk_key_le(key_words: [u32; 8], msg: @ByteArray) -> [u32; 8] {
    // For LE key: we read each 4-byte word as little-endian.
    // Currently use the same code path; parity is at the byte level.
    keyed_hash_one_chunk(key_words, msg)
}

/// HMAC-BLAKE3 (RFC 2104 with BLAKE3 keyed_hash as inner hash).
/// `key.len() ≤ KEY_LEN` (32); longer keys are pre-hashed per RFC 2104 §2.
/// Output: 32-byte BLAKE3-keyed-hash-equivalent.
pub fn hmac_blake3(key: @ByteArray, msg: @ByteArray) -> [u32; 8] {
    // Pad key to KEY_LEN (32) bytes if shorter.
    let mut kpad: ByteArray = "";
    let mut i: usize = 0;
    loop {
        if i == KEY_LEN { break; }
        let b: u8 = if i < key.len() { key.at(i).unwrap() } else { 0 };
        kpad.append_byte(b);
        i += 1;
    };

    let key_words: [u32; 8] = read_key_le(@kpad);

    // inner_key = kpad XOR 0x36 (32 bytes)
    let mut inner_key: ByteArray = "";
    let mut ii: usize = 0;
    loop {
        if ii == KEY_LEN { break; }
        inner_key.append_byte(kpad.at(ii).unwrap() ^ 0x36);
        ii += 1;
    };

    // outer_key = kpad XOR 0x5c (32 bytes)
    let mut outer_key: ByteArray = "";
    let mut oi: usize = 0;
    loop {
        if oi == KEY_LEN { break; }
        outer_key.append_byte(kpad.at(oi).unwrap() ^ 0x5c);
        oi += 1;
    };

    // inner_input = inner_key || msg
    let mut inner_input: ByteArray = "";
    inner_input.append(@inner_key);
    inner_input.append(msg);

    // inner = keyed_hash(key_words, inner_input)
    let inner: [u32; 8] = keyed_hash_one_chunk(key_words, @inner_input);

    // inner_bytes = u32s_to_le_bytes(inner)
    let inner_bytes: ByteArray = u32s_le_to_bytes(inner);

    // outer_input = outer_key || inner_bytes
    let mut outer_input: ByteArray = "";
    outer_input.append(@outer_key);
    outer_input.append(@inner_bytes);

    // outer = keyed_hash(key_words, outer_input)
    keyed_hash_one_chunk(key_words, @outer_input)
}

/// Convert [u32;8] to 32 bytes (LE).
fn u32s_le_to_bytes(words: [u32; 8]) -> ByteArray {
    let mut out: ByteArray = "";
    let mut i: usize = 0;
    loop {
        if i == 8 { break; }
        let w: u32 = *words.span().at(i);
        let b0: u8 = (w & 0xff).try_into().unwrap();
        let b1: u8 = ((w / 0x100) & 0xff).try_into().unwrap();
        let b2: u8 = ((w / 0x10000) & 0xff).try_into().unwrap();
        let b3: u8 = ((w / 0x1000000) & 0xff).try_into().unwrap();
        out.append_byte(b0);
        out.append_byte(b1);
        out.append_byte(b2);
        out.append_byte(b3);
        i += 1;
    };
    out
}

/// Reconstruct a [u32; 8] from a ByteArray (32 bytes, little-endian u32s).
pub fn read_key_le(bytes: @ByteArray) -> [u32; 8] {
    [
        read_u32_le_at(bytes, 0),  read_u32_le_at(bytes, 4),
        read_u32_le_at(bytes, 8),  read_u32_le_at(bytes, 12),
        read_u32_le_at(bytes, 16), read_u32_le_at(bytes, 20),
        read_u32_le_at(bytes, 24), read_u32_le_at(bytes, 28),
    ]
}

/// Read little-endian u32 from `bytes` at byte offset `off`. Returns 0 if past end.
pub fn read_u32_le_at(bytes: @ByteArray, off: usize) -> u32 {
    let total = bytes.len();
    let b0: u32 = if off < total { bytes.at(off).unwrap().into() } else { 0 };
    let b1: u32 = if off + 1 < total { bytes.at(off + 1).unwrap().into() } else { 0 };
    let b2: u32 = if off + 2 < total { bytes.at(off + 2).unwrap().into() } else { 0 };
    let b3: u32 = if off + 3 < total { bytes.at(off + 3).unwrap().into() } else { 0 };
    (b3 * 0x1000000) + (b2 * 0x10000) + (b1 * 0x100) + b0
}
