#!/usr/bin/env python3
"""Mutation harness for the 2026-10-04 red-team remediation, and for the
file-scanning guards added since.

A guard that has never failed is not a guard. Each entry is either the exact
revert of a fix or a planted violation of the rule a guard enforces, so a green
run here means the guard detects the real thing rather than a substring that
happens to be present.

Two kinds of entry live here, and they differ in what they can prove. The
red-team entries mutate code, so the test must recompile; their value is that a
guard catches a change it was written for. The file-scanning entries mutate a
file the guard *reads* at run time, so no compilation is involved and they
establish that the check inspects the tree rather than trusting its own logic.

Three outcomes are distinguished, because collapsing them hides real defects:

  RED ok        the guard fired, and it fired for the right reason
  BAD-MUTATION  the mutation did not compile, so cargo exited non-zero and
                the guard never ran - this is NOT a red guard
  VACUOUS       the guard stayed green, i.e. it does not detect its defect

Usage: python3 scripts/mutate-zk-guards.py [--only SUBSTRING] [--cheap] [--list]

`--list` prints each entry with what it costs to run and derives the coverage
figure from this table, so the count of guarded guards is never a hand-kept
number in prose.

An argument this script does not recognise is refused rather than interpreted.
The default here used to be the full table, which is expensive enough on a
constrained disk to matter.
"""
USAGE = """Usage: python3 scripts/mutate-zk-guards.py [--only SUBSTRING] [--cheap] [--list] [--help]

  --list           print every entry with what it costs to run, plus coverage
  --cheap          run only the entries that need no recompile
  --only SUBSTRING run entries whose label contains SUBSTRING
  --help           print this message and run nothing

Unrecognised arguments are refused. They used to run the entire table."""
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]  # scripts/ -> repo root

# (label, file, old, new, test-command)
MUTATIONS = [
    # ── C-1: the stub verifier feature must never reach a production build ──
    (
        "C-1: production feature edge restored",
        "crates/quota-router-core/Cargo.toml",
        r'''zk-verifier = { path = "../zk-verifier" }''',
        r'''zk-verifier = { path = "../zk-verifier", features = ["allow-stub-verifier"] }''',
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "c1_stub_verifier_feature_is_dev_only", "--", "--exact"],
    ),
    (
        "C-1: feature smuggled into a default feature array",
        "crates/zk-verifier/Cargo.toml",
        "allow-stub-verifier = []",
        'default = ["allow-stub-verifier"]\nallow-stub-verifier = []',
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "c1_stub_verifier_feature_is_dev_only", "--", "--exact"],
    ),
    (
        "C-1: fail-closed arm replaced with accept",
        "crates/zk-verifier/src/lib.rs",
        "Err(VerifyError::StubDisabled)\n    }",
        "Ok(())\n    }",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "c1_the_fail_closed_arm_still_exists", "--", "--exact"],
    ),

    # ── M-4: the FFI library path is a trust boundary ──────────────────────
    (
        "M-4: validation removed from loaded_library",
        "crates/zk-vendor/src/lib.rs",
        r'''        if let Err(e) = validate_library_path(&path) {
            warn!(
                "refusing to load stwo-sys from {}: {} (treating library as absent; \
                 verification falls back to the fail-closed stub-disabled arm)",
                path.display(),
                e
            );
            return None;
        }
''',
        "",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m4_loaded_library_validates_before_dlopen", "--", "--exact"],
    ),
    (
        "M-4: world-writable permission check weakened",
        "crates/zk-vendor/src/lib.rs",
        "if md.permissions().mode() & 0o022 != 0 {",
        "if md.permissions().mode() & 0o400 != 0 {",
        ["cargo", "test", "-p", "zk-vendor", "--test", "library_path"],
    ),
    (
        # Anchor is the expression, not the surrounding statement: rustfmt
        # rewrote this `match` into a `let ... else`, so a statement-shaped
        # anchor silently stopped matching and the mutation got skipped.
        "M-4: symlinks no longer resolved before the trust check",
        "crates/zk-vendor/src/lib.rs",
        "let Ok(md) = std::fs::metadata(path) else {",
        "let Ok(md) = std::fs::symlink_metadata(path) else {",
        ["cargo", "test", "-p", "zk-vendor", "--test", "library_path",
         "m4_symlink_is_followed_to_the_resolved_target", "--", "--exact"],
    ),
    (
        "M-4: absolute-path rule dropped",
        "crates/zk-vendor/src/lib.rs",
        "    if !path.is_absolute() {\n        return Err(LibraryPathError::NotAbsolute(path.to_path_buf()));\n    }\n\n",
        "",
        ["cargo", "test", "-p", "zk-vendor", "--test", "library_path",
         "m4_relative_path_is_refused", "--", "--exact"],
    ),

    # ── M-5: the commitment must cover its whole input type ────────────────
    (
        "M-5: a public input field dropped from the transcript",
        "crates/zk-verifier/src/lib.rs",
        "    out.extend_from_slice(&public.proof_issued_at_unix.to_le_bytes());\n",
        "",
        ["cargo", "test", "-p", "zk-verifier", "m5_"],
    ),
    (
        # All three variable-length fields, not just one. Dropping a single
        # prefix is NOT exploitable: the next field's prefix re-syncs the
        # boundary, so the transcripts still differ. Only total absence of
        # prefixes makes ("ab","c") and ("a","bc") collide.
        "M-5: length prefixes removed from the transcript",
        "crates/zk-verifier/src/lib.rs",
        r'''    out.extend_from_slice(&leb128_len(public.compiled_casm_hash.as_bytes()));
    out.extend_from_slice(public.compiled_casm_hash.as_bytes());
    out.extend_from_slice(&leb128_len(public.capability_root_hash.as_bytes()));
    out.extend_from_slice(public.capability_root_hash.as_bytes());
    out.extend_from_slice(&leb128_len(public.provider_slot_id.as_bytes()));
    out.extend_from_slice(public.provider_slot_id.as_bytes());''',
        r'''    out.extend_from_slice(public.compiled_casm_hash.as_bytes());
    out.extend_from_slice(public.capability_root_hash.as_bytes());
    out.extend_from_slice(public.provider_slot_id.as_bytes());''',
        ["cargo", "test", "-p", "zk-verifier", "m5_length_prefixes"],
    ),

    # ── M-11: the session-swap script must not trust its base directory ────
    (
        "M-11: group/world-writable check dropped",
        "scripts/swap_sessions.sh",
        '    if [[ -n "$dir_mode" ]] && (( 8#$dir_mode & 0022 )); then\n        die "persist dir is group- or world-writable (mode $dir_mode): $DIR"\n    fi\n',
        "",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m11_swap_sessions_refuses_an_untrusted_persist_dir", "--", "--exact"],
    ),
    (
        # The bug actually introduced and caught during that fix: `stat -c %a`
        # prints octal, and reading it as decimal makes a stock 0755 directory
        # look group/world-writable.
        "M-11: mode re-read as decimal instead of octal",
        "scripts/swap_sessions.sh",
        "(( 8#$dir_mode & 0022 ))",
        "(( 10#$dir_mode & 0022 ))",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m11_swap_sessions_refuses_an_untrusted_persist_dir", "--", "--exact"],
    ),
    (
        "M-11: symlink check dropped",
        "scripts/swap_sessions.sh",
        '[[ ! -L "$DIR" ]] || die "persist dir must not be a symlink: $DIR"\n',
        "",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m11_swap_sessions_refuses_a_relative_or_symlinked_persist_dir", "--", "--exact"],
    ),
    (
        "M-11: post-swap identity verification removed",
        "scripts/swap_sessions.sh",
        '    ok "identity verified: $A and $B swapped inodes as expected"\n',
        "",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m11_swap_sessions_verifies_that_identity_actually_moved", "--", "--exact"],
    ),

    # ── H-6: WhatsApp session dumps must never be world-readable ───────────
    (
        "H-6: a dump goes back to bare std::fs::write (umask 0644)",
        "crates/whatsapp_chrome_session_extract/src/bin/whatsapp_capture_import_key.rs",
        "write_private_json(&out_path, &parsed)?;",
        "std::fs::write(&out_path, serde_json::to_string_pretty(&parsed)?)?;",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "h6_no_session_dump_is_written_with_a_umask_derived_mode", "--", "--exact"],
    ),
    (
        # The "tighten an existing loose file" branch is what stops a dump
        # left behind by an older build from staying readable forever.
        "H-6: pre-existing loose files no longer tightened",
        "crates/cipherocto-fs-perms/src/lib.rs",
        "    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;\n    Ok(f)",
        "    Ok(f)",
        ["cargo", "test", "-p", "cipherocto-fs-perms",
         "tests::tv_fs_perms_create_private_is_private_from_birth", "--", "--exact"],
    ),
    (
        # The directory twin: `create_dir_all` is a no-op when the path already
        # exists, so without the explicit force a directory left permissive by
        # an older build keeps its old mode forever.
        "H-6: create_dir_private stops forcing 0700 on an existing directory",
        "crates/cipherocto-fs-perms/src/lib.rs",
        "    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;\n    Ok(())",
        "    Ok(())",
        ["cargo", "test", "-p", "cipherocto-fs-perms",
         "tests::tv_fs_perms_create_dir_private_covers_existing_and_new", "--", "--exact"],
    ),

    # ── H-7: the session export bundle must not be world-readable ──────────
    (
        "H-7: export bundle back to fs::write (umask 0644)",
        "crates/octo-whatsapp-onboard-core/src/multi_account.rs",
        "write_private_bytes(out, &compressed).map_err(|e| CoreError::Read {",
        "fs::write(out, &compressed).map_err(|e| CoreError::Read {",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "h7_the_session_export_is_not_written_with_a_umask_derived_mode", "--", "--exact"],
    ),
    (
        "H-7: the local writer reimplements the mode instead of delegating",
        "crates/octo-whatsapp-onboard-core/src/multi_account.rs",
        "    let mut f = cipherocto_fs_perms::create_private(path)?;",
        "    let mut f = std::fs::File::create(path)?;",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "h7_the_session_export_is_not_written_with_a_umask_derived_mode", "--", "--exact"],
    ),
    (
        # The anti-parallel-abstraction guard: this is the exact shape the
        # shared crate exists to prevent - a second private copy of the rule.
        "H-7: a second private copy of create_private reappears in the wallet",
        "crates/octo-wallet/src/fs_perms.rs",
        "pub use cipherocto_fs_perms::{create_dir_private, create_private, sync_parent_dir};",
        "pub use cipherocto_fs_perms::{create_dir_private, sync_parent_dir};\n\n/// A private second copy of the rule.\npub fn create_private(path: &std::path::Path) -> std::io::Result<std::fs::File> {\n    std::fs::File::create(path)\n}",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "h7_private_write_primitives_have_exactly_one_definition", "--", "--exact"],
    ),
    # ── File-scanning guards: the mutation is in a file the guard READS, not in
    # code the guard COMPILES, so the check cannot be satisfied by a rebuild.
    #
    # Both target crates/octo-adapter-irc deliberately. It sits outside
    # octo-cli's 38-crate dependency closure, so mutating it cannot invalidate a
    # fingerprint in the graph `cargo test -p octo-cli` has to build. Mutating a
    # crate octo-cli does depend on would turn a cheap check into a rebuild, and
    # under a tight disk budget that is how a verification step silently stops
    # being runnable.
    (
        # A test with an empty body passes trivially and asserts nothing. The
        # guard exists because a whole suite once did that unnoticed.
        "G-A: a test with an empty body appears in the tree",
        "crates/octo-adapter-irc/src/lib.rs",
        '            "key 20 should be the remaining entry"\n        );\n    }\n}\n',
        '            "key 20 should be the remaining entry"\n        );\n    }\n}\n'
        "\n#[cfg(test)]\nmod probe_vacuity {\n    #[test]\n    fn probe_empty_body() {}\n}\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "no_test_has_an_empty_body", "--", "--exact"],
    ),
    (
        # A path dependency spelled as its own table rather than inline. This
        # form was the guard's blind spot until the parse was taught to take
        # the name from the table header; without a mutation for it, that fix
        # would rest on nothing but the session that made it.
        #
        # The table is named after the package it points at, not after an
        # invented crate. An earlier version of this entry used a made-up name,
        # and cargo refused it with "no matching package named" — which the
        # harness scored as a red guard even though the guard never ran. A
        # dependency key must match the target package unless renamed with
        # `package =`, so the mutation has to name a crate that exists.
        "G-B: an unused section-form path dependency reappears",
        "crates/octo-adapter-irc/Cargo.toml",
        'octo-algo-ed25519 = { path = "../octo-algo-ed25519" }\n',
        'octo-algo-ed25519 = { path = "../octo-algo-ed25519" }\n'
        '\n[dependencies.cipherocto-encoding]\npath = "../cipherocto-encoding"\n',
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "no_path_dependency_is_declared_and_never_referenced", "--", "--exact"],
    ),
    (
        # A `.rs` file naming a private working note. The guard holds Rust source
        # to zero; markdown is a deliberately counted baseline instead, so this
        # mutation targets source where the rule is absolute. A comment is
        # enough to trigger it and cannot fail to compile.
        "G-C: a Rust source file cites a private working note",
        "crates/octo-adapter-irc/src/lib.rs",
        '            "key 20 should be the remaining entry"\n        );\n    }\n}\n',
        '            "key 20 should be the remaining entry"\n        );\n    }\n}\n'
        "\n// Probe: see no-parallel-abstractions for the rationale.\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "no_source_or_doc_file_cites_a_private_working_note", "--", "--exact"],
    ),
    (
        # A mission claiming a named vector that is not in the tree. This is the
        # exact shape the guard exists for: an acceptance box ticked on a claim
        # about code that does not exist. Editing a mission file costs nothing
        # to build, so this entry needs no closure argument at all.
        "G-D: a mission claims a test vector that is absent",
        "missions/claimed/0009-e3-wallet-node-signing-oracle.md",
        "| `the_sign_request_wire_form_is_unchanged`                     | RFC-0871's request contract did not move          |\n",
        "| `the_sign_request_wire_form_is_unchanged`                     | RFC-0871's request contract did not move          |\n"
        "| `probe_absent_vector_name`                                    | a vector that does not exist in the tree          |\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "missions_claiming_named_vectors_have_those_names_in_the_tree", "--", "--exact"],
    ),
    (
        # A second copy of the unverified decoder, outside the one crate allowed
        # to hold it. This is the highest-consequence guard in the suite: a
        # caller reaching a decoder that skips verification is the failure the
        # whole capability chain is built to prevent, so the mutation plants a
        # real duplicate rather than a token-shaped string.
        #
        # It compiles standalone, which is what makes it usable here — reaching
        # the real symbol from another crate would need an import and would put
        # octo-wallet inside the rebuild this harness is trying to avoid.
        "G-E: a second copy of the unverified decoder reappears",
        "crates/octo-adapter-irc/src/lib.rs",
        '            "key 20 should be the remaining entry"\n        );\n    }\n}\n',
        '            "key 20 should be the remaining entry"\n        );\n    }\n}\n'
        "\n/// A private second copy of the unverified decoder, outside the one home the\n"
        "/// guard permits. Exactly the shape that must never reach a caller.\n"
        "#[allow(dead_code)]\n"
        "pub fn unverified_decode_bearer(token: &str) -> String {\n"
        "    token.to_string()\n"
        "}\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "no_production_code_calls_a_placeholder_decoder", "--", "--exact"],
    ),
    (
        # A verification site rebuilding its key from bytes instead of taking the
        # algorithm's key — which is what makes the algorithm parameter able to
        # be anything at all. The guard's own comment records that a past pass
        # proved this by hand; this makes that proof re-runnable.
        #
        # It compiles, which is the part that was not obvious. A fully-qualified
        # path resolves because the crate depends on ed25519-dalek directly, and
        # the path still contains the substring the guard matches. A mutation
        # that merely named the constructor in a comment would have been caught
        # by nothing and proved nothing.
        "G-F: a verification site rebuilds its key from bytes",
        "crates/octo-wallet/src/capability/hop_envelope.rs",
        '        assert_eq!(&wrapped.inner.aad[..4], b"hpaa");\n    }\n}\n',
        '        assert_eq!(&wrapped.inner.aad[..4], b"hpaa");\n    }\n}\n'
        "\n/// Probe: a verification site that rebuilds the key instead of taking it.\n"
        "#[allow(dead_code)]\n"
        "fn probe_hardcoded_key_ctor(bytes: &[u8; 32]) -> bool {\n"
        "    ed25519_dalek::VerifyingKey::from_bytes(bytes).is_ok()\n"
        "}\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "no_verification_site_hardcodes_an_algorithm", "--", "--exact"],
    ),
    (
        # An excuse that outlives the thing it excuses. AC11_OUT_OF_SCOPE holds
        # one entry — the nostr adapter — and the guard asserts the crate still
        # contains the site justifying the exclusion. Deleting the site is the
        # ordinary way that happens (the code gets routed through the registry),
        # and nothing else in the suite would notice: a crate that no longer
        # needs an excuse keeps carrying one, exempting it from the gate for
        # ever with a reason that describes nothing.
        #
        # Commented out rather than deleted so the entry cannot fail to
        # compile. The guard matches against `strip_comments`, so a commented
        # site and an absent site are the same input to it — and this entry
        # additionally pins the fact that it strips comments before looking,
        # which a deletion-based mutation could not distinguish from a guard
        # that simply found the crate empty.
        "G-G: the out-of-scope excuse no longer describes any site",
        "crates/octo-adapter-nostr/src/lib.rs",
        "        let verifying_key = VerifyingKey::from_bytes(&pk_arr).unwrap();",
        "        // let verifying_key = VerifyingKey::from_bytes(&pk_arr).unwrap();",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "ac11_every_out_of_scope_crate_is_justified_and_disjoint", "--", "--exact"],
    ),
    (
        # The workspace-wide coverage walk, one level above every fixed list: a
        # crate that calls `verify_dispatch` and is named in no AC-11 list has
        # never been read. This plants that exact situation — an adapter crate
        # with a call site and no explanation — in a crate that is in no list
        # today, which is the case the guard's own doc comment says is
        # indistinguishable from a clean crate.
        "G-H: an unaccounted crate calls the dispatch entry point",
        "crates/octo-adapter-irc/src/lib.rs",
        '            "key 20 should be the remaining entry"\n        );\n    }\n}\n',
        '            "key 20 should be the remaining entry"\n        );\n    }\n}\n'
        "\n/// Probe: a verification crate the AC-11 lists never name.\n"
        "#[allow(dead_code)]\n"
        "pub fn probe_dispatch(alg: &[u8], key: &[u8], msg: &[u8], sig: &[u8]) -> usize {\n"
        "    verify_dispatch(alg, key, msg, sig)\n"
        "}\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "ac11_every_crate_that_verifies_a_signature_is_scanned_or_explained", "--", "--exact"],
    ),
    (
        # M-15's half that keeps a known limitation declared. Adapters that
        # hardcode Ed25519 must say so at the call site and name the work that
        # would retire it; the note is three comment lines above the call and
        # the guard looks ten lines up for "registry" or "CD-6". Deleting the
        # note is how the declaration gets lost — a cleanup pass that tidies
        # prose above code never touches the code — and the rest of the file
        # keeps the guard green, so nothing else reports it.
        "G-I: the declared-assumption note is deleted from an adapter site",
        "crates/octo-adapter-signal/src/lib.rs",
        "        // Ed25519 named explicitly so this reads as an assumption rather than an\n"
        "        // accident. CD-6's peer registry binds the key AND names the algorithm\n"
        "        // it speaks, so this constant goes away when that registry lands.\n",
        "",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "every_adapter_naming_a_concrete_algorithm_declares_the_assumption", "--", "--exact"],
    ),
    (
        # M-15's "verify against the envelope's own key" half. A hardcoded peer
        # key is the theatre case the guard names in its own message: the call
        # still verifies, still passes two arguments, and still ends the return
        # path — it just proves nothing about the frame that arrived. Both other
        # checks on the same call (arity, trailing `Ok(`) still hold, so only
        # this one can catch it.
        "G-J: an adapter verifies against a hardcoded peer key",
        "crates/octo-adapter-signal/src/lib.rs",
        "        env.verify(&octo_algo_ed25519::ed25519_algorithm_id(), &env.source_peer)\n",
        "        env.verify(&octo_algo_ed25519::ed25519_algorithm_id(), &HARDCODED_PEER_KEY)\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m15_every_adapter_canonicalize_verifies_the_envelope", "--", "--exact"],
    ),
    (
        # The arity half, checked structurally rather than by naming. Since
        # RFC-0009 moved the algorithm to first position, a single-argument call
        # is the pre-dispatch shape: it verifies under whatever the
        # implementation assumes. The guard's comment records that the absence
        # of this check stranded fourteen adapters that did not compile while
        # the gate stayed green — a mutation is the only way to show the check
        # as it stands now would have caught that.
        "G-K: an adapter's verify call loses its algorithm argument",
        "crates/octo-adapter-dingtalk/src/lib.rs",
        "        env.verify(&octo_algo_ed25519::ed25519_algorithm_id(), &env.source_peer)\n",
        "        env.verify(&env.source_peer)\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m15_every_adapter_canonicalize_verifies_the_envelope", "--", "--exact"],
    ),
    (
        # The mutation that was VACUOUS against an earlier version of this
        # guard, recorded in the guard's own comment. Every needle —
        # `last_ping_nonce`, `nonce <= last`, `is_some_and`, the drop message —
        # survives `if false` in front of the whole thing, so the replay branch
        # could be dead code with all four assertions still passing. The guard
        # was hardened to pin the condition afterwards; this entry is what makes
        # that hardening falsifiable instead of merely asserted.
        "G-L: the quic replay branch is conditioned on a constant",
        "crates/octo-adapter-quic/src/lib.rs",
        "                                                                    if last_ping_nonce.is_some_and(\n",
        "                                                                    if false {\n"
        "                                                                    let _ = last_ping_nonce.is_some_and(\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "l10_quic_drops_a_replayed_ping_nonce", "--", "--exact"],
    ),
    (
        # The pairing URI is the secret the phone is about to prove possession
        # of, and terminal output is routinely captured. The guard requires the
        # print to sit behind `if show_secret` AND the withheld branch to state
        # why. Deleting the branch is how an "always useful" default gets
        # reintroduced — printing the URI because the operator debugging a QR
        # renderer could not see it.
        "G-M: the pairing URI is printed without --show-secret",
        "crates/octo-whatsapp-onboard/src/main.rs",
        "                if show_secret {\n",
        "                if true {\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "l8_pairing_uri_is_withheld_unless_show_secret_is_set", "--", "--exact"],
    ),
    (
        # `exists()` → `remove_file()` → `bind()` was the original defect: it
        # both races and cannot tell a socket from a regular file, so an
        # unrelated file at the socket path was silently deleted. `lstat` plus
        # an `is_socket` refusal is what replaced it; this plants the old
        # shape, which is also the shape a future refactor would most easily
        # write from memory.
        "G-N: unix socket bind goes back to an exists() check",
        "crates/octo-whatsapp/src/ipc/server.rs",
        "        match std::fs::symlink_metadata(path) {\n",
        "        if path.exists() {\n"
        "            std::fs::remove_file(path)?;\n"
        "        }\n"
        "        match std::fs::symlink_metadata(path) {\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "l6_unix_socket_bind_refuses_to_delete_a_non_socket", "--", "--exact"],
    ),
    (
        # A same-UID process creates `active` itself in the gap between
        # remove and symlink, and the operator's next session resolves to it.
        # `staging` plus `rename` is the atomic replacement; reverting to the
        # remove-then-create pair keeps every other needle in the function and
        # is exactly the form the guard's second assertion exists to reject.
        "G-O: the active symlink is removed before it is recreated",
        "crates/octo-whatsapp-onboard-core/src/multi_account.rs",
        "            let _ = fs::remove_file(&staging);\n",
        "            let _ = fs::remove_file(&staging);\n"
        "            let _ = fs::remove_file(&active);\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "l7_active_symlink_swap_is_atomic", "--", "--exact"],
    ),
    (
        # The bluetooth adapter has no radio link. Returning a receipt after
        # queueing told callers a message was delivered, and those receipts feed
        # settlement and audit — so this is a stub that reports a fact it did not
        # observe, not merely a stub that returns the wrong type. Swapping the
        # error for an `Ok` keeps the queueing behaviour and only changes the
        # claim made to the caller.
        "G-P: the bluetooth stub issues a delivery receipt",
        "crates/octo-adapter-bluetooth/src/lib.rs",
        "        Err(transport_err(\n"
        "            \"bluetooth adapter is a stub: the envelope was queued, not transmitted. \\\n"
        "             No delivery receipt is issued because no delivery occurred.\",\n"
        "        ))\n",
        "        let _ = transport_err(\n"
        "            \"bluetooth adapter is a stub: the envelope was queued, not transmitted. \\\n"
        "             No delivery receipt is issued because no delivery occurred.\",\n"
        "        );\n"
        "        Ok(DeliveryReceipt {\n"
        "            message_id: envelope.envelope_id.to_string(),\n"
        "            transport: \"bluetooth-stub\".to_string(),\n"
        "            accepted_at_unix: 0,\n"
        "        })\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m16_bluetooth_stub_does_not_issue_a_delivery_receipt", "--", "--exact"],
    ),
    (
        # `#[serde(default)]` on a bool is `false`, and a missing config key is
        # the ordinary case rather than an exceptional one — so the fail-open
        # default reached unauthenticated mutating RPCs, outbound send included,
        # for anyone who did not write the key. Reverting to the bare attribute
        # is the whole defect in one token.
        "G-Q: bearer_required falls back to serde's bool default",
        "crates/octo-whatsapp/src/config.rs",
        '    #[serde(default = "default_true")]\n    pub bearer_required: bool,\n',
        "    #[serde(default)]\n    pub bearer_required: bool,\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m8_bearer_required_defaults_to_true", "--", "--exact"],
    ),
    (
        # `create_dir_all` applies the process umask to newly created
        # directories and never tightens an existing one, so the call has to be
        # paired with an explicit restriction. The guard checks a four-line
        # window after every such call; dropping the pairing line is what an
        # "obvious cleanup" of the parent helper would do, and the directory
        # would end up group- or world-readable around a log of full message
        # bodies.
        "G-R: the events parent directory is created without being restricted",
        "crates/octo-whatsapp/src/events_persister.rs",
        "            tokio::fs::create_dir_all(parent).await?;\n"
        "            restrict_dir_to_owner(parent).await?;\n",
        "            tokio::fs::create_dir_all(parent).await?;\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m9_events_log_files_are_opened_owner_only", "--", "--exact"],
    ),
    (
        # The CDP trace holds request headers and WebSocket frames — the live
        # session. Its default parent is `XDG_RUNTIME_DIR`, which is 0700 by
        # convention precisely because `/tmp` is not. Falling back to `/tmp`
        # keeps the filename and the 0600 mode and still discloses, because the
        # directory is the part that was never restricted.
        "G-S: the CDP trace defaults back into /tmp",
        "crates/whatsapp_chrome_driver/src/main.rs",
        "        None => Some(default_private_dir().join(\"whatsapp_chrome_driver.jsonl\")),\n",
        "        None => Some(std::path::PathBuf::from(\"/tmp/whatsapp_chrome_driver.jsonl\")),\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m13_cdp_trace_is_private_and_defaults_outside_tmp", "--", "--exact"],
    ),
    (
        # The one class of dependency the Chrome guard has no escape hatch for:
        # these binaries drive a profile holding a live session, so a renderer
        # compromise is account compromise, and the flag is refused on the
        # string alone. Passing it to satisfy a container that cannot use user
        # namespaces is the change this catches.
        "G-T: a chrome launcher passes no-sandbox",
        "crates/whatsapp_chrome_driver/src/main.rs",
        "            opts.mode(0o600);\n",
        "            opts.mode(0o600);\n"
        "            let _sandbox_flag = \"--no-sandbox\";\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m12_no_chrome_launcher_disables_the_renderer_sandbox", "--", "--exact"],
    ),
    (
        # The `--systemd` branch interpolates these values into a unit-file
        # heredoc, where a newline opens a new directive and a bracket opens a
        # section header. The guard drives the real script with a hostile
        # `--name`, so the mutation is in the script: dropping the shape check
        # is how a broadened character class reopens the injection.
        "G-U: the systemd unit-file character check is dropped",
        "scripts/run-octo-whatsapp.sh",
        """    case "$v" in
        *$'\\n'*|*'['*|*']'*|'')
            echo "invalid path: illegal character in (got: $v)" >&2; exit 1 ;;
    esac
""",
        "",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "c6_systemd_script_rejects_non_absolute_or_bracket_paths", "--", "--exact"],
    ),
    (
        # The original H-10 defect was a divergence: `ReplayCache` and its
        # `check_and_insert` had thorough unit tests, and the spawned accept
        # loop never called them. The cache's own tests pass either way, which
        # is why the guard scopes to the loop's body. Removing the call is the
        # regression, restored exactly.
        "G-V: the quic accept loop stops consulting the replay cache",
        "crates/octo-adapter-quic/src/lib.rs",
        "                                                                            .check_and_insert(\n",
        "                                                                            .map(|_| ())\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "h10_quic_accept_loop_consults_the_replay_cache", "--", "--exact"],
    ),
    (
        # `axum::serve` exposes no `ConnectInfo`, so there is no socket peer to
        # compare the header against: any use of `X-Forwarded-For` is a value
        # the client chose. It reached the auth-failure metric, which made a
        # security signal forgeable. A diagnostics module is the most likely
        # way it comes back, since that is where it last lived.
        "G-W: the health server reads X-Forwarded-For again",
        "crates/octo-whatsapp/src/observability/health_server.rs",
        "        assert!(\n            cfg.is_ok(),\n            \"loopback must be accepted; err={:?}\",\n            cfg.err()\n        );\n    }\n}\n",
        "        assert!(\n            cfg.is_ok(),\n            \"loopback must be accepted; err={:?}\",\n            cfg.err()\n        );\n    }\n}\n"
        "\n/// Probe: labelling an auth failure with a client-supplied header.\n"
        "fn _probe_client_forwarded_for(headers: &axum::http::HeaderMap) -> Option<&str> {\n"
        "    headers.get(\"x-forwarded-for\").and_then(|v| v.to_str().ok())\n"
        "}\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "l5_health_server_does_not_trust_client_forwarded_for", "--", "--exact"],
    ),
    (
        # Defence in depth behind the `--name` validation: even a value that
        # slips through must not be word-split by systemd, which reads
        # ExecStart its own way. Validation catches known-hostile names; this
        # catches whatever else arrives.
        "G-X: the generated ExecStart drops its quotes",
        "scripts/run-octo-whatsapp.sh",
        "ExecStart=$BIN --socket '$SOCKET' --name '$NAME' daemon",
        "ExecStart=$BIN --socket $SOCKET --name $NAME daemon",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "c6_systemd_heredoc_quotes_the_interpolated_execstart_arguments", "--", "--exact"],
    ),
    (
        # The `--name` shape check is what refuses a newline, which is the
        # working injection into the unit-file heredoc.
        #
        # The first version of this entry *added* a newline arm to the case
        # pattern and the guard stayed green, which looked like a broken guard
        # and was not. `*[!a-zA-Z0-9._-]*` already rejects a newline, because a
        # newline is a character outside the class — so a further arm can only
        # match names the first arm already refused, and validation gets
        # strictly tighter. The mutation has to remove the class check, which is
        # the direction the shell semantics actually move in.
        "G-Y: the systemd launcher stops checking a name's shape",
        "scripts/run-octo-whatsapp.sh",
        "    ''|*[!a-zA-Z0-9._-]*)\n",
        "    '')\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "c6_systemd_script_rejects_names_that_can_inject_a_unit_directive", "--", "--exact"],
    ),
    (
        # LIMIT and OFFSET are interpolated straight into SQL text, and SQLite
        # accepts the two-argument `LIMIT offset, count` form, so `1,(SELECT sql
        # FROM sqlite_master)` is a working injection that reaches the events
        # database. The script validated its SOURCE table and not these two,
        # which is the shape a partial validation takes.
        "G-Z: the contacts script stops validating LIMIT and OFFSET",
        "scripts/persist-contacts-bulk.sh",
        """    if ! [[ "${!numeric}" =~ ^[0-9]+$ ]]; then
        echo "invalid $numeric: must be a non-negative integer (got: ${!numeric})" >&2
        exit 3
    fi
""",
        "",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m10_persist_contacts_rejects_non_integer_limit_and_offset", "--", "--exact"],
    ),
    (
        # `canonicalize` here verifies only when a key is configured, so the
        # guard's job is to notice that shape changing rather than to bless it.
        # Making the check unconditional is the change it asks a reader to make
        # deliberately — the recorded state is opt-in and nobody should arrive
        # there by editing one `if`.
        #
        # The first version rewrote only the right-hand side of the binding and
        # stayed green, because the guard's needle is the `if let Some(pk)`
        # prefix and the mutation left that prefix standing. Moving the key
        # inside the block is what actually removes the opt-in shape.
        "G-aa: telegram verification becomes unconditional",
        "crates/octo-adapter-telegram/src/adapter.rs",
        "        if let Some(pk) = &self.verifying_key {\n",
        "        if true {\n            let pk = &self.verifying_key;\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m15_the_telegram_adapter_is_a_documented_opt_out", "--", "--exact"],
    ),
    (
        # Six binaries writing the same private-write rule six ways is how this
        # regressed in the first place. The guard counts definitions of two
        # specific names rather than checking behaviour, so the mutation is a
        # second definition of the same name — which is what the assertion
        # claims to prevent.
        #
        # **Known limit, stated rather than papered over.** A parallel helper
        # under a *different* name does not match either count and passes. A
        # first attempt at this entry named its probe
        # `write_private_file_local` and was green for exactly that reason. So
        # the guard catches a duplicate, not a reimplementation. The H-7 entries
        # have the same shape for `create_private`. Closing it means looking for
        # the mode constant alongside the open, not for a name, which is a
        # different check and not one this entry claims to have made.
        "G-bb: the private writer is defined twice",
        "crates/whatsapp_chrome_session_extract/src/lib.rs",
        "pub fn write_private_file(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {",
        "/// A second, private copy of the rule.\n"
        "pub fn write_private_file(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {\n"
        "    std::fs::write(path, contents)\n"
        "}\n\n"
        "pub fn write_private_file(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "h6_the_private_writer_is_shared_not_duplicated", "--", "--exact"],
    ),
    (
        # The accept loop is spawned without `self`, so the cache must be shared
        # to be reachable from it at all. The guard documents a failure mode that
        # is a compile error rather than a behavioural one, so the mutation is
        # the field type: this is the one entry where the mutation need not even
        # be well-formed Rust, and the guard's value is that it says so in text
        # rather than leaving the next reader to re-derive it.
        "G-cc: the quic replay cache stops being shared",
        "crates/octo-adapter-quic/src/lib.rs",
        "    replay_cache: Arc<Mutex<ReplayCache>>,\n",
        "    replay_cache: Mutex<ReplayCache>,\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "h10_quic_replay_cache_is_reachable_from_the_accept_loop", "--", "--exact"],
    ),
    (
        # The pin the key-shape guard could not see. It resolves the key's
        # declared type by name, so any key argument that is not a bare
        # identifier ended the check — and an inline `&[u8; N]` has no name to
        # look up. That is the pin stated the one way the lookup could not
        # reach, so the guard passed over a hardcoded 32-byte key.
        #
        # It compiles: `verify_dispatch` takes `public_key: &[u8]`, and
        # `&[u8; 32]` coerces to a slice. Written as a separate function rather
        # than by editing an existing call so the mutation does not depend on
        # whatever key type that call site happens to hold.
        "G-dd: a verification site passes an inline fixed-size key",
        "crates/octo-cap-macaroon/src/token.rs",
        """        .map_err(|e| MintError::HolderSig(e.to_string()))?;
        Ok(())
    }

    /// Compose the holder signature message:""",
        """        .map_err(|e| MintError::HolderSig(e.to_string()))?;
        Ok(())
    }

    /// Probe: a verification site whose key is pinned to a 32-byte array.
    #[allow(dead_code)]
    fn probe_inline_pinned_key(msg: &[u8], sig: &[u8]) -> bool {
        octo_algo_ed25519::verify_dispatch(
            &octo_algo_ed25519::ed25519_algorithm_id(),
            &[0u8; 32],
            msg,
            sig,
        )
        .is_ok()
    }

    /// Compose the holder signature message:""",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "no_verification_site_pins_its_key_to_a_fixed_size_array", "--", "--exact"],
    ),
    (
        # The manifest and the source census have to agree. Three sites need
        # `quota_router_storage` today and are recorded, so the dependency is
        # declared; adding a fourth import without recording it makes layer one
        # fire and is exactly the "the gate has been widened" case the
        # assertion names. Recorded exception plus unrecorded import is the
        # combination that is invisible to either layer alone.
        "G-ee: a fourth wallet import appears without being recorded",
        "crates/octo-wallet/src/vault.rs",
        "fn base64_decode(s: &str) -> Result<Vec<u8>, WalletError> {",
        "fn base64_decode(s: &str) -> Result<Vec<u8>, WalletError> {\n"
        "    let _probe: Option<quota_router_storage::RegistryError> = None;",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "the_wallet_crate_does_not_depend_on_storage", "--", "--exact"],
    ),
    (
        # The bespoke decoder accepted `QQ=A`: it saw `chunk[2] == '='` and
        # skipped the `chunk[3]` check, so a misplaced-padded string decoded to
        # bytes instead of erroring. Delegating to the crate removes the whole
        # class rather than patching the index arithmetic, which is why the
        # guard checks for the delegation and not for a particular bound.
        #
        # Written as "stops decoding" rather than "decodes wrongly" because a
        # compiling hand-rolled decoder is a lot of text that proves the same
        # thing: the function no longer asks `base64` for anything, and the
        # guard's first assertion is exactly that.
        "G-ff: the vault base64 decoder stops delegating to the crate",
        "crates/octo-wallet/src/vault.rs",
        """    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    STANDARD
        .decode(s)
        .map_err(|e| WalletError::Config(format!("base64 invalid: {e}")))
}""",
        """    let _ = s;
    Err(WalletError::Config("base64 unavailable".to_string()))
}""",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "l9_vault_uses_the_base64_crate_not_a_hand_rolled_parser", "--", "--exact"],
    ),
    (
        # `verify_dispatch` is the one way in, and the point of the registry is
        # that no scan keyed on that entry point can see a caller who bypassed
        # it. A function that takes the registry itself and calls `verify`
        # directly is precisely that, and it is invisible to every other AC-11
        # guard here — they all walk for `verify_dispatch`.
        "G-gg: a verification site reaches the registry by hand",
        "crates/octo-cap-macaroon/src/token.rs",
        """        Ok(Self {
            algorithm_id,
            bytes: bytes.to_vec(),
        })
    }""",
        """        Ok(Self {
            algorithm_id,
            bytes: bytes.to_vec(),
        })
    }

    /// Probe: a verification site that takes the algorithm by hand.
    #[allow(dead_code)]
    fn probe_registry_by_hand(key: &[u8], msg: &[u8], sig: &[u8]) -> bool {
        let Ok(registry) = octo_algo_ed25519::shared_registry() else {
            return false;
        };
        let Ok(algorithm) = registry.get(octo_algo_ed25519::ed25519_algorithm_id()) else {
            return false;
        };
        algorithm.verify(key, msg, sig).is_ok()\n    }""",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "no_verification_site_reaches_the_algorithm_registry_by_hand", "--", "--exact"],
    ),
    (
        # Two output types means every handler's return type is ambiguous at the
        # call site and a response can be built against either. The guard walks
        # the whole crate tree, so a second declaration anywhere is the finding.
        # The guard collects the *path* of every file declaring the type, so a
        # second declaration beside the first is invisible to it — the vector
        # had one entry in the first version of this mutation and stayed green.
        # What it is actually for is a redeclaration in another crate, which is
        # the parallel abstraction the node crates are supposed to re-export
        # rather than copy.
        "G-hh: a second handler output type is declared in another crate",
        "crates/octo-adapter-irc/src/lib.rs",
        '            "key 20 should be the remaining entry"\n        );\n    }\n}\n',
        '            "key 20 should be the remaining entry"\n        );\n    }\n}\n'
        "\n/// A second response envelope type, declared outside octo-protocol.\n"
        "#[allow(missing_docs)]\n"
        "pub struct HandlerOutput {\n"
        "    pub payload_kind: u32,\n"
        "}\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "exactly_one_handler_output_type_is_defined", "--", "--exact"],
    ),
    (
        # `DispatchSet` used to carry `identity_linkage`, computed by running
        # both tokens through the placeholder decoders — which derive a DID and
        # an ask id from the first bytes of the token string. Two identical
        # strings produced `Linked`. Three tests asserted on it, which is what
        # made it look exercised. Reintroducing the field is the landmine going
        # back in the ground, and it is named like an authentication decision
        # so the next reader reaches for it.
        "G-ii: the parsed header set regains an identity verdict",
        "crates/octo-wallet/src/capability/dispatch.rs",
        """pub struct DispatchSet {
    pub bearer: Option<AuthHeader>,
    pub capability: Option<AuthHeader>,
}

/// Parse `Authorization` + `X-Capability-Token` headers from a request map.
///
/// Identity linkage evaluation (RFC-0969 §Phase 1 + AC-A2):
/// - Both bearer + capability present → decode stubs run → compare
///   `subject_did == holder_did` AND `ask_id == ask_id`.
///   - Both match → `Linked { subject_did, ask_id }`.
///   - Subject match + ask mismatch → `AskBindingMismatch { bearer_ask, cap_ask }`
///     (mission 0969-a3 AC-B2.1.b).
///   - Subject mismatch → `Mismatched`.
/// - One present, other absent → `Indeterminate` (cannot evaluate linkage).
/// - Neither present → `ParseError::NoAuthHeader` (upstream of `AuthError::NoAuthHeader`
///   via `From<ParseError> for AuthError`).
/// - Authorization header with unrecognized scheme (e.g., `Basic <b64>`) →
///   `ParseError::UnsupportedScheme(scheme)` (mission 0969-a3 AC-B2.1.a;
///   upstream of `AuthError::UnsupportedScheme`).
pub fn parse_auth_headers(headers: &[(String, String)]) -> Result<DispatchSet, ParseError> {
    let mut bearer = None;
    let mut capability = None;
    let mut cap_count = 0;
    for (name, value) in headers {
        let lower = name.to_lowercase();
        if lower == "authorization" {
            // RFC-7235 §2.1: scheme names are case-insensitive. Real-world
            // deployments send `bearer <token>` (lowercase) and
            // `Bearer <token>` interchangeably; matching only the exact
            // case would silently discard valid credentials and emit
            // `UnsupportedScheme("bearer")` instead of routing to the
            // bearer pipeline. Compare byte-by-byte against the
            // canonical scheme prefix, case-folded, to find the actual
            // token start in the original-case `value`.
            if let Some(token_start) = strip_scheme_ci(value, "bearer ") {
                bearer = Some(AuthHeader::Bearer(value[token_start..].to_string()));
            } else if let Some(token_start) = strip_scheme_ci(value, "cipherocto-cap ") {
                capability = Some(AuthHeader::CipherOctoCap(value[token_start..].to_string()));
                cap_count += 1;
            } else {
                // Unknown scheme — mission 0969-a3 AC-B2.1.a: surface the
                // scheme name so `authenticate()` can return
                // `AuthError::UnsupportedScheme(scheme)`. The previous
                // silent-discard policy (pre-AC-B2.1) dropped the scheme.
                let scheme = value.split_whitespace().next().unwrap_or("").to_owned();
                return Err(ParseError::UnsupportedScheme(scheme));
            }
        } else if lower == "x-capability-token" {
            capability = Some(AuthHeader::CipherOctoCap(value.clone()));
            cap_count += 1;
        }
    }
    if cap_count > 1 {
        return Err(ParseError::DuplicateCapabilityHeader);
    }
    if bearer.is_none() && capability.is_none() {
        return Err(ParseError::NoAuthHeader);
    }
    // No identity verdict here. Deciding linkage from these bytes would mean
    // deriving an identity from an unverified token, which is the placeholder
    // decoder's job and nothing else's. See `DispatchSet`.
    Ok(DispatchSet { bearer, capability })""",
        """pub struct DispatchSet {
    pub bearer: Option<AuthHeader>,
    pub capability: Option<AuthHeader>,
    pub identity_linkage: LinkageResult,
}

/// Parse `Authorization` + `X-Capability-Token` headers from a request map.
///
/// Identity linkage evaluation (RFC-0969 §Phase 1 + AC-A2):
/// - Both bearer + capability present → decode stubs run → compare
///   `subject_did == holder_did` AND `ask_id == ask_id`.
///   - Both match → `Linked { subject_did, ask_id }`.
///   - Subject match + ask mismatch → `AskBindingMismatch { bearer_ask, cap_ask }`
///     (mission 0969-a3 AC-B2.1.b).
///   - Subject mismatch → `Mismatched`.
/// - One present, other absent → `Indeterminate` (cannot evaluate linkage).
/// - Neither present → `ParseError::NoAuthHeader` (upstream of `AuthError::NoAuthHeader`
///   via `From<ParseError> for AuthError`).
/// - Authorization header with unrecognized scheme (e.g., `Basic <b64>`) →
///   `ParseError::UnsupportedScheme(scheme)` (mission 0969-a3 AC-B2.1.a;
///   upstream of `AuthError::UnsupportedScheme`).
pub fn parse_auth_headers(headers: &[(String, String)]) -> Result<DispatchSet, ParseError> {
    let mut bearer = None;
    let mut capability = None;
    let mut cap_count = 0;
    for (name, value) in headers {
        let lower = name.to_lowercase();
        if lower == "authorization" {
            // RFC-7235 §2.1: scheme names are case-insensitive. Real-world
            // deployments send `bearer <token>` (lowercase) and
            // `Bearer <token>` interchangeably; matching only the exact
            // case would silently discard valid credentials and emit
            // `UnsupportedScheme("bearer")` instead of routing to the
            // bearer pipeline. Compare byte-by-byte against the
            // canonical scheme prefix, case-folded, to find the actual
            // token start in the original-case `value`.
            if let Some(token_start) = strip_scheme_ci(value, "bearer ") {
                bearer = Some(AuthHeader::Bearer(value[token_start..].to_string()));
            } else if let Some(token_start) = strip_scheme_ci(value, "cipherocto-cap ") {
                capability = Some(AuthHeader::CipherOctoCap(value[token_start..].to_string()));
                cap_count += 1;
            } else {
                // Unknown scheme — mission 0969-a3 AC-B2.1.a: surface the
                // scheme name so `authenticate()` can return
                // `AuthError::UnsupportedScheme(scheme)`. The previous
                // silent-discard policy (pre-AC-B2.1) dropped the scheme.
                let scheme = value.split_whitespace().next().unwrap_or("").to_owned();
                return Err(ParseError::UnsupportedScheme(scheme));
            }
        } else if lower == "x-capability-token" {
            capability = Some(AuthHeader::CipherOctoCap(value.clone()));
            cap_count += 1;
        }
    }
    if cap_count > 1 {
        return Err(ParseError::DuplicateCapabilityHeader);
    }
    if bearer.is_none() && capability.is_none() {
        return Err(ParseError::NoAuthHeader);
    }
    // No identity verdict here. Deciding linkage from these bytes would mean
    // deriving an identity from an unverified token, which is the placeholder
    // decoder's job and nothing else's. See `DispatchSet`.
    Ok(DispatchSet { bearer, capability, identity_linkage: LinkageResult::Mismatched })""",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "a_parsed_auth_header_set_carries_no_identity_verdict", "--", "--exact"],
    ),
    (
        # The other half of the same guard: even with the field gone, the parse
        # function must not compute a verdict from unverified decoders. Nothing
        # production reads such a value today, which is exactly why it is a
        # landmine rather than a bypass — but the type it feeds is public and
        # the decoders are public, so a caller can hand it straight to
        # `evaluate_linkage`, which cannot tell it apart from a verified one.
        "G-jj: header parsing derives linkage from unverified decoders",
        "crates/octo-wallet/src/capability/dispatch.rs",
        """pub struct DispatchSet {
    pub bearer: Option<AuthHeader>,
    pub capability: Option<AuthHeader>,
}""",
        """pub struct DispatchSet {
    pub bearer: Option<AuthHeader>,
    pub capability: Option<AuthHeader>,
}

fn probe_unverified_linkage(b: &str, c: &str) -> LinkageResult {
    let _left = unverified_decode_bearer(b);
    let _right = unverified_decode_capability(c);
    LinkageResult::Mismatched
}""",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "a_parsed_auth_header_set_carries_no_identity_verdict", "--", "--exact"],
    ),
    (
        # A pending entry pins a *count*, so a file gaining a site without the
        # table being updated is the failure the table exists to catch: a
        # fifteenth site in a crate holding four would be invisible otherwise.
        # This file already holds four `Signature::from_bytes` sites, so the
        # mutation is one more and nothing else has to move.
        "G-ll: a pending file gains a site the table does not count",
        "crates/quota-router-core/src/receipt.rs",
        "        let sig = ed25519_dalek::Signature::from_bytes(&[0x42; 64]);",
        "        let sig = ed25519_dalek::Signature::from_bytes(&[0x42; 64]);\n"
        "        let extra_probe_site = ed25519_dalek::Signature::from_bytes(&[0x43; 64]);",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "ac11_pending_sites_are_counted_and_still_present", "--", "--exact"],
    ),
    (
        # The second check on the same table: a pending file is counted, an
        # allowlisted file is skipped, and a file cannot be both without the two
        # gates giving opposite answers to one question. Recording the path in
        # both tables is how that happens.
        # A pending file is counted; a dispatched one is gated for zero sites.
        # A path under a dispatched root is therefore answering both questions
        # at once, and the two gates would race to disagree. The path used here
        # exists, so the existence assertion cannot be what fires — only the
        # disjointness check can.
        "G-mm: a path under a dispatched root is also recorded as pending",
        "crates/octo-cli/tests/security_regressions.rs",
        '        "crates/octo-vault/src/cache_subscriber.rs",\n        0,',
        '        "crates/octo-wallet/src/vault.rs",\n        0,',
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "ac11_pending_sites_are_counted_and_still_present", "--", "--exact"],
    ),
    (
        # The pre-fix defect was duplicated across both reputation stores, each
        # checking shape, freshness and quorum and none checking the signature.
        # The comment above the check names the function it calls, which is why
        # the guard strips comments first — matching on the comment would let
        # the call be deleted with the guard still green.
        "G-nn: a reputation store stops verifying the governance signature",
        "crates/octo-reputation/src/store/memory.rs",
        "        proof.verify_signature(now_unix)?;\n",
        "",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "c4_every_reputation_store_verifies_the_governance_signature", "--", "--exact"],
    ),
    (
        # The other store, same defect, same guard. Fixing one implementation
        # and leaving the other is precisely the drift this guard exists to
        # prevent, so proving it on both halves is what makes it worth running.
        # The second store's `slash_recorder` sits inside a cfg-gated module, so
        # the memory store's signature call is the reliable anchor.
        "G-oo: the stoolap store stops verifying the governance signature",
        "crates/octo-reputation/src/store/stoolap.rs",
        "            proof.verify_signature(now_unix)?;\n",
        "",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "c4_every_reputation_store_verifies_the_governance_signature", "--", "--exact"],
    ),
    (
        # `cursor + addr_len` can wrap, turning a declared 4 GiB address into a
        # small number that passes the length check, and the slice below then
        # reads out of bounds. Checked arithmetic fails closed on overflow;
        # wrapping arithmetic turns a malformed frame into a read past the end.
        "G-pp: transport address length goes back to wrapping arithmetic",
        "crates/octo-runtime/src/handle/encoding.rs",
        "    let addr_end = cursor.checked_add(addr_len).ok_or_else(|| {",
        "    let addr_end = cursor + addr_len;\n    let addr_end = (addr_end >= cursor).then_some(addr_end).ok_or_else(|| {",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "l11_transport_addr_len_uses_checked_arithmetic", "--", "--exact"],
    ),
    (
        # The macaroon id is derived from the root secret and a nonce, and a
        # change to either the function or the truncation silently re-keys every
        # identifier already issued. A guard that reads the source is the only
        # thing standing between that and a fleet-wide identifier churn.
        "G-qq: the macaroon identifier derivation changes",
        "crates/octo-cap-macaroon/src/lib.rs",
        "    let mac = hmac_blake3(root_secret, nonce);\n    let mut id = [0u8; 16];\n    id.copy_from_slice(&mac[..16]);",
        "    let mac = hmac_blake3(root_secret, nonce);\n    let mut id = [0u8; 16];\n    id.copy_from_slice(&mac[..8]);",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "l12_macaroon_id_derivation_is_pinned", "--", "--exact"],
    ),
    (
        # A keystore file that carries its own KDF parameters is asking to be
        # re-encrypted with the attacker's cost. The import pins the memory cost
        # and refuses a file that disagrees, so the derived key never depends on
        # bytes the file chose.
        # Written first as `if false && kdf.memory_cost != ARGON2_M_COST`,
        # which left the guard green: the needle is the comparison text, and
        # prefixing it with a constant leaves the comparison standing. The same
        # trap as the quic replay branch, and the same rule — a guard that reads
        # text can only be defeated by changing the text it reads.
        "G-rr: keystore import stops pinning the KDF cost",
        "crates/octo-wallet/src/keystore.rs",
        "        if kdf.memory_cost != ARGON2_M_COST",
        "        if false",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m17_keystore_import_does_not_read_file_supplied_kdf_params", "--", "--exact"],
    ),
    (
        # RFC-0014 delegates signature verification to the domain, so the
        # frozen verifier's behaviour stays as specified. Its documentation is
        # what misleads: without the sentence, a caller reads an integrity
        # check as proof a router signed the receipt, which is what made this
        # look like a substrate bug rather than a missing domain check.
        "G-ss: the integrity-only verifier stops saying it is integrity-only",
        "crates/octo-settlement-core/src/chain.rs",
        "not an authenticity check",
        "an authenticity check that the caller should rely on",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "c5_the_integrity_only_verifier_no_longer_claims_authenticity", "--", "--exact"],
    ),
    (
        # Time caveats are evaluated before shape and policy. Reordering them
        # makes redemption answer on structure first, so an expired capability
        # reports a shape error instead of an expiry — and a caller with a
        # generous retry policy treats the two very differently.
        "G-tt: redemption checks shape before time caveats",
        "crates/octo-wallet/src/capability/redemption.rs",
        "    time_caveats_admit(cap, evaluators, now_unix)?;\n    redeem_shape_and_policy(cap, catalog)\n}",
        "    redeem_shape_and_policy(cap, catalog)?;\n    time_caveats_admit(cap, evaluators, now_unix)\n}",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "h4_redemption_evaluates_time_caveats_before_anything_else", "--", "--exact"],
    ),
    (
        # `time_bounds` classifies which variants carry a deadline, and its
        # match is exhaustive with no wildcard on purpose. A contributor facing
        # an exhaustiveness error after adding a variant reaches for `_ =>`,
        # and that is precisely what makes the new variant silently
        # unenforced — the defect this guard exists to catch, and the one a
        # capability whose only deadline was `Before` actually shipped.
        #
        # Written as adding the wildcard rather than deleting an arm: the match
        # has no fallback, so removing `Self::Before` from it would not compile.
        "G-uu: time_bounds grows a wildcard arm",
        "crates/octo-cap-macaroon/src/caveat/mod.rs",
        """            | Self::AssetBinding(_) => None,
        }
    }

    /// Registry key for caveat predicate evaluation""",
        """            | Self::AssetBinding(_) => None,
            | _ => None,
        }
    }

    /// Registry key for caveat predicate evaluation""",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "h4_every_time_bearing_caveat_variant_is_covered", "--", "--exact"],
    ),
    (
        # A token API typed as an Ed25519 signature asserts the algorithm in the
        # type itself, and no amount of dispatch elsewhere makes a non-Ed25519
        # holder signature expressible. The field has to carry bytes plus the
        # algorithm, or the registry has nothing to dispatch on.
        "G-yy: the capability token API names an ed25519 signature type",
        "crates/octo-cap-macaroon/src/token.rs",
        "        let other_vk = ed25519_dalek::SigningKey::from_bytes(&other_key)",
        "        let _probe_sig: ed25519_dalek::Signature =\n            ed25519_dalek::Signature::from_bytes(&[0u8; 64]);\n        let other_vk = ed25519_dalek::SigningKey::from_bytes(&other_key)",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "no_capability_token_api_names_ed25519_dalek", "--", "--exact"],
    ),
    (
        # A signature verified against a snapshot nobody checked is a signature
        # over a claim the verifier never saw. Freshness and quorum are read
        # from the snapshot, so verifying the signature while skipping the
        # snapshot leaves both unchecked.
        "G-aaa: a governance verification site stops checking its snapshot",
        "crates/octo-reputation/src/store/memory.rs",
        "        crate::auth::verify_governance_snapshot(snapshot, &auth.governance_set_hash, now_unix)?;\n",
        "",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "every_governance_signature_verification_also_verifies_its_snapshot", "--", "--exact"],
    ),
    (
        # `pub type Ed25519PublicKey = [u8; 32]` pins a key exactly as hard as a
        # literal annotation, one indirection away from any scan looking for the
        # literal. The key-taking function below takes a slice; re-pinning it to
        # the alias is the regression this guard was written for.
        "G-bbb: the ask verifier re-pins its key to the alias",
        "crates/quota-router-storage/src/ask.rs",
        """    pub fn verify(
        &self,
        algorithm_id: &octo_ident::AlgorithmId,
        asker_public_key: &[u8],
    ) -> Result<(), AskSignedError> {""",
        """    pub fn verify(
        &self,
        algorithm_id: &octo_ident::AlgorithmId,
        asker_public_key: &Ed25519PublicKey,
    ) -> Result<(), AskSignedError> {""",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "the_alias_helpers_find_the_indirection_they_exist_for", "--", "--exact"],
    ),
    (
        # A holder record written by a node that is not the issuer is a record
        # nobody asked for, and the list of construction sites is exactly two
        # files. Adding a third call is the drift the recorded list exists to
        # catch — and it is invisible to any guard that only asks whether the
        # records are correct.
        "G-ccc: a non-issuing node starts building holder records",
        "crates/octo-wallet/src/vault.rs",
        "fn base64_decode(s: &str) -> Result<Vec<u8>, WalletError> {",
        "/// Probe: an unrecorded holder-record construction site.\n#[allow(dead_code)]\nfn probe_holder_record() {\n    let _ = quota_router_storage::HolderRecord::from_capability;\n}\n\nfn base64_decode(s: &str) -> Result<Vec<u8>, WalletError> {",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "only_the_issuing_nodes_build_a_holder_record_in_production", "--", "--exact"],
    ),
    (
        # The vendor's FFI entry points are not thread-safe, so every call goes
        # through one lock. Without it two threads can be inside the C library
        # at once, and the failure is a corrupt proof rather than a crash.
        # Five occurrences of the lock, so this needs the replace-all too.
        "G-eee: the FFI calls stop being serialised",
        "crates/zk-vendor/src/lib.rs",
        "call_lock",
        "serial_probe",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m4_ffi_calls_are_serialized", "--", "--exact"],
        99,
    ),
    (
        # The anti-vacuity half of M-10. The sibling guard is satisfied by a
        # script that refuses everything, so this one exists to catch exactly
        # that: a well-formed pair must get past validation and fail later for
        # an unrelated reason.
        #
        # The mutation tightens the integer shape from "any digits" to "one or
        # two digits", which refuses `1000` — the value this guard passes. The
        # three hostile cases the sibling asserts all still fail validation, so
        # both guards stay consistent and only this one moves.
        "G-ggg: the contacts script rejects valid pagination",
        "scripts/persist-contacts-bulk.sh",
        '    if ! [[ "${!numeric}" =~ ^[0-9]+$ ]]; then',
        '    if ! [[ "${!numeric}" =~ ^[0-9]{1,2}$ ]]; then',
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "m10_persist_contacts_still_accepts_a_valid_pagination_pair", "--", "--exact"],
    ),
    (
        # A recorded "no production impl" set that drifts is a false claim: the
        # entry says the trait has no real verifier, and a real verifier nobody
        # recorded is exactly the thing it is meant to make visible.
        #
        # Planted in `octo-adapter-irc`, which sits outside octo-cli's closure
        # so the probe is never compiled. That is the same bargain the earlier
        # file-scanning entries make: the guard reads the tree, so what matters
        # is that the text is the shape the guard matches, not that it builds.
        "G-hhh: an unrecorded bearer verifier appears",
        "crates/octo-adapter-irc/src/lib.rs",
        '            "key 20 should be the remaining entry"\n        );\n    }\n}\n',
        '            "key 20 should be the remaining entry"\n        );\n    }\n}\n'
        "\n/// Probe: a bearer verifier the recorded stub set does not name.\n"
        "#[allow(dead_code)]\n"
        "struct ProbeBearerVerifier;\n"
        "\n"
        "#[allow(dead_code)]\n"
        "impl octo_ident::BearerVerifier for ProbeBearerVerifier {\n"
        "    fn verify(&self, _token: &str) -> Result<(), String> {\n"
        "        Err(\"probe\".to_string())\n"
        "    }\n"
        "}\n",
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "every_verifier_trait_has_a_production_implementation_or_is_accounted_for", "--", "--exact"],
    ),
    (
        # One verifier for the whole workspace. A second name for it is how a
        # caller ends up verifying under looser rules than everyone else while
        # every other guard stays green.
        #
        # **This entry is why the harness takes a list of files.** The rename
        # has to reach the definition in `auth.rs` *and* the call site in
        # `store/memory.rs`; editing only the definition does not compile, and
        # editing only the call site leaves the guard's assertion satisfied.
        # With one path per entry this guard was unreachable and the harness
        # said so by reporting BAD-MUTATION for as long as anyone tried.
        "G-iii: the governance verifier is renamed at every call site",
        ["crates/octo-reputation/src/auth.rs", "crates/octo-reputation/src/store/memory.rs"],
        ["verify_governance_signature", "verify_governance_signature"],
        ["governance_sig_check", "governance_sig_check"],
        ["cargo", "test", "-p", "octo-cli", "--test", "security_regressions",
         "c4_the_signature_verifier_is_a_single_shared_choke_point", "--", "--exact"],
        99,
    ),
]
# Seven entries were written and withdrawn rather than parked in the table,
# because an entry that can never go red is worse than no entry at all.
#
# The class: a guard asserts on a token, and the mutation that removes the
# token does not compile. `ac11_the_ed25519_registration…` needs the key
# constructor gone and the only construction path is the call being deleted;
# `c4_the_signature_verifier_is_a_single_shared_choke_point` needs a rename
# that spans `auth.rs` and `store/memory.rs`, and the harness mutates one file
# per entry.
#
# G-vv would have removed `VerifyingKey::from_bytes` from the Ed25519
# registration so `ac11_the_ed25519_registration_itself_is_where_keys_get_built`
# fires. That assertion is `src.contains("VerifyingKey::from_bytes")` over a
# compiled crate, and the constructor appears at several call sites — one
# contiguous replacement leaves the rest, so the guard stays green and the
# entry would have been a permanent false pass. Making it work needs a
# replace-all the harness does not have.
#
# G-ww would have added an algorithm dependency to `octo-ident` so
# `the_two_algorithm_crates_stay_independent_and_additive` fires. Cargo
# refused it first: `octo-algo-ed25519` already depends on `octo-ident`, so the
# reverse edge is a cycle and the workspace will not resolve. The layer rule
# that assertion checks is therefore already enforced one level down, by cargo
# itself. Recorded as a finding rather than as a mutation, because a check
# that cannot be reached is worth knowing about.
# G-kk (dropping HandlerOutput's bundle prefix) was written and removed: it
# targeted the same guard as G-hh, so it added no guard coverage, and adding a
# field breaks the struct's one construction site, which a single contiguous
# replacement cannot reach. Recorded rather than left in the table as an entry
# that never runs.

# NOT MUTATED, deliberately: `OpenOptionsExt::mode(0o600)` at creation inside
# create_private. Removing it is behaviourally undetectable - the post-open
# set_permissions lands on the same 0600 - but it is not redundant: it closes
# the window between open() creating the file at the umask mode and the
# correction. A behavioural test cannot observe a race window, so there is no
# mutation that means anything here. Recorded rather than counted as a pass.


def run(cmd):
    return subprocess.run(
        cmd, cwd=ROOT, capture_output=True, text=True, timeout=3000
    )


LOCKFILE = ROOT / ".mutate-zk-guards.lock"


def acquire_lock():
    """Refuse to run two harnesses at once.

    This is not defensive padding. Two concurrent runs of this script were
    started by accident during the H-7 wave, and they corrupted each other:
    each holds its own in-memory copy of the original file, so run B's
    mutation was restored over by run A's backup, and run A's mutation was
    likewise restored over by B. The run finished "clean" and left THREE
    source files in their mutated state — including `zk-verifier` with its
    fail-closed arm replaced by `Ok(())`, and `zk-vendor` with the
    `validate_library_path` call deleted.

    The regression guards caught it, which is the only reason it was caught:
    running the suite with no mutation applied and finding it green is the
    check that matters. This lock exists so the harness cannot manufacture
    that situation in the first place.
    """
    try:
        fd = os.open(LOCKFILE, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    except FileExistsError:
        print(
            f"ERROR: another harness run holds {LOCKFILE}. Two concurrent runs "
            "corrupt each other's restores. Delete the lock only if you are "
            "sure no run is in flight.",
            file=sys.stderr,
        )
        sys.exit(2)
    os.write(fd, str(os.getpid()).encode())
    os.close(fd)


def release_lock():
    LOCKFILE.unlink(missing_ok=True)


def octo_cli_closure(root: pathlib.Path) -> set[str]:
    """Workspace crates cargo must build to run `cargo test -p octo-cli`.

    Derived from the manifests on every run rather than recorded in the file, so
    a crate that gains or loses a dependency edge reclassifies its mutations
    automatically. A hand-maintained cost column would be the same kind of stale
    claim this harness exists to disprove.
    """
    crates = {p.name for p in (root / "crates").iterdir() if p.is_dir()}

    def deps_of(name: str) -> set[str]:
        manifest = root / "crates" / name / "Cargo.toml"
        if not manifest.exists():
            return set()
        text = manifest.read_text(encoding="utf-8", errors="replace")
        found = set()
        for other in crates:
            if re.search(rf"^\s*{re.escape(other)}\s*=", text, re.M):
                found.add(other)
            elif f'path = "../{other}"' in text:
                found.add(other)
        return found

    seen: set[str] = set()
    stack = ["octo-cli"]
    while stack:
        name = stack.pop()
        if name in seen:
            continue
        seen.add(name)
        stack.extend(deps_of(name) - seen)
    return seen


def mutation_cost(rel: str, closure: set[str]) -> str:
    """`free` when mutating the file cannot invalidate a fingerprint cargo needs.

    A mutation costs a recompile only when it edits something cargo actually
    compiles: a crate's Rust sources or its manifest. Anything else — a markdown
    mission, a shell script under `scripts/` — is not a build input, so changing
    it leaves every fingerprint intact.

    Keying on the path rather than on the extension matters. The first version of
    this treated anything outside `crates/` as a rebuild, which misfiled four
    mutations that edit `scripts/swap_sessions.sh`: cargo never reads it, so they
    are free, and `--cheap` would have skipped work it could have done.
    """
    if isinstance(rel, list):
        # A multi-file entry costs a rebuild if ANY file it touches does, so
        # taking the first would under-report it and put it in `--cheap`.
        return "rebuild" if any(mutation_cost(r, closure) == "rebuild" for r in rel) else "free"
    path = pathlib.Path(rel)
    if not (path.suffix == ".rs" or path.name == "Cargo.toml"):
        return "free"
    parts = path.parts
    if len(parts) > 1 and parts[0] == "crates":
        return "rebuild" if parts[1] in closure else "free"
    return "free"


def print_coverage():
    """Report how many distinct guards this table actually backs.

    Derived by reading the table rather than maintained by hand in prose. A
    mission recorded this count twice — once as 13 when the table held 15, once
    as 15 when it held 19 — because the figure lived in a sentence that only
    got edited when its author remembered. Counting entries instead of guards
    overstates coverage twice over: seven entries run other crates' suites
    rather than this one, and two entries share a guard. The two errors run in
    opposite directions, which is why neither showed up as a discrepancy.
    """
    guards = set()
    elsewhere = 0
    for entry in MUTATIONS:
        cmd = entry[4]
        if "security_regressions" in cmd:
            guard = cmd[cmd.index("security_regressions") + 1]
            if guard != "--":
                guards.add(guard)
        else:
            elsewhere += 1
    print(
        f"\n  {len(MUTATIONS)} entries -> {len(guards)} distinct guards in "
        f"security_regressions.rs"
        f"  (+{elsewhere} entries targeting other crates' suites)"
    )


def main():
    acquire_lock()
    # `--only SUBSTRING` runs a subset. `--cheap` runs every entry that needs no
    # recompile. A red-team entry that mutates a crate in octo-cli's dependency
    # closure forces a rebuild, so running the whole table is expensive enough
    # that people skip it — and being able to run the affordable half, or one
    # entry, is what keeps a specific claim re-checkable between full sweeps.
    selected = MUTATIONS
    cheap_only = False
    args = sys.argv[1:]
    # An unrecognised argument used to fall through every branch above and run
    # the entire table. Asking for `--help` mutated the workspace and ran the
    # full suite, because help was simply not handled — and the sibling
    # document-guard harness had already shipped with the same shape of defect,
    # where an unknown flag was treated as a filter that matched nothing and the
    # run reported a successful verification of zero guards. Here the default is
    # worse than doing nothing: it is the expensive path.
    #
    # So an unrecognised argument is refused rather than interpreted, and help
    # is answered from the same usage string the module docstring carries.
    known = {"--list", "--cheap", "--only", "--help", "-h"}
    if args and args[0] not in known:
        print(f"unknown argument {args[0]!r}", file=sys.stderr)
        print(USAGE, file=sys.stderr)
        print(
            "Nothing was run. Passing an argument this script does not recognise "
            "used to run the entire table.",
            file=sys.stderr,
        )
        release_lock()
        return 2
    if args and args[0] in {"--help", "-h"}:
        print(USAGE)
        release_lock()
        return 0
    if args and args[0] == "--list":
        closure = octo_cli_closure(ROOT)
        for entry in MUTATIONS:
            print(f"  {mutation_cost(entry[1], closure):8s} {entry[0]}")
        print_coverage()
        release_lock()
        return 0
    if args and args[0] == "--cheap":
        closure = octo_cli_closure(ROOT)
        cheap_only = True
        selected = [m for m in MUTATIONS if mutation_cost(m[1], closure) == "free"]
        if not selected:
            print("no mutation is free of a recompile")
            release_lock()
            return 2
    if len(args) > 1 and args[0] == "--only":
        needle = args[1]
        selected = [m for m in selected if needle in m[0]]
        if not selected:
            print(f"no mutation label contains {needle!r}")
            release_lock()
            return 2
    vacuous, bad_mutation, bad_anchor = [], [], []
    # An entry is (label, file, old, new, cmd) or (…, cmd, count). `count`
    # defaults to 1 — one contiguous replacement, which is right for nearly
    # every entry and is what made a replace-all look unnecessary.
    #
    # It is needed for a specific shape: a guard asserting on a token that
    # occurs several times in a crate that must compile. Renaming the token
    # breaks every call site, and replacing one occurrence leaves the rest, so
    # the `contains` assertion still holds and the entry would be a permanent
    # false pass. Three guards are unreachable without this.
    for entry in selected:
        label, rel, old, new, cmd = entry[:5]
        count = entry[5] if len(entry) > 5 else 1
        # `rel` is either one path or a list of them. A rename that has to
        # span two files is a real mutation and one contiguous replacement
        # cannot express it: editing only the definition breaks every call
        # site, and the harness would report BAD-MUTATION forever. The list
        # form applies one replacement per file, all restored together.
        rels = rel if isinstance(rel, list) else [rel]
        olds = old if isinstance(rel, list) else [old]
        news = new if isinstance(rel, list) else [new]
        path = ROOT / rels[0]
        paths = [ROOT / r for r in rels]
        originals = [pt.read_text() for pt in paths]
        original = originals[0]
        if any(o not in orig for o, orig in zip(olds, originals)):
            bad_anchor.append(label)
            print(f"[SKIP-BAD-ANCHOR] {label}", flush=True)
            continue

        backups = []
        for orig in originals:
            b = tempfile.NamedTemporaryFile(delete=False, suffix=".bak")
            b.write(orig.encode())
            b.close()
            backups.append(b.name)

        try:
            # `count` 1 is one contiguous replacement, which is right for
            # nearly every entry. Anything higher means replace-all, and it
            # needs no count check beyond the `in` above: the point is to clear
            # every occurrence of a token a `contains` assertion matches on.
            for pt, o, nw, orig_text in zip(paths, olds, news, originals):
                pt.write_text(
                    orig_text.replace(o, nw) if count > 1 else orig_text.replace(o, nw, 1)
                )

            res = run(cmd)
            output = res.stdout + res.stderr
            # A mutation that does not compile makes cargo exit non-zero for a
            # reason unrelated to the guard; recording that separately keeps a
            # broken mutation from looking like a red guard.
            if res.returncode == 0:
                vacuous.append(label)
                print(f"[VACUOUS] {label}: guard still GREEN", flush=True)
            elif "could not compile" in output:
                bad_mutation.append(label)
                print(f"[BAD-MUTATION] {label}: did not compile; guard never ran", flush=True)
            else:
                first = next(
                    (ln for ln in output.splitlines() if "test result:" in ln),
                    None,
                )
                # A genuine red always prints a test-result line, because that
                # line is the test harness reporting. Its absence means cargo
                # failed for some other reason and the guard never ran — a
                # manifest that no longer resolves, a missing file, a lockfile
                # clash. Scoring that as RED ok is the harness reporting a pass
                # it never earned, and it happened here for real: a mutation
                # using an invented dependency name was refused by cargo with
                # "no matching package named" and came back green-looking.
                if first is None:
                    bad_mutation.append(label)
                    detail = next(
                        (ln for ln in output.splitlines() if ln.startswith("error")),
                        "(no error line either)",
                    )
                    print(
                        f"[BAD-MUTATION] {label}: cargo failed before the guard ran "
                        f"-> {detail.strip()[:120]}",
                        flush=True,
                    )
                else:
                    print(f"[RED ok] {label} -> {first.strip()}", flush=True)
        finally:
            for pt, bname in zip(paths, backups):
                shutil.copyfile(bname, pt)
                pathlib.Path(bname).unlink(missing_ok=True)

    red = len(selected) - len(vacuous) - len(bad_mutation) - len(bad_anchor)
    print("\n--- summary ---")
    shown = len(selected) if len(selected) == len(MUTATIONS) else f"{len(selected)}/{len(MUTATIONS)}"
    print(f"mutations: {shown}  red: {red}  vacuous: {len(vacuous)}  "
          f"bad-mutation: {len(bad_mutation)}  bad-anchor: {len(bad_anchor)}")
    for v in vacuous:
        print(f"  VACUOUS: {v}")
    for b in bad_mutation:
        print(f"  BAD-MUTATION: {b}")
    for f in bad_anchor:
        print(f"  ERROR:   {f}")
    return 1 if (vacuous or bad_mutation or bad_anchor) else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    finally:
        release_lock()