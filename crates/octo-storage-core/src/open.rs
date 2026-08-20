//! [`open`] / [`open_in_memory`] — Database constructor wrappers.
//!
//! Stoolap-fork DSN scheme is `file://` for persistent files and
//! `memory://` for in-memory. [`open`] prepends `file://` to its input;
//! callers pass a plain filesystem path (no scheme prefix required).

use crate::database::Database;
use crate::error::SubstrateError;

/// Open a persistent [`Database`] at `path`.
///
/// Thin wrapper around `stoolap::Database::open` that wraps the result
/// in the substrate's `Database` newtype. Owner crates should call this
/// (or [`Database::open`]) rather than touching `stoolap::Database::open`
/// directly, so the substrate owns the newtype boundary and every
/// execution path routes through [`Database::execute_checked`].
///
/// # Errors
/// Returns [`SubstrateError::Storage`] with operation tag `"open"` on
/// underlying failure.
pub fn open(path: &str) -> Result<Database, SubstrateError> {
    Database::open(path)
}

/// Open an ephemeral in-memory [`Database`].
///
/// Equivalent to [`Database::open_in_memory`]; surface shim so owner
/// crates have one error type for "storage failed".
///
/// # Errors
/// Returns [`SubstrateError::Storage`] with operation tag `"open_in_memory"`
/// on underlying failure.
pub fn open_in_memory() -> Result<Database, SubstrateError> {
    Database::open_in_memory()
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
