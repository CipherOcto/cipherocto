//! `whatsapp_session_introspect` — full dump of a WA session DB as observed
//! by the daemon's CryptoProvider/PersistenceManager on the next boot.
//!
//! Phase 7.J follow-up: after `dump_noise_key` (binary) we want a single
//! command that shows ALL fields wacore could examine at startup, in a
//! stable shape that can be diffed across runs (`--json`).
//!
//! Includes:
//!   - device row (key fields + JSON-decoded server_cert_chain + edge_routing_info
//!     + length + meta)
//!   - identities, sessions, prekeys, signed_prekeys (counted + sampled)
//!   - lid_pn_mapping (full dump — usually 0-many rows)
//!   - app_state_keys / app_state_versions (counts + names)
//!   - device_registry row (devices_json)
//!   - tc_tokens / sender_keys / base_keys / sent_messages counts
//!   - SHA-256 fingerprints of each crypto blob on disk
//!
//! Usage:
//!   cargo run -p octo-adapter-whatsapp --bin whatsapp_session_introspect -- [session_db_path]
//!   cargo run -p octo-adapter-whatsapp --bin whatsapp_session_introspect -- [session_db_path] --json

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use octo_adapter_whatsapp::store::StoolapStore;
use sha2::{Digest, Sha256};

fn main() -> ExitCode {
    let mut session_path: Option<PathBuf> = None;
    let mut json_mode = false;
    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--json" => json_mode = true,
            other => {
                if session_path.is_none() {
                    session_path = Some(PathBuf::from(other));
                }
            }
        }
    }
    let path: PathBuf = session_path.unwrap_or_else(|| {
        let home = env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(format!("{home}/.local/share/octo/whatsapp/default.session.db"))
    });
    if !path.exists() {
        eprintln!("error: session path does not exist: {}", path.display());
        return ExitCode::from(3);
    }
    if json_mode {
        match dump_as_json(&path) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("json dump failed: {e}");
                return ExitCode::from(1);
            }
        }
    } else {
        dump_text(&path);
    }
    ExitCode::SUCCESS
}

fn dsn(p: &std::path::Path) -> String {
    format!("file://{}", p.display())
}

fn count_table(p: &std::path::Path, table: &str) -> i64 {
    let Ok(db) = stoolap::Database::open(&dsn(p)) else {
        return -1;
    };
    let Ok(mut rows) = db.query(&format!("SELECT COUNT(*) FROM {table}"), ()) else {
        return -1;
    };
    if let Some(Ok(row)) = rows.next() {
        row.get(0).unwrap_or(0)
    } else {
        0
    }
}

fn dump_text(path: &std::path::Path) {
    println!("== whatsapp_session_introspect ==");
    println!("session path    : {}", path.display());
    println!();

    // Top-line row counts (matches `inspect_session_db`).
    println!("-- row counts --");
    let tables = [
        "device",
        "identities",
        "sessions",
        "prekeys",
        "signed_prekeys",
        "sender_keys",
        "app_state_keys",
        "app_state_versions",
        "app_state_mutation_macs",
        "lid_pn_mapping",
        "device_registry",
        "sender_key_devices",
        "sent_messages",
        "base_keys",
        "tc_tokens",
    ];
    for t in tables {
        let n = count_table(path, t);
        println!("  {t:<26} : {n}");
    }

    let store = match StoolapStore::new(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not open store: {e}");
            return;
        }
    };

    // Device-level crypto blobs + fingerprints.
    if let Some((noise_key, identity_key, signed_pre_key, push_name, avp, avs, avt, registration_id)) =
        store.read_device_keys().ok().flatten()
    {
        println!("\n-- device (key fields) --");
        println!("  registration_id      : {registration_id}");
        println!("  push_name            : {push_name:?}    (EMPTY = red flag)");
        println!("  app_version          : {avp}.{avs}.{avt}");
        println!(
            "  noise_key        sha256 = {}",
            hex::encode(Sha256::digest(&noise_key))
        );
        println!(
            "  identity_key     sha256 = {}",
            hex::encode(Sha256::digest(&identity_key))
        );
        println!(
            "  signed_pre_key   sha256 = {}",
            hex::encode(Sha256::digest(&signed_pre_key))
        );
        println!(
            "  concatenated     sha256 = {}",
            hex::encode(Sha256::digest(
                [noise_key, identity_key, signed_pre_key].concat()
            ))
        );
    } else {
        println!("\n-- device: NONE (fresh / unpaired) --");
    }

    // Decoded JSON columns from device.
    dump_decoded_blob(path, "server_cert_chain", "device", "id = 1");
    dump_decoded_blob(path, "edge_routing_info", "device", "id = 1");
    dump_decoded_blob(path, "account", "device", "id = 1");

    // identities.
    if let Ok(db) = stoolap::Database::open(&dsn(path)) {
        if let Ok(mut rows) = db.query("SELECT address, length(\"key\") FROM identities", ()) {
            println!("\n-- identities --");
            let mut any = false;
            while let Some(Ok(row)) = rows.next() {
                any = true;
                let addr: String = row.get(0).unwrap_or_default();
                let len: i64 = row.get(1).unwrap_or(0);
                println!("  {addr}    key={len}B");
            }
            if !any {
                println!("  (empty)");
            }
        }
        if let Ok(mut rows) = db.query("SELECT address, length(record) FROM sessions", ()) {
            println!("\n-- sessions --");
            let mut any = false;
            while let Some(Ok(row)) = rows.next() {
                any = true;
                let addr: String = row.get(0).unwrap_or_default();
                let len: i64 = row.get(1).unwrap_or(0);
                println!("  {addr}    record={len}B");
            }
            if !any {
                println!("  (empty)");
            }
        }
        if let Ok(mut rows) = db.query(
            "SELECT id, length(\"key\"), uploaded FROM prekeys ORDER BY id",
            (),
        ) {
            println!("\n-- prekeys --");
            let mut total = 0;
            let mut uploaded = 0;
            let mut first_id = i64::MAX;
            let mut last_id = i64::MIN;
            while let Some(Ok(row)) = rows.next() {
                let id: i64 = row.get(0).unwrap_or(0);
                let len: i64 = row.get(1).unwrap_or(0);
                let u: i64 = row.get(2).unwrap_or(0);
                total += 1;
                if u != 0 {
                    uploaded += 1;
                }
                if id < first_id {
                    first_id = id;
                }
                if id > last_id {
                    last_id = id;
                }
                if total <= 5 {
                    println!("  id={id:<6} key={len}B  uploaded={u}");
                }
            }
            println!("  total={total}  uploaded={uploaded}  id_range=[{first_id}..{last_id}]");
        }
        if let Ok(mut rows) = db.query(
            "SELECT id, length(record) FROM signed_prekeys ORDER BY id",
            (),
        ) {
            println!("\n-- signed_prekeys --");
            let mut any = false;
            while let Some(Ok(row)) = rows.next() {
                any = true;
                let id: i64 = row.get(0).unwrap_or(0);
                let len: i64 = row.get(1).unwrap_or(0);
                println!("  id={id:<6} record={len}B");
            }
            if !any {
                println!("  (empty — should have >=1 after first connect)");
            }
        }
        if let Ok(mut rows) = db.query(
            "SELECT lid, phone_number, learning_source FROM lid_pn_mapping",
            (),
        ) {
            println!("\n-- lid_pn_mapping --");
            let mut any = false;
            while let Some(Ok(row)) = rows.next() {
                any = true;
                let lid: String = row.get(0).unwrap_or_default();
                let pn: String = row.get(1).unwrap_or_default();
                let src: String = row.get(2).unwrap_or_default();
                println!("  {lid}  ->  {pn}  ({src})");
            }
            if !any {
                println!("  (empty)");
            }
        }
        if let Ok(mut rows) = db.query(
            "SELECT name, length(state_data) FROM app_state_versions",
            (),
        ) {
            println!("\n-- app_state_versions --");
            let mut any = false;
            while let Some(Ok(row)) = rows.next() {
                any = true;
                let n: String = row.get(0).unwrap_or_default();
                let len: i64 = row.get(1).unwrap_or(0);
                println!("  {n:<32} state_data={len}B");
            }
            if !any {
                println!("  (empty)");
            }
        }
        if let Ok(mut rows) = db.query(
            "SELECT raw_id, length(devices_json) FROM device_registry",
            (),
        ) {
            println!("\n-- device_registry --");
            let mut any = false;
            while let Some(Ok(row)) = rows.next() {
                any = true;
                let r: String = row.get(0).unwrap_or_default();
                let len: i64 = row.get(1).unwrap_or(0);
                println!("  raw_id={r} devices_json={len}B");
            }
            if !any {
                println!("  (empty)");
            }
        }
    }
}

fn dump_decoded_blob(p: &std::path::Path, col: &str, table: &str, where_: &str) {
    let Ok(db) = stoolap::Database::open(&dsn(p)) else {
        return;
    };
    let sql = format!("SELECT {col} FROM {table} WHERE {where_}");
    let Ok(mut rows) = db.query(&sql, ()) else {
        return;
    };
    let Some(Ok(row)) = rows.next() else {
        return;
    };
    let bytes: Vec<u8> = row.get(0).unwrap_or_default();
    println!("\n-- device.{col} ({} bytes) --", bytes.len());
    if bytes.is_empty() {
        println!("  (empty)");
        return;
    }
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(v) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&v)
                    .unwrap_or_default()
                    .lines()
                    .map(|l| format!("  {l}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        Err(_) => {
            println!("  (not JSON — likely raw bytes; showing first 32B hex)");
            for chunk in bytes.chunks(32).take(2) {
                println!("  {}", hex::encode(chunk));
            }
        }
    }
}

fn dump_as_json(p: &std::path::Path) -> anyhow::Result<String> {
    // Lightweight JSON dump — captures the key signal of a session.
    let mut out = serde_json::Map::new();
    out.insert(
        "session_path".into(),
        serde_json::Value::String(p.display().to_string()),
    );

    // Row counts.
    let tables = [
        "device",
        "identities",
        "sessions",
        "prekeys",
        "signed_prekeys",
        "lid_pn_mapping",
        "device_registry",
        "app_state_versions",
        "tc_tokens",
    ];
    let mut counts = serde_json::Map::new();
    for t in tables {
        counts.insert(t.into(), serde_json::Value::from(count_table(p, t)));
    }
    out.insert("row_counts".into(), serde_json::Value::Object(counts));

    // device fields.
    let store = StoolapStore::new(p)?;
    if let Some((noise_key, identity_key, signed_pre_key, push_name, avp, avs, avt, registration_id)) =
        store.read_device_keys().ok().flatten()
    {
        out.insert("device".into(), serde_json::json!({
            "registration_id": registration_id,
            "push_name": push_name,
            "app_version": format!("{avp}.{avs}.{avt}"),
            "noise_key_sha256": hex::encode(Sha256::digest(&noise_key)),
            "identity_key_sha256": hex::encode(Sha256::digest(&identity_key)),
            "signed_pre_key_sha256": hex::encode(Sha256::digest(&signed_pre_key)),
        }));
    }
    Ok(serde_json::to_string_pretty(&out).unwrap_or_default())
}
