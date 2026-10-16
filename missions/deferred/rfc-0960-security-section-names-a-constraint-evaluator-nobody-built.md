---
name: rfc-0960-security-section-names-a-constraint-evaluator-nobody-built
description: "RFC-0960 §Security Considerations names six threats, each with a specific named mitigation. Six mechanisms are genuinely built: time-bounded caveats, caveat-level anti-replay via NonceRegistry, the Frozen dispute transition table, shard_for_segment, MultiEnvelope, and the RFC-0957 macaroon verifier. The load-bearing remainder is not built. There is no constraint evaluator anywhere in the tree, so Threat 2's central claim (the verifier runs before any state mutation) cannot hold, Threat 6's CallerBound binding is an encoding that nothing applies, and Threat 1's MaxUses/SingleUse consumption count is never tracked anywhere. Threat 5's three privacy mechanisms have no type to attach to. Threat 4 is the best built of the six: MultiEnvelope is complete and has zero consumers."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0960
---

# Mission `rfc-0960-security-section-names-a-constraint-evaluator-nobody-built`

## Status

Deferred. Every finding here is an RFC edit, a spec gap, or a mechanism to
build. None of it is a break. Nothing was actioned.

## Finding

RFC-0960 §Security Considerations is six threats, carrying sixteen named
mechanisms between them. That specificity is what makes the section auditable —
and auditing it is the whole of the exercise, because the section reads as a
security argument while roughly ten of those sixteen mechanisms are not there.

| Threat                 | Mitigation as written                                                                               | What the tree has                                          |
| ---------------------- | --------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| 1 Capability replay    | `expires_at` checked at every redemption; capability `nonce`; `MaxUses` / `SingleUse` enforce count | time expiry and anti-replay real; **count never enforced** |
| 2 Constraint bypass    | capability verifier runs **before** any state mutation                                              | **no constraint evaluator exists**                         |
| 3 Audit-window gaming  | `Frozen` freezes the transfer; governance capability to resolve; false disputes cut reputation      | **state machine real; freeze and reputation absent**       |
| 4 Event-log divergence | `shard_for_vault` + `shard_for_segment` + `MultiEnvelope`                                           | **two of three real; vault absent**                        |
| 5 Privacy leak         | `Confidential` encrypt + commit; `Private` ZK proof; keys committed                                 | **no privacy type in Rust at all**                         |
| 6 Envelope forgery     | `CallerBound` binding; RFC-0957 macaroon rules; WAL segment hash                                    | **macaroon real; CallerBound never applied**               |

Six of the sixteen hold. The problem is not that this section is empty — it is
that it is specific enough to check, checks well, and does not fully pass. And
the ten that fail are not scattered: nine of them reduce to the single missing
constraint evaluator described next.

## The core finding: no constraint evaluator exists

Threats 1, 2 and 6 all bottom out in the same missing thing. `Constraint` is
defined in `cipherocto-encoding` with 25 variants — the same 25 that RFC-0964
claims to canonically encode, which is consistent and worth saying. Three of
them are the ones these threats name: `MaxUses`, `SingleUse` and `CallerBound`.
All 25 encode. None of the 25 is evaluated.

- `CallerBound` appears in the encoding crate, and exactly once outside it — in
  a `caveat/mod.rs` location that sits **after** the file's `#[cfg(test)]`. It is
  test-only.
- `SingleUse` is a canonical-encoded variant with a roundtrip test. Nothing
  matches on it to apply it.
- `MaxUses` is a caveat that parses, serializes, attenuates and renders in the
  CLI. Outside those, it appears in two fuzz/test sites and nowhere else.

So Threat 2's claim — "capability verifier runs **before** any state mutation" —
describes an ordering guarantee about a verifier that does not exist. There is
nothing to run ahead of the mutation. That is a stronger finding than "the check
is missing": the section asserts a property of the system, and the property is
vacuous rather than violated.

Threat 6 is the same shape. "Capability is bound to holder public key via
`CallerBound` constraint" is true in the sense that the constraint can be encoded
into a macaroon. The binding is never evaluated, so a token carrying it is
indistinguishable at verify time from one without it.

## Threat 1: the count is never tracked, and the nonce is the wrong nonce

Three separate corrections, in increasing order of interest.

**Time expiry is real, under a different name.** `valid_until` / `expires_at` in
the RFC is `Caveat::Before(UnixTimeSecs)` in the tree, with a genuine
`expires_at` check on the verify path. This mitigation holds.

**"Capability carries `nonce` for replay protection" misattributes the field.**
The macaroon's `mint_nonce` is a mint-time seed that diversifies root-key
derivation — it makes two tokens issued from the same secret distinct. It is not
a redemption counter, and it is not checked at verify.

Anti-replay _is_ built, one layer down and correctly: `caveat/payment.rs` carries
a `Nonce` keyed into a `NonceRegistry`, `verify()` observes it, and a second
observation returns `PaymentRejectionReason::Replay`. There is a test for it. So
the mechanism the RFC wants is real; the RFC points at the wrong object for it.

**And the replay window is volatile.** `NonceRegistry` has exactly one
implementation in the whole tree, and it is `InMemoryNonceRegistry`. A restart
empties the observed-nonce set. A capability replayed across a process restart is
not caught. For a security section, an anti-replay guarantee that does not
survive a restart is worth stating as the bound it actually has.

**`MaxUses` / `SingleUse` enforce nothing.** Every `MaxUses` site outside the
caveat enum, its serde mapping, `set_subsumes` and the CLI view is a test. The
attenuation logic compares a child count against a parent count — which is about
what a delegated token is _allowed_ to carry, not how many times it has been
spent. And a repo-wide search for redemption-count tracking (`use_count`,
`uses_remaining`, `redemption_count`, `consumed_count`, `redeem_count`) returns
**zero hits**. There is no counter to enforce against.

## Threat 3: the state machine is real, and nothing consults it

`quota-router-sm-engine` has a proper transition table with `Frozen` reachable
from both `Settled` and `Auditable`, and escapable to `Released` or `Settled`.
That is a real, tested state machine, and it is the part of this section most at
risk of being over-read.

But the RFC says "`Frozen` state **freezes the transfer**", and the table only
validates transitions. It is a predicate, not an interlock. Nothing asks "is this
reservation Frozen?" before applying a transfer — outside the state machine
itself and its tests, `ReservationState::` appears in exactly one constructor.

There is also a second `ReservationState` in `octo-settlement-core`, with a
different variant set (`Pending`, `Active`, `Redeemed`, `Denied`, `Settling`)
and **no `Frozen` and no `Auditable` at all**. The dispute lifecycle lives in a
type the settlement reservation does not share. A settlement cannot represent
being frozen, so the freeze cannot reach the thing it is supposed to freeze.

The reputation half is absent too: `escrow.rs` implements
`Locked → Disputed → Slashed` and states that the last transition "cannot drive"
without authority, but there is no reputation linkage in that file at all. And
the escrow lives under `marketplace/`, which has zero production callers — the
standing caveat from earlier in this audit.

## Threat 5: there is no privacy level to attach anything to

The three mechanisms — encrypt-and-commit for `Confidential`, ZK proof for
`Private`, committed attribute keys — have no type to attach to. A repo-wide
search for a privacy-level enum (`PrivacyLevel`, `DataLevel`, `Confidential`,
`SHARED`) finds nothing in Rust. The only near-hit is an unrelated doc comment in
the Telegram onboarding crate, citing a Google PII guide.

This is the one that reaches past RFC-0960. `CLAUDE.md` documents the Data
Flagging System — `PRIVATE`, `CONFIDENTIAL`, `SHARED`, `PUBLIC` — as a core
architectural concept with `CONFIDENTIAL` defined as "restricted to trusted
agents". Nothing in the tree can represent the distinction. The specification has
a vocabulary the implementation never adopted, which is why §Security
Considerations Threat 5 describes a system with no types in it.

## What is genuinely built

Worth stating plainly, because this section is not empty:

- **Time-bounded caveats** — `Caveat::Before`, checked on the verify path.
- **Caveat-level anti-replay** — `Nonce` + `NonceRegistry`, with a real
  `PaymentRejectionReason::Replay` and a test. In-memory only.
- **The `Frozen` transition table** — reachable, escapable, tested.
- **`shard_for_segment`** — deterministic, in range, distributed across segments,
  five tests.
- **`MultiEnvelope`** — the most completely built mechanism in this section. It
  is a real struct in `quota-router-sm-engine/src/envelope.rs` with a nesting
  depth cap, a `check_nesting_depth` validator, wire serialization with
  `from_wire`, a builder, and a test suite including the RFC-0962 §7 R8-F5
  round-trip.
- **The RFC-0957 macaroon verifier** — real, with a fuzz target at
  `crates/octo-wallet/fuzz/fuzz_targets/capability_verify.rs`.

Threat 4 is therefore the best-implemented threat in the section: two of its
three mechanisms are done, and the third (`shard_for_vault`) has no analogue at
all rather than a partial one.

But `MultiEnvelope` has **zero consumers**. Outside its own module the only
mention anywhere in the tree is a TODO in `octo-wallet`:

> TODO: wire `CapabilityToken::redeem` into the envelope / network
> verifier when a `verify_capability(cap: &&CapabilityToken, ...) -> Result<...>`
> hook lands in `crates/octo-network/src/dot/pce/` (likely alongside
> Gap 2 — MultiEnvelope nesting — per RFC-0962 §7 R8-F5).

Read carefully, that TODO is _not_ saying MultiEnvelope is missing. It is saying
the wallet cannot yet reach it, because the envelope verifier it would plug into
does not exist. The mechanism is built and disconnected. That is the same shape
as the `Marketplace` reachability caveat that has run through this whole audit —
a correct implementation with no caller — and it is why the honest summary of
this section is not "mostly unimplemented":

> The mechanisms missing from Threats 1, 2, 3, 5 and 6 are missing silently.
> The mechanism present in Threat 4 is present but unreachable. Both are worth
> the owner's attention, and they are different work.

## Errors I made while measuring this

Five, all measurement, all recorded because each produced a wrong reading before
being caught.

**I called `MultiEnvelope` absent when it is fully built.** This is the worst of
them, because it went into a draft of this document as a finding. My first search
piped through `head`, and the first ten hits were all comments in files that
mention the name. That read as "referenced but not implemented". The reality is a
complete implementation — struct, depth cap, validator, wire codec, builder,
tests — which the truncated output never showed. The `head` was not a display
convenience; it decided the finding. **This is the third time in this audit that
`head` truncation has turned into a wrong claim, and the second time it has been
caught only by re-running the search without it.** The rule has to be: never
write a negative claim from output a filter can see but a reader cannot. The
correct version of this finding is that `MultiEnvelope` is built and has zero
consumers, which is a better finding than the one I first wrote.

**I searched the repo root and got the worktrees.** `.claude/worktrees/` holds
three gitignored agent worktrees at older commits, and a root-level `grep` with
`head` returns those before `crates/`. My first search for `MaxUses` showed only
worktree hits and I briefly read that as "absent". It is present in six files in
the real tree. The hazard is already known to the repository — two earlier
missions state their census excluded `.claude/worktrees/` explicitly — but the
exclusion is a convention, not an enforcement.

**I truncated with `head` and read the truncation as the result.** The same
search, untruncated, is a per-file count across six files. "The first twenty
lines" was never the same claim as "what exists".

**I kept writing `cd A 2>/dev/null; cd B`.** The workspace already starts in the
repo, so every `cd` is added risk and no benefit. This one has now bitten me
enough times in this audit to be worth naming as a rule rather than a habit. It
cost a whole search: a `missions/` path came back "No such file or directory"
while `missions/` was demonstrably right there.

**I graded Threat 1 partly wrong on the nonce** before reading what `mint_nonce`
is for. It took opening `macaroon.rs` to see it is a derivation seed rather than
a redemption counter. "A field with the right name exists" is not "the named
field does the named job", and I had checked the first and written the second.

## What the owner should do

- **Decide whether the constraint evaluator is in scope.** Three of the six
  threats assume one. Nothing in the tree evaluates `Constraint`. This is the
  single decision that unblocks the rest — until it is made, Threats 1, 2 and 6
  describe properties nothing maintains.
- **Correct Threat 1's attribution** while that is pending. The anti-replay that
  exists is caveat-level, not the macaroon mint nonce, and it is in-memory. Both
  are one-line corrections that make the section describe the system.
- **State the `Frozen` bound honestly.** It is a transition table, not an
  interlock, and `octo-settlement-core` cannot represent a frozen reservation.
  Either wire the interlock or downgrade the claim.
- **Give Threat 5 something to attach to,** or mark it aspirational. A privacy
  level enum in Layer A would make the section's vocabulary real; without one,
  the threat describes nothing implementable.
- **Keep the built half.** Threat 4 and the time/anti-replay mechanisms under
  Threat 1 are real work. The section should not be read as a to-do list; it is
  roughly half implemented, and the missing half is concentrated in one
  dependency: the constraint evaluator.
- **Note `MultiEnvelope`'s reachability in the RFC** if the RFC wants the threat
  to mean something operationally. It is built and disconnected.
- **Resolve the duplicate `ReservationState`.** Two enums with overlapping names
  and disjoint variant sets is a live hazard independent of this RFC.

## Reproduction

```bash
# CONTROL — a symbol known to exist, so a zero-hit search is distinguishable
# from a broken search
grep -rc --include=*.rs 'Marketplace' crates/ | grep -vE ':0$'

# Threat 2 — is there a constraint evaluator?
grep -rn --include=*.rs -iE 'fn (eval|enforce|check|satisf).*constraint' \
  crates/ | grep -v '/tests/'
# empty

# all 25 encode; the three the threats name are among them
awk '/^pub enum Constraint/,/^}/' crates/cipherocto-encoding/src/lib.rs \
  | grep -oE '^    [A-Z][A-Za-z]*' | tr -d ' ' | tr '\n' ' '

# Threat 6 — CallerBound outside the encoding crate is test-only
# caveat/mod.rs opens its test module before line 2280
awk 'NR<2280 && /^#\[cfg\(test\)\]/{n=NR} END{print n}' \
  crates/octo-cap-macaroon/src/caveat/mod.rs

# Threat 1 — MaxUses is confined to encoding, subsumes and the CLI
grep -rn --include=*.rs 'MaxUses' crates/ \
  | grep -vE 'caveat/mod.rs|octo-cli/src/commands/capability.rs|cipherocto-encoding'
# leaves only: two `Constraint::MaxUses` in octo-policy, one fuzz target,
# and a test vector — no production call site

# ...and the two octo-policy sites are inside its test module
awk '/^#\[cfg\(test\)\]/{print NR; exit}' crates/octo-policy/src/lib.rs
awk 'NR<1500 && /^#\[cfg\(test\)\]/{n=NR} END{print "cfg(test) before 1500: " n}' \
  crates/octo-policy/src/lib.rs

# Threat 1 — nothing tracks redemption count, anywhere
grep -rn --include=*.rs -iE \
  'use_count|uses_remaining|redemption_count|consumed_count|redeem_count' crates/
# empty

# Threat 1 — anti-replay exists, in memory, with one impl
grep -rn --include=*.rs 'impl NonceRegistry for' crates/

# Threat 4 — one of the two shard functions exists
grep -nE '^pub (fn|struct|enum)' crates/quota-router-sm-engine/src/shard.rs
grep -rn --include=*.rs 'shard_for_vault' crates/
# empty

# Threat 4 — MultiEnvelope IS implemented. Count the non-test surface first,
# then check who calls it. Do not grep|head this: the first ten hits are
# comments and will read as "referenced but absent".
awk '/^#\[cfg\(test\)\]/{print "tests start: " NR; exit}' \
  crates/quota-router-sm-engine/src/envelope.rs
grep -rn --include=*.rs 'MultiEnvelope' crates/ \
  | grep -v 'sm-engine/src/envelope.rs' | grep -v 'sm-engine/src/shard.rs'
# one line, and it is a TODO in octo-wallet: built, zero consumers

# Threat 5 — no privacy level type exists in Rust
grep -rn --include=*.rs -E 'enum (PrivacyLevel|DataLevel)|Confidential|SHARED' \
  crates/
# empty

# Threat 3 — two ReservationState enums, and only one can be frozen
grep -rn --include=*.rs 'pub enum ReservationState' crates/
```

## Cross-references

- Mission `two-of-rfc-0960s-four-integration-scenarios-name-surface-that-does-not-exist`
  — the same document, where the named verification is thinner than the named
  threat table
- Mission `rfc-0960-closes-its-review-at-r28-and-records-sixteen-rounds` — the
  document that recorded sixteen rounds and still ships this section
- Mission `documentation-that-describes-a-mechanism-nobody-built` — the class
  this belongs to; this is its purest instance, because §Security Considerations
  is the section that exists specifically to describe mechanisms
- Mission `the-marketplace-facade-has-no-production-caller` — the standing
  reachability caveat that makes Threat 3's escrow half inert
- Mission `marketplace-escrow-is-not-wired-into-the-marketplace-facade` — the
  specific reason Threat 3's dispute path cannot arbitrate anything today

There is no existing mission covering the in-memory `NonceRegistry`, which is why
Threat 1's restart bound is recorded here for the first time rather than linked.
