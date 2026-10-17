#!/usr/bin/env bash
# RFC-0973 §Phase 5 CI lint: a Layer-B core crate must not gain a dependency
# on a Layer-E strategy crate.
#
# **Contract (RFC-0973 Lifecycle Requirement 8):** registration is performed by
# a host — a binary or node crate that links strategy crates — never by core.
# A core crate that depends on a strategy crate to discover one inverts the
# dependency direction the layer model forbids, AND it destroys the property
# that makes the host trustworthy: the host is the only party that can observe
# crates at link time, so its claim about who owns a type id is the only claim
# worth believing. Let core link a strategy crate and the allocation-ownership
# gate in the registry becomes decorative again.
#
# **Scans `[dependencies]` only. This is a decision, not an omission.**
#
# The two wrong builds are both consistent with the phrase "a dependency":
#
#   - Scanning `[dev-dependencies]` too: fires on the wallet's own ZK tests,
#     which legitimately exercise `octo-cap-zk`'s API, and gets "fixed" with an
#     exemption. The exemption then becomes the hole, because nothing
#     distinguishes a *test* importing an extension crate from *core code*
#     importing one — the lint can only see the table, and the table does not
#     say whether the import reached `src/`.
#   - Scanning `[dependencies]` only (chosen): a dev-edge cannot make a
#     shipped `octo-wallet` binary link a strategy crate, because cargo does not
#     link dev-dependencies into a non-test build. The property that matters —
#     "no core *code* reaches a strategy crate" — holds, and it is the property
#     the layer model is about.
#
# A dev-edge that grew into a production import would fail the *build*, not this
# lint: `octo_cap_zk::` is not in scope for `cargo build -p octo-wallet`. So the
# failure mode of choosing this scope is a loud compile error, while the failure
# mode of the other scope is a silent exemption.
#
# **Layer-B core crates.** Scoped to the crates that participate in the
# attestation mechanism's core. A strategy crate depending on a core crate is
# the legal direction and is not flagged.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

# Layer-E strategy crates. A core crate may not name any of these in
# `[dependencies]`.
STRATEGY_CRATES=(
  "octo-cap-zk"
  "octo-cap-poi"
)

# Layer-B core crates subject to the rule. The wallet is the one that carried
# the violating edge before this lint existed.
#
# `octo-wallet-node` is deliberately ABSENT. It is the registration host — the
# binary-or-node crate RFC-0973 Lifecycle Requirement 8 *requires* to link
# strategy crates, and the only party able to observe them at link time. Listing
# it here would flag the one crate whose linking is the property being protected.
# It is Layer D, not core; the rule is about core not naming a strategy.
CORE_CRATES=(
  "crates/octo-wallet"
  "crates/octo-cap-macaroon"
)

violations=0

for crate_dir in "${CORE_CRATES[@]}"; do
  manifest="${crate_dir}/Cargo.toml"
  if [[ ! -f "$manifest" ]]; then
    echo "lint: ${manifest} not found — is the crate renamed?" >&2
    violations=$((violations + 1))
    continue
  fi

  # Extract only the `[dependencies]` table. `[dev-dependencies]`,
  # `[build-dependencies]` and `[target.*.dependencies]` are out of scope by the
  # decision documented above.
  #
  # A line belongs to `[dependencies]` if it is not inside any other table
  # header. Table headers are `[...]` lines.
  in_deps=0
  while IFS= read -r line; do
    if [[ "$line" =~ ^\s*\[.*\]\s*$ ]]; then
      if [[ "$line" =~ ^\[dependencies\] ]]; then
        in_deps=1
      else
        in_deps=0
      fi
      continue
    fi
    [[ $in_deps -eq 1 ]] || continue

    for strategy in "${STRATEGY_CRATES[@]}"; do
      if [[ "$line" =~ ^\s*${strategy}[[:space:]]*= ]]; then
        echo "${manifest}: Layer-B core crate depends on Layer-E strategy crate '${strategy}'"
        violations=$((violations + 1))
      fi
    done
  done < "$manifest"
done

if [[ $violations -gt 0 ]]; then
  cat >&2 <<'EOF'

RFC-0973 Lifecycle Requirement 8: a Layer-B core crate may not depend on a
strategy crate to discover or register one. Registration belongs to a host.

  - Reach strategy crates through the registration host (e.g. octo-wallet-node).
  - Import shared types from the crate that DECLARES them, which is usually a
    Layer-A substrate crate rather than the Layer-E extension that re-exports it.
  - Do not "fix" this with an exemption. That is the other wrong build.
EOF
  exit 1
fi

echo "no-core-strategy-dep: OK (${#CORE_CRATES[@]} core crates scanned, [dependencies] only)"