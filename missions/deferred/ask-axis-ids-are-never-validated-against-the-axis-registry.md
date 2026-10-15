---
name: ask-axis-ids-are-never-validated-against-the-axis-registry
description: "octo-wallet publish takes each --axes axis_id verbatim and never checks it against PricingAxis::standard_axes, the PricingAxisRegistry, or the axis_registry_toml loader. A misspelling is not an error: settlement looks the id up in the Ask rate table, misses, and falls back to the axis default. Measured: publishing input_tokens_per_1K:90000 with a capital K settles at 30000, the default, so a typo silently under-charges the seller. The fallback also narrows RFC-0959 UnknownAxis, which requires a consumed axis missing from the Ask's signed rate table to be an error."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
    - RFC-0959
---

# Mission `ask-axis-ids-are-never-validated-against-the-axis-registry`

## Status

Deferred — validation infrastructure exists but is not wired to the
producer, and wiring it raises a composition question about custom axes
that is an owner decision. The **hazard is documented on the CLI flag**;
no behaviour changed.

The produce-side gap turned out to be the smaller half of a conformance
deviation on the consume side (see §Spec conformance below), which is
recorded here because the CLI is what makes it reachable in practice.

This is the first finding in this audit with a **live** code path rather
than a latent one in an uncalled facade.

## Finding

`octo-wallet publish` parses each `--axes` entry as `axis_id:rate_per_1k`:

```rust
let (axis_id, rate_str) = entry.split_once(':')...;
let rate: u128 = rate_str.parse()...;
// axis_id is used verbatim — no registry lookup
rates.push(AxisRate { axis: axis_id.to_owned(), rate_per_1k: ... });
```

The **rate** is validated: parsed as `u128`, so a negative is impossible,
then `try_into()` to `i64` rejects anything above `i64::MAX` with a clear
error. That part is sound — a live producer cannot emit the negative-rate
defect that `StoolapAskRepository::put` now refuses.

The **axis id** is not validated at all, and it is the field that decides
which rate applies.

## Measurement

Seller intends 90_000 for input tokens:

```text
correct axis id -> settled 90000
TYPO'd axis id  -> settled 30000
default rate    -> 30000
```

`input_tokens_per_1K` (capital K) instead of `input_tokens_per_1k`
publishes without error, signs without error, and settles at 30_000 —
the axis default. `settlement_cost` asks `cost_for`, which looks for the
id in the Ask's own rate table, misses, and falls back:

```rust
let rate = self.rates.iter().find(|r| r.axis == axis_id).map(|r| r.rate_per_1k)
    .or_else(|| PricingAxis::find_by_id(axes, axis_id).map(|a| a.default_rate_per_1k))?;
```

The fallback exists on purpose, so an Ask that _omits_ an axis is charged
the default rather than nothing. The consequence is that an Ask carrying a
_misspelled_ axis is indistinguishable from one that omitted it.

## Spec conformance: RFC-0959 §Compute cost is fail-closed

The fallback is not merely a design preference — it contradicts the RFC.

RFC-0959 defines the Ask's signed rate table as
`AskUnsignedPayload.axes: BTreeMap<String, MicroOCTO_W>` ("axis_id → rate"),
and `compute_cost` looks the consumed axis up **in that map**:

```rust
let rate = ask.payload.axes
    .get(axis_id)
    .ok_or_else(|| SettlementError::UnknownAxis(axis_id.clone()))?;
```

A consumed axis the Ask does not price is `UnknownAxis`. There is no
registry default in the RFC's cost path, and no second lookup. So the
RFC-conformant rule is: **an Ask must price every axis it will be settled
on, and an omission is an error.**

The implementation narrows that error condition. `compute_cost` delegates to
`ModelRateTable::cost_for`, which tries the Ask's table and then falls back to
the registry default, so `SettlementError::UnknownAxis` fires only when the
axis is in **neither** the Ask's rate table **nor** the registry.

Two places in RFC-0959 point the same way:

- §Error Handling lists `UnknownAxis` as recoverable, with the remedy
  "asker publishes axis in TOML registry first" — a publish-time remedy,
  not a silent per-settlement default.
- Appendix C: "Verifiers reading old TOML reject unknown axes (**fail-closed**)."

So this is a conformance deviation on the settlement-cost path, not a
usability wart. It also reframes the produce-side gap: the CLI not
validating axis ids is only half the story. Even a perfectly correct
publisher that deliberately under-specifies its rate table is silently
charged at defaults, where the RFC requires a hard failure.

## Direction of the failure

Always **under-charge**. A typo can never make a seller charge more than
intended, because the fallback is the axis default and the seller's rate
is simply not found. So this is lost revenue and a silently wrong listing,
not an over-billing hazard.

That bounds the _money_ harm, but not the conformance harm: RFC-0959
requires an unpriced axis to be an error, and a silent success is the
wrong shape regardless of which way the number moves.

## What the RFC owner needs to decide

## The validation infrastructure exists and is unused

```text
PricingAxisRegistry::register   -> Result<_, AxisRegistryError>, rejects Duplicate
axis_registry_toml::load_from_str / load_from_path -> PricingAxisRegistry
```

Neither is consulted by the CLI, by `Ask::new`, or by
`AskUnsignedPayload::new`. The last of those validates asker_did, model
family, jurisdiction and nonce — everything except the rates.

## Why it was not simply validated against `standard_axes()`

Because custom axes are a supported configuration: `Marketplace::with_axes`
overrides the axis set, and `axis_registry_toml` exists precisely to load
`pricing-axes.toml`. Hard-coding the three standard axes into the CLI would
reject legitimate listings in any deployment using custom axes.

So the correct fix is not "validate against `standard_axes()`" but
"validate against the axis registry this deployment actually uses" — which
means the CLI needs to be given that registry, or the Ask needs to record
which registry it was priced against.

## What the RFC owner needs to decide

- Should `octo-wallet publish` gain an `--axes-file` (or similar) pointing
  at the deployment's `pricing-axes.toml`, validating every id against it?
- Or should unknown axis ids be a hard error, with custom axes enabled by
  explicitly supplying the registry?
- Should `AskUnsignedPayload` record the axis ids it priced against, so a
  later settlement can detect that the two no longer agree? The Ask is
  signed, so this would be tamper-evident.
- Should `Ask::new` / `AskUnsignedPayload::new` at least reject an Ask whose
  rate table is entirely unknown axes, as a cheap proxy — an Ask that can
  never match a consumed axis is almost certainly a typo?
- Should publishing warn (without failing) on an unrecognised axis id, as an
  interim measure while the registry plumbing lands?

## Reproduction

```bash
# the CLI takes axis_id verbatim
grep -n -A6 'split_once' crates/octo-wallet/src/bin/octo-wallet.rs | head -10

# the rate is validated, the id is not
grep -n -B2 -A6 'let rate: u128' crates/octo-wallet/src/bin/octo-wallet.rs

# the fallback that makes a typo look like an omission
grep -n -A7 'pub fn cost_for' crates/quota-router-storage/src/ask.rs

# validation that exists but is unused here
grep -rn 'PricingAxisRegistry::register\|axis_registry_toml::' --include=*.rs crates/ \
  | grep -v 'src/axis_registry_toml.rs\|src/ask.rs'

# AskUnsignedPayload validates everything except rates
grep -n -A24 'impl AskUnsignedPayload' crates/quota-router-storage/src/ask.rs | grep 'Empty\|Duplicate'
```

## Cross-references

- RFC-0959 §Data Structures — `AskUnsignedPayload.axes` as the signed rate
  table, and the registry
- RFC-0959 §Compute cost — the `UnknownAxis` lookup this implementation
  narrows
- RFC-0959 §Error Handling, Appendix C — fail-closed on unknown axes
- RFC-0900 §Market Operations — where cheapest lookup consumes these rates
- Mission `negative-axis-rates-are-refused-at-the-storage-boundary` — the
  write-side guard; this is the produce-side gap that guard catches late
- Mission `marketplace-advertised-price-ignores-default-rate-fallback` —
  the same fallback seen from the advertised-price side, and where the
  RFC-conformance correction is recorded
- Mission `marketplace-empty-axes-prices-every-listing-free` — what happens
  when the axis set itself is empty rather than the ids being wrong
- Mission `marketplace-repo-trait-decouple` — the read path these asks flow into
