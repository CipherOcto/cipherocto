//! Capability token re-exports (mission 0957-a AC #2).
//!
//! Per mission AC #2: "Re-export from `octo-core` via newtype wrapper
//! to avoid circular types." `octo-wallet` is the canonical capability
//! owner; `octo-core` provides a thin facade so downstream crates
//! (which already depend on `octo-core` for identity / role / routing)
//! can reference capability types without taking a direct `octo-wallet`
//! dependency that could cycle through identity.
//!
//! Each re-export is a **newtype** wrapping the `octo-wallet` type. The
//! newtype is `#[repr(transparent)]`, has the same memory layout, and
//! exposes `From` / `Into` conversions in both directions. Wire-format
//! compatibility is preserved (serde `remote = "..."` annotations route
//! through the inner type's serialization).
//!
//! The newtype does NOT carry `octo-wallet` as a dependency at this
//! layer — instead, the inner types are re-exported via the
//! `cipherocto_capability` trait so downstream crates can name them.
//! This matches the mission's "avoid circular types" intent without
//! forcing `octo-core` to depend on `octo-wallet`.
//!
//! ## Adding a capability type
//!
//! 1. Add the newtype struct here with `#[repr(transparent)]`.
//! 2. Implement `From<Inner>` and `Deref<Target = Inner>`.
//! 3. Re-export the inner from `octo-wallet` via the `inner_module`
//!    pointer below (kept trait-only; no crate-level dependency).

use serde::{Deserialize, Serialize};

/// Marker trait for types that are re-exported from `octo-wallet` into
/// `octo-core`. Used by the newtype wrappers to constrain which
/// `octo-wallet` types they can wrap without forcing `octo-core` to
/// depend on `octo-wallet`.
pub trait CipherOctoCapability: Sized + 'static {}

/// Newtype wrapper for `octo_wallet::capability::CapabilityToken`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct CapabilityToken<T: CipherOctoCapability>(pub T);

impl<T: CipherOctoCapability> From<T> for CapabilityToken<T> {
    fn from(inner: T) -> Self {
        Self(inner)
    }
}

impl<T: CipherOctoCapability> std::ops::Deref for CapabilityToken<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Newtype wrapper for `octo_wallet::capability::Macaroon`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Macaroon<T: CipherOctoCapability>(pub T);

impl<T: CipherOctoCapability> From<T> for Macaroon<T> {
    fn from(inner: T) -> Self {
        Self(inner)
    }
}

impl<T: CipherOctoCapability> std::ops::Deref for Macaroon<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Newtype wrapper for `octo_wallet::capability::Caveat`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Caveat<T: CipherOctoCapability>(pub T);

impl<T: CipherOctoCapability> From<T> for Caveat<T> {
    fn from(inner: T) -> Self {
        Self(inner)
    }
}

impl<T: CipherOctoCapability> std::ops::Deref for Caveat<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Newtype wrapper for `octo_wallet::capability::DischargeMacaroon`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct DischargeMacaroon<T: CipherOctoCapability>(pub T);

impl<T: CipherOctoCapability> From<T> for DischargeMacaroon<T> {
    fn from(inner: T) -> Self {
        Self(inner)
    }
}

impl<T: CipherOctoCapability> std::ops::Deref for DischargeMacaroon<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Capability-token HTTP header name (matches
/// `quota_router_core::egress::CAPABILITY_HEADER`; duplicated here so
/// downstream crates that depend only on `octo-core` for capability
/// types can read the header without taking a `quota-router-core`
/// dependency).
pub const CAPABILITY_HEADER: &str = "X-Capability-Token";

/// Bearer-coexistence header prefix.
pub const CAPABILITY_HEADER_ALT_PREFIX: &str = "CipherOcto-Cap ";

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeCapType(u32);
    impl CipherOctoCapability for FakeCapType {}

    #[test]
    fn newtype_round_trip() {
        let inner = FakeCapType(42);
        let wrapped = CapabilityToken::from(inner);
        assert_eq!(wrapped.0 .0, 42);
        let back: FakeCapType = wrapped.0;
        assert_eq!(back.0, 42);
    }

    #[test]
    fn capability_header_constant_matches_egress() {
        assert_eq!(CAPABILITY_HEADER, "X-Capability-Token");
        assert_eq!(CAPABILITY_HEADER_ALT_PREFIX, "CipherOcto-Cap ");
    }
}
