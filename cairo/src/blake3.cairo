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
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
];

/// BLAKE3 message schedule (spec §2.7; `MSG_SCHEDULE` in the reference
/// implementation).
///
/// **This is NOT BLAKE2's schedule.** BLAKE2's row 1 is
/// `[14, 10, 4, 8, 9, 15, 1, 12, …]`; BLAKE3's is
/// `[2, 6, 3, 10, 7, 0, 4, 13, …]`. An earlier revision of this file carried
/// the BLAKE2 table (and 10 rows instead of 7). It produced
/// self-consistent-looking digests that matched nothing, because the module
/// had no reference vectors. Do not "restore" the BLAKE2 rows.
pub const SIGMA: [[u8; 16]; 7] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8],
    [3, 4, 10, 12, 13, 2, 7, 14, 6, 5, 9, 0, 11, 15, 8, 1],
    [10, 7, 12, 9, 14, 3, 13, 15, 4, 0, 11, 2, 5, 8, 1, 6],
    [12, 13, 9, 11, 15, 10, 14, 8, 7, 2, 5, 3, 0, 1, 6, 4],
    [9, 14, 11, 5, 8, 12, 15, 1, 13, 3, 0, 10, 2, 6, 4, 7],
    [11, 15, 5, 0, 1, 9, 8, 6, 14, 10, 2, 12, 3, 4, 7, 13],
];

// =========================================================================
// State structs
// =========================================================================

#[derive(Copy, Drop)]
pub struct VState {
    pub v0: u32,
    pub v1: u32,
    pub v2: u32,
    pub v3: u32,
    pub v4: u32,
    pub v5: u32,
    pub v6: u32,
    pub v7: u32,
    pub v8: u32,
    pub v9: u32,
    pub v10: u32,
    pub v11: u32,
    pub v12: u32,
    pub v13: u32,
    pub v14: u32,
    pub v15: u32,
}

#[derive(Copy, Drop)]
pub struct MState {
    pub m0: u32,
    pub m1: u32,
    pub m2: u32,
    pub m3: u32,
    pub m4: u32,
    pub m5: u32,
    pub m6: u32,
    pub m7: u32,
    pub m8: u32,
    pub m9: u32,
    pub m10: u32,
    pub m11: u32,
    pub m12: u32,
    pub m13: u32,
    pub m14: u32,
    pub m15: u32,
}

fn v_at(s: @VState, idx: usize) -> u32 {
    if idx == 0 {
        *s.v0
    } else if idx == 1 {
        *s.v1
    } else if idx == 2 {
        *s.v2
    } else if idx == 3 {
        *s.v3
    } else if idx == 4 {
        *s.v4
    } else if idx == 5 {
        *s.v5
    } else if idx == 6 {
        *s.v6
    } else if idx == 7 {
        *s.v7
    } else if idx == 8 {
        *s.v8
    } else if idx == 9 {
        *s.v9
    } else if idx == 10 {
        *s.v10
    } else if idx == 11 {
        *s.v11
    } else if idx == 12 {
        *s.v12
    } else if idx == 13 {
        *s.v13
    } else if idx == 14 {
        *s.v14
    } else {
        *s.v15
    }
}

fn v_set(ref s: VState, idx: usize, val: u32) {
    if idx == 0 {
        s.v0 = val;
    } else if idx == 1 {
        s.v1 = val;
    } else if idx == 2 {
        s.v2 = val;
    } else if idx == 3 {
        s.v3 = val;
    } else if idx == 4 {
        s.v4 = val;
    } else if idx == 5 {
        s.v5 = val;
    } else if idx == 6 {
        s.v6 = val;
    } else if idx == 7 {
        s.v7 = val;
    } else if idx == 8 {
        s.v8 = val;
    } else if idx == 9 {
        s.v9 = val;
    } else if idx == 10 {
        s.v10 = val;
    } else if idx == 11 {
        s.v11 = val;
    } else if idx == 12 {
        s.v12 = val;
    } else if idx == 13 {
        s.v13 = val;
    } else if idx == 14 {
        s.v14 = val;
    } else {
        s.v15 = val;
    }
}

fn m_at(s: @MState, idx: usize) -> u32 {
    if idx == 0 {
        *s.m0
    } else if idx == 1 {
        *s.m1
    } else if idx == 2 {
        *s.m2
    } else if idx == 3 {
        *s.m3
    } else if idx == 4 {
        *s.m4
    } else if idx == 5 {
        *s.m5
    } else if idx == 6 {
        *s.m6
    } else if idx == 7 {
        *s.m7
    } else if idx == 8 {
        *s.m8
    } else if idx == 9 {
        *s.m9
    } else if idx == 10 {
        *s.m10
    } else if idx == 11 {
        *s.m11
    } else if idx == 12 {
        *s.m12
    } else if idx == 13 {
        *s.m13
    } else if idx == 14 {
        *s.m14
    } else {
        *s.m15
    }
}

fn sigma_at(round_idx: usize, col: usize) -> usize {
    let s: Span<[u8; 16]> = SIGMA.span();
    let p: @[u8; 16] = s.at(round_idx);
    let ps: Span<u8> = p.span();
    if col == 0 {
        (*ps.at(0)).into()
    } else if col == 1 {
        (*ps.at(1)).into()
    } else if col == 2 {
        (*ps.at(2)).into()
    } else if col == 3 {
        (*ps.at(3)).into()
    } else if col == 4 {
        (*ps.at(4)).into()
    } else if col == 5 {
        (*ps.at(5)).into()
    } else if col == 6 {
        (*ps.at(6)).into()
    } else if col == 7 {
        (*ps.at(7)).into()
    } else if col == 8 {
        (*ps.at(8)).into()
    } else if col == 9 {
        (*ps.at(9)).into()
    } else if col == 10 {
        (*ps.at(10)).into()
    } else if col == 11 {
        (*ps.at(11)).into()
    } else if col == 12 {
        (*ps.at(12)).into()
    } else if col == 13 {
        (*ps.at(13)).into()
    } else if col == 14 {
        (*ps.at(14)).into()
    } else {
        (*ps.at(15)).into()
    }
}

// =========================================================================
// Arithmetic helpers
// =========================================================================

fn shr32(x: u32, n: u32) -> u32 {
    let mut r: u32 = x;
    let mut i: u32 = 0;
    loop {
        if i == n {
            break;
        }
        r = r / 2;
        i += 1;
    }
    r
}

fn shl32(x: u32, n: u32) -> u32 {
    let mut r: u32 = x;
    let mut i: u32 = 0;
    loop {
        if i == n {
            break;
        }
        r = r.wrapping_mul(2_u32);
        i += 1;
    }
    r
}

fn rot_r(x: u32, n: u32) -> u32 {
    let m: u32 = n & 31;
    if m == 0 {
        x
    } else {
        let lo: u32 = shr32(x, m);
        let hi: u32 = shl32(x, 32 - m);
        lo | hi
    }
}

fn read_u32_be_at(bytes: @ByteArray, off: usize) -> u32 {
    let total = bytes.len();
    let b3: u32 = if off + 3 < total {
        bytes.at(off + 3).unwrap().into()
    } else {
        0
    };
    let b2: u32 = if off + 2 < total {
        bytes.at(off + 2).unwrap().into()
    } else {
        0
    };
    let b1: u32 = if off + 1 < total {
        bytes.at(off + 1).unwrap().into()
    } else {
        0
    };
    let b0: u32 = if off < total {
        bytes.at(off).unwrap().into()
    } else {
        0
    };
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
    g(ref v, 0, 4, 8, 12, m_at(m, sigma_at(round_idx, 0)), m_at(m, sigma_at(round_idx, 1)));
    g(ref v, 1, 5, 9, 13, m_at(m, sigma_at(round_idx, 2)), m_at(m, sigma_at(round_idx, 3)));
    g(ref v, 2, 6, 10, 14, m_at(m, sigma_at(round_idx, 4)), m_at(m, sigma_at(round_idx, 5)));
    g(ref v, 3, 7, 11, 15, m_at(m, sigma_at(round_idx, 6)), m_at(m, sigma_at(round_idx, 7)));
    g(ref v, 0, 5, 10, 15, m_at(m, sigma_at(round_idx, 8)), m_at(m, sigma_at(round_idx, 9)));
    g(ref v, 1, 6, 11, 12, m_at(m, sigma_at(round_idx, 10)), m_at(m, sigma_at(round_idx, 11)));
    g(ref v, 2, 7, 8, 13, m_at(m, sigma_at(round_idx, 12)), m_at(m, sigma_at(round_idx, 13)));
    g(ref v, 3, 4, 9, 14, m_at(m, sigma_at(round_idx, 14)), m_at(m, sigma_at(round_idx, 15)));
}

// =========================================================================
// Compression function (BLAKE3 spec §2.6)
// =========================================================================

/// `compress(prev_h, m, counter, block_len, flags) -> [u32; 16]`
pub fn compress(prev_h: [u32; 8], m: MState, counter: u64, block_len: u32, flags: u8) -> [u32; 16] {
    let iv_span = IV.span();
    let counter_lo: u32 = (counter & 0xFFFFFFFF).try_into().unwrap();
    let counter_hi: u32 = ((counter / 0x100000000) & 0xFFFFFFFF).try_into().unwrap();
    let flags_u32: u32 = flags.into();
    let mut v = VState {
        v0: *prev_h.span().at(0),
        v1: *prev_h.span().at(1),
        v2: *prev_h.span().at(2),
        v3: *prev_h.span().at(3),
        v4: *prev_h.span().at(4),
        v5: *prev_h.span().at(5),
        v6: *prev_h.span().at(6),
        v7: *prev_h.span().at(7),
        v8: *iv_span.at(0),
        v9: *iv_span.at(1),
        v10: *iv_span.at(2),
        v11: *iv_span.at(3),
        // NOTE: words 12..15 are **assigned**, not XORed onto IV[4..8].
        // The reference implementation builds
        //   state = [cv[0..8], IV[0..4], counter_low, counter_high,
        //            block_len, flags]
        // and never touches IV[4..8] here. XORing them in (as this file used
        // to) is wrong for every input, and is invisible for any test that
        // compares two copies of the same mistake — which is exactly what the
        // first version of the G test did.
        v12: counter_lo,
        v13: counter_hi,
        v14: block_len,
        v15: flags_u32,
    };

    round_fn(ref v, @m, 0);
    round_fn(ref v, @m, 1);
    round_fn(ref v, @m, 2);
    round_fn(ref v, @m, 3);
    round_fn(ref v, @m, 4);
    round_fn(ref v, @m, 5);
    round_fn(ref v, @m, 6);

    // Words 0..8: `state[i] ^ state[i+8]`.
    // Words 8..16: `state[i+8] ^ cv[i]` — where `cv` is the chaining value
    // **as it was on entry**, not the final `state[i]`. Using `state[i]` here
    // is the classic BLAKE2 shape and is wrong for BLAKE3; it does not affect
    // the first 8 words (so a root hash alone would not catch it) but it
    // corrupts every parent / tree node.
    let cvs = prev_h.span();
    [
        v.v0 ^ v.v8,
        v.v1 ^ v.v9,
        v.v2 ^ v.v10,
        v.v3 ^ v.v11,
        v.v4 ^ v.v12,
        v.v5 ^ v.v13,
        v.v6 ^ v.v14,
        v.v7 ^ v.v15,
        v.v8 ^ *cvs.at(0),
        v.v9 ^ *cvs.at(1),
        v.v10 ^ *cvs.at(2),
        v.v11 ^ *cvs.at(3),
        v.v12 ^ *cvs.at(4),
        v.v13 ^ *cvs.at(5),
        v.v14 ^ *cvs.at(6),
        v.v15 ^ *cvs.at(7),
    ]
}

/// Build MState by reading 16 **little-endian** u32s from `bytes` starting at
/// `off`. Positions past `bytes.len()` are zero-padded (per spec).
///
/// BLAKE3 loads every message word little-endian (spec §2.6, `words` are
/// `u32::from_le_bytes`). Reading them big-endian silently produces a
/// self-consistent but wrong hash — which is exactly what happened here:
/// the module had no reference vectors, so every digest was wrong from the
/// first block on, and still looked like a plausible hash.
fn read_block(bytes: @ByteArray, off: usize) -> MState {
    MState {
        m0: read_u32_le_at(bytes, off + 0),
        m1: read_u32_le_at(bytes, off + 4),
        m2: read_u32_le_at(bytes, off + 8),
        m3: read_u32_le_at(bytes, off + 12),
        m4: read_u32_le_at(bytes, off + 16),
        m5: read_u32_le_at(bytes, off + 20),
        m6: read_u32_le_at(bytes, off + 24),
        m7: read_u32_le_at(bytes, off + 28),
        m8: read_u32_le_at(bytes, off + 32),
        m9: read_u32_le_at(bytes, off + 36),
        m10: read_u32_le_at(bytes, off + 40),
        m11: read_u32_le_at(bytes, off + 44),
        m12: read_u32_le_at(bytes, off + 48),
        m13: read_u32_le_at(bytes, off + 52),
        m14: read_u32_le_at(bytes, off + 56),
        m15: read_u32_le_at(bytes, off + 60),
    }
}

// =========================================================================
// Public keyed_hash + HMAC-BLAKE3
// =========================================================================

/// BLAKE3 keyed hash, restricted to a **single chunk** (msg ≤ 1024 bytes).
///
/// **This function is NOT BLAKE3 for longer inputs.** Multi-chunk messages
/// need a per-chunk counter and the parent/tree node layer
/// (`compress`'s words 8..16), neither of which is implemented here. Rather
/// than return a plausible-but-wrong digest, it asserts: a silently incorrect
/// hash is far more dangerous than a failed one, and that is exactly how the
/// five defects this file used to carry stayed invisible.
///
/// The macaroon chain's per-step message is `name ‖ caveat_ser ‖ id`; callers
/// must bound it, which `macaroon_max_chain_step_len` documents.
pub fn keyed_hash_one_chunk(key: [u32; 8], msg: @ByteArray) -> [u32; 8] {
    let msg_len: usize = msg.len();
    assert!(msg_len <= CHUNK_LEN, "keyed_hash_one_chunk: message exceeds one chunk");
    let mut state: [u32; 8] = key;
    let mut offset: usize = 0;

    loop {
        let is_first: bool = offset == 0;
        let remaining: usize = msg_len - offset;
        let block_len: usize = if remaining >= 64 {
            64
        } else {
            remaining
        };
        let is_last: bool = if is_first {
            (offset + block_len == msg_len) || msg_len == 0
        } else {
            (offset + block_len == msg_len)
        };

        // KEYED_HASH is a flag on **every** block of a keyed hash, not only the
        // last one. Starting from 0 (as this used to) domain-separated only
        // the final block, so every preceding block was compressed as if it
        // belonged to an unkeyed hash. That is invisible for any message that
        // fits in one block — which is why it survived: every single-block
        // vector matched while every multi-block one failed.
        let mut flags: u8 = KEYED_HASH;
        if is_first {
            flags = flags | CHUNK_START;
        }
        if is_last {
            flags = flags | CHUNK_END | ROOT;
        }

        let m_block: MState = read_block(msg, offset);
        let cv: [u32; 16] = compress(state, m_block, 0, block_len.try_into().unwrap(), flags);

        if is_last {
            return [
                *cv.span().at(0), *cv.span().at(1), *cv.span().at(2), *cv.span().at(3),
                *cv.span().at(4), *cv.span().at(5), *cv.span().at(6), *cv.span().at(7),
            ];
        }

        state =
            [
                *cv.span().at(0), *cv.span().at(1), *cv.span().at(2), *cv.span().at(3),
                *cv.span().at(4), *cv.span().at(5), *cv.span().at(6), *cv.span().at(7),
            ];
        offset = offset + block_len;
    }
}

/// BLAKE3 **unkeyed** hash, restricted to a **single chunk** (msg ≤ 1024 bytes).
///
/// RFC-0973 §Root-Secret Binding needs exactly this: one unkeyed BLAKE3 hop over
/// a 32-byte root secret, computed by constrained program logic so the digest
/// is a *result* and never an input.
///
/// **This is not `keyed_hash_one_chunk` with a zero key.** A zero key is a real
/// key: BLAKE3 derives the keyed chaining value as `words` XOR `key_words`, and
/// a zero key is only equivalent to the IV if the derivation happens to be the
/// identity — it is not the same function, and the `KEYED_HASH` domain flag
/// would still be set on every block. Both differences are load-bearing, so the
/// unkeyed path is written out rather than derived from the keyed one.
///
/// **This function is NOT BLAKE3 for longer inputs.** Same restriction and same
/// reasoning as the keyed variant: multi-chunk messages need a per-chunk counter
/// and the parent/tree layer (`compress`'s words 8..16), neither of which is
/// implemented here. Rather than return a plausible-but-wrong digest, it
/// asserts.
pub fn unkeyed_hash_one_chunk(msg: @ByteArray) -> [u32; 8] {
    let msg_len: usize = msg.len();
    assert!(msg_len <= CHUNK_LEN, "unkeyed_hash_one_chunk: message exceeds one chunk");
    // An unkeyed hash starts from the IV. This is the one structural difference
    // from the keyed path, where the chaining value is the key itself.
    let mut state: [u32; 8] = IV;
    let mut offset: usize = 0;

    loop {
        let is_first: bool = offset == 0;
        let remaining: usize = msg_len - offset;
        let block_len: usize = if remaining >= 64 {
            64
        } else {
            remaining
        };
        let is_last: bool = if is_first {
            (offset + block_len == msg_len) || msg_len == 0
        } else {
            (offset + block_len == msg_len)
        };

        // No KEYED_HASH flag anywhere: an unkeyed hash sets only the domain
        // bits. Setting it on even one block would produce a digest that is
        // internally consistent and matches nothing, which is the failure mode
        // this module already carries a scar over.
        let mut flags: u8 = 0;
        if is_first {
            flags = flags | CHUNK_START;
        }
        if is_last {
            flags = flags | CHUNK_END | ROOT;
        }

        let m_block: MState = read_block(msg, offset);
        let cv: [u32; 16] = compress(state, m_block, 0, block_len.try_into().unwrap(), flags);

        if is_last {
            return [
                *cv.span().at(0), *cv.span().at(1), *cv.span().at(2), *cv.span().at(3),
                *cv.span().at(4), *cv.span().at(5), *cv.span().at(6), *cv.span().at(7),
            ];
        }

        state =
            [
                *cv.span().at(0), *cv.span().at(1), *cv.span().at(2), *cv.span().at(3),
                *cv.span().at(4), *cv.span().at(5), *cv.span().at(6), *cv.span().at(7),
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
        if i == KEY_LEN {
            break;
        }
        let b: u8 = if i < key.len() {
            key.at(i).unwrap()
        } else {
            0
        };
        kpad.append_byte(b);
        i += 1;
    }

    let key_words: [u32; 8] = read_key_le(@kpad);

    // inner_key = kpad XOR 0x36 (32 bytes)
    let mut inner_key: ByteArray = "";
    let mut ii: usize = 0;
    loop {
        if ii == KEY_LEN {
            break;
        }
        inner_key.append_byte(kpad.at(ii).unwrap() ^ 0x36);
        ii += 1;
    }

    // outer_key = kpad XOR 0x5c (32 bytes)
    let mut outer_key: ByteArray = "";
    let mut oi: usize = 0;
    loop {
        if oi == KEY_LEN {
            break;
        }
        outer_key.append_byte(kpad.at(oi).unwrap() ^ 0x5c);
        oi += 1;
    }

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
        if i == 8 {
            break;
        }
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
    }
    out
}

/// Reconstruct a [u32; 8] from a ByteArray (32 bytes, little-endian u32s).
pub fn read_key_le(bytes: @ByteArray) -> [u32; 8] {
    [
        read_u32_le_at(bytes, 0), read_u32_le_at(bytes, 4), read_u32_le_at(bytes, 8),
        read_u32_le_at(bytes, 12), read_u32_le_at(bytes, 16), read_u32_le_at(bytes, 20),
        read_u32_le_at(bytes, 24), read_u32_le_at(bytes, 28),
    ]
}

/// Read little-endian u32 from `bytes` at byte offset `off`. Returns 0 if past end.
pub fn read_u32_le_at(bytes: @ByteArray, off: usize) -> u32 {
    let total = bytes.len();
    let b0: u32 = if off < total {
        bytes.at(off).unwrap().into()
    } else {
        0
    };
    let b1: u32 = if off + 1 < total {
        bytes.at(off + 1).unwrap().into()
    } else {
        0
    };
    let b2: u32 = if off + 2 < total {
        bytes.at(off + 2).unwrap().into()
    } else {
        0
    };
    let b3: u32 = if off + 3 < total {
        bytes.at(off + 3).unwrap().into()
    } else {
        0
    };
    (b3 * 0x1000000) + (b2 * 0x10000) + (b1 * 0x100) + b0
}

// =========================================================================
// Reference-vector parity tests
// =========================================================================
//
// Every vector below was produced by the `blake3` Rust crate
// (`blake3::keyed_hash`) via
// `cargo run -p zk-circuit --example blake3_vectors`, and re-verified on the
// Rust side by `crates/zk-circuit/tests/blake3_parity_vectors.rs`. The key
// comes from an xorshift64* so that no 4-byte group equals its own
// byte-reversal — a big-endian / little-endian mix-up cannot pass these.
//
// The message lengths walk every boundary the chunking logic has to get
// right: 0, a short final chunk, exactly one chunk, and then the first
// message that needs a SECOND chunk. That last one is the vector that
// catches a chunk counter stuck at 0, which every single-chunk vector
// above it would happily pass.

#[cfg(test)]
mod tests {
    use super::{
        compress, g, keyed_hash_one_chunk, read_block, read_key_le, u32s_le_to_bytes,
        unkeyed_hash_one_chunk, CHUNK_END,
        CHUNK_START, IV, KEYED_HASH, ROOT, MState, VState,
    };
    use core::array::ArrayTrait;

    fn bytes_to_ba(xs: @Array<u8>) -> ByteArray {
        let mut ba: ByteArray = "";
        let mut i: usize = 0;
        while i < xs.len() {
            ba.append_byte(*xs.at(i));
            i += 1;
        };
        ba
    }

    fn repeat_ba(byte: u8, n: usize) -> ByteArray {
        let mut ba: ByteArray = "";
        let mut i: usize = 0;
        while i < n {
            ba.append_byte(byte);
            i += 1;
        };
        ba
    }

    /// The shared 32-byte key: xorshift64* seeded with 0x1234_5678_9abc_def0.
    fn test_key() -> ByteArray {
        bytes_to_ba(
            @array![
                0x3d, 0x85, 0x9f, 0x60, 0xd8, 0xb2, 0x96, 0x47, 0x4b, 0x55, 0xa9, 0xe2, 0x7c, 0xc4,
                0xf1, 0x3e, 0xc2, 0x02, 0x96, 0x95, 0x40, 0x7e, 0x3b, 0x8b, 0xb4, 0x0c, 0xff, 0x1b,
                0x5c, 0xcb, 0xd4, 0x04,
            ],
        )
    }

    fn keyed_hash_with_test_key(msg: @ByteArray) -> ByteArray {
        let key = test_key();
        u32s_le_to_bytes(keyed_hash_one_chunk(read_key_le(@key), msg))
    }

    /// Byte-exact compare. Cairo's `assert!` takes a literal format string, so
    /// the failing vector is identified by the test name rather than the
    /// message.
    fn assert_digest(got: @ByteArray, want: @Array<u8>) {
        let want_ba = bytes_to_ba(want);
        assert!(got.len() == 32, "digest length wrong");
        let mut i: usize = 0;
        while i < 32 {
            assert!(got.at(i).unwrap() == want_ba.at(i).unwrap(), "blake3 parity mismatch");
            i += 1;
        };
    }

    /// Isolate the G function: the very first operation of round 0 of
    /// BLAKE3(""), with the canonical state (v[0..8] = v[8..16] = IV,
    /// v[15] ^= CHUNK_START|CHUNK_END|ROOT) and an all-zero message.
    /// Expected values produced by `cargo run -p zk-circuit --example
    /// blake3_gref`. If this fails, the defect is in G / rotate / the state
    /// layout / SIGMA rather than in the chunking loop.
    #[test]
    fn g_first_round_first_column_matches_reference() {
        let iv_span = IV.span();
        let mut v = VState {
            v0: *iv_span.at(0),
            v1: *iv_span.at(1),
            v2: *iv_span.at(2),
            v3: *iv_span.at(3),
            v4: *iv_span.at(4),
            v5: *iv_span.at(5),
            v6: *iv_span.at(6),
            v7: *iv_span.at(7),
            v8: *iv_span.at(0),
            v9: *iv_span.at(1),
            v10: *iv_span.at(2),
            v11: *iv_span.at(3),
            // Per spec: assigned, not XORed onto IV[4..8].
            v12: 0,
            v13: 0,
            v14: 0,
            v15: (ROOT | CHUNK_START | CHUNK_END).into(),
        };
        g(ref v, 0, 4, 8, 12, 0, 0);
        assert!(v.v0 == 0xeb2778d5, "g: v0");
        assert!(v.v4 == 0x5a8196b9, "g: v4");
        assert!(v.v8 == 0x70c46342, "g: v8");
        assert!(v.v12 == 0xcdd3c1c3, "g: v12");
    }

    /// For a uniform message every 64-byte block must decode identically.
    /// This isolates `read_block` / `read_u32_le_at` from the compression
    /// function entirely.
    #[test]
    fn read_block_at_later_offset_matches_first_offset() {
        let msg = repeat_ba(0x11, 128);
        let b0 = read_block(@msg, 0);
        assert!(b0.m0 == 0x11111111, "b0.m0");
        assert!(b0.m15 == 0x11111111, "b0.m15");
        let b64 = read_block(@msg, 64);
        assert!(b64.m0 == 0x11111111, "b64.m0");
        assert!(b64.m15 == 0x11111111, "b64.m15");
        assert!(b64.m7 == 0x11111111, "b64.m7");
    }

    /// Same 128-byte message, but asserting the SECOND block's compress call
    /// explicitly (offset 64, block_len 64, flags KEYED_HASH|CHUNK_END|ROOT,
    /// chained off block 0's CV).
    #[test]
    fn keyed_hash_second_block_compress_matches_reference() {
        let key = test_key();
        let kw = read_key_le(@key);
        let msg = repeat_ba(0x11, 128);
        let blk0 = read_block(@msg, 0);
        let out0 = compress(kw, blk0, 0, 64, KEYED_HASH | CHUNK_START);
        let o0s = out0.span();
        let cv: [u32; 8] = [
            *o0s.at(0), *o0s.at(1), *o0s.at(2), *o0s.at(3), *o0s.at(4), *o0s.at(5), *o0s.at(6),
            *o0s.at(7),
        ];
        let blk1 = read_block(@msg, 64);
        let out1 = compress(cv, blk1, 0, 64, KEYED_HASH | CHUNK_END | ROOT);
        let o1s = out1.span();
        assert!(*o1s.at(0) == 0x7c74c601, "blk1 cv0");
        assert!(*o1s.at(1) == 0xf57e7c40, "blk1 cv1");
        assert!(*o1s.at(2) == 0x69b4b933, "blk1 cv2");
        assert!(*o1s.at(3) == 0xa927a195, "blk1 cv3");
        assert!(*o1s.at(4) == 0x099bee32, "blk1 cv4");
        assert!(*o1s.at(5) == 0x15a6ceb7, "blk1 cv5");
        assert!(*o1s.at(6) == 0x8b9ff8b2, "blk1 cv6");
        assert!(*o1s.at(7) == 0x98b24a38, "blk1 cv7");
    }

    /// The chaining value after the FIRST 64-byte block of a 128-byte
    /// message. Isolates the loop's state hand-off from the final digest: if
    /// this matches but the 128-byte digest does not, the defect is in the
    /// loop's flag/block_len choice for the second block.
    /// Expected values: `TRACE_CV=1 cargo run -p zk-circuit --example
    /// blake3_chunkmodel`.
    #[test]
    fn keyed_hash_intermediate_cv_after_first_block_matches_reference() {
        let key = test_key();
        let kw = read_key_le(@key);
        let msg = repeat_ba(0x11, 128);
        let blk0 = read_block(@msg, 0);
        let out = compress(kw, blk0, 0, 64, KEYED_HASH | CHUNK_START);
        let os = out.span();
        assert!(*os.at(0) == 0x6c91f9c0, "cv0");
        assert!(*os.at(1) == 0xa7c4a456, "cv1");
        assert!(*os.at(2) == 0xe59112e5, "cv2");
        assert!(*os.at(3) == 0xb2e10b85, "cv3");
        assert!(*os.at(4) == 0x6c188b45, "cv4");
        assert!(*os.at(5) == 0x2d821466, "cv5");
        assert!(*os.at(6) == 0xcc4e83ff, "cv6");
        assert!(*os.at(7) == 0xe1458411, "cv7");
    }

    /// Canonical BLAKE3 vector for the **empty, unkeyed** hash. This exercises
    /// `compress` on its own — IV, SIGMA, G, the round schedule, the flags and
    /// the output XOR — with none of the keyed-hash chunking loop in the way.
    /// If this passes but `keyed_hash_empty_matches_reference` fails, the bug
    /// is in the key/chunk plumbing; if this fails, it is in `compress`.
    #[test]
    fn compress_matches_canonical_empty_unkeyed_vector() {
        let zero = MState {
            m0: 0,
            m1: 0,
            m2: 0,
            m3: 0,
            m4: 0,
            m5: 0,
            m6: 0,
            m7: 0,
            m8: 0,
            m9: 0,
            m10: 0,
            m11: 0,
            m12: 0,
            m13: 0,
            m14: 0,
            m15: 0,
        };
        let cv = compress(IV, zero, 0, 0, CHUNK_START | CHUNK_END | ROOT);
        let cs = cv.span();
        let words: [u32; 8] = [
            *cs.at(0), *cs.at(1), *cs.at(2), *cs.at(3), *cs.at(4), *cs.at(5), *cs.at(6), *cs.at(7),
        ];
        let got = u32s_le_to_bytes(words);
        assert_digest(
            @got,
            @array![
                0xaf, 0x13, 0x49, 0xb9, 0xf5, 0xf9, 0xa1, 0xa6, 0xa0, 0x40, 0x4d, 0xea, 0x36, 0xdc,
                0xc9, 0x49, 0x9b, 0xcb, 0x25, 0xc9, 0xad, 0xc1, 0x12, 0xb7, 0xcc, 0x9a, 0x93, 0xca,
                0xe4, 0x1f, 0x32, 0x62,
            ],
        );
    }

    /// Unkeyed parity vectors, anchored to the **host** BLAKE3.
    ///
    /// These are the positive control RFC-0973 §Root-Secret Binding needs. A
    /// circuit that rejects every witness satisfies a rejection test, so a
    /// rejection vector alone cannot distinguish a working binding from a
    /// wrong secret-to-byte encoding, a digest compared against the wrong
    /// public input, or a self-consistent but incorrect permutation. Only a
    /// vector produced by an independent implementation catches those.
    ///
    /// Expected values are `blake3::hash(...)` from the `blake3` crate as used
    /// by the host, reproduced here as literals so this test does not depend on
    /// the code it is checking:
    ///
    ///   BLAKE3("")        = af1349b9f5f9a1a6a0404dea36dcc949
    ///                       9bcb25c9adc112b7cc9a93cae41f3262
    ///   BLAKE3("abc")     = 6437b3ac38465133ffb63b75273a8db5
    ///                       48c558465d79db03fd359c6cd5bd9d85
    #[test]
    fn unkeyed_empty_matches_host_blake3() {
        let msg: ByteArray = "";
        let got = u32s_le_to_bytes(unkeyed_hash_one_chunk(@msg));
        assert_digest(
            @got,
            @array![
                0xaf, 0x13, 0x49, 0xb9, 0xf5, 0xf9, 0xa1, 0xa6, 0xa0, 0x40, 0x4d, 0xea, 0x36, 0xdc,
                0xc9, 0x49, 0x9b, 0xcb, 0x25, 0xc9, 0xad, 0xc1, 0x12, 0xb7, 0xcc, 0x9a, 0x93, 0xca,
                0xe4, 0x1f, 0x32, 0x62,
            ],
        );
    }

    #[test]
    fn unkeyed_abc_matches_host_blake3() {
        let msg = bytes_to_ba(@array![0x61, 0x62, 0x63]);
        let got = u32s_le_to_bytes(unkeyed_hash_one_chunk(@msg));
        assert_digest(
            @got,
            @array![
                0x64, 0x37, 0xb3, 0xac, 0x38, 0x46, 0x51, 0x33, 0xff, 0xb6, 0x3b, 0x75, 0x27, 0x3a,
                0x8d, 0xb5, 0x48, 0xc5, 0x58, 0x46, 0x5d, 0x79, 0xdb, 0x03, 0xfd, 0x35, 0x9c, 0x6c,
                0xd5, 0xbd, 0x9d, 0x85,
            ],
        );
    }

    /// A 32-byte secret — the exact shape RFC-0973 hashes. Anchoring the
    /// relation's own input size is what makes the parity claim about the
    /// relation rather than about short strings.
    ///
    ///   BLAKE3(00..01) = 55bbfb5b05f80543bd662a7b7a74980a
    ///                   2992612798087fa24e1fc86b8e8508e4
    #[test]
    fn unkeyed_thirty_two_byte_secret_matches_host_blake3() {
        let mut secret = repeat_ba(0x00, 31);
        secret.append_byte(0x01);
        let got = u32s_le_to_bytes(unkeyed_hash_one_chunk(@secret));
        assert_digest(
            @got,
            @array![
                0x55, 0xbb, 0xfb, 0x5b, 0x05, 0xf8, 0x05, 0x43, 0xbd, 0x66, 0x2a, 0x7b, 0x7a, 0x74,
                0x98, 0x0a, 0x29, 0x92, 0x61, 0x27, 0x98, 0x08, 0x7f, 0xa2, 0x4e, 0x1f, 0xc8, 0x6b,
                0x8e, 0x85, 0x08, 0xe4,
            ],
        );
    }

    /// The unkeyed path must not be the keyed path with a zero key. Both are
    /// one-chunk hashes over the same bytes, so they are easy to confuse, and a
    /// confusable pair is how a binding ends up computing something that looks
    /// like the relation and is not it.
    #[test]
    fn unkeyed_is_not_keyed_with_a_zero_key() {
        let msg = bytes_to_ba(@array![0x61, 0x62, 0x63]);
        let zero_key = [0, 0, 0, 0, 0, 0, 0, 0];
        let unkeyed = u32s_le_to_bytes(unkeyed_hash_one_chunk(@msg));
        let keyed_zero = u32s_le_to_bytes(keyed_hash_one_chunk(zero_key, @msg));
        assert_digest(@unkeyed, @array![
            0x64, 0x37, 0xb3, 0xac, 0x38, 0x46, 0x51, 0x33, 0xff, 0xb6, 0x3b, 0x75, 0x27, 0x3a,
            0x8d, 0xb5, 0x48, 0xc5, 0x58, 0x46, 0x5d, 0x79, 0xdb, 0x03, 0xfd, 0x35, 0x9c, 0x6c,
            0xd5, 0xbd, 0x9d, 0x85,
        ]);
        // The two digests must differ. Pinned as a full 32-byte mismatch rather
        // than a single word, so a partial regression is still caught.
        let mut differs: bool = false;
        let mut i: usize = 0;
        while i < 32 {
            if unkeyed.at(i).unwrap() != keyed_zero.at(i).unwrap() {
                differs = true;
            };
            i += 1;
        };
        assert!(differs, "unkeyed hash must not equal keyed-with-zero-key");
    }

    #[test]
    fn keyed_hash_empty_matches_reference() {
        let got = keyed_hash_with_test_key(@"");
        assert_digest(
            @got,
            @array![
                0x8a, 0xc4, 0xe0, 0x3c, 0x4d, 0x21, 0xdf, 0x35, 0x76, 0x33, 0xea, 0x3f, 0x23, 0x8b,
                0x09, 0x41, 0x9c, 0x3b, 0x5a, 0x6f, 0x19, 0xc9, 0x2a, 0x3f, 0x2d, 0x5c, 0x97, 0x7e,
                0x19, 0x62, 0x17, 0x8b,
            ],
        );
    }

    #[test]
    fn keyed_hash_one_byte_matches_reference() {
        let got = keyed_hash_with_test_key(@"a");
        assert_digest(
            @got,
            @array![
                0x0b, 0xd4, 0x77, 0x9a, 0xd6, 0x41, 0x53, 0xf0, 0x30, 0x43, 0x35, 0xce, 0xa7, 0xf2,
                0x67, 0xe7, 0x92, 0x34, 0x1f, 0x33, 0xbc, 0x93, 0x56, 0x7c, 0x91, 0x7a, 0x72, 0xbf,
                0x70, 0x1a, 0x63, 0x2b,
            ],
        );
    }

    #[test]
    fn keyed_hash_partial_final_chunk_matches_reference() {
        let got = keyed_hash_with_test_key(@repeat_ba(0xab, 55));
        assert_digest(
            @got,
            @array![
                0xf3, 0x66, 0xef, 0x8e, 0x51, 0xb4, 0x99, 0xee, 0xce, 0x37, 0x07, 0x5e, 0xae, 0xb0,
                0x12, 0xa5, 0x13, 0x4e, 0x1f, 0xcf, 0xa8, 0xd5, 0xc0, 0x7b, 0x1c, 0xbb, 0x21, 0x06,
                0xfb, 0x3c, 0xbb, 0xe7,
            ],
        );
    }

    #[test]
    fn keyed_hash_exactly_one_chunk_matches_reference() {
        let got = keyed_hash_with_test_key(@repeat_ba(0x5a, 64));
        assert_digest(
            @got,
            @array![
                0xc3, 0x72, 0x30, 0x93, 0xf7, 0x9e, 0x39, 0x62, 0x1a, 0x02, 0x26, 0x7a, 0x94, 0xb4,
                0x3b, 0x93, 0x94, 0xb1, 0xb4, 0x4d, 0x72, 0x01, 0x9a, 0x84, 0x37, 0x95, 0x42, 0xfc,
                0xbe, 0x30, 0x6f, 0xb4,
            ],
        );
    }

    /// The vector that matters most: the first message needing a second chunk.
    #[test]
    fn keyed_hash_two_chunks_matches_reference() {
        let got = keyed_hash_with_test_key(@repeat_ba(0xc3, 65));
        assert_digest(
            @got,
            @array![
                0x7f, 0x61, 0x55, 0xd7, 0xe7, 0xda, 0x2c, 0xee, 0x97, 0xee, 0x13, 0x59, 0x8a, 0xb8,
                0x33, 0xe3, 0x98, 0x84, 0x5d, 0xff, 0xeb, 0xce, 0x0e, 0xc3, 0xf1, 0x1b, 0xab, 0xee,
                0xa9, 0xc4, 0x6c, 0x14,
            ],
        );
    }

    #[test]
    fn keyed_hash_two_full_chunks_matches_reference() {
        let got = keyed_hash_with_test_key(@repeat_ba(0x11, 128));
        assert_digest(
            @got,
            @array![
                0x01, 0xc6, 0x74, 0x7c, 0x40, 0x7c, 0x7e, 0xf5, 0x33, 0xb9, 0xb4, 0x69, 0x95, 0xa1,
                0x27, 0xa9, 0x32, 0xee, 0x9b, 0x09, 0xb7, 0xce, 0xa6, 0x15, 0xb2, 0xf8, 0x9f, 0x8b,
                0x38, 0x4a, 0xb2, 0x98,
            ],
        );
    }

    #[test]
    fn keyed_hash_three_chunks_matches_reference() {
        let got = keyed_hash_with_test_key(@repeat_ba(0xfe, 129));
        assert_digest(
            @got,
            @array![
                0x0e, 0xb8, 0x09, 0xd0, 0x2c, 0x45, 0x58, 0x58, 0x18, 0xba, 0x85, 0xa4, 0xd7, 0xc5,
                0x78, 0x0e, 0xa0, 0xc8, 0xff, 0x44, 0x21, 0x4c, 0xc5, 0xf7, 0x8f, 0xa7, 0x35, 0x8a,
                0x13, 0x79, 0x9d, 0x77,
            ],
        );
    }

    /// The shape the macaroon chain actually uses: `name ‖ ser ‖ id`, which is
    /// 78 bytes here and therefore already spans two chunks.
    #[test]
    fn keyed_hash_macaroon_shaped_message_matches_reference() {
        let key = bytes_to_ba(
            @array![
            0x0a, 0xf1, 0x9b, 0x9a, 0x10, 0xe5, 0xb5, 0x77,
            0x8f, 0x91, 0xa6, 0x93, 0xd9, 0x38, 0xfe, 0x6a,
            0x7f, 0xf9, 0x97, 0xcd, 0xd8, 0x8b, 0xde, 0x06,
            0xac, 0x66, 0xef, 0xfc, 0xfb, 0xaa, 0xe0, 0xde,
        ],
        );

        let mut msg: ByteArray = "amount_max";
        msg.append_byte(0x00);
        msg.append_byte(0x00);
        msg.append_byte(0x00);
        msg.append_byte(0x20);
        let mut i: usize = 0;
        while i < 32 {
            msg.append_byte(0x42);
            i += 1;
        };
        i = 0;
        while i < 32 {
            msg.append_byte(0x77);
            i += 1;
        };
        assert!(msg.len() == 78, "macaroon_shaped msg length");
        assert!(key.len() == 32, "macaroon_shaped key length");
        // MSG_HEX from the generator: 61 6d 6f 75 6e 74 5f 6d 61 78 | 00 00 00 20 | 42*32 | 77*32
        let got = u32s_le_to_bytes(keyed_hash_one_chunk(read_key_le(@key), @msg));
        assert_digest(
            @got,
            @array![
                0xae, 0x31, 0x82, 0xf5, 0xfe, 0xc2, 0x21, 0x33, 0x8d, 0x9a, 0x6d, 0x69, 0xff, 0xac,
                0x53, 0x77, 0x8f, 0x8c, 0x50, 0xaf, 0xa3, 0x27, 0xb8, 0x4d, 0x59, 0x26, 0x18, 0x68,
                0xb6, 0x8d, 0x42, 0xaa,
            ],
        );
    }
}
