//! Invariants of the verification harness itself — C3.
//!
//! The guide's §33 is the chapter that describes how this repository proves
//! its multi-node claims, and it is the one chapter the audit named as having
//! "no meta-test": nothing checked that the harness §33 describes still
//! exists, still runs, or is still scheduled. §27 (CI/CD integration) has the
//! same gap.
//!
//! Both tests here are deliberately about SCHEDULING and DECLARATION rather
//! than about behaviour. Behaviour is what L3/L4 and the unit suites are for.
//! These two ask the question the behaviour tests cannot: is this work
//! actually wired to run, and would a new contributor find out?
//!
//! Counting rules are stated inline because the numbers below are claims.

use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let path = workspace_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()))
}

/// The multi-node test targets, derived by a stated rule rather than by a
/// count someone typed.
///
/// RULE: a MULTI-NODE TARGET is a test target that exercises more than one
/// `octo` process or more than one `$OCTO_HOME` — that is, the targets the
/// audit's §2.5 calls "two independent multi-node layers" plus the
/// cross-process revocation target. They are the targets that need a docker
/// engine, and therefore the targets that cannot run inside `cargo test
/// --workspace` and are invisible unless something schedules them.
///
/// The list is explicit rather than glob-derived because a glob cannot tell a
/// multi-node target from any other `#[ignore]`d test — 159 of those exist
/// repo-wide and almost none of them are multi-node. The COUNT is asserted
/// below, so adding a 7th multi-node target without adding it to the nightly
/// fails this test rather than passing quietly.
const MULTI_NODE_TARGETS: [&str; 6] = [
    "e2e_l4_multinode_docker",
    "layer3_cross_process_tcp",
    "layer4_2node",
    "layer4_3node_gossip",
    "layer4_disconnect_heal",
    "cross_process",
];

/// C3-1 — every multi-node target is reachable from a scheduled workflow.
///
/// This is the meta-test §33 lacked. Before the nightly landed, all 21
/// scenarios across these six targets were `#[ignore]`d with no `schedule:`
/// reaching them: the work existed, was correct, and was invisible. A test
/// cannot prevent that by asserting behaviour, because the behaviour is
/// never executed. It can only assert that SOMETHING is wired to execute it.
#[test]
fn tv_c3_every_multi_node_target_is_scheduled() {
    let nightly = read(".github/workflows/multi-node-nightly.yml");

    // The workflow has to actually be scheduled, not merely present.
    assert!(
        nightly.contains("schedule:"),
        "the multi-node nightly declares no `schedule:` — the targets below are \
         reachable only by hand, which is the failure this test exists to catch"
    );
    assert!(
        nightly.contains("cron:"),
        "the multi-node nightly declares a `schedule:` with no `cron:`"
    );

    let mut unscheduled: Vec<&str> = Vec::new();
    for target in MULTI_NODE_TARGETS {
        if !nightly.contains(target) {
            unscheduled.push(target);
        }
    }
    assert!(
        unscheduled.is_empty(),
        "multi-node test targets no scheduled workflow runs: {unscheduled:?}. \
         Each is `#[ignore]`d, so nothing else in CI will execute it — add it \
         to .github/workflows/multi-node-nightly.yml"
    );

    // A scenario that is NOT `#[ignore]`d is a different defect: it would
    // never run in the nightly (`-- --ignored` filters it out) while looking,
    // in `cargo test --workspace`, like covered work. So the count is
    // asserted from both directions.
    // A scenario that is NOT `#[ignore]`d is a different defect: the nightly
    // filters with `-- --ignored`, so an un-ignored scenario never runs there
    // while looking, in `cargo test --workspace`, like covered work. Counting
    // the ignored tests also pins the strategy's size, from source.
    //
    // The targets live in three different crates, so each is looked up by
    // stem across all of them rather than assuming `octo-cli`.
    let crates = [
        "crates/octo-cli",
        "crates/quota-router-integration-tests",
        "crates/octo-runtime-revocation-store",
    ];
    let mut ignored_total = 0usize;
    let mut missing: Vec<&str> = Vec::new();
    for target in MULTI_NODE_TARGETS {
        let found = crates.iter().find_map(|c| {
            let path = workspace_root().join(format!("{c}/tests/{target}.rs"));
            std::fs::read_to_string(path).ok().map(|body| {
                body.lines()
                    .filter(|l| l.trim_start().starts_with("#[ignore"))
                    .count()
            })
        });
        match found {
            Some(n) if n > 0 => ignored_total += n,
            _ => missing.push(target),
        }
    }
    assert!(
        missing.is_empty(),
        "multi-node targets whose test file has no \\`#[ignore]\\`d scenario (or          does not exist): {missing:?}. The nightly filters on \\`-- --ignored\\`, so a \
         target in this list would never execute there."
    );
    assert_eq!(
        ignored_total, 21,
        "the multi-node scenarios across the six targets should be 21 \
         (15 L4 + 5 router + 1 cross-process); found {ignored_total}. If this \
         moved, §33's description of the strategy and the nightly's coverage \
         both need re-checking before the expectation is changed"
    );
}

/// C3-2 — the two workflows declare the triggers the guide says they do.
///
/// §27 tells an operator that CI is wired for unattended use. This asserts
/// the wiring exists, which is otherwise a claim about YAML that nothing
/// checks.
#[test]
fn tv_c3_ci_and_nightly_workflows_declare_their_triggers() {
    let ci = read(".github/workflows/ci.yml");
    let nightly = read(".github/workflows/multi-node-nightly.yml");

    assert!(
        ci.contains("pull_request:"),
        "ci.yml declares no `pull_request:` trigger; §27's PR-gating claim is \
         then false"
    );
    assert!(
        ci.contains("cargo test"),
        "ci.yml runs no `cargo test`, so the integration suite this repository \
         relies on is not part of CI"
    );

    assert!(
        nightly.contains("workflow_dispatch:"),
        "the nightly offers no manual dispatch, so a failure cannot be \
         reproduced on demand without a local docker engine"
    );

    // R13. `workflow_dispatch:` is NOT a substitute for `schedule:`. Manual
    // dispatch needs a human with a docker engine; a schedule needs neither.
    // This guard is named for the triggers and C3 exists precisely because
    // these 21 scenarios were unreachable from any scheduled run — so
    // asserting the manual trigger while ignoring the scheduled one leaves
    // the original invisibility intact, reached through this very guard.
    //
    // The key is matched with its indentation so the assertion cannot be
    // satisfied by the word appearing in a comment: this file's header
    // explains what the trigger is FOR, and `nightly.contains("schedule:")`
    // would match that prose forever.
    assert!(
        nightly.contains("\n  schedule:\n"),
        "the nightly declares no `schedule:` trigger, so it will only ever run \
         when a human dispatches it by hand — the 21 multi-node scenarios are \
         unscheduled and therefore invisible again, which is the exact failure \
         C3 was written to prevent. Add a `schedule:` with a real cron."
    );

    for job in ["mesh", "router", "revocation"] {
        assert!(
            nightly.contains(&format!("{job}:")),
            "the nightly declares no `{job}` job; the layer selector and the \
             job it guards have drifted apart"
        );
    }

    // `--test-threads=1` is a correctness requirement, not a politeness
    // setting: the scenarios share a docker image tag, so running them
    // concurrently races on the build. Dropping it would make the nightly
    // intermittently red for reasons that have nothing to do with the code.
    assert!(
        nightly.contains("--test-threads=1"),
        "the nightly dropped `--test-threads=1`; the L4 scenarios share one \
         image tag and will race on it"
    );
}

/// C3-3 — the guide's §33 disposition table and this file agree.
///
/// Cheap, and it stops the two halves of C3 from drifting: the guide's table
/// is parsed by `guide_unit_dispositions_are_complete` in `guide_examples.rs`,
/// and this asserts the harness claims those rows make are still true.
#[test]
fn tv_c3_guide_disposition_table_agrees_with_the_harness() {
    let guide = read("docs/06-operations/operator-guide.md");
    let start = guide
        .find("### Coverage disposition")
        .expect("§33 carries a coverage disposition table");
    let table = &guide[start..];

    for (unit, cited) in [
        (
            "§27",
            "tv_c3_ci_and_nightly_workflows_declare_their_triggers",
        ),
        ("§33", "tv_c3_every_multi_node_target_is_scheduled"),
    ] {
        let row = table
            .lines()
            .find(|l| l.starts_with(&format!("| {unit} ")))
            .unwrap_or_else(|| panic!("{unit} has a row in the disposition table"));
        assert!(
            row.contains("covered"),
            "{unit} is claimed covered by `{cited}`, but its table row says: {row}"
        );
        assert!(
            row.contains(cited),
            "{unit}'s table row does not name `{cited}`, so the row and this \
             file disagree: {row}"
        );
    }

    // The `covered`/`open` headline is derived, and both halves are counted
    // the same way the guard counts them, so the two can never disagree.
    let rows: Vec<&str> = table
        .lines()
        .filter(|l| l.starts_with("| §") || l.starts_with("| Appendix"))
        .filter(|l| !l.contains("---"))
        .collect();
    let covered = rows.iter().filter(|r| r.contains("| covered")).count();
    assert_eq!(
        rows.len(),
        36,
        "the disposition table has {} rows, expected one per guide unit (36)",
        rows.len()
    );
    assert_eq!(
        covered, 17,
        "expected 17 covered units, found {covered}; the guide's stated split \
         and this file's must move together"
    );
}
