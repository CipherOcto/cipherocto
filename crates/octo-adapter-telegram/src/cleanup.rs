//! Best-effort temp-file cleanup helpers.
//!
//! A central helper for `let _ = std::fs::remove_file(...)` so the intent
//! ("this is a best-effort cleanup; we don't care if it fails") is documented
//! in one place rather than scattered across the crate.

use std::io::ErrorKind;
use std::path::Path;

/// Best-effort temp-file removal.
///
/// Silently ignores "file does not exist" (no log noise for files that were
/// never created — R4 H3). Other I/O errors are logged at `warn` level so
/// operators can audit permission / disk-full issues without the program
/// crashing.
pub fn cleanup_temp_file(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() == ErrorKind::NotFound {
            // File was never created (e.g. File::create failed before
            // cleanup was called). Not an actionable error.
            return;
        }
        tracing::warn!(path = %path.display(), error = %e, "failed to remove temp file (non-fatal)");
    }
}
