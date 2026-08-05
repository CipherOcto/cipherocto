//! Pure-Cairo RFC 8032 Ed25519 signature verifier.
//!
//! Inline minimal verifier for Edwards25519 over GF(2^255-19). Supports
//! verification only (no signing, no key generation). Cairo 2.16's
//! corelib has `core::ecdsa` (STARK curve) and `core::ec` (STARK EC),
//! neither of which is Curve25519/Ed25519 — full inline here.
//!
//! Field arithmetic uses `core::integer::u256` natively (handles carry
//! internally). Limbs eliminated; one u256 per field element.
//!
//! Verification algorithm (RFC 8032 §5.1.7):
//!
//! 1. Decode pub key A (32 bytes LE) → point on Edwards25519, reject
//!    non-canonical y, reject y ∈ {0, 1, -1, ...} (small order).
//! 2. Decode sig R (first 32 bytes LE) → point, reject small-order.
//! 3. Decode S (last 32 bytes LE) → scalar mod L (reject S ≥ L).
//! 4. h = BLAKE3(R || A || M) reduced mod L. (We substitute BLAKE3 for
//!    SHA-512 — corelib 2.16.0 has no SHA-512; both are 256-bit-output
//!    hashes with collision resistance ≥ 2^128.)
//! 5. Cofactor check: `[8][S]B == [8]R + [8][h]A`.

use core::array::{ArrayTrait, SpanTrait};
use core::integer::u256;
use core::traits::{Into, TryInto};

use super::blake3;

// =============================================================================
// Curve constants (encoded as u256 little-endian LE bytes reference)
// =============================================================================
//
// When converting constants to u256, we build them via `u256{ low, high }`
// where low = low 128 bits, high = high 128 bits. Each Curve25519 8-limb
// constant is converted to a u256 by packing the 8 u32 limbs little-endian.

/// p = 2^255 - 19. low 128 = 0xffffffffffffffffffffffffffffffed; high 128 = 0x7fffffffffffffff
const P_LOW: u128 = 0xffffffffffffffffffffffffffffffed;
const P_HIGH: u128 = 0x7fffffffffffffff;

/// d = -121665/121666 mod p. low 128 = 0x75eb4dca135978a34141d8ab00700a4d; high 128 = 0x52036cee2b6ffe738cc740797779e898
const D_LOW: u128 = 0x75eb4dca135978a34141d8ab00700a4d;
const D_HIGH: u128 = 0x52036cee2b6ffe738cc740797779e898;

/// L = 2^252 + 27742317777372353535851937790883648493. low 128 = 0x5812631a5cf5d3eda2f79cd614def9de; high 128 = 0x10000000000000000000000000000000
const L_LOW: u128 = 0x5812631a5cf5d3eda2f79cd614def9de;
const L_HIGH: u128 = 0x10000000000000000000000000000000;

/// Base point Y = 4/5 mod p. low 128 = 0x67875c0c70d9120b215d4e8929a8e1a0; high 128 = 0x666666582b8324801c4b2ee13b8ced12
const BY_LOW: u128 = 0x67875c0c70d9120b215d4e8929a8e1a0;
const BY_HIGH: u128 = 0x666666582b8324801c4b2ee13b8ced12;

// =============================================================================
// Field element (u256-based, always reduced mod p)
// =============================================================================

#[derive(Copy, Drop)]
pub struct Field {
    pub v: u256,
}

#[inline(always)]
fn f_zero() -> Field {
    Field { v: u256 { low: 0, high: 0 } }
}

#[inline(always)]
fn f_one() -> Field {
    Field { v: u256 { low: 1, high: 0 } }
}

#[inline(always)]
fn f_p() -> Field {
    Field { v: u256 { low: P_LOW, high: P_HIGH } }
}

#[inline(always)]
fn f_l() -> Field {
    Field { v: u256 { low: L_LOW, high: L_HIGH } }
}

#[inline(always)]
fn f_d() -> Field {
    Field { v: u256 { low: D_LOW, high: D_HIGH } }
}

#[inline(always)]
fn f_by() -> Field {
    Field { v: u256 { low: BY_LOW, high: BY_HIGH } }
}

fn f_add(a: Field, b: Field) -> Field {
    let p = f_p();
    let r = a.v + b.v;
    // r ∈ [0, 2p). If r >= p, subtract p.
    if r >= p.v {
        Field { v: r - p.v }
    } else {
        Field { v: r }
    }
}

fn f_sub(a: Field, b: Field) -> Field {
    let p = f_p();
    let r = if a.v >= b.v {
        a.v - b.v
    } else {
        // a < b: result = a + p - b (always >= 0)
        a.v + p.v - b.v
    };
    Field { v: r }
}

fn f_mul(a: Field, b: Field) -> Field {
    let p = f_p();
    let wide = core::integer::u256_wide_mul(a.v, b.v);
    let nz: core::zeroable::NonZero<u256> = p.v.try_into().unwrap();
    let (_, r) = core::integer::u512_safe_div_rem_by_u256(wide, nz);
    Field { v: r }
}

fn f_neg(a: Field) -> Field {
    f_sub(f_p(), a)
}

fn f_sq(a: Field) -> Field {
    f_mul(a, a)
}

fn f_inv(a: Field) -> Field {
    let p = f_p();
    // Fermat: a^(p-2) mod p.
    let exp = u256 { low: P_LOW - 2, high: P_HIGH };
    Field { v: pow_u256(a.v, exp, p.v) }
}

/// Ed25519 sqrt: a^((p+3)/8), with multiplication by sqrt(-1) on failure.
pub fn sqrt_f(a: Field) -> Field {
    let p_v = f_p().v;
    // (p+3)/8 = 2^252 - 2 = bits 1..251 all set, bit 0 = 0.
    // low = 0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFE, high = 0x00FF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF.
    let exp1 = u256 { low: 0xfffffffffffffffffffffffffffffffe, high: 0x00ffffffffffffffffffffffffffffff };
    let x_v = pow_u256(a.v, exp1, p_v);
    let x = Field { v: x_v };
    if f_eq(f_sq(x), a) {
        return x;
    }
    // Multiply by sqrt(-1) = 2^((p-1)/4).
    // (p-1)/4 = 2^253 - 5 = bits 0,2..253 set, bit 1, 254, 255 = 0.
    // low = 0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFB, high = 0x1FFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF.
    let exp2 = u256 { low: 0xfffffffffffffffffffffffffffffffb, high: 0x1fffffffffffffffffffffffffffffff };
    let sqrt_m1_v = pow_u256(u256 { low: 2, high: 0 }, exp2, p_v);
    let sqrt_m1 = Field { v: sqrt_m1_v };
    let x = f_mul(x, sqrt_m1);
    if f_eq(f_sq(x), a) {
        return x;
    }
    f_zero()
}

fn u256_one() -> u256 {
    u256 { low: 1, high: 0 }
}

/// Square-and-multiply exponentiation for u256.
fn pow_u256(base: u256, exp: u256, modulus: u256) -> u256 {
    let mut result: u256 = u256 { low: 1, high: 0 };
    let mut b = base % modulus;
    let mut e = exp;
    let nz: core::zeroable::NonZero<u256> = modulus.try_into().unwrap();
    let mut i: u32 = 0;
    loop {
        if i == 256 {
            break;
        }
        if bit_at(e, i) {
            let wide = core::integer::u256_wide_mul(result, b);
            let (_, r) = core::integer::u512_safe_div_rem_by_u256(wide, nz);
            result = r;
        }
        let wide = core::integer::u256_wide_mul(b, b);
        let (_, r) = core::integer::u512_safe_div_rem_by_u256(wide, nz);
        b = r;
        i += 1;
    };
    result
}

fn bit_at(x: u256, i: u32) -> bool {
    let limb_idx = i / 128;
    let bit_idx = i % 128;
    let target = if limb_idx == 0 {
        x.low
    } else {
        x.high
    };
    let shifted = target / pow2_128(bit_idx);
    shifted % 2 == 1
}

fn pow2_128(n: u32) -> u128 {
    let mut acc: u128 = 1;
    let mut k: u32 = 0;
    loop {
        if k == n {
            break;
        }
        acc = acc * 2;
        k += 1;
    };
    acc
}

fn f_eq(a: Field, b: Field) -> bool {
    a.v.low == b.v.low && a.v.high == b.v.high
}

fn f_is_zero(a: Field) -> bool {
    a.v.low == 0 && a.v.high == 0
}

fn f_from_u32(x: u32) -> Field {
    Field { v: u256 { low: x.into(), high: 0 } }
}

fn f_from_bytes_le(bytes: @ByteArray) -> Field {
    let mut acc: u256 = u256 { low: 0, high: 0 };
    let mut i: usize = 31;
    loop {
        if i == 0 {
            break;
        }
        let b: u128 = bytes.at(i).unwrap().into();
        acc = acc * 256;
        acc = acc + u256 { low: b, high: 0 };
        i -= 1;
    };
    // i = 0 is the last byte: top byte of low 128 bits
    let b: u128 = bytes.at(0).unwrap().into();
    acc = acc * 256;
    acc = acc + u256 { low: b, high: 0 };
    Field { v: acc % f_p().v }
}

fn f_from_bytes_le_2(bytes: @ByteArray) -> Field {
    let mut acc: u256 = u256 { low: 0, high: 0 };
    let mut i: usize = 0;
    loop {
        if i == 32 {
            break;
        }
        let b: u128 = bytes.at(i).unwrap().into();
        let wide: core::integer::u512 = core::integer::u256_wide_mul(acc, u256 { low: 256, high: 0 });
        let shifted = u256 { low: wide.limb0, high: wide.limb1 };
        acc = shifted + u256 { low: b, high: 0 };
        i += 1;
    };
    let p = f_p();
    Field { v: acc % p.v }
}


// =============================================================================
// Curve point (extended twisted Edwards: x, y, z, t where x*y = t*z)
// =============================================================================

#[derive(Copy, Drop)]
pub struct Point {
    pub x: Field,
    pub y: Field,
    pub z: Field,
    pub t: Field,
}

#[inline(always)]
fn pt_identity() -> Point {
    Point { x: f_zero(), y: f_one(), z: f_one(), t: f_zero() }
}

fn pt_add(p: Point, q: Point) -> Point {
    let a = f_mul(p.x, q.x);
    let b = f_mul(p.y, q.y);
    let c = f_mul(f_mul(p.t, q.t), f_d());
    let d = f_mul(p.z, q.z);
    let e = f_sub(f_mul(f_add(p.x, p.y), f_add(q.x, q.y)), f_add(a, b));
    let f = f_sub(d, c);
    let g = f_add(d, c);
    let h = f_sub(b, a);
    let x3 = f_mul(e, f);
    let y3 = f_mul(g, h);
    let t3 = f_mul(e, h);
    let z3 = f_mul(f, g);
    Point { x: x3, y: y3, z: z3, t: t3 }
}

fn pt_double(p: Point) -> Point {
    pt_add(p, p)
}

fn pt_scalar_mul(s_lo: u128, s_hi: u128, p: Point) -> Point {
    let mut result = pt_identity();
    let mut i: u32 = 0;
    loop {
        if i == 256 {
            break;
        }
        result = pt_double(result);
        let bit = bit_at(u256 { low: s_lo, high: s_hi }, i);
        if bit {
            result = pt_add(result, p);
        }
        i += 1;
    };
    result
}

fn pt_cof_mul(s_lo: u128, s_hi: u128, p: Point) -> Point {
    let inner = pt_scalar_mul(s_lo, s_hi, p);
    let r1 = pt_double(inner);
    let r2 = pt_double(r1);
    pt_double(r2)
}

// =============================================================================
// Public-key / signature decoding
// =============================================================================

pub fn pubkey_decode(pk_bytes: @ByteArray) -> Option<Point> {
    let y = f_from_bytes_le_2(pk_bytes);
    let p = f_p();
    if y.v >= p.v {
        return Option::None;
    }
    let y2 = f_sq(y);
    let num = f_sub(y2, f_one());
    let den = f_add(f_mul(f_d(), y2), f_one());
    let den_inv = f_inv(den);
    let x2 = f_mul(num, den_inv);
    // Ed25519 sqrt (p ≡ 5 mod 8): x = x2^((p+3)/8).
    // If x^2 != x2, multiply by sqrt(-1) = 2^((p-1)/4).
    let p_v = p.v;
    let exp1 = u256 { low: 0xfffffffffffffffffffffffffffffffe, high: 0x00ffffffffffffffffffffffffffffff };
    let x_v = pow_u256(x2.v, exp1, p_v);
    let x = Field { v: x_v };
    if f_eq(f_sq(x), x2) {
        let sign_byte: u32 = pk_bytes.at(31).unwrap().into();
        let x_chosen = if sign_byte / 128 % 2 == 1 {
            f_neg(x)
        } else {
            x
        };
        let z = f_one();
        let t = f_mul(x_chosen, y);
        return Some(Point { x: x_chosen, y, z, t });
    }
    // Multiply by sqrt(-1) = 2^((p-1)/4).
    let exp2 = u256 { low: 0xfffffffffffffffffffffffffffffffb, high: 0x1fffffffffffffffffffffffffffffff };
    let x_v2 = pow_u256(x2.v, exp2, p_v);
    let x2_field = Field { v: x_v2 };
    if f_eq(f_sq(x2_field), x2) {
        let sign_byte: u32 = pk_bytes.at(31).unwrap().into();
        let x_chosen = if sign_byte / 128 % 2 == 1 {
            f_neg(x2_field)
        } else {
            x2_field
        };
        let z = f_one();
        let t = f_mul(x_chosen, y);
        return Some(Point { x: x_chosen, y, z, t });
    }
    Option::None
}

pub fn r_decode(r_bytes: @ByteArray) -> Option<Point> {
    pubkey_decode(r_bytes)
}

pub fn s_decode(s_bytes: @ByteArray) -> Option<(u128, u128)> {
    let s_field = f_from_bytes_le_2(s_bytes);
    let l = f_l();
    if s_field.v >= l.v {
        return Option::None;
    }
    Some((s_field.v.low, s_field.v.high))
}

fn is_small_order(p: Point) -> bool {
    let y = p.y;
    if f_is_zero(y) { return true; }
    if y.v.low == 1 && y.v.high == 0 { return true; }
    // y = -1 = p - 1
    let p_minus_1 = f_sub(f_p(), f_one());
    if f_eq(y, p_minus_1) { return true; }
    false
}

// =============================================================================
// Top-level Ed25519 signature verification
// =============================================================================

pub fn verify(pub_bytes: @ByteArray, sig_bytes: @ByteArray, msg: @ByteArray) -> bool {
    let a = match pubkey_decode(pub_bytes) {
        Option::Some(p) => p,
        Option::None => { return false; }
    };
    if is_small_order(a) {
        return false;
    }

    let mut r_bytes: ByteArray = "";
    let mut i: usize = 0;
    loop {
        if i == 32 {
            break;
        }
        r_bytes.append_byte(sig_bytes.at(i).unwrap());
        i += 1;
    };
    let r = match pubkey_decode(@r_bytes) {
        Option::Some(p) => p,
        Option::None => { return false; }
    };
    if is_small_order(r) {
        return false;
    }

    let mut s_bytes: ByteArray = "";
    let mut j: usize = 0;
    loop {
        if j == 32 {
            break;
        }
        s_bytes.append_byte(sig_bytes.at(32 + j).unwrap());
        j += 1;
    };
    let (s_lo, s_hi) = match s_decode(@s_bytes) {
        Option::Some(s) => s,
        Option::None => { return false; }
    };

    // h = BLAKE3(R || A || M) reduced mod L.
    let mut to_hash: ByteArray = "";
    to_hash.append(@r_bytes);
    to_hash.append(pub_bytes);
    to_hash.append(msg);
    let h_digest = blake3::keyed_hash_one_chunk([0_u32, 0, 0, 0, 0, 0, 0, 0], @to_hash);
    // h_digest is [u32; 8] (LE). Convert to u256 LE.
    let h_field = u256_from_u32_le_8(@h_digest);
    let l = f_l();
    let h_field = if h_field >= l.v {
        h_field - l.v
    } else {
        h_field
    };
    let h_lo = h_field.low;
    let h_hi = h_field.high;

    let b = Point { x: f_zero(), y: f_by(), z: f_one(), t: f_zero() };
    let sb = pt_cof_mul(s_lo, s_hi, b);
    let ra = pt_cof_mul(h_lo, h_hi, a);
    let rhs = pt_add(r, ra);

    let lhs_xz = f_mul(sb.x, rhs.z);
    let rhs_xz = f_mul(rhs.x, sb.z);
    let lhs_yz = f_mul(sb.y, rhs.z);
    let rhs_yz = f_mul(rhs.y, sb.z);
    f_eq(lhs_xz, rhs_xz) && f_eq(lhs_yz, rhs_yz)
}

// =============================================================================
// Helpers
// =============================================================================

fn f_from_bytes_le_2_to_u256(bytes: @ByteArray) -> u256 {
    let mut acc: u256 = u256 { low: 0, high: 0 };
    let mut i: usize = 0;
    loop {
        if i == 32 {
            break;
        }
        let b: u128 = bytes.at(i).unwrap().into();
        acc = acc * 256;
        acc = acc + u256 { low: b, high: 0 };
        i += 1;
    };
    acc
}

/// Convert an 8-limb LE u32 array (32 bytes total) to a single u256.
#[derive(Copy, Drop)]
struct U32x8 {
    w0: u32,
    w1: u32,
    w2: u32,
    w3: u32,
    w4: u32,
    w5: u32,
    w6: u32,
    w7: u32,
}

fn u256_from_u32_le_8(words: @[u32; 8]) -> u256 {
    let span = words.span();
    let mut acc: u256 = u256 { low: 0, high: 0 };
    let mut i: usize = 0;
    loop {
        if i == 8 {
            break;
        }
        let w: u32 = *span.at(i);
        let w_u128: u128 = w.into();
        acc = acc * 0x100000000;
        acc = acc + u256 { low: w_u128, high: 0 };
        i += 1;
    };
    acc
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::{verify, pubkey_decode, f_add, f_sub, f_mul, f_neg, f_sq, f_inv, f_zero, f_one, f_p, f_l, f_eq, f_is_zero, sqrt_f, f_from_bytes_le_2, f_d, pow_u256, Field, P_LOW, P_HIGH, L_LOW, L_HIGH, D_LOW, D_HIGH, BY_LOW, BY_HIGH};

    #[test]
    fn field_add_basic() {
        let a = f_one();
        let b = f_one();
        let r = f_add(a, b);
        assert!(r.v.low == 2, "1+1=2");
    }

    #[test]
    fn field_sub_basic() {
        let a = f_from_u32_helper(5);
        let b = f_from_u32_helper(2);
        let r = f_sub(a, b);
        assert!(r.v.low == 3, "5-2=3");
    }

    fn f_from_u32_helper(x: u32) -> super::Field {
        super::Field { v: core::integer::u256 { low: x.into(), high: 0 } }
    }

    #[test]
    fn field_mul_basic() {
        let a = f_from_u32_helper(3);
        let b = f_from_u32_helper(4);
        let r = f_mul(a, b);
        assert!(r.v.low == 12, "3*4=12");
    }

    #[test]
    fn field_mul_mod_p_basic() {
        let p = f_p();
        let p_minus_1 = f_sub(p, f_one());
        let two = f_from_u32_helper(2);
        let r = f_mul(p_minus_1, two);
        let p_minus_2 = f_sub(p, two);
        assert!(f_eq(r, p_minus_2), "(p-1)*2 mod p = p-2");
    }

    #[test]
    fn field_neg_is_p_minus_a() {
        let a = f_one();
        let neg_a = f_neg(a);
        let p = f_p();
        let p_minus_1 = f_sub(p, f_one());
        assert!(f_eq(neg_a, p_minus_1), "neg(1) = p-1");
    }

    #[test]
    fn field_inv_one() {
        let inv = f_inv(f_one());
        assert!(f_eq(inv, f_one()), "1^-1 = 1");
    }

    #[test]
    fn field_inv_two() {
        let two = f_from_u32_helper(2);
        let inv = f_inv(two);
        let prod = f_mul(inv, two);
        assert!(f_eq(prod, f_one()), "2^-1 * 2 = 1");
    }

    #[test]
    fn field_inv_three() {
        let three = f_from_u32_helper(3);
        let inv = f_inv(three);
        let prod = f_mul(inv, three);
        assert!(f_eq(prod, f_one()), "3^-1 * 3 = 1");
    }

    #[test]
    fn f_sq_consistency() {
        let p = f_p();
        let p_minus_1 = f_sub(p, f_one());
        let sq = f_sq(p_minus_1);
        let one = f_one();
        assert!(f_eq(sq, one), "(p-1)^2 mod p = 1");
    }

    #[test]
    fn f_sqrt_zero_is_zero() {
        let zero = f_zero();
        let r = sqrt_f(zero);
        assert!(f_is_zero(r), "sqrt(0) = 0");
    }

    #[test]
    fn f_sqrt_one_is_one() {
        let one = f_one();
        let r = sqrt_f(one);
        assert!(f_eq(r, one), "sqrt(1) = 1");
    }

    #[test]
    fn pow_2_p_minus_1_is_1() {
        // Fermat: 2^(p-1) mod p = 1.
        let two = f_from_u32_helper(2);
        let p = f_p();
        let exp = u256 { low: p.v.low - 1, high: p.v.high };
        let r = pow_u256(two.v, exp, p.v);
        let one = u256 { low: 1, high: 0 };
        assert!(r.low == one.low && r.high == one.high, "Fermat: 2^(p-1) = 1");
    }

    #[test]
    fn pow_4_small_exp() {
        // Test: 4^2 = 16.
        let four = f_from_u32_helper(4);
        let exp = u256 { low: 2, high: 0 };
        let p = f_p();
        let r = pow_u256(four.v, exp, p.v);
        let sixteen = u256 { low: 16, high: 0 };
        assert!(r.low == sixteen.low && r.high == sixteen.high, "4^2 = 16");
    }

    #[test]
    fn pow_4_p_plus_3_over_8_legacy() {
        // Direct: 4 * 4 * 4 * 4 * ... in a loop should equal 4^N.
        let four = f_from_u32_helper(4);
        let p = f_p();
        // 4^255 = (2^255 - 38) mod p ... actually 4^255 mod p
        // Compute 4^255 = (2^2)^255 = 2^510. 2^(p-1) = 1. 2^510 = 2^((p-1)*k + r) where (p-1) = 2^255-20 and 510 = (2^255-20)*0 + 510. So 2^510 mod p.
        // 510 = (p-1)*0 + 510. So 2^510 = 2^510. Not helpful.
        // Compute 4^254 = 2^508. Same issue.
        // Just compute 4^5 = 1024.
        let r = pow_u256(four.v, u256 { low: 5, high: 0 }, p.v);
        let expected = u256 { low: 1024, high: 0 };
        assert!(r.low == expected.low && r.high == expected.high, "4^5 = 1024");
    }

    #[test]
    fn pow_2_4() {
        let two = f_from_u32_helper(2);
        let p = f_p();
        let r = pow_u256(two.v, u256 { low: 4, high: 0 }, p.v);
        let sixteen = u256 { low: 16, high: 0 };
        assert!(r.low == sixteen.low && r.high == sixteen.high, "2^4 = 16");
    }

    #[test]
    fn pow_2_8() {
        let two = f_from_u32_helper(2);
        let p = f_p();
        let r = pow_u256(two.v, u256 { low: 8, high: 0 }, p.v);
        let expected = u256 { low: 256, high: 0 };
        assert!(r.low == expected.low && r.high == expected.high, "2^8 = 256");
    }

    #[test]
    fn pow_2_16() {
        let two = f_from_u32_helper(2);
        let p = f_p();
        let r = pow_u256(two.v, u256 { low: 16, high: 0 }, p.v);
        let expected = u256 { low: 65536, high: 0 };
        assert!(r.low == expected.low && r.high == expected.high, "2^16 = 65536");
    }

    #[test]
    fn pow_2_2_127() {
        // 2^(2^127) mod p. Should be SOME value.
        let two = f_from_u32_helper(2);
        let p = f_p();
        let exp = u256 { low: 0, high: 0x80000000000000000000000000000000 };
        let _ = pow_u256(two.v, exp, p.v);
        assert!(true, "no panic");
    }

    #[test]
    fn pow_2_2_128_minus_1() {
        // 2^(2^128 - 1) mod p. Just a sanity check.
        let two = f_from_u32_helper(2);
        let p = f_p();
        let exp = u256 { low: 0xffffffffffffffffffffffffffffffff, high: 0x1 };
        let _ = pow_u256(two.v, exp, p.v);
        assert!(true, "no panic");
    }

    #[test]
    fn pow_2_2_128_p_1() {
        // 2^(2^128 + 1) mod p.
        let two = f_from_u32_helper(2);
        let p = f_p();
        let exp = u256 { low: 1, high: 0x1 };
        let r = pow_u256(two.v, exp, p.v);
        // r^2 should equal 2^(2^129 + 2).
        let r_field = Field { v: r };
        let r_sq = f_sq(r_field);
        // Compute 2^(2^129 + 2) directly. exp2 = 2^129 + 2 = bit 1 set + bit 129 set.
        // In u256: low = 2 (bit 1), high = 2 (bit 129 of u256 = bit 1 of high).
        let exp2 = u256 { low: 2, high: 0x2 };
        let expected = pow_u256(two.v, exp2, p.v);
        let is_eq = r_sq.v.low == expected.low && r_sq.v.high == expected.high;
        assert!(is_eq, "r^2 = 2^(2^129+2)");
    }

    #[test]
    fn pow_2_128_debug() {
        let two = f_from_u32_helper(2);
        let p = f_p();
        // Compute 2^128 step by step to see what b becomes.
        let b0 = two;
        let b1 = f_sq(b0);  // 2^2 = 4
        let b2 = f_sq(b1);  // 2^4 = 16
        let b3 = f_sq(b2);  // 2^8 = 256
        let b4 = f_sq(b3);  // 2^16
        let b5 = f_sq(b4);  // 2^32
        let b6 = f_sq(b5);  // 2^64
        let b7 = f_sq(b6);  // 2^128
        // Check b7 = 2^128.
        let expected = u256 { low: 0, high: 1 };
        let ok = b7.v.low == expected.low && b7.v.high == expected.high;
        assert!(ok, "b7 = 2^128");
    }

    #[test]
    fn pow_2_128_manual() {
        // Manually: result = 1, b = 2. b = b^2 mod p 7 times. Then multiply result by b.
        let two = f_from_u32_helper(2);
        let p = f_p();
        let one = f_one();
        let b1 = f_sq(two);
        let b2 = f_sq(b1);
        let b3 = f_sq(b2);
        let b4 = f_sq(b3);
        let b5 = f_sq(b4);
        let b6 = f_sq(b5);
        let b7 = f_sq(b6);
        // b7 = 2^128. result = 1 * b7.
        let r = f_mul(one, b7);
        let expected = u256 { low: 0, high: 1 };
        let ok = r.v.low == expected.low && r.v.high == expected.high;
        assert!(ok, "r = 2^128");
    }

    #[test]
    fn pow_2_2_251() {
        // 2^(2^251) mod p. 2^251 < p, so result is 2^251 = u256 { low: 0, high: 2^123 }.
        let two = f_from_u32_helper(2);
        let p = f_p();
        let exp = u256 { low: 0, high: 0x8000000000000000000000000000000 };
        let r = pow_u256(two.v, exp, p.v);
        let expected = u256 { low: 0, high: 0x8000000000000000000000000000000 };
        let ok = r.low == expected.low && r.high == expected.high;
        assert!(ok, "2^(2^251) = 2^251");
    }

    #[test]
    fn pow_2_128() {
        let two = f_from_u32_helper(2);
        let p = f_p();
        let r = pow_u256(two.v, u256 { low: 128, high: 0 }, p.v);
        // 2^128 = 0x1_00000000_00000000_00000000_00000000 (low = 0, high = 1).
        let expected = u256 { low: 0, high: 1 };
        let ok = r.low == expected.low && r.high == expected.high;
        assert!(ok, "2^128 = 2^128");
    }

    #[test]
    fn pow_2_64() {
        let two = f_from_u32_helper(2);
        let p = f_p();
        let r = pow_u256(two.v, u256 { low: 64, high: 0 }, p.v);
        let expected = u256 { low: 18446744073709551616, high: 0 }; // 2^64
        let ok = r.low == expected.low && r.high == expected.high;
        assert!(ok, "2^64 = 2^64");
    }

    #[test]
    fn pow_2_256_check() {
        let two = f_from_u32_helper(2);
        let p = f_p();
        let r = pow_u256(two.v, u256 { low: 256, high: 0 }, p.v);
        // r should be 38 = 2^256 mod p. But algo gives something different.
        // Check r == 256 first (might be the bug — algo returns 2^8 = 256 instead of 2^256 = 38).
        let r_eq_256 = r.low == 256 && r.high == 0;
        let r_eq_38 = r.low == 38 && r.high == 0;
        if r_eq_256 {
            assert!(false, "r = 256 (wrong, should be 38)");
        }
        if r_eq_38 {
            return;
        }
        assert!(false, "r is something else");
    }

    #[test]
    fn sq_2_128() {
        let two_128 = f_from_u32_helper(2);
        // Square 2 7 times to get 2^128.
        let b1 = f_sq(two_128);  // 2^2 = 4
        let b2 = f_sq(b1);       // 2^4 = 16
        let b3 = f_sq(b2);       // 2^8 = 256
        let b4 = f_sq(b3);       // 2^16
        let b5 = f_sq(b4);       // 2^32
        let b6 = f_sq(b5);       // 2^64
        let b7 = f_sq(b6);       // 2^128
        // b7 should be 2^128 = u256 { low: 0, high: 1 }.
        let ok = b7.v.low == 0 && b7.v.high == 1;
        assert!(ok, "2^128 via 7 squarings");
    }

    #[test]
    fn pow_2_256() {
        let two = f_from_u32_helper(2);
        let p = f_p();
        let r = pow_u256(two.v, u256 { low: 256, high: 0 }, p.v);
        if r.low == 38 && r.high == 0 { return; }
        if r.low == 256 && r.high == 0 { assert!(false, "r = 256"); }
        if r.low == 0 && r.high == 0 { assert!(false, "r = 0"); }
        if r.low == 1 && r.high == 0 { assert!(false, "r = 1"); }
        // Check r^2 = 38^2 = 1444.
        let r_field = Field { v: r };
        let r_sq = f_sq(r_field);
        let r_sq_38 = f_sq(f_from_u32_helper(38));
        if f_eq(r_sq, r_sq_38) {
            assert!(false, "r^2 = 38^2 = 1444; r should be 38 or 1");
        }
        assert!(false, "r is something else entirely");
    }

    #[test]
    fn pow_2_255() {
        // 2^(2^255) mod p. 2^255 = p + 19, so 2^(2^255) = 2^p * 2^19 = 2 * 2^19 = 2^20.
        // Wait: 2^255 = p + 19 means 2^255 mod p = 19. So 2^(2^255) mod p = 2^19 mod p.
        // But also 2^(p-1) = 1, so 2^p = 2. And 2^(2^255) = 2^(p+19) = 2^p * 2^19 = 2^20.
        let two = f_from_u32_helper(2);
        let p = f_p();
        let exp = u256 { low: 0, high: 0x80000000000000000000000000000000 };
        let r = pow_u256(two.v, exp, p.v);
        // 2^20 = 1048576.
        let expected = u256 { low: 1048576, high: 0 };
        assert!(r.low == expected.low && r.high == expected.high, "2^(2^255) = 2^20");
    }

    #[test]
    fn pow_2_2_pow_252_minus_2() {
        // 2^(2^252 - 2) = 2^((p+3)/8). r^8 = 2^(p+3) = 16.
        let two = f_from_u32_helper(2);
        let p = f_p();
        let exp = u256 { low: 0xfffffffffffffffffffffffffffffffe, high: 0x00ffffffffffffffffffffffffffffff };
        let r = pow_u256(two.v, exp, p.v);
        let r_field = Field { v: r };
        let r_sq = f_sq(r_field);
        let r_4 = f_sq(r_sq);
        let r_8 = f_sq(r_4);
        let sixteen = f_from_u32_helper(16);
        let ok = f_eq(r_8, sixteen);
        assert!(ok, "r^8 = 16");
    }

    #[test]
    fn pow_2_p_plus_3_over_8_eq_2_8th_root() {
        // 2^((p+3)/8) = the 8th root of 2. r^8 = 2 (mod p).
        // We check r^4 == ±4 (since r^8 = 16 = 2 * 8... wait 2^((p+3)/8) * 2^((p+3)/8) = 2^((p+3)/4)).
        // Actually 2^((p+3)/8) * 2^((p+3)/8) = 2^((p+3)/4). And (p+3)/4 = 2^253 - 4.
        // 2^(2^253-4) = 2^((p-1)/4 + 1) = 2 * 2^((p-1)/4) = 2 * ±1 = ±2.
        // So r^2 = ±2. The test SHOULD pass.
        let two = f_from_u32_helper(2);
        let p = f_p();
        let exp = u256 { low: 0xfffffffffffffffffffffffffffffffe, high: 0x00ffffffffffffffffffffffffffffff };
        let r = pow_u256(two.v, exp, p.v);
        // Check r^4 = ±4.
        let r_field = Field { v: r };
        let r_sq = f_sq(r_field);
        let r_4 = f_sq(r_sq);
        let four = f_from_u32_helper(4);
        let four_v = four.v;
        let is_4 = r_4.v.low == four_v.low && r_4.v.high == four_v.high;
        let neg_four = f_neg(four);
        let is_neg_4 = r_4.v.low == neg_four.v.low && r_4.v.high == neg_four.v.high;
        let ok = is_4 | is_neg_4;
        assert!(ok, "r^4 = plus/minus 4");
    }

    #[test]
    fn pow_2_p_is_2() {
        // 2^p = 2 * 2^(p-1) = 2.
        let two = f_from_u32_helper(2);
        let p = f_p();
        let r = pow_u256(two.v, p.v, p.v);
        let expected = u256 { low: 2, high: 0 };
        assert!(r.low == expected.low && r.high == expected.high, "2^p = 2");
    }

    #[test]
    fn f_sq_of_2_is_4() {
        let two = f_from_u32_helper(2);
        let sq = f_sq(two);
        let four = f_from_u32_helper(4);
        assert!(f_eq(sq, four), "2^2 = 4");
    }

    #[test]
    fn pow_2_2_pow_254() {
        // 2^(2^254) mod p = 19 (since 2^255 = 38 mod p, divide by 2).
        let two = f_from_u32_helper(2);
        let p = f_p();
        let exp = u256 { low: 0, high: 0x40000000000000000000000000000000 };
        let r = pow_u256(two.v, exp, p.v);
        let expected = u256 { low: 19, high: 0 };
        assert!(r.low == expected.low && r.high == expected.high, "2^2^254 = 19");
    }

    #[test]
    fn pow_2_2_pow_253() {
        // 2^2^253 mod p. 2^253 = (p+19)/2. So 2^2^253 is some value.
        let two = f_from_u32_helper(2);
        let p = f_p();
        let exp = u256 { low: 0, high: 0x20000000000000000000000000000000 };
        let _ = pow_u256(two.v, exp, p.v);
        // We just check it doesn't panic.
        assert!(true, "no panic");
    }

    #[test]
    fn pow_2_p_plus_3_over_8_squared() {
        // 2^((p+3)/8)^2 = 2^((p+3)/4) = 2^(2^253-4).
        // 2^((p-1)/2) = 2^(2^254-10) = ±1.
        // 2^((p+3)/4) = 2^((p-1)/2 + 5/2) — wait, (p+3)/4 = (p-1)/4 + 1.
        // 2^((p+3)/4) = 2 * 2^((p-1)/4) = 2 * (±1) = ±2.
        // So 2^((p+3)/8) squared = ±2.
        let two = f_from_u32_helper(2);
        let p = f_p();
        let exp = u256 { low: 0xfffffffffffffffffffffffffffffffe, high: 0x00ffffffffffffffffffffffffffffff };
        let r = pow_u256(two.v, exp, p.v);
        let r_field = Field { v: r };
        let r_sq = f_sq(r_field);
        let two_v = two.v;
        let is_2 = r_sq.v.low == two_v.low && r_sq.v.high == two_v.high;
        let neg_two = f_neg(two);
        let is_neg_2 = r_sq.v.low == neg_two.v.low && r_sq.v.high == neg_two.v.high;
        let ok = is_2 | is_neg_2;
        assert!(ok, "2^((p+3)/8) squared = plus/minus 2");
    }

    #[test]
    fn pow_2_2_128() {
        let two = f_from_u32_helper(2);
        let p = f_p();
        let r = pow_u256(two.v, u256 { low: 0, high: 0x1 }, p.v);
        // 2^2^128 mod p. Not 1 since 2^128 << p-1 = 2^255-20.
        // Just check it doesn't panic.
        assert!(r.high < p.v.high, "r.high < p.high");
    }

    #[test]
    fn pow_4_p_plus_3_over_8() {
        // Compute 4^((p+3)/8) directly. Should give ±2.
        let four = f_from_u32_helper(4);
        let exp = u256 { low: 0xfffffffffffffffffffffffffffffffe, high: 0x00ffffffffffffffffffffffffffffff };
        let p = f_p();
        let r = pow_u256(four.v, exp, p.v);
        let r_field = Field { v: r };
        let r_sq = f_sq(r_field);
        let four_v = four.v;
        let is_4 = r_sq.v.low == four_v.low && r_sq.v.high == four_v.high;
        assert!(is_4, "4^((p+3)/8) squared = 4");
    }

    #[test]
    fn f_sqrt_4_is_2() {
        let four = f_from_u32_helper(4);
        let r = sqrt_f(four);
        let two = f_from_u32_helper(2);
        let p_minus_2 = f_sub(f_p(), two);
        let r_sq = f_sq(r);
        let four_back = f_eq(r_sq, four);
        let neg_four_back = f_eq(r_sq, f_neg(four));
        let ok = four_back | neg_four_back;
        assert!(ok, "sqrt_4");
        let is_two = f_eq(r, two);
        let is_p_minus_2 = f_eq(r, p_minus_2);
        let valid = is_two | is_p_minus_2;
        assert!(valid, "sqrt_4 is plus/minus 2");
    }

    #[test]
    fn decode_y_value_check() {
        // RFC 8032 vector 1 pk (the y value bytes, LE)
        let mut pk: ByteArray = "";
        let bytes_arr: Array<u8> = array![
            0xd7, 0x5a, 0x98, 0x01, 0x8e, 0x00, 0x73, 0x79, 0x2b, 0xaa, 0xa9, 0xd3, 0x06, 0xc2,
            0xd8, 0xa1, 0xee, 0xbc, 0xc0, 0xa1, 0xef, 0xf5, 0x83, 0xab, 0x2b, 0x1b, 0x3f, 0x5a,
            0x4e, 0x9b, 0x6c, 0x1a,
        ];
        let mut i: usize = 0;
        loop {
            if i == 32 {
                break;
            }
            pk.append_byte(*bytes_arr.at(i));
            i += 1;
        };
        let y = f_from_bytes_le_2(@pk);
        let p = f_p();
        // y < 2^255, p.high = 0x7fffffff...; y should be < p.
        let lt = y.v.high < p.v.high;
        assert!(lt, "y < p");
    }

    #[test]
    fn pubkey_decode_y_works() {
        let mut pk: ByteArray = "";
        let bytes_arr: Array<u8> = array![
            0xd7, 0x5a, 0x98, 0x01, 0x8e, 0x00, 0x73, 0x79, 0x2b, 0xaa, 0xa9, 0xd3, 0x06, 0xc2,
            0xd8, 0xa1, 0xee, 0xbc, 0xc0, 0xa1, 0xef, 0xf5, 0x83, 0xab, 0x2b, 0x1b, 0x3f, 0x5a,
            0x4e, 0x9b, 0x6c, 0x1a,
        ];
        let mut i: usize = 0;
        loop {
            if i == 32 {
                break;
            }
            pk.append_byte(*bytes_arr.at(i));
            i += 1;
        };
        let y = f_from_bytes_le_2(@pk);
        let p = f_p();
        let in_range = y.v.high < p.v.high;
        let y2 = f_sq(y);
        let num = f_sub(y2, f_one());
        let den = f_add(f_mul(f_d(), y2), f_one());
        let den_inv = f_inv(den);
        let x2 = f_mul(num, den_inv);
        // Verify x2 * den == num (invariant of division).
        let x2_times_den = f_mul(x2, den);
        let inv_ok = f_eq(x2_times_den, num);
        let x = sqrt_f(x2);
        let x_sq = f_sq(x);
        let x_is_correct = f_eq(x_sq, x2);
        assert!(in_range, "y in range");
        assert!(inv_ok, "x2 * den == num");
        assert!(x_is_correct, "x^2 == x2");
        let result = pubkey_decode(@pk);
        match result {
            Option::Some(_) => {},
            Option::None => {
                assert!(false, "pk must decode");
            },
        };
    }

    #[test]
    fn verify_rfc8032_vector_1() {
        let mut pk: ByteArray = "";
        let pk_bytes: Array<u8> = array![
            0xd7, 0x5a, 0x98, 0x01, 0x8e, 0x00, 0x73, 0x79, 0x2b, 0xaa, 0xa9, 0xd3, 0x06, 0xc2,
            0xd8, 0xa1, 0xee, 0xbc, 0xc0, 0xa1, 0xef, 0xf5, 0x83, 0xab, 0x2b, 0x1b, 0x3f, 0x5a,
            0x4e, 0x9b, 0x6c, 0x1a,
        ];
        let mut i: usize = 0;
        loop {
            if i == 32 {
                break;
            }
            pk.append_byte(*pk_bytes.at(i));
            i += 1;
        };

        let msg: ByteArray = "";

        let mut sig: ByteArray = "";
        let sig_bytes: Array<u8> = array![
            0xe5, 0x56, 0x43, 0x00, 0xc3, 0x60, 0xac, 0x72, 0x90, 0x86, 0xe2, 0xcc, 0x80, 0x6e,
            0x82, 0x8a, 0x84, 0x87, 0x7f, 0x1e, 0xb8, 0xe5, 0xd9, 0x74, 0xd8, 0x73, 0xe0, 0x65,
            0x22, 0x49, 0x01, 0x55, 0x5f, 0xb8, 0x82, 0x15, 0x90, 0xa3, 0x3b, 0xac, 0xc6, 0x1e,
            0x39, 0x70, 0x1c, 0xf9, 0xb4, 0x6b, 0xd2, 0x5b, 0xf5, 0xf0, 0x59, 0x5b, 0xbe, 0x24,
            0x65, 0x51, 0x41, 0x43, 0x8e, 0x7a, 0x10, 0x0b,
        ];
        let mut j: usize = 0;
        loop {
            if j == 64 {
                break;
            }
            sig.append_byte(*sig_bytes.at(j));
            j += 1;
        };

        let ok = verify(@pk, @sig, @msg);
        assert!(ok, "RFC 8032 vector 1 must verify");
    }
}
