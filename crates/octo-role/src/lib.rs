//! `octo-role` — CipherOcto role provisioning substrate.
//!
//! Layer B per [[cipherocto-design-principles]].
//! Per RFC-0011-d §7.4 (Substrate `[ADD]` signatures).
//!
//! ## Mission sequence
//!
//! - **M1** (this crate): scaffold only
//! - **M2**: substrate types + `RoleError` + `RoleAction`
//! - **M3**: `octo_role::list` + `octo_role::show` (read paths)
//! - **M4**: `octo_role::select` (write path; SYNC + Stoolap `BEGIN IMMEDIATE`)
//! - **M5**: `OctoRoleBinding` cached projection + `next_nonce_counter`
//!   (lives in `octo-wallet`)
//!
//! Public API surface lands incrementally per the atomic missions above;
//! this crate starts as a stub so dependents can `cargo add` without
//! build break.

#![deny(missing_docs)]
#![deny(rust_2018_idioms)]
#![warn(clippy::all)]

/// Crate version (matches workspace).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");