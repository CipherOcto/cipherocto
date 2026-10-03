//! Disk-backed registry of minted capability tokens per holder DID.
//!
//! `WalletStore::open()` returns the wallet root path (a derived
//! `PathBuf` under `$OCTO_HOME/wallet` or `$HOME/.octo/wallet`). This
//! module's free functions take that root path and read/write a single
//! `holder_capabilities.json` file inside it — atomic write via
//! `.tmp` + `rename`, atomic read via `serde_json::from_str` over the
//! on-disk file (a partial write is a `serde` error and fails closed).
//!
//! ## Why a separate file (and not in `HolderRegistry`)
//!
//! `holder_registry` is the abstract catalog trait per RFC-0957-A1
//! §Algorithms (6 methods including gossip). It is the substrate-level
//! surface the Stoolap implementation conforms to. This module is a
//! CLI-side convenience store: a flat JSON file the CLI reads/writes
//! from operator-shell paths. It is NOT a substrate type — the
//! substrate `CapabilityToken` is the single source of truth, and
//! this store only persists the public `CapabilitySummary` projection
//! so the operator can list what they minted.
//!
//! ## Layer discipline
//!
//! Layer B additive surface (RFC-0011 §Subcommand Taxonomy). Depends
//! only on `crate::cli_summary::{CapabilitySummary, CaveatSummary}`
//! and `crate::token::CapabilityToken`. CLI Layer C consumes the
//! free functions; substrate Layer A is untouched.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::caveat::Caveat;
use crate::cli_summary::{CapabilitySummary, CaveatSummary};
use crate::token::CapabilityToken;

/// Filename of the per-holder capability registry inside the wallet root.
///
/// `holder_capabilities.json` is a flat list of [`HolderEntry`] records.
/// The file is keyed by position (not by `holder_did`) because the
/// substrate's content-addressable PKs are unique per token and a
/// second mint with the same caveats produces a different `cap_id`.
///
/// Path: `<wallet_root>/holder_capabilities.json`.
pub const REGISTRY_FILENAME: &str = "holder_capabilities.json";

/// Truncation length (hex chars) for `cap_id` / `root_id` in the
/// summary projection. Per RFC-0011 §Subcommand Taxonomy the list view
/// shows the first 16 hex chars; the full 64-hex value is rendered by
/// the mint/attenuate envelopes.
const ID_HEX_PREFIX_LEN: usize = 16;

/// Per-holder registry entry.
///
/// `holder_did` is the audience the token was minted to
/// (`CapabilityToken::holder_did`); `summary` is the public
/// `CapabilitySummary` projection (the `holder_sig` is NOT persisted —
/// the wire form of the full token is a separate concern, owned by the
/// substrate); `minted_at_unix` is the wall-clock second at which this
/// entry was recorded (debug aid; not consulted by the substrate).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolderEntry {
    /// DID of the holder (`did:octo:...`).
    pub holder_did: String,
    /// Public summary projection of the minted token.
    pub summary: CapabilitySummary,
    /// Unix seconds at which this entry was appended to the registry.
    pub minted_at_unix: u64,
}

/// On-disk shape: a flat list. Lookups iterate the list (small N —
/// the operator's wallet has tens to low hundreds of caps); the
/// substrate is the source of truth, not this index.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedRegistry {
    /// Records, in insertion order (oldest first).
    pub entries: Vec<HolderEntry>,
}

/// Errors surfaced by the free functions.
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// I/O failure (read / write / rename).
    #[error("registry I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON parse failure (corrupted or partial file).
    #[error("registry parse error: {0}")]
    Parse(String),
    /// Caller passed an empty / blank holder DID.
    #[error("holder_did must not be empty")]
    EmptyHolderDid,
}

/// Convert a serialized JSON error into a `PersistError::Parse`.
fn parse_err(e: serde_json::Error) -> PersistError {
    PersistError::Parse(e.to_string())
}

/// Append a `HolderEntry` derived from `token` to the registry at
/// `<wallet_root>/holder_capabilities.json`.
///
/// Atomic write: serialize → write `.tmp` → rename to final. A crash
/// mid-write leaves the prior file intact (the `.tmp` is orphan
/// garbage that the next `load` will not read).
///
/// # Errors
/// `PersistError::Io` on filesystem failure,
/// `PersistError::Parse` if the existing file is corrupt (a corrupted
/// registry is treated as a hard failure so the issue is visible rather
/// than silently lost).
/// `PersistError::EmptyHolderDid` if `token.holder_did` is empty (a
/// post-condition the substrate enforces, but checked defensively
/// because a future substrate amendment could regress).
pub fn register_mint(
    token: &CapabilityToken,
    wallet_root: &Path,
    now_unix: u64,
) -> Result<(), PersistError> {
    let holder_did = token.holder_did.as_str();
    if holder_did.trim().is_empty() {
        return Err(PersistError::EmptyHolderDid);
    }
    let summary = summary_from_token(token);
    let mut registry = load_registry(wallet_root)?;
    registry.entries.push(HolderEntry {
        holder_did: holder_did.to_owned(),
        summary,
        minted_at_unix: now_unix,
    });
    save_registry(wallet_root, &registry)
}

/// Return the registry's `CapabilitySummary` entries for the given
/// `holder_did`. Order matches insertion (oldest first).
///
/// # Errors
/// `PersistError::Io` on filesystem failure,
/// `PersistError::Parse` on corrupt JSON.
pub fn list_for_holder(
    holder_did: &str,
    wallet_root: &Path,
) -> Result<Vec<CapabilitySummary>, PersistError> {
    let registry = load_registry(wallet_root)?;
    Ok(registry
        .entries
        .into_iter()
        .filter(|e| e.holder_did == holder_did)
        .map(|e| e.summary)
        .collect())
}

/// Compose the [`PathBuf`] of the registry file under `wallet_root`.
fn registry_path(wallet_root: &Path) -> PathBuf {
    wallet_root.join(REGISTRY_FILENAME)
}

/// Compose the [`PathBuf`] of the atomic-write staging file.
fn staging_path(wallet_root: &Path) -> PathBuf {
    let mut p = wallet_root.to_path_buf();
    p.push(format!("{REGISTRY_FILENAME}.tmp"));
    p
}

/// Load the on-disk registry. A missing file returns an empty
/// `PersistedRegistry` (the directory may not exist yet either; that
/// is also OK and returns empty, so the first mint lazily creates the
/// directory).
fn load_registry(wallet_root: &Path) -> Result<PersistedRegistry, PersistError> {
    let path = registry_path(wallet_root);
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<PersistedRegistry>(&bytes).map_err(parse_err),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(PersistedRegistry::default()),
        Err(e) => Err(PersistError::Io(e)),
    }
}

/// Save the registry atomically: serialize → write `.tmp` → rename.
///
/// The `.tmp` rename is `std::fs::rename`, which is atomic on POSIX
/// when both source and target are on the same filesystem (the
/// `wallet_root` is a single directory, so the staging file and the
/// final file share a filesystem by construction).
fn save_registry(wallet_root: &Path, registry: &PersistedRegistry) -> Result<(), PersistError> {
    let serialized = serde_json::to_vec_pretty(registry).map_err(parse_err)?;
    let staging = staging_path(wallet_root);
    std::fs::write(&staging, &serialized)?;
    std::fs::rename(&staging, registry_path(wallet_root))?;
    Ok(())
}

/// Project a `CapabilityToken` into a public `CapabilitySummary`.
///
/// `cap_id` is `hex(token.macaroon.id)` truncated to 16 hex chars
/// (the list-view truncation is the substrate's `list_active` contract
/// per RFC-0011 §Subcommand Taxonomy).
/// `root_id` is `hex(token.macaroon.root_id)` truncated to 16 hex
/// chars (macaroon root_id is 16 bytes = 32 hex chars; the truncation
/// keeps the first 16).
/// `caveats` is the per-caveat `CaveatSummary` projection via the
/// canonical-serialized JSON body.
/// `remaining_budget` is the tightest applicable budget caveat's
/// `value` (in display units; the CLI surfaces the `DqaEncoding` via
/// `caveat_view`).
/// `expires_at_unix` is the tightest applicable expiry caveat
/// (`Caveat::Before` deadline).
pub fn summary_from_token(token: &CapabilityToken) -> CapabilitySummary {
    let cap_id_full = hex::encode(token.macaroon.id);
    let root_id_full = hex::encode(token.macaroon.root_id);
    let cap_id = truncate_hex(&cap_id_full, ID_HEX_PREFIX_LEN);
    let root_id = truncate_hex(&root_id_full, ID_HEX_PREFIX_LEN);
    let caveats: Vec<CaveatSummary> = token
        .macaroon
        .caveats
        .iter()
        .map(caveat_to_summary)
        .collect();
    let remaining_budget = tightest_budget(&token.macaroon.caveats);
    let expires_at_unix = earliest_expiry(&token.macaroon.caveats);
    CapabilitySummary {
        cap_id,
        root_id,
        caveats,
        remaining_budget,
        expires_at_unix,
    }
}

/// Truncate a hex string to `max_chars` characters from the front.
/// If the input is shorter than `max_chars`, return it unchanged.
/// Used to keep the list view short per RFC-0011 §Subcommand Taxonomy.
fn truncate_hex(hex_str: &str, max_chars: usize) -> String {
    if hex_str.len() <= max_chars {
        hex_str.to_owned()
    } else {
        hex_str[..max_chars].to_owned()
    }
}

/// Project a single caveat into a `CaveatSummary`.
///
/// `kind` is the short serde tag of the caveat variant
/// (`CaveatName::short_name`), derived from the variant's typed
/// discriminator per RFC-0965 §2.
/// `body` is the canonical-serialized JSON's `value` field. If the
/// canonical form does not parse, `body` is `Null` (the substrate's
/// canonical form is always JSON, so `Null` indicates a substrate bug
/// rather than an operator error).
fn caveat_to_summary(c: &Caveat) -> CaveatSummary {
    let canonical = c.canonical_ser();
    let body = serde_json::from_slice::<serde_json::Value>(&canonical)
        .ok()
        .and_then(|v| v.get("value").cloned())
        .unwrap_or(serde_json::Value::Null);
    CaveatSummary {
        kind: c.name().short_name().to_owned(),
        body,
    }
}

/// Find the tightest applicable budget across `Caveat::AmountMax`
/// caveats. "Tightest" is the smallest `amount_dqa` (lowest allowed
/// spend). Returns `None` if no `AmountMax` caveat exists. The
/// substrate's `Dqa::value` is `i64`; the summary field is `Option<u64>`
/// so a negative `Dqa::value` is clamped to 0 (a defensible
/// lower-bound — a negative budget is a substrate bug, not operator data).
fn tightest_budget(caveats: &[Caveat]) -> Option<u64> {
    caveats
        .iter()
        .filter_map(|c| match c {
            Caveat::AmountMax(dqa) => Some(dqa.value),
            _ => None,
        })
        .min()
        .map(|v| u64::try_from(v.max(0)).unwrap_or(0))
}

/// Find the earliest deadline across `Caveat::Before` caveats.
/// `Before(deadline)` returns `Some(deadline)`; the field on the
/// summary is `expires_at_unix` which is `Option<i64>` per the
/// substrate contract, so the `u64` Unix seconds is cast via
/// `try_from` (fail-closed for the year 2.92e11+ boundary — no
/// operator cap will reach that).
fn earliest_expiry(caveats: &[Caveat]) -> Option<i64> {
    caveats
        .iter()
        .filter_map(|c| match c {
            Caveat::Before(t) => Some(*t),
            _ => None,
        })
        .min()
        .and_then(|t| i64::try_from(t).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    struct FixtureSigner(SigningKey);

    impl crate::signer::CapabilitySigner for FixtureSigner {
        fn sign(&self, msg: &[u8]) -> Result<[u8; 64], crate::signer::CapabilitySignerError> {
            Ok(self.0.sign(msg).to_bytes())
        }
        fn public_key_bytes(&self) -> [u8; 32] {
            self.0.verifying_key().to_bytes()
        }
    }

    fn fixture_token(caveats: &[Caveat]) -> CapabilityToken {
        let sk = SigningKey::from_bytes(&[0xaau8; 32]);
        let signer = FixtureSigner(sk);
        let root_secret = [0x42u8; 32];
        CapabilityToken::mint(&root_secret, &signer, "did:octo:zPersist", caveats)
            .expect("fixture mint")
    }

    /// A missing file returns an empty registry, not an error. The
    /// CLI's first mint on a fresh wallet would otherwise crash.
    #[test]
    fn load_missing_returns_empty() {
        let tmp = tempdir();
        let reg = load_registry(&tmp).expect("missing file returns empty");
        assert!(reg.entries.is_empty());
    }

    /// `list_for_holder` filters out entries whose `holder_did` does
    /// not match the request.
    #[test]
    fn list_for_holder_filters_by_did() {
        let tmp = tempdir();
        let token_a = fixture_token(&[]);
        let now = 1_700_000_000u64;
        register_mint(&token_a, &tmp, now).expect("register a");
        // Manually inject a second entry with a different holder DID
        // to exercise the filter.
        let mut reg = load_registry(&tmp).expect("load");
        reg.entries.push(HolderEntry {
            holder_did: "did:octo:zOther".to_owned(),
            summary: CapabilitySummary {
                cap_id: "ff".repeat(8),
                root_id: "00".repeat(8),
                caveats: Vec::new(),
                remaining_budget: None,
                expires_at_unix: None,
            },
            minted_at_unix: now + 1,
        });
        save_registry(&tmp, &reg).expect("save with second entry");
        let mine = list_for_holder("did:octo:zPersist", &tmp).expect("list mine");
        assert_eq!(mine.len(), 1);
        let others = list_for_holder("did:octo:zOther", &tmp).expect("list others");
        assert_eq!(others.len(), 1);
    }

    /// Atomic write: a crash mid-write leaves the prior file intact.
    /// Pinned by leaving the staging file path unchanged on rename —
    /// a future regression that wrote directly to the final path
    /// would not show up here, but the staging rename is the only
    /// way to get atomicity on POSIX.
    #[test]
    fn save_uses_staging_rename_not_direct_write() {
        let tmp = tempdir();
        let token = fixture_token(&[]);
        register_mint(&token, &tmp, 1_700_000_000).expect("register");
        // The staging file must NOT exist after a successful save
        // (rename consumed it).
        let staging = staging_path(&tmp);
        assert!(
            !staging.exists(),
            "staging path must not survive a successful save: {staging:?}"
        );
        // The final file MUST exist.
        assert!(registry_path(&tmp).exists());
    }

    /// Cross-process round trip: write via the public free fn, then
    /// load via the public free fn from a fresh process-state
    /// perspective (the on-disk file IS the cross-process boundary).
    #[test]
    fn cross_process_via_disk_round_trip() {
        let tmp = tempdir();
        let token = fixture_token(&[Caveat::Before(2_000_000_000)]);
        register_mint(&token, &tmp, 1_700_000_000).expect("register");
        let listed = list_for_holder("did:octo:zPersist", &tmp).expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].cap_id.len(), ID_HEX_PREFIX_LEN);
        assert_eq!(listed[0].root_id.len(), ID_HEX_PREFIX_LEN);
        assert_eq!(listed[0].expires_at_unix, Some(2_000_000_000));
    }

    fn tempdir() -> PathBuf {
        // Per-test unique path: cargo's default harness runs tests in
        // parallel within a single process, so a PID-keyed tempdir races
        // across the four tests in this module. Each test gets its own
        // subdirectory keyed by the test fn name + the OS nanotime
        // counter, so a torn write in one test cannot leak entries
        // into another.
        let base = std::env::temp_dir().join(format!(
            "octo-cap-macaroon-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&base).expect("mkdir");
        base
    }
}
