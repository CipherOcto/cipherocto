//! SQL statement splitter. Lifted from `quota-router-storage/migrations.rs`
//! §split_sql_statements; identical semantics preserved.
//!
//! Splits a multi-statement SQL string on `;` boundaries. Strips `--`
//! line comments. Handles `; `, `;\n`, `;;` patterns.

/// Split a multi-statement SQL string on `;` boundaries.
///
/// - Strips `--` line comments before parsing boundaries.
/// - Trailing `;` on the last statement is optional.
/// - Returns a `Vec<String>` of trimmed, bare statements (the trailing
///   `;` is removed from each).
pub fn split_sql_statements(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        // Strip line comments (-- ... <EOL>). Accept LF, CR, or CRLF as the
        // line terminator so SQL authored on Windows / old-Mac systems works.
        // LF / CR chars are consumed entirely (dropped) — `buf` only captures
        // meaningful characters.
        if c == '-' && chars.peek() == Some(&'-') {
            while let Some(&nc) = chars.peek() {
                chars.next();
                if nc == '\n' || nc == '\r' {
                    break;
                }
            }
            continue;
        }
        buf.push(c);
        // End-of-statement delimiter: `;` (with optional trailing whitespace).
        if c == ';' {
            let stmt = buf.trim().to_owned();
            // Strip trailing `;` from the captured statement.
            let stmt = stmt.trim_end_matches(';').trim().to_owned();
            if !stmt.is_empty() {
                out.push(stmt);
            }
            buf.clear();
        }
    }
    let tail = buf.trim().to_owned();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_two_statements() {
        let sql = "CREATE TABLE foo (id INT); CREATE INDEX bar ON foo(id);";
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0], "CREATE TABLE foo (id INT)");
        assert_eq!(stmts[1], "CREATE INDEX bar ON foo(id)");
    }

    #[test]
    fn strips_line_comments() {
        let sql = "-- header\nCREATE TABLE foo (id INT); -- tail\nCREATE INDEX bar ON foo(id);";
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn no_trailing_semicolon() {
        let sql = "CREATE TABLE foo (id INT)";
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn empty_input() {
        let stmts = split_sql_statements("");
        assert!(stmts.is_empty());
    }

    #[test]
    fn comment_only_input() {
        let stmts = split_sql_statements("-- nothing here\n");
        assert!(stmts.is_empty());
    }

    #[test]
    fn double_semicolon_treated_as_single() {
        let sql = "CREATE TABLE a (id INT);;";
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 1, "double ;; must not yield an empty stmt");
    }

    #[test]
    fn semicolons_within_statements_split_anyway() {
        // Documented limitation: the splitter does NOT understand SQL
        // string literals. A `;` inside a quoted string is treated as a
        // boundary. Owner migration SQL must avoid semicolons inside
        // string literals (use a SEPARATOR column or escape via
        // concat() at runtime instead).
        let sql = "INSERT INTO x (note) VALUES ('a;b;c'); INSERT INTO y (id) VALUES (1);";
        let stmts = split_sql_statements(sql);
        // Naive split: 4 fragments (the embedded `;`s do cut).
        assert_eq!(stmts.len(), 4);
    }

    #[test]
    fn handles_crlf_line_endings() {
        // SQL authored on Windows systems has CRLF. The comment-strip loop
        // must terminate on \r OR \n; otherwise trailing \r ends up inside
        // the statement text and breaks a downstream execute.
        let sql = "-- header\r\nCREATE TABLE x (id INT);\r\nCREATE TABLE y (id INT);\r\n";
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 2);
        for stmt in &stmts {
            assert!(
                !stmt.contains('\r'),
                "CR must be stripped from statement text: {stmt:?}"
            );
        }
    }

    #[test]
    fn handles_cr_only_line_endings() {
        // Old Mac (pre-OSX) style. CRLF + LF + CR all terminate a comment.
        let sql = "-- head\rCREATE TABLE x (id INT);\rCREATE TABLE y (id INT);\r";
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 2);
        for stmt in &stmts {
            assert!(!stmt.contains('\r'));
        }
    }
}
