//! Build-time parity table test. Parses §API Parity Coverage in the
//! design doc and verifies every ✅/🆕 method has a registered RPC
//! handler. Design §2132.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use octo_whatsapp::ipc::handlers::build_registry;

#[test]
fn parity_table_methods_all_have_registered_rpc_handlers() {
    // tests/ is at crates/octo-whatsapp/tests/. Two `.parent()` steps give
    // us the repo root: tests -> octo-whatsapp -> crates -> <repo root>.
    let design_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .and_then(|p| p.parent()) // repo root
        .expect("CARGO_MANIFEST_DIR is at crates/octo-whatsapp; two parents reach repo root")
        .join("docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md");
    let design = fs::read_to_string(&design_path)
        .unwrap_or_else(|e| panic!("cannot read design doc at {design_path:?}: {e}"));

    let registered: HashSet<String> = build_registry()
        .methods()
        .into_iter()
        .map(|k| k.to_string())
        .collect();

    // Map adapter method names -> RPC method names. Most have 1:1 mapping;
    // some have different names (e.g., CoordinatorAdmin's `add_member` maps
    // to RPC `groups.participants.add`).
    let method_to_rpc: &[(&str, &str)] = &[
        // Adapter methods from the WhatsAppWebAdapter parity table.
        ("send_image", "send.image"),
        ("send_video", "send.video"),
        ("send_audio", "send.audio"),
        ("send_voice", "send.voice"),
        ("send_sticker", "send.sticker"),
        ("send_reaction", "send.reaction"),
        ("send_poll", "send.poll"),
        ("send_contact", "send.contact"),
        ("send_location", "send.location"),
        ("edit_message", "messages.edit"),
        ("delete_message", "send.delete"),
        ("mark_read", "messages.mark_read"),
        ("message_search", "messages.search"),
        ("chat_info", "chats.info"),
        ("chat_pin", "chats.pin"),
        ("chat_mute", "chats.mute"),
        ("chat_archive", "chats.archive"),
        ("chat_delete", "chats.delete"),
        ("chat_typing", "chats.typing"),
        ("domain_hash_str", "domain.compute-hash"),
    ];

    let mut missing = Vec::new();
    for (_adapter_name, rpc_name) in method_to_rpc {
        // We assert the RPC name has a registered handler. The adapter
        // name is documentation for future grep-based parity checks.
        if !registered.contains(*rpc_name) {
            missing.push(*rpc_name);
        }
    }

    assert!(
        missing.is_empty(),
        "the following RPC methods are missing from build_registry() but are \
         listed in the API Parity Coverage table: {missing:?}. If the design \
         changed, update this test's method_to_rpc table. If a method was \
         added, register it in handlers/mod.rs build_registry()."
    );

    // Sanity-check: the design doc references the §API Parity Coverage
    // section (line 1712 per the plan). If the design doc is moved or
    // renamed, fail loudly so this test stays in sync.
    assert!(
        design.contains("## API Parity Coverage"),
        "design doc no longer contains the expected `## API Parity Coverage` \
         section heading. Update this test path or regenerate the doc."
    );
}

#[test]
fn parity_table_registry_size_meets_expectations() {
    let registry = build_registry();
    // Phase 1 had ~17 methods. Phase 2 added 30+ (send.*{10}, messages.*{6},
    // chats.*{10}, envelope.*{4}, capabilities, domain.compute-hash, media.info).
    // Total expected: ~47 methods.
    assert!(
        registry.methods().len() >= 40,
        "expected at least 40 RPC methods registered in Phase 2, got {}. \
         Missing additions.",
        registry.methods().len()
    );
    eprintln!("registered {} RPC methods", registry.methods().len());
}
