---
name: rfc-0936-two-of-three-checks-are-unit-tested-and-never-constructed-in-production
description: "RFC-0936 specifies three pre-call checks behind a trait, wired into the router behind a configuration toggle. All three checks exist and all thirteen of their tests pass, including the two rejection cases the Test Plan names. Production constructs exactly one of them. TagFilterCheck and HealthCheck have no construction site anywhere outside their own module, so no HTTP health probe is ever issued and no tag is ever filtered. Three independent conditions each prevent the tag check from rejecting anything: it is never constructed, request tags are hardcoded empty at the one production site, and the deployment's allowed and blocked lists are hardcoded empty at that same site. The thirteen tests pass because the test fixture builds DeploymentInfo a second way, taking allowed and blocked as parameters, so the suite exercises the logic the production path cannot reach. The trait that makes the checks extensible is never named outside its own module and no trait object exists, so the RFC's extension mechanism is absent while its concrete types landed. The router integration is absent entirely, the configuration toggle does not exist, and the production wiring that does exist is attributed by its own comment to RFC-0954 rather than to RFC-0936."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0936
    - 0936-a-pre-call-checks
    - RFC-0928
    - RFC-0954
---

# Mission `rfc-0936-two-of-three-checks-are-unit-tested-and-never-constructed-in-production`

## Status

Deferred. The substantive gap is a missing production wiring, which is an
implementation decision and not an RFC edit. The documentation corrections are
small and listed below. No code change is made here and nothing was
actioned. The archived `0936-a` mission was read, never edited.

Ninth systematic sweep of a self-describing structure in an accepted economics
RFC, and the first in which **the implementation is better than the RFC's own
account of it in the opposite direction** — the mechanism is correct and the
document describing how to reach it is not what shipped.

## Finding 1 — all three checks are implemented, and all their tests pass

This is the part that holds, and it holds completely.

`pre_call_checks.rs` defines the `PreCallCheck` trait and three implementations:
`ContextWindowCheck`, `TagFilterCheck` and `HealthCheck`. Thirteen tests run
green, and among them are precisely the negative cases the RFC's Test Plan
asks for:

| Test Plan item                                  | Test                             | Result         |
| ----------------------------------------------- | -------------------------------- | -------------- |
| 1. filters deployments with insufficient tokens | `test_context_window_fail_input` | present, green |
| 2. tag filter passes when tags match            | `test_tag_filter_pass_allowed`   | present, green |
| 3. tag filter fails when tags blocked           | `test_tag_filter_fail_blocked`   | present, green |
| 4. health check marks unhealthy deployments     | `test_health_check_unhealthy`    | present, green |

So the logic the RFC specifies was built, and the rejection paths work when
exercised. This is a real result and it is the opposite of what most of this
audit has been finding.

## Finding 2 — production constructs one of the three

`TagFilterCheck` and `HealthCheck` have **no construction site anywhere outside
their own module**. The only pre-call check production builds is
`ContextWindowCheck`, and it is built inline at a single site inside the
request handler.

The consequences are concrete rather than theoretical:

- **No HTTP health probe is ever issued.** `HealthCheck` performs a
  `GET {api_base}/health` with a timeout and maintains an
  `Arc<RwLock<HashMap<String, HealthState>>>` cache. With nothing constructing
  it, no deployment health endpoint is ever contacted and the cache is never
  written. The `HealthState` struct does not exist under that name at all.
- **No request tag is ever filtered.** `TagFilterCheck` is unreachable.

The RFC's §Note that "the Router creates one `HealthCheck` instance and shares
it across all requests via `Arc`" describes an instance that does not exist.

## Finding 3 — three independent conditions each block the tag check

Even setting aside that the check is never constructed, the tag filter cannot
reject anything at the one production site. Three conditions each suffice, and
all three hold:

1. **The check is never constructed** (above).
2. **Request tags are hardcoded empty.** The `CompletionRequest` built in
   production sets `tags: vec![]`. `TagFilterCheck::check` returns `Pass`
   immediately when `request.tags.is_empty()`, before reaching either list.
3. **The deployment's lists are hardcoded empty.** The same site sets
   `allowed_tags: vec![]` and `blocked_tags: vec![]`. With `blocked_tags`
   empty, `contains` is always false; with `allowed_tags` empty, the allowed
   check is skipped by the check's own logic.

Any one of these would make the check inert. All three are present.

## Finding 4 — the tests pass because they build the struct a second way

This is the finding that explains the previous three without condemning the
code.

The unit tests construct `DeploymentInfo` through a `test_deployment` helper
that **takes `allowed` and `blocked` as parameters** and supplies them. The
production handler constructs `DeploymentInfo` with those two fields hardcoded
empty. They are two different construction sites for the same type, and only
one of them is reachable by a request.

So the suite genuinely covers the tag and health logic, and that coverage says
nothing about whether a live request can ever reach it. This is a stronger and
more actionable position than the usual finding in this audit — the code is
right, the tests are honest, and the gap is a single missing wiring.

**The site was confirmed to be production code, not test code**, because
indentation alone was ambiguous: a nearby `#[cfg(test)] mod rfc0905_tests` sits
above it and eight-space indentation fits both a top-level function and one
nested in a test module. Inserting a `compile_error!` at the construction site
and running `cargo build`, which does not compile `cfg(test)` items, failed at
that line. The site is production.

## Finding 5 — the extensibility mechanism does not exist

RFC-0936 §1 defines a trait and says implementations must be async, and §0
adds `pre_call_checks: Vec<Box<dyn PreCallCheck>>` to the `Router`. The intent
is an open, extensible check pipeline.

- `Box<dyn PreCallCheck>` appears **nowhere** in the workspace.
- The identifier `PreCallCheck` is **never named outside `pre_call_checks.rs`**.
- Production calls `check_with_fallbacks`, an inherent method on the concrete
  `ContextWindowCheck` type. It does not call the trait's `check` at all, and
  `check_with_fallbacks` is **not described anywhere in the RFC**.

So the concrete types landed, the trait did not reach production, and the method
production actually calls is one the RFC never specifies. A second implementor
cannot be added without writing the integration the RFC describes and does not
have.

This is the extension-over-enumeration principle from the project's own
architectural guidance, arriving by the opposite route: the concrete cases were
built and the open type was not.

## Finding 6 — the router integration is absent entirely

§5 specifies three integration symbols. None exist in any crate:

| Symbol                     | Status |
| -------------------------- | ------ |
| `get_available_deployment` | absent |
| `route_to_valid`           | absent |
| `deployment_groups`        | absent |
| `weighted_random`          | absent |

The filtering that does happen is inlined into the request handler rather than
performed by a router method, and it produces a context-window-specific result
type carrying fallback model names, which the RFC never describes.

**The production comment attributes this work to a different RFC.** The site
reads "Context window pre-check (Issue #3 — RFC-0954 Round 2)". RFC-0936
specifies the checks; RFC-0954 appears to have wired one of them, and RFC-0954
is itself one of the accepted economics RFCs with no live mission. So this RFC's
types were consumed by a successor specification, and neither document records
the relationship.

## Finding 7 — the configuration toggle does not exist

§6 specifies `router.enable_pre_call_checks` plus a per-check `enabled` flag
for each of the three checks. **No such configuration key exists anywhere.**

There is a gate, but it is a different mechanism: a Cargo feature condition on
`litellm-mode` or `full`. So the context window check cannot be disabled by an
operator at runtime, which is what the RFC's toggle exists to allow, and the
three per-check toggles have no counterpart in any form.

This also makes Test Plan item 7, "Configuration enables/disables checks",
unsatisfiable as written.

## Finding 8 — the field types and names drifted from the specification

§0 specifies `allowed_tags: Option<Vec<String>>` and `blocked_tags:
Option<Vec<String>>`, as `Option` "to maintain backward compatibility". The
implemented `DeploymentInfo` uses plain `Vec<String>` for both. The `Option`
wrapper is how the RFC distinguishes "no policy configured" from "empty
policy"; without it the check cannot tell them apart, which is part of why the
empty-list case silently passes.

§0 also lists default values for `last_health_check: None` and `is_healthy:
true`, but the struct it shows adds neither field — it adds four. §4 defines a
`HealthState` with `last_check`, not `last_health_check`, and **neither name
exists in the workspace**. The implemented struct instead carries
`health_endpoint` and `is_healthy`, neither of which the RFC specifies.

So of six promised defaults, two are on fields that were never added under
those names, and the struct's actual shape is the RFC's plus one undeclared
field.

## What the RFC owner should do

- **Wire the missing two checks, or say they are not wired.** `TagFilterCheck`
  and `HealthCheck` are built and tested; production never constructs them. This
  is the only finding here that represents missing function rather than
  missing documentation.
- **Populate the three hardcoded fields at the production construction site.**
  Until request tags and the deployment's tag lists are read from
  configuration, the tag check is inert regardless of whether it is constructed.
- **Decide between the trait and the inherent method.** Production calls
  `check_with_fallbacks` on a concrete type, so the pipeline is closed. Either
  put the trait object in the path, or drop the RFC's extensibility claim and
  document the single hardcoded check honestly.
- **Add the configuration toggle** specified in §6, or remove the section.
  Today an operator cannot turn the context window check off.
- **Correct the §0 field types and the two field names**, and add
  `health_endpoint` to the specification since the implementation has it.
- **Record the RFC-0954 relationship.** The wiring that exists credits RFC-0954;
  RFC-0936 does not mention it.

## Reproduction

```bash
# the three checks and their thirteen tests
cargo test -p quota-router-core --lib pre_call_checks     # 13 passed

# production constructs exactly one of them
grep -rn 'ContextWindowCheck::new()\|TagFilterCheck::new()\|HealthCheck::new()' \
  crates/ --include=*.rs | grep -v pre_call_checks.rs

# the trait and its trait object never reach production
grep -rn 'PreCallCheck' crates/ --include=*.rs | grep -v pre_call_checks.rs
grep -rn 'Box<dyn PreCallCheck>' crates/ --include=*.rs

# production calls a method the RFC never describes
grep -rn 'check_with_fallbacks' crates/ --include=*.rs
grep -c 'check_with_fallbacks' rfcs/accepted/economics/0936-pre-call-checks.md

# the three hardcoded fields at the production construction site
grep -n 'allowed_tags: vec!\[\]\|blocked_tags: vec!\[\]\|tags: vec!\[\]\|is_healthy: true' \
  crates/quota-router-core/src/proxy.rs

# and the test helper that supplies them as parameters instead
grep -n 'fn test_deployment' crates/quota-router-core/src/pre_call_checks.rs

# the router integration
grep -rn 'get_available_deployment\|route_to_valid\|deployment_groups' crates/ --include=*.rs

# the configuration toggle
grep -rn 'enable_pre_call_checks' crates/ --include=*.rs

# the field names that were never added
grep -rn 'last_health_check' crates/ --include=*.rs

# the production site attributes itself to a different RFC
grep -n 'RFC-0954 Round 2' crates/quota-router-core/src/proxy.rs
```

## Cross-references

- RFC-0936 §0 New Types Required — the field types and the two names that
  drifted
- RFC-0936 §1 Pre-call Check Trait — the extension mechanism that does not
  reach production
- RFC-0936 §3 Tag Filter Check and §4 Health Check — implemented, tested, and
  unreachable
- RFC-0936 §5 Integration with Router — absent entirely
- RFC-0936 §6 Configuration — the toggle that does not exist
- RFC-0936 Test Plan — four of its eight items have no corresponding test, and
  two of those four describe mechanisms that do not exist
- RFC-0928 — the configuration schema this RFC extends, itself with no live
  mission
- RFC-0954 — the RFC the production wiring credits, also with no live mission,
  and the natural place to record the relationship
- Mission `0936-a-pre-call-checks` (archived) — read, not edited
- Mission `nine-accepted-rfcs-have-a-version-history-whose-only-row-is-that-the-table-was-added`
  — this RFC's Version History is that retroactive row, which is consistent
  with a document never revised after acceptance and therefore with §0 and §5
  being written as a plan rather than as a record
- Mission `rfc-0960s-security-mechanisms-name-sixteen-constraints-and-no-evaluator-exists`
  — the same shape of finding in the opposite direction: here the evaluator
  exists and is tested, and nothing can reach it

## Errors made while producing this mission

- The production construction site was initially assumed to be test code,
  because it sits below a `#[cfg(test)] mod rfc0905_tests` and is indented as
  though it were inside it. Had that stood, every finding in this mission would
  have inverted. It was settled by inserting a `compile_error!` at the site and
  running `cargo build`, which does not compile `cfg(test)` items. The probe was
  reverted and the tree confirmed clean.
- A first search for production calls to the check trait used the pattern
  `.check(` across the workspace. That matched roughly two hundred unrelated
  `check` methods on allowlists, rate limiters and guards, none of them
  pre-call checks, and would have produced a fabricated claim that the trait
  method is called in production. The correct query is on the trait's name,
  which is never mentioned outside its own module.
