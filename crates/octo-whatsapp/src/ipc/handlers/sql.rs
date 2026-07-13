//! `sql.{execute,query,tables}` — dynamic DDL/DML on the daemon's
//! embedded stoolap database.
//!
//! `sql.execute` runs a single write statement (`INSERT/UPDATE/DELETE/
//! CREATE/DROP/ALTER/...`) and returns rows-affected.
//! `sql.query` is read-only (`SELECT/WITH/SHOW/EXPLAIN`) and returns the
//! rows as JSON. `sql.tables` is a thin wrapper around `SHOW TABLES`.
//!
//! Three safety rails are non-negotiable for v1:
//!
//! 1. Single-statement enforcement — split on `;`, reject >1 stmt.
//!    Blocks `DROP TABLE x; DROP TABLE y` style injection.
//! 2. Allow-list on first keyword per handler.
//!    `execute`: write verbs only, never `SHUTDOWN` / `ATTACH` / `DETACH`.
//!    `query`:  read verbs only, never `DELETE` / `UPDATE` / `INSERT`.
//! 3. Result-row cap on `query` (`MAX_ROWS = 10000`) to avoid
//!    `SELECT * FROM huge_table` blowing up the RPC payload.
//!
//! All three handlers inherit the daemon-wide bearer-token gate
//! (see `ipc/server.rs`) — no relaxation here.

use serde::Deserialize;
use serde_json::{json, Value};
use stoolap::Database;

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

// === Tunables =============================================================

/// Hard cap on rows returned by `sql.query`. Operators wanting more
/// must `LIMIT` themselves; the server-side cap is the safety net.
const MAX_ROWS: usize = 10_000;

// === Verb allow-lists =====================================================

/// Statements accepted by `sql.execute` (write side of the DDL/DML).
const WRITE_VERBS: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "REPLACE", "CREATE", "DROP", "ALTER", "TRUNCATE", "BEGIN",
    "COMMIT", "ROLLBACK", "PRAGMA", "ANALYZE", "VACUUM",
];

/// Statements accepted by `sql.query` (read side).
const READ_VERBS: &[&str] = &["SELECT", "WITH", "SHOW", "EXPLAIN", "DESCRIBE", "DESC"];

// === Helpers ==============================================================

/// Lift the daemon's bundled subsystem to the raw `Database` handle.
/// Mirrors `QueryService::from_parts` failures: `NotConnected` if the
/// `query` cargo feature wasn't compiled in or the subsystem didn't
/// boot.
fn require_db(h: &DaemonHandle) -> Result<Database, RpcError> {
    let sub = h.query_subsystem().ok_or(RpcError {
        code: RpcErrorCode::NotConnected.as_i32(),
        message: "query subsystem not installed (rebuild with --features query)".into(),
        data: None,
    })?;
    Ok(sub.db.clone())
}

/// Reject empty input + >1 statement separated by `;`.
/// Trailing `;` is allowed (it's the natural terminator). Comment-only
/// input is treated as empty (the `first_keyword` pass skips comments
/// but still needs a real token to find).
fn single_statement(sql: &str) -> Result<&str, RpcError> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err(RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: "sql must not be empty".into(),
            data: Some(json!({ "hint": "pass the statement as the `sql` argument" })),
        });
    }
    // Walk byte-by-byte so we don't split inside a string literal.
    // Stoolap uses single-quoted strings; we tolerate `'a;b'` as one
    // statement and only count top-level `;`.
    let mut stmts = 0usize;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut has_real_token = false;
    let bytes = trimmed.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        let next = bytes.get(i + 1).copied();
        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
        } else if in_block_comment {
            if b == b'*' && next == Some(b'/') {
                in_block_comment = false;
                i += 1;
            }
        } else if in_single_quote {
            if b == b'\'' {
                in_single_quote = false;
            } else {
                has_real_token = true;
            }
        } else if in_double_quote {
            if b == b'"' {
                in_double_quote = false;
            } else {
                has_real_token = true;
            }
        } else {
            match b {
                b'\'' => in_single_quote = true,
                b'"' => in_double_quote = true,
                b'-' if next == Some(b'-') => {
                    in_line_comment = true;
                    i += 1;
                }
                b'/' if next == Some(b'*') => {
                    in_block_comment = true;
                    i += 1;
                }
                b';' => stmts += 1,
                c if !c.is_ascii_whitespace() => has_real_token = true,
                _ => {}
            }
        }
        i += 1;
    }
    if !has_real_token {
        return Err(RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: "sql is comment-only or whitespace".into(),
            data: None,
        });
    }
    // Strip the trailing terminator so it doesn't count as its own
    // empty statement — common when operators paste `SELECT 1;\n`.
    if stmts > 0 && trimmed.ends_with(';') {
        stmts -= 1;
    }
    if stmts > 0 {
        return Err(RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!(
                "sql must contain exactly one statement (found {})",
                stmts + 1
            ),
            data: Some(json!({ "hint": "remove the inner `;` or split into separate calls" })),
        });
    }
    Ok(trimmed)
}

/// First identifier of the statement, upper-cased. Empty tokens before
/// the keyword (whitespace + comments) are skipped. Returns `InvalidParams`
/// if no identifier is found.
fn first_keyword(sql: &str) -> Result<String, RpcError> {
    let bytes = sql.as_bytes();
    let mut i = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    while i < bytes.len() {
        let b = bytes[i];
        let next = bytes.get(i + 1).copied();
        if in_single {
            if b == b'\'' {
                in_single = false;
            }
            i += 1;
            continue;
        }
        if in_double {
            if b == b'"' {
                in_double = false;
            }
            i += 1;
            continue;
        }
        // Skip line comments.
        if b == b'-' && next == Some(b'-') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Skip block comments.
        if b == b'/' && next == Some(b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Collect the identifier: ASCII letters / digits / underscore /
        // dot (for things like `WITH RECURSIVE` or schema-qualified
        // identifiers — but the first token should be a single word).
        let start = i;
        while i < bytes.len() {
            let c = bytes[i];
            if c.is_ascii_whitespace() || c == b'(' || c == b';' {
                break;
            }
            i += 1;
        }
        return Ok(std::str::from_utf8(&bytes[start..i])
            .unwrap_or("")
            .to_ascii_uppercase());
    }
    Err(RpcError {
        code: RpcErrorCode::InvalidParams.as_i32(),
        message: "sql has no recognizable keyword".into(),
        data: None,
    })
}

/// `true` iff `kw` is in the write allow-list.
fn is_write_verb(kw: &str) -> bool {
    WRITE_VERBS.contains(&kw)
}

/// `true` iff `kw` is in the read allow-list.
fn is_read_verb(kw: &str) -> bool {
    READ_VERBS.contains(&kw)
}

/// Coerce a stoolap [`Value`] into a `serde_json::Value`. Falls back to
/// stringification when the type doesn't map cleanly.
fn stoolap_value_to_json(v: &stoolap::Value) -> serde_json::Value {
    use stoolap::Value as SV;
    match v {
        SV::Null(_) => serde_json::Value::Null,
        SV::Integer(i) => serde_json::Value::Number((*i).into()),
        SV::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        SV::Text(s) => serde_json::Value::String(s.as_str().to_string()),
        SV::Boolean(b) => serde_json::Value::Bool(*b),
        SV::Timestamp(ts) => serde_json::Value::String(ts.to_rfc3339()),
        SV::Blob(bytes) => serde_json::Value::String(format!("0x{}", hex_lower(bytes))),
        // Extension wraps JSON (tag=6) and Vector (tag=7); we render
        // the raw hex so the operator can spot it instead of silently
        // losing the value.
        SV::Extension(bytes) => serde_json::Value::String(format!("0x{}", hex_lower(bytes))),
    }
}

fn hex_lower(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for byte in b {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

/// Map `stoolap::Error` into an RPC `InternalError`.
fn stoolap_err(method: &str, e: stoolap::Error) -> RpcError {
    RpcError {
        code: RpcErrorCode::InternalError.as_i32(),
        message: format!("{method}: {e}"),
        data: None,
    }
}

// === Handlers =============================================================

#[derive(Deserialize, Default)]
struct ExecuteParams {
    sql: String,
}

#[derive(Debug)]
pub struct SqlExecute;

#[async_trait::async_trait]
impl RpcHandler for SqlExecute {
    fn name(&self) -> &'static str {
        "sql.execute"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: ExecuteParams = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let sql = single_statement(&p.sql)?;
        let kw = first_keyword(sql)?;
        if !is_write_verb(&kw) {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!(
                    "first keyword {kw:?} is not in the write allow-list; \
                     use sql.query for {kw:?} statements"
                ),
                data: Some(json!({
                    "allowed": WRITE_VERBS,
                    "read_path": "sql.query",
                })),
            });
        }
        let db = require_db(&h)?;
        let sql_owned = sql.to_string();
        // Single-threaded DB: run on spawn_blocking so the per-RPC
        // timeout actually fires (DB::execute is sync).
        let res = tokio::task::spawn_blocking(move || db.execute(&sql_owned, ()))
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("worker panic: {e}"),
                data: None,
            })?;
        let rows_affected = res.map_err(|e| stoolap_err("sql.execute", e))?;
        Ok(json!({
            "sql": sql,
            "first_keyword": kw,
            "rows_affected": rows_affected,
        }))
    }
}

#[derive(Deserialize, Default)]
struct QueryParams {
    sql: String,
    #[serde(default)]
    limit: Option<usize>, // client-side cap, smaller than server cap
}

#[derive(Debug)]
pub struct SqlQuery;

#[async_trait::async_trait]
impl RpcHandler for SqlQuery {
    fn name(&self) -> &'static str {
        "sql.query"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: QueryParams = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let sql = single_statement(&p.sql)?;
        let kw = first_keyword(sql)?;
        if !is_read_verb(&kw) {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!(
                    "first keyword {kw:?} is read-only-restricted; \
                     use sql.execute for {kw:?} statements"
                ),
                data: Some(json!({
                    "allowed": READ_VERBS,
                    "write_path": "sql.execute",
                })),
            });
        }
        let db = require_db(&h)?;
        let client_cap = p.limit.unwrap_or(MAX_ROWS).min(MAX_ROWS);
        let sql_owned = sql.to_string();
        let (columns, rows, truncated) = tokio::task::spawn_blocking(move || {
            let mut rows = db
                .query(&sql_owned, ())
                .map_err(|e| format!("query: {e}"))?;
            // Clamp to client_cap (further limit atop MAX_ROWS).
            let cap = client_cap.min(MAX_ROWS);
            let mut out: Vec<Vec<Value>> = Vec::new();
            let mut stop_collecting = false;
            while let Some(row_res) = rows.next() {
                if out.len() >= cap {
                    stop_collecting = true;
                    break;
                }
                let row = row_res.map_err(|e| format!("row: {e}"))?;
                let mut cells = Vec::with_capacity(rows.columns().len());
                for i in 0..rows.columns().len() {
                    cells.push(match row.get_value(i) {
                        Some(v) => stoolap_value_to_json(v),
                        None => Value::Null,
                    });
                }
                out.push(cells);
            }
            // If we hit cap early, drain the rest to keep the iterator
            // well-behaved (Rows::Drop closes). We don't count past cap.
            if stop_collecting {
                while rows.next().is_some() {}
            }
            Ok::<_, String>((rows.columns().to_vec(), out, stop_collecting))
        })
        .await
        .map_err(|e| RpcError {
            code: RpcErrorCode::InternalError.as_i32(),
            message: format!("worker panic: {e}"),
            data: None,
        })?
        .map_err(|e| RpcError {
            code: RpcErrorCode::InternalError.as_i32(),
            message: format!("sql.query: {e}"),
            data: None,
        })?;
        Ok(json!({
            "sql": sql,
            "first_keyword": kw,
            "columns": columns,
            "rows": rows,
            "count": rows.len(),
            "limit": client_cap,
            "truncated": truncated,
        }))
    }
}

#[derive(Debug)]
pub struct SqlTables;

#[async_trait::async_trait]
impl RpcHandler for SqlTables {
    fn name(&self) -> &'static str {
        "sql.tables"
    }

    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let db = require_db(&h)?;
        let res = tokio::task::spawn_blocking(move || db.query("SHOW TABLES", ()))
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("worker panic: {e}"),
                data: None,
            })?
            .map_err(|e| stoolap_err("sql.tables", e))?;
        let mut names: Vec<String> = Vec::new();
        for row in res {
            let row = row.map_err(|e| stoolap_err("sql.tables", e))?;
            if let Ok(s) = row.get::<String>(0) {
                names.push(s);
            } else if let Some(v) = row.get_value(0) {
                names.push(format!("{v:?}"));
            }
        }
        Ok(json!({
            "tables": names,
            "count": names.len(),
        }))
    }
}

// === Tests ================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;
    use crate::query::embedder::{Embedder, MockEmbedder};
    use crate::query::{open_subsystem, JobConfig};
    use std::sync::Arc;

    fn handle_with_query() -> (tempfile::TempDir, DaemonHandle) {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let h = Daemon::new_for_tests(tmp.path()).1;
        let base = tempfile::tempdir().expect("subdir");
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("test", 384));
        let s =
            open_subsystem(base.path(), embedder, JobConfig::default()).expect("open subsystem");
        h.install_query_subsystem(Arc::new(s));
        (tmp, h)
    }

    async fn execute(h: &DaemonHandle, sql: &str) -> Result<Value, RpcError> {
        SqlExecute.call(h.clone(), json!({ "sql": sql })).await
    }

    async fn query(h: &DaemonHandle, sql: &str) -> Result<Value, RpcError> {
        SqlQuery.call(h.clone(), json!({ "sql": sql })).await
    }

    // --- pure helpers ---

    #[test]
    fn first_keyword_skips_whitespace_and_comments() {
        assert_eq!(
            first_keyword("   \n -- preamble\n SELECT 1").unwrap(),
            "SELECT"
        );
        assert_eq!(
            first_keyword("/* ansi style */ INSERT INTO t").unwrap(),
            "INSERT"
        );
        assert_eq!(first_keyword("CREATE TABLE x (id INT)").unwrap(), "CREATE");
    }

    #[test]
    fn first_keyword_handles_strings_and_comments_inside_text() {
        assert_eq!(
            first_keyword("INSERT INTO t (s) VALUES ('drop table x')").unwrap(),
            "INSERT"
        );
    }

    #[test]
    fn single_statement_rejects_empty() {
        assert!(single_statement("").is_err());
        assert!(single_statement("   ").is_err());
        assert!(single_statement("-- only a comment").is_err());
        assert!(single_statement("/* block only */").is_err());
    }

    #[test]
    fn single_statement_allows_trailing_semicolon() {
        assert!(single_statement("SELECT 1;").is_ok());
        assert!(single_statement("SELECT 1;\n").is_ok());
    }

    #[test]
    fn single_statement_rejects_two_statements() {
        assert!(single_statement("DROP TABLE a; SELECT 1").is_err());
        assert!(single_statement("SELECT 'a;b' AS x").is_ok());
        assert!(single_statement("-- note ; still a comment\nDROP TABLE a").is_ok());
    }

    #[test]
    fn allow_lists_partition_writes_and_reads() {
        assert!(is_write_verb("INSERT"));
        assert!(is_write_verb("CREATE"));
        assert!(!is_write_verb("SELECT"));
        assert!(is_read_verb("SELECT"));
        assert!(is_read_verb("EXPLAIN"));
        assert!(!is_read_verb("DELETE"));
    }

    // --- end-to-end against the daemon's query subsystem ---

    #[tokio::test]
    async fn create_insert_select_round_trip() {
        let (_t, h) = handle_with_query();
        // Stoolap only supports INTEGER PRIMARY KEY (rowid alias);
        // use a plain UNIQUE on a TEXT column instead.
        execute(&h, "CREATE TABLE demo (k TEXT UNIQUE, v INTEGER)")
            .await
            .unwrap();
        execute(
            &h,
            "INSERT INTO demo (k, v) VALUES ('a', 1), ('b', 2), ('c', 3)",
        )
        .await
        .unwrap();
        let r = query(&h, "SELECT k, v FROM demo ORDER BY k").await.unwrap();
        let rows = r["rows"].as_array().expect("rows arr");
        let keys: Vec<&str> = rows.iter().map(|r| r[0].as_str().unwrap()).collect();
        assert_eq!(keys, vec!["a", "b", "c"]);
        assert_eq!(rows[2][1].as_i64().unwrap(), 3);
    }

    #[tokio::test]
    async fn sql_execute_rejects_read_verb() {
        let (_t, h) = handle_with_query();
        let err = execute(&h, "SELECT 1").await.unwrap_err();
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("write allow-list"));
    }

    #[tokio::test]
    async fn sql_execute_rejects_unknown_verb() {
        let (_t, h) = handle_with_query();
        let err = execute(&h, "SHUTDOWN").await.unwrap_err();
        assert_eq!(err.code, -32602);
    }

    #[tokio::test]
    async fn sql_execute_rejects_multi_statement() {
        let (_t, h) = handle_with_query();
        let err = execute(&h, "CREATE TABLE x (id INT); DROP TABLE x")
            .await
            .unwrap_err();
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("exactly one statement"));
    }

    #[tokio::test]
    async fn sql_query_rejects_write_verb() {
        let (_t, h) = handle_with_query();
        execute(&h, "CREATE TABLE t (id INT)").await.unwrap();
        let err = query(&h, "DELETE FROM t").await.unwrap_err();
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("read-only-restricted"));
    }

    #[tokio::test]
    async fn sql_tables_returns_created_table() {
        let (_t, h) = handle_with_query();
        execute(&h, "CREATE TABLE sample (id INTEGER PRIMARY KEY)")
            .await
            .unwrap();
        let r = SqlTables.call(h.clone(), Value::Null).await.unwrap();
        let tbl = r["tables"].as_array().expect("tables arr");
        assert!(
            tbl.iter().any(|v| v.as_str() == Some("sample")),
            "expected 'sample' in {tbl:?}"
        );
    }

    #[tokio::test]
    async fn sql_query_returns_all_under_cap() {
        let (_t, h) = handle_with_query();
        execute(&h, "CREATE TABLE t (k INTEGER)").await.unwrap();
        execute(&h, "INSERT INTO t VALUES (1),(2),(3),(4),(5)")
            .await
            .unwrap();
        let r = query(&h, "SELECT k FROM t ORDER BY k").await.unwrap();
        assert_eq!(r["count"].as_u64().unwrap(), 5);
        let rows = r["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 5);
    }

    #[tokio::test]
    async fn empty_sql_rejected() {
        let (_t, h) = handle_with_query();
        let err = execute(&h, "   ").await.unwrap_err();
        assert_eq!(err.code, -32602);
    }
}
