//! Health check: open every store the daemon owns and print
//! counts + integrity probes. Useful to run after a daemon lifecycle
//! (replay, shutdown, restart) to catch schema drift or replay bugs.
//!
//! Usage:
//!   cargo run -p octo-whatsapp --features query --example diag_count
#[cfg(feature = "query")]
use octo_storage_core::Database;

#[cfg(feature = "query")]
fn main() {
    let home = std::env::var("HOME").expect("HOME");
    let qdir = format!("{home}/.local/share/octo/whatsapp");
    let events_db = format!("file://{qdir}/query/events.db");
    let ndjson = format!("{qdir}/events/events.ndjson");
    let tbls = ["events", "messages", "embeddings", "query_meta"];

    println!("=== SQL row counts ===");
    let db = match Database::open(&events_db) {
        Ok(d) => d,
        Err(e) => {
            println!("FAIL: open {events_db}: {e}");
            std::process::exit(2);
        }
    };
    for t in &tbls {
        let cnt = try_count(&db, t);
        match cnt {
            Some(n) => println!("  {t}: {n}"),
            None => println!("  {t}: TABLE MISSING"),
        }
    }

    println!();
    println!("=== events schema sanity ===");
    // columns present?
    match db.query(
        "SELECT id, kind, variant, peer, sender, chat_jid, ts_unix_ms, payload FROM events LIMIT 0",
        (),
    ) {
        Ok(_) => println!("  events columns: OK (8 columns)"),
        Err(e) => println!("  events columns: FAIL {e}"),
    }
    // PK uniqueness probe
    match db.query("SELECT COUNT(*) - COUNT(DISTINCT id) FROM events", ()) {
        Ok(mut rows) => {
            if let Some(Ok(r)) = rows.next() {
                let dupes: i64 = r.get::<i64>(0).unwrap_or(-1);
                if dupes == 0 {
                    println!("  events.id uniqueness: OK (no dupes)");
                } else {
                    println!("  events.id uniqueness: FAIL ({dupes} dupes)");
                }
            }
        }
        Err(e) => println!("  events.id uniqueness: FAIL {e}"),
    }
    // id contiguous?
    match db.query("SELECT MIN(id), MAX(id), COUNT(*) FROM events", ()) {
        Ok(mut rows) => {
            if let Some(Ok(r)) = rows.next() {
                let mn: i64 = r.get::<i64>(0).unwrap_or(0);
                let mx: i64 = r.get::<i64>(1).unwrap_or(0);
                let cnt: i64 = r.get::<i64>(2).unwrap_or(0);
                let expected_range = mx - mn + 1;
                if expected_range == cnt && mn >= 1 {
                    println!("  events.id contiguous: OK (id={mn}..={mx}, count={cnt})");
                } else {
                    println!(
                        "  events.id contiguous: GAPS — id={mn}..={mx}, count={cnt} (gap={})",
                        expected_range - cnt
                    );
                }
            }
        }
        Err(e) => println!("  events.id contiguous: FAIL {e}"),
    }
    // ts_unix_ms sanity: each row should have a positive ts
    match db.query(
        "SELECT COUNT(*) FROM events WHERE ts_unix_ms <= 0 OR ts_unix_ms IS NULL",
        (),
    ) {
        Ok(mut rows) => {
            if let Some(Ok(r)) = rows.next() {
                let n: i64 = r.get::<i64>(0).unwrap_or(-1);
                if n == 0 {
                    println!("  events.ts_unix_ms sanity: OK (all positive)");
                } else {
                    println!("  events.ts_unix_ms sanity: WARN {n} rows with ts<=0");
                }
            }
        }
        Err(e) => println!("  events.ts_unix_ms sanity: FAIL {e}"),
    }
    // kind distribution
    println!();
    println!("=== events.kind distribution ===");
    if let Ok(mut rows) = db.query(
        "SELECT kind, COUNT(*) FROM events GROUP BY kind ORDER BY 2 DESC",
        (),
    ) {
        while let Some(Ok(r)) = rows.next() {
            let k: String = r.get::<String>(0).unwrap_or_default();
            let c: i64 = r.get::<i64>(1).unwrap_or(0);
            println!("  {k}: {c}");
        }
    }

    // messages schema sanity (table may legitimately be 0 rows)
    println!();
    println!("=== messages schema sanity ===");
    if let Ok(mut rows) = db.query("SELECT COUNT(*) FROM messages", ()) {
        if let Some(Ok(r)) = rows.next() {
            let n: i64 = r.get::<i64>(0).unwrap_or(0);
            println!("  messages: {n} rows");
        }
    }
    if let Ok(mut rows) = db.query(
        "SELECT event_id, peer, sender, ts_unix_ms, kind, text, media_token, from_me, is_group \
         FROM messages LIMIT 0",
        (),
    ) {
        println!("  messages columns: OK (9 columns)");
        let _ = rows.next();
    } else {
        println!("  messages columns: FAIL");
    }

    println!();
    println!("=== NDJSON on-disk ===");
    let path = std::path::Path::new(&ndjson);
    if !path.exists() {
        println!("  {}: ABSENT", path.display());
    } else {
        let data = std::fs::read_to_string(path).expect("read");
        let mut lines_total = 0u64;
        let mut lines_blank = 0u64;
        let mut lines_bad_parse = 0u64;
        let mut lines_ok = 0u64;
        let mut min_id = u64::MAX;
        let mut max_id = 0u64;
        let mut dup_ids = std::collections::HashSet::new();
        let mut seen_ids = std::collections::HashSet::new();
        for line in data.lines() {
            lines_total += 1;
            if line.trim().is_empty() {
                lines_blank += 1;
                continue;
            }
            // Outer shape: {id, ts_unix_ms, ts_mono_ns, event}
            let v: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => {
                    lines_bad_parse += 1;
                    continue;
                }
            };
            let id = v.get("id").and_then(|x| x.as_u64()).unwrap_or(0);
            if id == 0 {
                lines_bad_parse += 1;
                continue;
            }
            if !seen_ids.insert(id) {
                dup_ids.insert(id);
            }
            min_id = min_id.min(id);
            max_id = max_id.max(id);
            lines_ok += 1;
        }
        let expected = if max_id >= min_id {
            max_id - min_id + 1
        } else {
            0
        };
        println!("  lines_total: {lines_total}");
        println!("  lines_blank: {lines_blank}");
        println!("  lines_ok:    {lines_ok}");
        println!("  lines_bad:   {lines_bad_parse}");
        println!("  id range:    {min_id}..={max_id} (span {expected})");
        let dup_n = dup_ids.len();
        if dup_n == 0 {
            println!("  unique ids:  OK");
        } else {
            println!("  unique ids:  FAIL — {dup_n} duplicate ids in NDJSON");
            let mut sample: Vec<u64> = dup_ids.iter().copied().take(5).collect();
            sample.sort();
            println!("    sample duplicates: {sample:?}");
        }
        if min_id >= 2 && max_id <= lines_total + lines_blank + 4 {
            println!("  monotonic:   OK");
        } else {
            println!("  monotonic:   WARN — min={min_id} max={max_id}");
        }
    }

    // Unknown event envelopes: classify by leading token. Each row's
    // payload is the raw `{:?}`-formatted envelope string; we bucket
    // by the first identifier (e.g. `Messages(`, `Receipt(`,
    // `PairPasskey(`) so we can see which envelope types the parser
    // doesn't yet route to a typed variant.
    println!();
    println!("=== unknown envelope breakdown ===");
    let mut prefix_counts: std::collections::BTreeMap<String, i64> =
        std::collections::BTreeMap::new();
    let mut samples: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    if let Ok(mut rows) = db.query(
        "SELECT payload FROM events WHERE kind = 'unknown' ORDER BY id",
        (),
    ) {
        while let Some(Ok(r)) = rows.next() {
            let raw: String = r.get::<String>(0).unwrap_or_default();
            // First identifier before `(` or `<` or whitespace.
            let prefix = raw
                .split(|c: char| c == '(' || c == '<' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_string();
            if prefix.is_empty() {
                continue;
            }
            *prefix_counts.entry(prefix.clone()).or_insert(0) += 1;
            samples
                .entry(prefix)
                .or_insert(raw.chars().take(120).collect());
        }
    }
    for (k, v) in &prefix_counts {
        let sample = samples.get(k).cloned().unwrap_or_default();
        println!("  {k}: {v}  e.g. {}", sample);
    }

    // ID gap map: which ids are missing in the largest contiguous
    // missing run, and the total count.
    println!();
    println!("=== events.id gap map ===");
    if let Ok(mut rows) = db.query("SELECT id FROM events ORDER BY id", ()) {
        let mut ids: Vec<i64> = Vec::new();
        while let Some(Ok(r)) = rows.next() {
            if let Ok(v) = r.get::<i64>(0) {
                ids.push(v);
            }
        }
        let mut gaps_total: i64 = 0;
        let mut runs: Vec<(i64, i64, i64)> = Vec::new(); // (start, end, len)
        let mut prev: Option<i64> = None;
        for &id in &ids {
            if let Some(p) = prev {
                if id > p + 1 {
                    let len = id - p - 1;
                    gaps_total += len;
                    if runs.len() < 6 {
                        runs.push((p + 1, id - 1, len));
                    }
                }
            }
            prev = Some(id);
        }
        println!("  total gaps: {gaps_total}");
        if !runs.is_empty() {
            println!("  top gap runs (start..end, len):");
            for (s, e, l) in &runs {
                println!("    {s}..={e}  (len={l})");
            }
        }
        let first = ids.first().copied().unwrap_or(0);
        let last = ids.last().copied().unwrap_or(0);
        if first > 1 {
            println!("  leading gap: 1..={}", first - 1);
        }
        if let Some(last_in_ndjson) = std::fs::read_to_string(&ndjson).ok().and_then(|s| {
            s.lines().rfind(|l| !l.trim().is_empty()).and_then(|l| {
                serde_json::from_str::<serde_json::Value>(l)
                    .ok()
                    .and_then(|v| v.get("id").and_then(|x| x.as_i64()))
            })
        }) {
            if last_in_ndjson > last {
                println!(
                    "  trailing gap: SQL ends at id={last}, NDJSON ends at id={last_in_ndjson} ({} missing)",
                    last_in_ndjson - last
                );
                // Show what kinds live in the trailing NDJSON range
                // so we can tell whether the missing IDs are typed
                // events the persister failed to ingest into SQL, or
                // raw envelopes we already classified.
                let ndjson_data = std::fs::read_to_string(&ndjson).unwrap_or_default();
                let mut kind_counts: std::collections::BTreeMap<String, i64> =
                    std::collections::BTreeMap::new();
                for line in ndjson_data.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let v: serde_json::Value = match serde_json::from_str(line) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let id = match v.get("id").and_then(|x| x.as_i64()) {
                        Some(i) => i,
                        None => continue,
                    };
                    if id <= last {
                        continue;
                    }
                    let event_kind = v
                        .get("event")
                        .and_then(|e| e.get("event"))
                        .and_then(|k| k.as_str())
                        .unwrap_or("?")
                        .to_string();
                    *kind_counts.entry(event_kind).or_insert(0) += 1;
                }
                println!("  trailing NDJSON kind breakdown:");
                for (k, v) in &kind_counts {
                    println!("    {k}: {v}");
                }
            }
        }
    }

    // tantivy index sanity
    println!();
    println!("=== tantivy FTS ===");
    let tantivy_dir = format!("{qdir}/query/tantivy");
    let td = std::path::Path::new(&tantivy_dir);
    if !td.exists() {
        println!("  {}: ABSENT", td.display());
    } else {
        let mut count = 0;
        for _entry in std::fs::read_dir(td).unwrap() {
            count += 1;
        }
        println!("  {}: {count} entries", td.display());
        let meta = std::fs::read_to_string(td.join("meta.json")).unwrap_or_default();
        if meta.contains("\"docstore_compress\":") || meta.contains("segments") {
            println!("  meta.json:    OK");
        } else {
            println!("  meta.json:    empty");
        }
    }
}

#[cfg(feature = "query")]
fn try_count(db: &Database, t: &str) -> Option<i64> {
    let sql = format!("SELECT COUNT(*) FROM {t}");
    let mut rows = db.query(&sql, ()).ok()?;
    let r = rows.next()?.ok()?;
    r.get::<i64>(0).ok()
}

#[cfg(not(feature = "query"))]
fn main() {
    eprintln!("diag_count requires --features query");
    std::process::exit(2);
}
