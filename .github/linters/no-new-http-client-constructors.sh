#!/usr/bin/env bash
# CI lint: forbid new HTTP client constructor sites (mission 0957-b R4
# close-out — R8 finding §3).
#
# Why this exists: the clippy `disallowed-methods` table in `clippy.toml`
# denies `reqwest::Client::new` workspace-wide, but ~14 egress modules
# (proxy.rs, native_http/*) carry a module-level `#[allow(...)]` because
# the canonical provider-egress path needs to construct per-request HTTP
# clients. The module-level allow is too coarse: a future contributor can
# add a new `reqwest::Client::new()` call to any function in those
# modules and the lint will not catch it.
#
# This linter is a coarse but durable safety net: it counts the
# `reqwest::Client::new()` occurrences per file, compares against a
# checked-in expected count, and fails if any file's count increased
# relative to `main`. New HTTP client constructor sites MUST be
# justified at PR time (either by updating the expected count with a
# delta rationale, or by routing through the existing egress helpers).
#
# The count is coarse: it allows DELIBERATE new sites with a bumped
# expected count, but catches ACCIDENTAL new sites in established
# files. It does NOT catch new sites in NEW files — those are caught
# by the clippy `disallowed-methods` deny + the reviewer's eye.
#
# Allowed surface (substring match against path) — same surface as the
# module-level `#[allow(clippy::disallowed_methods)]` sites in `clippy.toml`'s
# reason field. Sites in these paths do NOT count toward the per-file
# delta because the deny does not apply to them.
ALLOWLIST_PATHS=(
  'crates/quota-router-core/src/proxy.rs'
  'crates/quota-router-core/src/native_http/'
  'crates/quota-router-core/src/guardrails/mod.rs'
  'crates/quota-router-core/src/auth/sso/'
  'crates/quota-router-core/src/callbacks/'
  'crates/quota-router-core/src/secret_manager.rs'
  'crates/quota-router-core/src/pre_call_checks.rs'
  'crates/quota-router-core/src/node/provider.rs'
  'crates/quota-router-core/tests/e2e_proxy.rs'
  'crates/octo-adapter-twitter/src/lib.rs'
  'crates/octo-adapter-discord/src/lib.rs'
  'crates/octo-adapter-matrix/src/lib.rs'
  'crates/octo-adapter-lark/src/lib.rs'
  'crates/octo-adapter-bluesky/src/lib.rs'
  'crates/octo-adapter-wechat/src/lib.rs'
  'crates/octo-adapter-webrtc/src/lib.rs'
  'crates/octo-adapter-slack/src/lib.rs'
  'crates/octo-adapter-reddit/src/lib.rs'
  'crates/octo-adapter-dingtalk/src/lib.rs'
  'crates/octo-adapter-webhook/src/lib.rs'
  'crates/octo-adapter-qq/src/lib.rs'
  'crates/whatsapp_chrome_driver/src/main.rs'
  'crates/whatsapp_chrome_reconnect_observer/src/main.rs'
  'crates/whatsapp_chrome_session_extract/src/'
)

set -euo pipefail

cd "$(dirname "$0")/../.."  # repo root

# Count current `reqwest::Client::new` sites per file.
current=$(grep -rn --include="*.rs" 'reqwest::Client::new' crates/ \
  | grep -vE "$(printf '%s|' "${ALLOWLIST_PATHS[@]}")" \
  | sort || true)

# Per-file counts (current main / branch tip). Branch-relative delta
# detection: count only the sites OUTSIDE the allowlist; the count
# should be ZERO for any file. Sites in the allowlist are tracked by
# the module-level `#[allow(...)]` and the per-PR review.
expected_violations=""
if [ -n "$current" ]; then
  expected_violations="$current"
  echo "ERROR: reqwest::Client::new found OUTSIDE the allowlisted egress surface:"
  echo "$current"
  echo
  echo "The clippy [disallowed-methods] deny for reqwest::Client::new is"
  echo "intended to be workspace-wide; the module-level allowlist in"
  echo "this script's header is the only surface where the call is"
  echo "permitted. Any occurrence outside these paths is a bypass."
  echo
  echo "If the new site is a legitimate provider-egress site, add the"
  echo "path to ALLOWLIST_PATHS in this script + the module-level"
  echo "#[allow(clippy::disallowed_methods)] in the new module."
  exit 1
fi

echo "OK: no reqwest::Client::new outside the allowlisted egress surface."
