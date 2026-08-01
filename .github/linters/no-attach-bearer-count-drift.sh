#!/usr/bin/env bash
# CI lint: forbid attach_bearer site count drift (mission 0957-b R9-6).
#
# Why this exists: `egress::key_swap::attach_bearer` is the canonical
# egress helper for provider-bound Authorization headers. Every site
# that builds an outbound provider request MUST route through it. The
# total number of call sites is a structural invariant — if a new
# provider egress path is added that constructs an outbound request
# without going through `attach_bearer`, the count increases AND the
# boundary guarantee silently regresses.
#
# This linter compares the current `attach_bearer(` call count (in
# production source files only — comments and tests excluded) against a
# checked-in baseline. New call sites are allowed (with a bumped
# baseline + delta rationale), but accidental count drift is caught.
#
# Allowed surface (substring match against path) — the canonical
# `attach_bearer` definition + tests that exercise the helper. Sites
# inside the helper definition or test code are excluded from the count.
set -euo pipefail

cd "$(dirname "$0")/../.."  # repo root

BASELINE=36  # mission 0957-b R9-6 measured 2026-08-01:
#   proxy.rs = 8
#   native_http/openai.rs = 12
#   native_http/replicate.rs = 4
#   native_http/{together,perplexity,mistral,groq,databricks}.rs = 2 each = 10
#   native_http/mod.rs = 1
#   guardrails/mod.rs = 1
# TOTAL = 36 production call sites (excluding helper definition + tests + comments)

# Count `attach_bearer(` calls in PRODUCTION source only.
# Exclude: comments (// or ///), tests, the helper definition itself.
count=$(grep -rn --include="*.rs" 'attach_bearer(' crates/ \
  | grep -vE ':\s*(///|//)' \
  | grep -vE 'crates/quota-router-core/tests/' \
  | grep -vE 'crates/quota-router-core/src/egress/key_swap\.rs:' \
  | wc -l)

if [ "$count" -ne "$BASELINE" ]; then
  echo "ERROR: attach_bearer() call count drift (expected $BASELINE, got $count):"
  echo
  git -C "$(pwd)" grep -n 'attach_bearer(' crates/ \
    | grep -vE ':\s*(///|//)' \
    | grep -vE 'crates/quota-router-core/tests/' \
    | grep -vE 'crates/quota-router-core/src/egress/key_swap\.rs:'
  echo
  echo "If you added a new provider egress site that legitimately needs"
  echo "to attach a provider Authorization header, bump BASELINE in this"
  echo "linter with a delta rationale. If you removed a site, drop"
  echo "BASELINE accordingly. If you added a site WITHOUT routing through"
  echo "attach_bearer(), you have a key-swap boundary regression — fix"
  echo "the new site to call attach_bearer() and the count will align."
  exit 1
fi

echo "OK: attach_bearer() call count matches baseline ($BASELINE)."
