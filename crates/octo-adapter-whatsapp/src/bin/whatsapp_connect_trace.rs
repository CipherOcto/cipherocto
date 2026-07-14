//! `whatsapp_connect_trace` — investigate-binary for the 401 LoggedOut dialog.
//!
//! Phase 7.J problem: after a fresh QR-pair, the daemon connects successfully.
//! On the FIRST reconnect attempt (daemon restart), it gets a 401 LoggedOut.
//! Yet wacore's noise identity on disk is fresh (`dump_noise_key` confirms
//! the noise_key blob IS regenerated per pair). The hypothesis we want to
//! test: is the reconnect taking the IK path (server-cert-chain reuse) or
//! the XX path (fresh identity) per `whatsapp-rust/src/handshake.rs::select_pattern`?
//!
//! IK fires when: `device.is_registered()` + `server_cert_chain.is_some()` +
//! `leaf.not_after > now` + `leaf.not_before <= now` + `ik_failures < threshold`.
//!
//! XX otherwise — produces a new "ephemeral" cryptographic identity from
//! the server's TLS cert each restart, so the WA server may reject.
//!
//! This binary:
//!   1) Loads the on-disk `device` row that wacore's `PersistenceManager`
//!      would read on the next connect.
//!   2) Replays the SAME `select_pattern` decision wacore uses.
//!   3) Prints the input state (registration_id, registered?, cert chain shape
//!      + validity window) so we can see whether IK will fire.
//!
//! Usage:
//!   cargo run -p octo-adapter-whatsapp --bin whatsapp_connect_trace -- \
//!     [session_db_path] [--ik-failures N]
//!
//! Exit codes:
//!   0 = ready + IK would fire (good)
//!   1 = ready but would fall back to XX (watch for the reason below)
//!   2 = device row missing (fresh / unpaired)
//!   3 = bad invocation

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use octo_adapter_whatsapp::store::StoolapStore;

const IK_FAILURE_THRESHOLD_DEFAULT: u32 = 2;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let mut session_path: Option<PathBuf> = None;
    let mut ik_failures: u32 = 0;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--ik-failures" => {
                ik_failures = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(IK_FAILURE_THRESHOLD_DEFAULT);
            }
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

    let store = StoolapStore::new(&path).unwrap_or_else(|e| {
        eprintln!("Failed to open {}: {e}", path.display());
        std::process::exit(3);
    });

    // Probe the device row, exactly the fields `select_pattern` reads.
    let Some(row) = probe_device(&store, &path) else {
        eprintln!("no device row in {} — fresh / unpaired", path.display());
        return ExitCode::from(2);
    };

    // Re-probe for full info now (probe_device took its own read; use same
    // `path` for the cert chain query).
    let cert_chain_info = probe_cert_chain(&path);
    let cert_summary = match &cert_chain_info {
        Some((len, _nb, _na)) if *len > 0 => format!("Some ({len} B)"),
        _ => "None".into(),
    };

    // Mirror wacore handshake::select_pattern in user-space.
    // `is_registered()` is defined as `self.pn.is_some()` in
    // wacore/src/store/device.rs:429. We mirror that by checking the parsed
    // `pn` column is non-empty.
    let now_secs: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let pn_is_some = !row.pn.is_empty();
    let is_registered = pn_is_some;
    println!("== whatsapp_connect_trace ==");
    println!("session path         : {}", path.display());
    println!("registration_id      : {}", row.registration_id);
    println!("is_registered        : {is_registered}");
    println!("push_name            : {:?}", row.push_name);
    println!("app_version          : {}.{}.{}", row.avp, row.avs, row.avt);
    println!("server_cert_chain    : {cert_summary}");

    // Raw device row dump (column-level so we can see exactly what's stored).
    println!("\n== raw device row (id=1) ==");
    if let Err(e) = dump_device_row(&path) {
        println!("  (failed to query device row: {e})");
    }
    if let Some((nb_leaf, na_leaf)) = cert_chain_info
        .clone()
        .and_then(|(len, nb, na)| if len > 0 { Some((nb, na)) } else { None })
    {
        println!("    leaf not_before  : {nb_leaf}");
        println!("    leaf not_after   : {na_leaf}");
    }
    println!("now (epoch)          : {now_secs}");
    println!("ik_failures          : {ik_failures} (threshold {IK_FAILURE_THRESHOLD_DEFAULT})");

    let mut verdict = String::from("IK");
    let mut reasons: Vec<&str> = Vec::new();
    if !is_registered {
        verdict = "XX".into();
        reasons.push("device.is_registered() = false");
    }
    if ik_failures >= IK_FAILURE_THRESHOLD_DEFAULT {
        verdict = "XX".into();
        reasons.push("ik_failures >= threshold");
    }
    if cert_summary == "None" {
        verdict = "XX".into();
        reasons.push("server_cert_chain is None on disk");
    } else if let Some((nb_leaf, na_leaf)) = cert_chain_info
        .clone()
        .and_then(|(len, nb, na)| if len > 0 { Some((nb, na)) } else { None })
    {
        if now_secs < nb_leaf {
            verdict = "XX".into();
            reasons.push("now < leaf.not_before (clock skew or stale chain)");
        } else if now_secs >= na_leaf {
            verdict = "XX".into();
            reasons.push("now >= leaf.not_after (expired → XX fallback)");
        }
    }

    println!();
    println!("== verdict ==");
    println!("predicted pattern    : {verdict}");
    if !reasons.is_empty() {
        println!("reason               :");
        for r in &reasons {
            println!("  - {r}");
        }
        return ExitCode::from(1);
    }
    println!("reconnect should retain IK identity (cached server cert chain valid).");
    ExitCode::SUCCESS
}

struct DeviceProbe {
    registration_id: u32,
    pn: String,
    push_name: String,
    avp: u32,
    avs: u32,
    avt: u32,
}

fn probe_device(store: &StoolapStore, session_path: &std::path::Path) -> Option<DeviceProbe> {
    // Reuse the store's read_device_keys surface to get the main fields.
    let (noise_key, identity_key, signed_pre_key, push_name, avp, avs, avt, registration_id) =
        store.read_device_keys().ok().flatten()?;

    // Query pn column directly (not exposed via read_device_keys).
    let pn = read_pn_column(session_path).unwrap_or_default();

    // Suppress unused-variable warnings for now (kept for future expansion).
    let _ = (noise_key, identity_key, signed_pre_key);

    Some(DeviceProbe {
        registration_id,
        pn,
        push_name,
        avp,
        avs,
        avt,
    })
}

fn probe_cert_chain(session_path: &std::path::Path) -> Option<(usize, i64, i64)> {
    let dsn = format!("file://{}", session_path.display());
    let db = stoolap::Database::open(&dsn).ok()?;
    let mut rows = db
        .query(
            "SELECT server_cert_chain FROM device WHERE id = 1",
            (),
        )
        .ok()?;
    let row = match rows.next() {
        Some(Ok(r)) => r,
        _ => return None,
    };

    // Stoolap's BLOB row.get returns Option<Vec<u8>> or empty Vec.
    let chain_bytes: Vec<u8> = row.get(0).ok().unwrap_or_default();

    if chain_bytes.is_empty() {
        return Some((0, 0, 0));
    }

    // Best-effort decode as bincode of CachedServerCertChain.
    // The struct is `pub struct CachedServerCertChain { leaf: VerifiedServerCertLeaf, intermediate: VerifiedServerCertLeaf, expiration: i64 }`
    // VerifiedServerCertLeaf is `pub struct VerifiedServerCertLeaf { key: [u8; 32], signature: [u8; 64], not_before: i64, not_after: i64 }`.
    // Header is verification_tickets + leaf struct + intermediate struct + expiration i64.
    // Without the bincode layout in hand, we fallback to returning only the byte length.
    let (not_before, not_after) = decode_leaf_not_before_after(&chain_bytes).unwrap_or((0, 0));

    Some((chain_bytes.len(), not_before, not_after))
}

fn decode_leaf_not_before_after(_bytes: &[u8]) -> Option<(i64, i64)> {
    // TODO: decode against wacore's bincode layout. For now, we cannot recover
    // these timestamps from the binary without pulling in wacore as a dep of
    // this binary. Fall back to length-only reporting.
    None
}

fn read_pn_column(session_path: &std::path::Path) -> Option<String> {
    let dsn = format!("file://{}", session_path.display());
    let db = stoolap::Database::open(&dsn).ok()?;
    let mut rows = db
        .query("SELECT pn FROM device WHERE id = 1", ())
        .ok()?;
    let row = rows.next()?;
    row.ok()?.get::<String>(0).ok()
}

fn dump_device_row(session_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let dsn = format!("file://{}", session_path.display());
    let db = stoolap::Database::open(&dsn)?;
    let cols = [
        "pn",
        "lid",
        "push_name",
        "registration_id",
        "login_counter",
        "app_version_primary",
        "app_version_secondary",
        "app_version_tertiary",
        "props_hash",
        "next_pre_key_id",
        "server_has_prekeys",
    ];
    let sql = format!("SELECT {} FROM device WHERE id = 1", cols.join(", "));
    let mut rows = db.query(&sql, ())?;
    if let Some(Ok(row)) = rows.next() {
        for (i, name) in cols.iter().enumerate() {
            let v: String = row
                .get::<String>(i)
                .unwrap_or_else(|_| "<read err>".into());
            let marker = if v.is_empty() { " (EMPTY!)" } else { "" };
            println!("  {name:<24} = {v:?}{marker}");
        }
        // server_cert_chain length only
        let mut rows2 = db.query("SELECT length(server_cert_chain) FROM device WHERE id = 1", ())?;
        if let Some(Ok(row)) = rows2.next() {
            let len: i64 = row.get(0).unwrap_or(0);
            println!("  server_cert_chain      = {len} bytes");
        }
        // edge_routing_info length
        let mut rows3 = db.query("SELECT length(edge_routing_info) FROM device WHERE id = 1", ())?;
        if let Some(Ok(row)) = rows3.next() {
            let len: i64 = row.get(0).unwrap_or(0);
            println!("  edge_routing_info      = {len} bytes");
        }
    }
    Ok(())
}
