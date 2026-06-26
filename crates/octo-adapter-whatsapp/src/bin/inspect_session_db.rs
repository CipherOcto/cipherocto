use std::collections::HashSet;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{home}/.local/share/octo/whatsapp/default.session.db")
    });

    let dsn = format!("file://{path}");
    let db = stoolap::Database::open(&dsn).unwrap_or_else(|e| {
        eprintln!("Failed to open {path}: {e}");
        std::process::exit(1);
    });

    // Row counts for all tables
    println!("=== Row counts ===");
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
    for table in &tables {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        match db.query(&sql, ()) {
            Ok(mut rows) => {
                if let Some(Ok(row)) = rows.next() {
                    let count: i64 = row.get(0).unwrap_or(0);
                    println!("  {table}: {count}");
                }
            }
            Err(e) => println!("  {table}: error: {e}"),
        }
    }

    // 1. All unique group_jids from sender_key_devices
    println!("\n=== sender_key_devices group_jids ===");
    let mut rows = db
        .query(
            "SELECT DISTINCT group_jid FROM sender_key_devices WHERE device_id = 1",
            (),
        )
        .unwrap();
    let mut groups: HashSet<String> = HashSet::new();
    while let Some(Ok(row)) = rows.next() {
        if let Ok(jid) = row.get::<String>(0) {
            groups.insert(jid);
        }
    }
    println!("Found {} unique group JIDs:", groups.len());
    for g in &groups {
        println!("  {}", g);
    }

    // 2. All unique addresses from sessions (includes @g.us groups)
    println!("\n=== sessions addresses (group chats) ===");
    let mut rows = db
        .query(
            "SELECT DISTINCT address FROM sessions WHERE device_id = 1",
            (),
        )
        .unwrap();
    let mut session_groups: Vec<String> = Vec::new();
    while let Some(Ok(row)) = rows.next() {
        if let Ok(addr) = row.get::<String>(0) {
            if addr.contains("@g.us") {
                session_groups.push(addr);
            }
        }
    }
    println!("Found {} group chat sessions:", session_groups.len());
    for g in &session_groups {
        println!("  {}", g);
    }

    // 3. All unique addresses from identities (includes @g.us groups)
    println!("\n=== identities addresses (group chats) ===");
    let mut rows = db
        .query(
            "SELECT DISTINCT address FROM identities WHERE device_id = 1",
            (),
        )
        .unwrap();
    let mut identity_groups: Vec<String> = Vec::new();
    while let Some(Ok(row)) = rows.next() {
        if let Ok(addr) = row.get::<String>(0) {
            if addr.contains("@g.us") {
                identity_groups.push(addr);
            }
        }
    }
    println!("Found {} group chat identities:", identity_groups.len());
    for g in &identity_groups {
        println!("  {}", g);
    }

    // 4. sent_messages unique chat_jids
    println!("\n=== sent_messages unique chat_jids (groups) ===");

    // 8. conversations table
    println!("\n=== conversations ===");
    let mut rows = db
        .query(
            "SELECT jid, name, is_group, updated_at FROM conversations LIMIT 20",
            (),
        )
        .unwrap();
    let mut conv_count = 0i64;
    while let Some(Ok(row)) = rows.next() {
        conv_count += 1;
        let jid: String = row.get(0).unwrap_or_default();
        let name: String = row.get(1).unwrap_or_default();
        let is_group: i64 = row.get(2).unwrap_or(0);
        let updated_at: i64 = row.get(3).unwrap_or(0);
        println!(
            "  jid={} name={:?} is_group={} updated_at={}",
            jid, name, is_group, updated_at
        );
    }
    // total count
    let mut rows = db.query("SELECT COUNT(*) FROM conversations", ()).unwrap();
    if let Some(Ok(row)) = rows.next() {
        let total: i64 = row.get(0).unwrap_or(0);
        println!("Total conversations: {}", total);
    }
    let mut rows = db
        .query("SELECT COUNT(*) FROM conversations WHERE is_group = 1", ())
        .unwrap();
    if let Some(Ok(row)) = rows.next() {
        let total: i64 = row.get(0).unwrap_or(0);
        println!("  (of which {} are groups)", total);
    }
    let mut rows = db
        .query(
            "SELECT DISTINCT chat_jid FROM sent_messages WHERE device_id = 1",
            (),
        )
        .unwrap();
    let mut msg_groups: Vec<String> = Vec::new();
    while let Some(Ok(row)) = rows.next() {
        if let Ok(jid) = row.get::<String>(0) {
            if jid.contains("@g.us") {
                msg_groups.push(jid);
            }
        }
    }
    println!(
        "Found {} group chat_jids in sent_messages:",
        msg_groups.len()
    );
    for g in &msg_groups {
        println!("  {}", g);
    }

    // 5. tc_tokens unique jids (includes groups)
    println!("\n=== tc_tokens unique jids (groups only) ===");
    let mut rows = db.query("SELECT DISTINCT jid FROM tc_tokens", ()).unwrap();
    let mut tc_groups: Vec<String> = Vec::new();
    while let Some(Ok(row)) = rows.next() {
        if let Ok(jid) = row.get::<String>(0) {
            if jid.contains("@g.us") {
                tc_groups.push(jid);
            }
        }
    }
    println!("Found {} group JIDs in tc_tokens:", tc_groups.len());
    for g in &tc_groups {
        println!("  {}", g);
    }

    // 6. Sample app_state_mutation_macs entries
    println!("\n=== Sample app_state_mutation_macs (first 10 from regular_high) ===");
    let mut rows = db
        .query(
            "SELECT name, version, index_mac FROM app_state_mutation_macs WHERE name = 'regular_high' LIMIT 10",
            (),
        )
        .unwrap();
    while let Some(Ok(row)) = rows.next() {
        let name: String = row.get(0).unwrap_or_default();
        let version: i64 = row.get(1).unwrap_or(0);
        let index_mac: Vec<u8> = row.get(2).unwrap_or_default();
        println!(
            "  name={} version={} index_mac={:?}",
            name, version, index_mac
        );
    }

    // 7. app_state_versions
    println!("\n=== app_state_versions ===");
    let mut rows = db
        .query(
            "SELECT name, state_data FROM app_state_versions WHERE device_id = 1",
            (),
        )
        .unwrap();
    while let Some(Ok(row)) = rows.next() {
        let name: String = row.get(0).unwrap_or_default();
        let state_data: Vec<u8> = row.get(1).unwrap_or_default();
        println!("  name={} state_data_len={}", name, state_data.len());
    }
}
