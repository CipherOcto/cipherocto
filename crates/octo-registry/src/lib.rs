//! CipherOcto Registry
//!
//! Proto-blockchain state management for local development.
//!
//! Stores:
//! - User identity
//! - Selected role
//! - Agent registry
//! - Local state persistence
//!
//! In full implementation, this becomes blockchain state.

use anyhow::Result;
use sled::Db;
use std::path::PathBuf;

pub const REGISTRY_DIR: &str = ".octo";
pub const IDENTITY_FILE: &str = "identity.json";
pub const ROLE_KEY: &[u8] = b"user_role";

/// Initialize the local registry
pub fn init() -> Result<()> {
    let registry_dir = PathBuf::from(REGISTRY_DIR);
    std::fs::create_dir_all(&registry_dir)?;

    let db = open_db()?;
    let identity_id = uuid::Uuid::new_v4();

    // Store identity
    db.insert(IDENTITY_FILE.as_bytes(), identity_id.as_bytes_as_slice())?;

    println!("📁 Registry initialized at: {}", registry_dir.display());
    Ok(())
}

/// Open the registry database
fn open_db() -> Result<Db> {
    let registry_dir = PathBuf::from(REGISTRY_DIR);
    sled::open(registry_dir).map_err(Into::into)
}

/// Get the user's identity
pub fn get_identity() -> Option<String> {
    let db = open_db().ok()?;
    db.get(IDENTITY_FILE.as_bytes())
        .ok()?
        .map(|bytes| String::from_utf8(bytes.to_vec()).ok())
        .flatten()
}

/// Set the user's ecosystem role
pub fn set_role(role: &str) -> Result<()> {
    let db = open_db()?;
    db.insert(ROLE_KEY, role.as_bytes())?;
    Ok(())
}

/// Get the user's ecosystem role
pub fn get_role() -> Option<String> {
    let db = open_db().ok()?;
    db.get(ROLE_KEY)
        .ok()?
        .map(|bytes| String::from_utf8(bytes.to_vec()).ok())
        .flatten()
}
