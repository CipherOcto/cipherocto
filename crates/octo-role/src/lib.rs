//! `octo-role` — CipherOcto role provisioning substrate.
//!
//! Layer B per [[cipherocto-design-principles]].
//! Per RFC-0011-d §7.4 (Substrate `[ADD]` signatures).
//!
//! ## Mission sequence
//!
//! - **M1** (landed): crate scaffold
//! - **M2** (landed): substrate types + `RoleError` + `RoleAction`
//! - **M3**: `octo_role::list` + `octo_role::show` (read paths)
//! - **M4**: `octo_role::select` (write path; SYNC + Stoolap `BEGIN IMMEDIATE`)
//! - **M5**: `OctoRoleBinding` cached projection + `next_nonce_counter`
//!   (lives in `octo-wallet`)

#![deny(missing_docs)]
#![deny(rust_2018_idioms)]
#![warn(clippy::all)]

pub mod action;
pub mod error;
pub mod list_show;
pub mod registry;
pub mod types;

/// Crate version (matches workspace).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use action::RoleAction;
pub use error::RoleError;
pub use list_show::{list, show};
pub use types::{
    ChainId, Hash32, RoleBinding, RoleFilter, RoleKindUuid, RoleRecord, RoleSummary, SlashingRule,
};