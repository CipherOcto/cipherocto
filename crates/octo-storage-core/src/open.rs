//! [`open`] / [`open_in_memory`] — Database constructor wrappers.
//!
//! Stoolap-fork DSN scheme is `file://` for persistent files and
//! `memory://` for in-memory. [`open`] prepends `file://` to its input;
//! callers pass a plain filesystem path (no scheme prefix required).

use crate::error::SubstrateError;

/// Open the fork's `stoolap::Database` at `path`.
///
/// Thin wrapper around `stoolap::Database::open` that surfaces failures
/// as [`SubstrateError::stoolap`]. Owner crates should call this rather
/// than touching `stoolap::Database::open` directly, so errors carry
/// the operation tag (`"open"`) and don't leak `stoolap` types into
/// the API.
///
/// # Errors
/// Returns `SubstrateError::stoolap("open", _)` on underlying failure.
pub fn open(path: &str) -> Result<stoolap::Database, SubstrateError> {
    let dsn = format!("file://{path}");
    stoolap::Database::open(&dsn).map_err(|e| SubstrateError::stoolap("open", e))
}

/// Open an ephemeral in-memory `stoolap::Database`.
///
/// Equivalent to `Database::open_in_memory()`; surface shim so owner
/// crates have one error type for "storage failed".
///
/// # Errors
/// Returns `SubstrateError::stoolap("open_in_memory", _)` on underlying failure.
pub fn open_in_memory() -> Result<stoolap::Database, SubstrateError> {
    stoolap::Database::open_in_memory().map_err(|e| SubstrateError::stoolap("open_in_memory", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_returns_usable_database() {
        let db = open_in_memory().expect("open_in_memory must succeed");
        // Sanity: a trivial execute + query round-trip.
        db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY)", ())
            .unwrap();
        db.execute("INSERT INTO t (id) VALUES (1)", ()).unwrap();
        let rows = db.query("SELECT id FROM t", ()).unwrap();
        let mut got: Vec<i64> = Vec::new();
        for r in rows.into_iter() {
            got.push(r.unwrap().get::<i64>(0).unwrap());
        }
        assert_eq!(got, vec![1]);
    }

    #[test]
    fn open_with_tempdir_persists() {
        // Use a tempdir-like location unique per run.
        let dir = std::env::temp_dir().join(format!(
            "octo-storage-core-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let path_str = path.to_str().expect("tempdir path is utf8");

        // Open + create a table + insert.
        {
            let db = open(path_str).expect("first open");
            db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)", ())
                .unwrap();
            db.execute("INSERT INTO t (id, name) VALUES (1, 'first')", ())
                .unwrap();
            // Drop closes the database (Database::Drop impl).
            drop(db);
        }

        // Re-open from the same path; row must persist.
        {
            let db = open(path_str).expect("second open");
            let rows = db.query("SELECT name FROM t WHERE id = 1", ()).unwrap();
            let mut iter = rows.into_iter();
            let row = iter.next().expect("row").unwrap();
            let name: String = row.get(0).unwrap();
            assert_eq!(name, "first");
        }

        // Clean up.
        let _ = std::fs::remove_dir_all(&dir);
    }
}
