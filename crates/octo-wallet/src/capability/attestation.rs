//! RFC-0973 attestation surface.
//!
//! The declaration lives in [`octo_cap_macaroon::attestation`] and is
//! re-exported here unchanged. Both paths are valid; this one exists because
//! the dispatch, the registry and the verification wiring are wallet-side.
//!
//! # Why the traits are not declared here
//!
//! A Layer-E strategy crate has to *implement* `AttestationStrategy` and
//! `AttestationAdmission`. If those traits were declared in this crate, every
//! strategy crate would need to depend on the wallet — while the wallet depends
//! on `octo-cap-zk` for its ZK mint substrate, which would close a cycle.
//!
//! Both crates sit below Layer E, so declaring the traits in `octo-cap-macaroon`
//! and re-exporting them here is the one arrangement where the strategy crates,
//! the registry and the verification path all see the same types without any of
//! them depending on a peer.
//!
//! The re-export is total and unconditional. A strategy crate names
//! `octo_cap_macaroon::attestation`; core names this module; both get the same
//! trait objects, so a `dyn AttestationStrategy` is the same type either way.

pub use octo_cap_macaroon::attestation::*;
