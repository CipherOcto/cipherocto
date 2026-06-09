//! Best-effort temp-file cleanup helpers.
//!
//! A central helper for `let _ = std::fs::remove_file(...)` so the intent
//! ("this is a best-effort cleanup; we don't care if it fails") is documented
//! in one place rather than scattered across the crate.

use std::path::Path;

/// Best-effort temp-file removal.
///
/// Silently ignores both "file does not exist" and any I/O error. Used at the
/// end of upload paths where the temp file is no longer needed but a failure
/// to remove it should not propagate.
pub fn cleanup_temp_file(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        tracing::warn!(path = %path.display(), error = %e, "failed to remove temp file (non-fatal)");
    }
}
