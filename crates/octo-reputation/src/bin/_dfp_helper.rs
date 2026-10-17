//! Helper binary: print the 24-byte Dfp wire form of an f64 score.
//! Built ad-hoc by scripts/verify_canonical_blobs.py for cross-impl
//! canonical_blobs verification. Not part of the production binary
//! surface.

use octo_determin::Dfp;
use octo_reputation::types::dfp_to_blob;

fn main() {
    let arg = std::env::args().nth(1).expect("score arg");
    let score: f64 = arg.parse().expect("f64 parse");
    let dfp = Dfp::from_f64(score);
    let bytes = dfp_to_blob(&dfp);
    for b in bytes {
        print!("{:02x}", b);
    }
    println!();
}
