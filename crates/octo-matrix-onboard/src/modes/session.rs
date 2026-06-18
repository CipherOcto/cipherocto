//! Multi-account session-store subcommands (mission 0850h-d).
//!
//! Four subcommands, all operating on the stoolap-backed store
//! (`octo-matrix-session-store`):
//!
//! - `session list` — print all sessions, ordered by insertion
//!   position. Each row includes user_id, device_id, homeserver,
//!   login type, and a redacted token preview.
//! - `session use <user_id> <device_id>` — mark a session as the
//!   most-recently-used (`set_latest_session`). Updates
//!   `last_used` only; never changes `position`.
//! - `session remove <user_id> <device_id>` — drop a session from
//!   the store. Refuses when the row is missing.
//! - `session import <file>` — read a legacy 0850h-a / 0850h-c JSON
//!   config and insert a row. Refuses to overwrite an existing
//!   `(user_id, device_id)` unless `--force` is set.

use crate::cli::{SessionImportArgs, SessionListArgs, SessionRemoveArgs, SessionUseArgs};
use crate::error::{OnboardError, Result};
use crate::logging::format_rfc3339_secs;
use octo_matrix_session_store::{
    default_store_path, now_epoch, LoginType, SessionRow, SessionStore, SessionStoreError,
    StoolapSessionStore,
};
use std::path::PathBuf;

/// R20-L1: `From<LoginTypeArg> for LoginType`. The CLI's
/// `LoginTypeArg` enum (in `crate::cli`) is a clap-friendly mirror
/// of the store's `LoginType` (in `octo-matrix-session-store`).
/// The conversion is mechanical (each variant maps 1:1 to its
/// store-side counterpart), so a `From` impl is the single
/// source of truth. Previously the call site in
/// `session::import::login_type_match` was a hand-coded 4-arm
/// `match` that had to be updated alongside the enums whenever
/// a new variant was added. The `From` impl makes that
/// maintenance hazard go away: a future 5th variant (e.g.,
/// `MagicLink`) would be a one-line change here, and the call
/// site would continue to work via `.into()`.
impl From<crate::cli::LoginTypeArg> for LoginType {
    fn from(arg: crate::cli::LoginTypeArg) -> Self {
        match arg {
            crate::cli::LoginTypeArg::Password => LoginType::Password,
            crate::cli::LoginTypeArg::Oidc => LoginType::Oidc,
            crate::cli::LoginTypeArg::Sso => LoginType::Sso,
            crate::cli::LoginTypeArg::Qr => LoginType::Qr,
        }
    }
}

/// Open the store at the operator-specified path, or at the
/// per-platform default when `--store` is not set.
fn open_store(path: Option<&PathBuf>) -> Result<StoolapSessionStore> {
    let resolved = match path {
        Some(p) if !p.as_os_str().is_empty() => p.clone(),
        _ => default_store_path().map_err(|e| {
            OnboardError::BadConfig(format!(
                "{e} — pass --store <path> to specify the location explicitly"
            ))
        })?,
    };
    StoolapSessionStore::new(&resolved)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("open store: {}", e)))
}

/// Redact a token for display: show the first ≤8 bytes (walked
/// back to a char boundary so a non-ASCII token doesn't panic)
/// and `***`. R10-L1: a previous version of this docstring
/// said "first 8 characters", but the implementation is
/// byte-based (with the R6-M2 char-boundary walk); for a
/// non-ASCII token the byte count is less than the char count,
/// so the docstring is now phrased in bytes to match the
/// impl. The cross-reference below at the `logging.rs` site
/// uses the same `first ≤8 bytes` phrasing.
///
/// R5-L1: this is one of FOUR `redact_*` implementations across
/// the four mission crates. Each site has a deliberately
/// different format policy because each display context calls
/// for a different balance of brevity and operator-recognizability:
///
/// - `crates/octo-matrix-onboard/src/modes/session.rs` (THIS FILE)
///   — tabular `session list` output. Uses a compact 2-tier form
///   (first8*** / ***) that keeps the column width predictable.
///   Unlike the other three sites, this one does NOT show the
///   tail of the token, because the rows are aligned in a
///   multi-account table and the tail would be cut off by the
///   column width. R6-M2 fixed the byte-slicing to walk back to
///   a char boundary (R2-H2 missed this site when fixing the
///   adapter copy).
/// - `crates/octo-adapter-matrix-sdk/src/lib.rs:redact_token` — free-form
///   diagnostic output (error messages, debug logs). Char-based
///   slicing so a non-ASCII token gets the first 8 / last 4 CHARS.
///   3-tier shape: `first8...last4` / `all***` / `***`.
/// - `crates/octo-matrix-onboard-core/src/lib.rs:redact_token` — the
///   one-time "logged in" confirmation message
///   (`Session::access_token_preview`). 2-tier shape:
///   `first8...last4` / `first4...`.
/// - `crates/octo-matrix-onboard/src/logging.rs:redact_value` —
///   tracing-subscriber `FormatEvent` redaction. Char-boundary-
///   walked byte slice (the only site that walks back, so a
///   4S recovery key with non-ASCII bytes can't panic).
///   Shape: `first ≤8 bytes + ***` / `***`.
///
/// If you change this implementation, audit the other three for
/// consistency. The per-site policies are intentional; the
/// cross-reference is the missing piece a future maintainer
/// needs to avoid silent divergence.
fn redact_token(token: &str) -> String {
    if token.len() > 8 {
        // R6-M2: the previous shape was `&token[..8]`, which
        // byte-slices. If byte 8 falls in the middle of a
        // multi-byte UTF-8 codepoint, the slice panics with
        // "byte index N is not a char boundary". The adapter
        // copy at `lib.rs:redact_token` was fixed in R2-H2 to use
        // char-based slicing, and the logging copy at
        // `logging.rs:redact_value` walks back to a char boundary on
        // the same byte slice. This site was missed; the fix
        // matches the logging copy (the format is identical
        // — first 8 + ***).
        let mut end = 8.min(token.len());
        while end > 0 && !token.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}***", &token[..end])
    } else {
        "***".to_string()
    }
}

fn epoch_to_iso(epoch: i64) -> String {
    // R3-L1: produce real RFC 3339 (`YYYY-MM-DDTHH:MM:SSZ`) so the
    // `LAST_USED` column can be parsed by `date -d` and other RFC
    // 3339 tools. The previous implementation used `format!("{:?}",
    // SystemTime)` which renders as Rust's debug format (e.g.
    // `SystemTime { tv_sec: …, tv_nsec: … }`), not ISO 8601 — the
    // column claimed to be ISO but wasn't. Defer to the shared
    // helper in `logging.rs` (`epoch_days_to_ymd` does the heavy
    // lifting) so there's exactly one date-formatting code path
    // in the crate.
    format_rfc3339_secs(epoch)
}

pub async fn list(args: SessionListArgs) -> Result<()> {
    let store = open_store(args.store.store.as_ref())?;
    let sessions: Vec<SessionRow> = store
        .get_all_sessions()
        .await
        .map_err(|e: SessionStoreError| OnboardError::Generic(anyhow::anyhow!("list: {}", e)))?;
    if sessions.is_empty() {
        eprintln!("(no sessions in the store)");
        return Ok(());
    }
    // R2-M11: compute column widths from the actual data so a
    // long user_id / homeserver URL doesn't get silently
    // truncated. We pad each value to the max of its column's
    // data width and the header width, then use `eprintln!` with
    // the same widths for alignment. (Adding `comfy-table` would
    // be heavier than this — 4 lines of code, no new dep.)
    let header = [
        "POS",
        "USER_ID",
        "DEVICE_ID",
        "HOMESERVER",
        "TYPE",
        "LOGIN_AGE",
    ];
    let width_user = sessions
        .iter()
        .map(|s| s.user_id.chars().count())
        .max()
        .unwrap_or(0)
        .max(header[1].len());
    let width_device = sessions
        .iter()
        .map(|s| s.device_id.chars().count())
        .max()
        .unwrap_or(0)
        .max(header[2].len());
    let width_homeserver = sessions
        .iter()
        .map(|s| s.homeserver_url.chars().count())
        .max()
        .unwrap_or(0)
        .max(header[3].len());
    eprintln!(
        "{:<4} {:<wu$} {:<wd$} {:<wh$} {:<10} {:<12} LAST_USED",
        header[0],
        header[1],
        header[2],
        header[3],
        header[4],
        header[5],
        wu = width_user,
        wd = width_device,
        wh = width_homeserver,
    );
    for s in &sessions {
        // R1-M16: LOGIN_AGE is "time since the store's recorded
        // `login_timestamp`". For sessions added via `session import`
        // the legacy 0850h-a / 0850h-c config didn't carry a
        // timestamp, so the store overwrites it to `now_epoch()` at
        // import time. The column therefore reports the time since
        // import, not the time since the original login.
        let age_label = if s.login_timestamp == 0 {
            "unknown".to_string()
        } else {
            format!("{}s", now_epoch().saturating_sub(s.login_timestamp))
        };
        eprintln!(
            "{:<4} {:<wu$} {:<wd$} {:<wh$} {:<10} {:<12} {}",
            s.position,
            s.user_id,
            s.device_id,
            s.homeserver_url,
            s.login_type.as_str(),
            age_label,
            epoch_to_iso(s.last_used),
            wu = width_user,
            wd = width_device,
            wh = width_homeserver,
        );
        eprintln!(
            "     access_token: {}  refresh_token: {}",
            redact_token(&s.access_token),
            s.refresh_token
                .as_deref()
                .map(redact_token)
                .unwrap_or_else(|| "<none>".to_string()),
        );
    }
    Ok(())
}

pub async fn use_(args: SessionUseArgs) -> Result<()> {
    let store = open_store(args.store.store.as_ref())?;
    store
        .set_latest_session(&args.user_id, &args.device_id)
        .await
        .map_err(|e: SessionStoreError| {
            OnboardError::Generic(anyhow::anyhow!(
                "set latest {} / {}: {}",
                args.user_id,
                args.device_id,
                e
            ))
        })?;
    eprintln!(
        "Marked {} / {} as the most-recently-used session (last_used updated; position unchanged).",
        args.user_id, args.device_id
    );
    Ok(())
}

pub async fn remove(args: SessionRemoveArgs) -> Result<()> {
    let store = open_store(args.store.store.as_ref())?;
    store
        .remove_session(&args.user_id, &args.device_id)
        .await
        .map_err(|e: SessionStoreError| match e {
            SessionStoreError::NotFound { .. } => OnboardError::Generic(anyhow::anyhow!(
                "no session for {} / {}",
                args.user_id,
                args.device_id
            )),
            other => OnboardError::Generic(anyhow::anyhow!("remove: {}", other)),
        })?;
    eprintln!("Removed session {} / {}.", args.user_id, args.device_id);
    Ok(())
}

pub async fn import(args: SessionImportArgs) -> Result<()> {
    let store = open_store(args.store.store.as_ref())?;

    // Read the on-disk JSON directly. The on-disk shape is
    // `homeserver_url / user_id / device_id / access_token /
    // refresh_token / rooms` (see `octo-adapter-matrix-sdk::config_writer::OnDiskConfig`).
    // We deliberately do NOT try to `restore_session` here — that
    // would dial the homeserver, which has no value for a pure
    // import. The field-level checks below catch malformed JSON
    // and missing fields; the e2ee subcommands do the network
    // validation when they actually need the SDK.
    let bytes = std::fs::read(&args.file)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("read {:?}: {}", args.file, e)))?;
    let on_disk: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("parse {:?}: {}", args.file, e)))?;

    let homeserver_url = on_disk["homeserver_url"]
        .as_str()
        .ok_or_else(|| OnboardError::Generic(anyhow::anyhow!("missing homeserver_url")))?
        .to_string();
    let user_id = on_disk["user_id"]
        .as_str()
        .ok_or_else(|| OnboardError::Generic(anyhow::anyhow!("missing user_id")))?
        .to_string();
    let device_id = on_disk["device_id"]
        .as_str()
        .ok_or_else(|| OnboardError::Generic(anyhow::anyhow!("missing device_id")))?
        .to_string();
    let access_token = on_disk["access_token"]
        .as_str()
        .ok_or_else(|| OnboardError::Generic(anyhow::anyhow!("missing access_token")))?
        .to_string();
    let refresh_token = on_disk["refresh_token"].as_str().map(str::to_string);

    // R2-M10: the operator can now set the login type via
    // `--login-type` (the legacy JSON does not carry one). Default
    // is still `Password` for back-compat, but OIDC / SSO / QR
    // operators should set the flag to avoid a misleading `password`
    // label in `session list`.
    //
    // R20-L1: replace the previous 4-arm `match` with a
    // `From<LoginTypeArg> for LoginType` `.into()` call. The match
    // was hand-coded and would have to be updated alongside
    // `LoginTypeArg` / `LoginType` whenever a 5th variant is added
    // (e.g., a hypothetical `MagicLink` flow). The `From` impl is
    // the single source of truth for the CLI↔store mapping.
    let login_type: LoginType = args.login_type.into();
    let row = SessionRow {
        user_id: user_id.clone(),
        device_id: device_id.clone(),
        homeserver_url,
        access_token,
        refresh_token,
        login_type,
        login_timestamp: 0,
        last_used: 0,
        position: 0,
        display_name: None,
        avatar_url: None,
    };
    store
        .add_session(&row, args.force)
        .await
        .map_err(|e: SessionStoreError| match e {
            SessionStoreError::AlreadyExists { .. } => OnboardError::Generic(anyhow::anyhow!(
                "session already exists for {} / {} (pass --force to overwrite)",
                user_id,
                device_id
            )),
            other => OnboardError::Generic(anyhow::anyhow!("import: {}", other)),
        })?;
    eprintln!(
        "Imported session {} / {} from {:?}.",
        user_id, device_id, args.file
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::redact_token;

    /// R6-M2 regression test: a long ASCII token gets the
    /// "first 8 + ***" form.
    #[test]
    fn redact_token_ascii_long() {
        let r = redact_token("syt_abcdefgh_long_token_xyz");
        assert_eq!(r, "syt_abcd***", "got: {r}");
    }

    /// R6-M2 regression test: a token of exactly 8 chars is
    /// treated as "short" and returns "***".
    #[test]
    fn redact_token_at_boundary() {
        let r = redact_token("12345678");
        assert_eq!(r, "***", "got: {r}");
    }

    /// R6-M2 regression test: a token of 9 chars gets the
    /// "first 8 + ***" form.
    #[test]
    fn redact_token_one_above_boundary() {
        let r = redact_token("123456789");
        assert_eq!(r, "12345678***", "got: {r}");
    }

    /// R6-M2 regression test: an empty token returns "***".
    #[test]
    fn redact_token_empty() {
        assert_eq!(redact_token(""), "***");
    }

    /// R6-M2 regression test: a token where byte 8 falls inside
    /// a multi-byte UTF-8 codepoint must NOT panic. The fix
    /// walks byte 8 back to the nearest char boundary. The
    /// 4S recovery key format in `modes/e2ee.rs` is
    /// ASCII-only, but the field is otherwise free-form and
    /// a future change could introduce Unicode — the
    /// `redact_token` function must be safe.
    ///
    /// `用户` is 6 bytes in UTF-8 (2 chars × 3 bytes each).
    /// `用户syt_abcdefgh_long` is 6 + 16 = 22 bytes; byte 8
    /// would fall in the middle of the second `户` codepoint
    /// (which starts at byte 3 and ends at byte 5 inclusive
    /// — wait, 3 + 3 = 6, so byte 6 starts `s` in `syt_…`).
    /// Construct a string where the boundary is exactly at
    /// risk: 8 bytes total, with 1 multi-byte char then ASCII.
    /// Actually 8 bytes is the "short" branch; need a 9+ byte
    /// string. Try 3 multi-byte chars + ASCII: `用户` is 6
    /// bytes, so `用户用户1` is 13 bytes; byte 8 falls mid the
    /// 2nd `户` (which spans bytes 6..=8, byte 8 is the last
    /// byte of `户`). Adding a 4th `用` to push past: 16 bytes,
    /// byte 8 still in the middle of the 3rd char.
    #[test]
    fn redact_token_non_ascii_does_not_panic() {
        // 3 × 用户 (6 bytes) + 1 × 用 (3 bytes) + "12" = 6+6+3+2 = 17 bytes.
        // Byte 8 falls in the middle of the 2nd `户` codepoint
        // (which starts at byte 6 and spans bytes 6..=8).
        // Pre-fix: would panic. Post-fix: walks back to byte 6
        // (the end of the 1st `户`), giving "用户用户1" — wait
        // let me trace this carefully:
        //   bytes 0..=2:  用
        //   bytes 3..=5:  户
        //   bytes 6..=8:  用
        //   bytes 9..=11: 户
        //   bytes 12..=14: 用
        //   bytes 15..=16: 1, 2
        // Byte 8 is the LAST byte of the 2nd `用` (bytes 6..=8).
        // A byte slice ending at 8 is `&v[..8]`, which is a
        // char boundary (just after byte 7, end of char 2).
        // Hmm, the pre-fix code would NOT panic on this.
        // Let me construct a different example.
        // Take a 4-byte char (e.g. an emoji like 😀 = 4 bytes).
        //   v = "😀😀abcd" (4+4+4 = 12 bytes)
        //   bytes 0..=3:   😀
        //   bytes 4..=7:   😀
        //   bytes 8..=11:  a, b, c, d
        // Byte 8 is the start of `a` — a char boundary. So
        // `&v[..8]` is `😀😀` (8 bytes = 2 chars), valid.
        // Need a string where byte 8 is INSIDE a multi-byte
        // char, not at its end.
        // Take `a😀😀abc` (1+4+4+3 = 12 bytes):
        //   bytes 0:       a
        //   bytes 1..=4:   😀
        //   bytes 5..=8:   😀
        //   bytes 9..=11:  a, b, c
        // Byte 8 is the LAST byte of the 2nd `😀` — so `&v[..8]`
        // ends at byte 7, mid-codepoint of the 2nd emoji. PANICS.
        let v = "a😀😀abc"; // 12 bytes
                            // Just calling redact_token must not panic.
        let r = redact_token(v);
        // The fix walks back from byte 8 to the nearest char
        // boundary below 8. Char boundaries: 0, 1, 5, 9, 10, 11, 12.
        // The largest one ≤ 8 is 5. So `&v[..5]` = "a😀" (5 bytes).
        assert_eq!(r, "a😀***", "got: {r}");
    }

    /// R6-M2 regression test: a Cyrillic-flavored token where
    /// byte 8 lands in the middle of a 2-byte UTF-8 char. The
    /// pre-fix code would panic. Post-fix: walks back to the
    /// previous char boundary.
    #[test]
    fn redact_token_cyrillic_boundary() {
        // 4 × `в` (2 bytes each) + "abcdef" (6 bytes) = 14 bytes.
        //   bytes 0..=1:   в
        //   bytes 2..=3:   в
        //   bytes 4..=5:   в
        //   bytes 6..=7:   в
        //   bytes 8..=13:  a, b, c, d, e, f
        // Byte 8 is `a` — a char boundary. Not a panic case.
        // Take 3 × `в` + "abcdefgh" = 6+8 = 14 bytes:
        //   bytes 0..=1:   в
        //   bytes 2..=3:   в
        //   bytes 4..=5:   в
        //   bytes 6..=13:  a..h
        // Byte 8 is `c` — a char boundary. Still not a panic case.
        // The right construction: 7 ASCII + 1 Cyrillic, so byte 8
        // is the FIRST byte of a 2-byte char (must be followed by
        // another byte, byte 9 is the second byte).
        //   v = "1234567Ж" = 7 + 2 = 9 bytes
        //   bytes 0..=6:  1, 2, 3, 4, 5, 6, 7
        //   bytes 7..=8:  Ж (Cyrillic capital Zhe, U+0416, 2 bytes)
        // Byte 8 is the SECOND byte of Ж — a char boundary, so
        // `&v[..8]` ends after `1, 2, 3, 4, 5, 6, 7` (7 bytes) and
        // is a valid boundary. Not a panic case.
        // The exact mid-codepoint case: I need a 2-byte char that
        // STARTS at byte 7 (so byte 8 is the second byte).
        //   v = "1234567" + 2-byte char starting at byte 7
        //   = "1234567" + a 2-byte char
        //   = 7 + 2 = 9 bytes
        // The 2-byte char spans bytes 7..=8. Byte 7 is the first
        // byte. So `&v[..7]` is fine (byte 7 is a char boundary
        // — start of the 2-byte char). `&v[..8]` is mid-codepoint.
        // That's the panic case.
        let v = "1234567Ж";
        let r = redact_token(v);
        // Walk back from byte 8 to the largest char boundary ≤ 8:
        //   boundaries: 0, 1, 2, 3, 4, 5, 6, 7, 9
        //   largest ≤ 8: 7
        // So the slice is "1234567" (7 bytes) + "***" = "1234567***".
        assert_eq!(r, "1234567***", "got: {r}");
    }
}
