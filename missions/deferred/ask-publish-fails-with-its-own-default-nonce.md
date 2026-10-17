---
name: ask-publish-fails-with-its-own-default-nonce
description: "octo-wallet ask publish is the marketplace's only live producer and it fails when run with no flags: the --nonce-hex default literal is 64 hex chars (32 bytes) while the validator demands 32 hex chars (16 bytes). Measured on the built binary, exit 1, 'nonce must be 16 bytes (32 hex chars), got 32'. Every publish test passes --nonce-hex explicitly, so the default is never exercised. Also measured: a rate_per_1k of i64::MAX publishes and signs successfully, confirming nothing bounds it."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0009
---

# Mission `ask-publish-fails-with-its-own-default-nonce`

## Status

Deferred — the fix is a one-line default literal, but changing a signed
artefact's default is a decision about whether the deterministic fixture
nonce is still wanted at all, and a wrong guess would change published
test fixtures. Not actioned; the contradiction is documented on the flag.

This is the first finding in the audit that is a **plain broken
command** rather than a latent defect behind an unwired facade.

## Finding

`octo-wallet ask publish` carries a default nonce that its own validator
rejects.

The flag:

```rust
/// 32-hex-character nonce (16 bytes). Defaults to BLAKE3("ask-nonce:v1")
/// for deterministic test fixtures; pass explicit hex for production.
#[arg(long, default_value = "8c3e6f4b2a1d9e7c5f8b3a6d9c2e5f8b1a4d7c0e3f6b9a2d5c8e1f4b7a0d3c6f")]
```

That literal is:

```text
hex chars : 64
bytes     : 32
```

The validator, reached a few lines later, requires **32 hex chars =
16 bytes**. The doc says "32-hex-character nonce (16 bytes)" in the same
struct that supplies a 64-hex-character default.

## Measurement — the built binary, no test doubles

The documented command, with no nonce flag:

```text
$ octo-wallet ask publish --node-type self-host --model openai/gpt-4 \
    --axes input_tokens_per_1k:30000 --ttl-unix 1900000000 \
    --jurisdiction US --seed /tmp/octo_probe.seed
error: nonce must be 16 bytes (32 hex chars), got 32
exit=1
```

The error message is itself confused: it states the requirement in bytes
_and_ hex chars, then reports the input in bytes, so "got 32" reads like
a hex count to anyone scanning the line.

With a correct 32-hex nonce, the same command succeeds:

```text
exit=0
```

So the nonce default is the only thing standing between a user and a
working publish.

## Why no test caught it

`ask_publish_cli.rs` invokes `publish` three times and **passes
`--nonce-hex` explicitly every time**. The default path is never taken.

There is even a test for the validator itself —
`ask_publish_rejects_oversize_nonce` asserts the error text. So the
_validator_ is covered and the _default value it rejects_ is not. The gap
is between the two, and no test spans it.

## Also settled here: nothing bounds `rate_per_1k`

The previous finding asserted, from reading the code, that no rate
boundary exists. That is now measured end to end.

```text
$ octo-wallet ask publish ... --axes input_tokens_per_1k:9223372036854775807 ...
exit=0
```

The signed artifact carries `rate_per_1k` encoding `i64::MAX`
(`0x7F...`), under `ask_id` + `payload` + `signature`.

So a legally published, signed Ask can carry a rate that makes the
pricing path abort with an overflow — see
`settlement-cost-panics-where-compute-cost-errors`. The input side is
live today; only the consumption side is still behind the unwired
marketplace facade.

The rate is parsed as `u128` and `try_into()`d to `i64`, so negatives
are impossible and anything above `i64::MAX` is rejected with a clear
error. `i64::MAX` itself is accepted, which is the whole gap.

## Consequence

`octo-wallet ask publish` is, per this audit, the marketplace's **only
live code path**. Every other marketplace finding in this file is latent
because nothing constructs an Ask that reaches it. This one is a command
that fails when run as documented.

It is also the command the axis-id mission measured against — that
finding had to pass `--nonce-hex` to get past this one, which is worth
noting: an earlier measurement in this audit used the working invocation
and never noticed the default was broken.

## What the RFC owner needs to decide

- Fix the default to 32 hex chars, or drop the default and require an
  explicit `--nonce-hex`? The doc says the default exists "for
  deterministic test fixtures", so shortening it changes published
  fixtures and dropping it changes the interface. Both are owner calls.
- If the default stays, should it be the BLAKE3-derived value the doc
  claims rather than a hand-written literal? As shipped, the literal
  does not correspond to the derivation the doc names.
- Should the error message state the unit once, unambiguously? "got 32"
  next to "32 hex chars" invites exactly the misreading I made first.
- Should a test assert that the **default** value passes validation, next
  to the existing test that asserts an oversize value fails it?

## Reproduction

```bash
cargo build -p octo-wallet --bin octo-wallet

# the default is 64 hex chars, the validator wants 32
grep -n 'default_value = "[0-9a-f]\{32,\}"' crates/octo-wallet/src/bin/octo-wallet.rs

# the doc states 32 hex chars / 16 bytes
grep -n -B2 '32-hex-character nonce' crates/octo-wallet/src/bin/octo-wallet.rs

# the run that fails
octo-wallet init --node-type self-host --seed-out /tmp/octo_probe.seed
octo-wallet ask publish --node-type self-host --model openai/gpt-4 \
  --axes input_tokens_per_1k:30000 --ttl-unix 1900000000 \
  --jurisdiction US --seed /tmp/octo_probe.seed

# the run that works, with an explicit correct nonce
octo-wallet ask publish --node-type self-host --model openai/gpt-4 \
  --axes input_tokens_per_1k:30000 --ttl-unix 1900000000 \
  --jurisdiction US --seed /tmp/octo_probe.seed \
  --nonce-hex 8c3e6f4b2a1d9e7c5f8b3a6d9c2e5f8b

# nothing bounds the rate
octo-wallet ask publish --node-type self-host --model openai/gpt-4 \
  --axes input_tokens_per_1k:9223372036854775807 --ttl-unix 1900000000 \
  --jurisdiction US --seed /tmp/octo_probe.seed \
  --nonce-hex 8c3e6f4b2a1d9e7c5f8b3a6d9c2e5f8b

# every existing publish test passes --nonce-hex explicitly
grep -n -A2 '"publish"' crates/octo-wallet/tests/ask_publish_cli.rs
grep -n 'ask_publish_rejects_oversize_nonce' crates/octo-wallet/tests/ask_publish_cli.rs
```

## Cross-references

- RFC-0959 §Roles — the CLI surface this implements
- RFC-0009 — the identity substrate the nonce protects
- Mission `ask-axis-ids-are-never-validated-against-the-axis-registry` —
  the other finding measured against this same command; it passed
  `--nonce-hex` and so never saw this one
- Mission `settlement-cost-panics-where-compute-cost-errors` — what the
  unbounded rate eventually reaches
- Mission `the-marketplace-facade-has-no-production-caller` — why every
  other marketplace finding is latent and this one is not
