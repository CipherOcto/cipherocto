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

Usage: python3 scripts/mutate-zk-guards.py [--only SUBSTRING]
"""
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
]

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
    path = pathlib.Path(rel)
    if not (path.suffix == ".rs" or path.name == "Cargo.toml"):
        return "free"
    parts = path.parts
    if len(parts) > 1 and parts[0] == "crates":
        return "rebuild" if parts[1] in closure else "free"
    return "free"


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
    if args and args[0] == "--list":
        closure = octo_cli_closure(ROOT)
        for label, rel, *_ in MUTATIONS:
            print(f"  {mutation_cost(rel, closure):8s} {label}")
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
    for label, rel, old, new, cmd in selected:
        path = ROOT / rel
        original = path.read_text()
        if old not in original:
            bad_anchor.append(label)
            print(f"[SKIP-BAD-ANCHOR] {label}", flush=True)
            continue

        backup = tempfile.NamedTemporaryFile(delete=False, suffix=".bak")
        backup.write(original.encode())
        backup.close()

        try:
            path.write_text(original.replace(old, new, 1))
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
            shutil.copyfile(backup.name, path)
            pathlib.Path(backup.name).unlink(missing_ok=True)

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