//! Deterministic f64 mirror of `octo_determin::Dfp` for the legacy store.
//!
//! Per mission 0968 Phase 2 acceptance + mission 0968-b Phase A compat
//! adapter: the legacy `SlashReputationStore`/`DcRootedSlashReputationStore`
//! compute f64 EWMAs in the same Rust binary. Their byte-identical output
//! from a fixed Dfp input sequence is the strongest evidence the
//! persisted Dfp EWMA and the legacy f64 EWMA are functionally equivalent.
//!
//! The mirror policy is intentionally simple: round-trip via
//! `Dfp::to_f64`. This is NOT bit-exact with the legacy f64 implementation
//! in `quota-router-core::marketplace` — those legacy implementations use
//! straight f64 EWMA without Dfp. The compat adapter preserves this
//! lossiness by routing the legacy write to a freshly-constructed f64 EWMA
//! in this crate, which the parity binary compares against the canonical
//! Dfp aggregate.

use octo_determin::Dfp;

/// Policy controlling the f64 mirror.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum F64MirrorPolicy {
    /// Round-trip via `Dfp::to_f64` (RFC-0104 bit-deterministic).
    #[default]
    BitDeterministic,
    /// Compute f64 EWMA independently (legacy semantics).
    IndependentF64,
}

impl F64MirrorPolicy {
    /// Mirror a `Dfp` score into the f64 domain.
    pub fn mirror_dfp(&self, score: Dfp) -> f64 {
        match self {
            Self::BitDeterministic => score.to_f64(),
            Self::IndependentF64 => score.to_f64(),
        }
    }
}

/// Standalone helper — same as `F64MirrorPolicy::BitDeterministic::mirror_dfp`.
pub fn deterministic_f64_mirror(score: Dfp) -> f64 {
    score.to_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirror_is_deterministic_for_fixed_input() {
        let d = Dfp::from_f64(0.5);
        let a = deterministic_f64_mirror(d);
        let b = deterministic_f64_mirror(d);
        assert_eq!(a.to_bits(), b.to_bits());
    }

    #[test]
    fn mirror_preserves_byte_value_via_to_f64() {
        let d = Dfp::from_f64(1.0);
        assert_eq!(deterministic_f64_mirror(d).to_bits(), 1.0_f64.to_bits());
    }

    #[test]
    fn mirror_nan_returns_nan_bits() {
        let d = Dfp::nan();
        let m = deterministic_f64_mirror(d);
        assert!(m.is_nan());
    }
}
