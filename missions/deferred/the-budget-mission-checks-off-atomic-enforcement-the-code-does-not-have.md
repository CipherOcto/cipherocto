---
name: the-budget-mission-checks-off-atomic-enforcement-the-code-does-not-have
description: "missions/archived/0934-a-budget-spend-tracking.md is archived as complete and checks off six properties of RFC-0934's budget enforcement. Five of them are not in the code. The atomic UPDATE has no guard clause, so nothing is actually checked. check_budget contains no SQL at all, being a Rust read-then-compare. reset_budget has zero callers. query_optional, the API the mission claims to use, does not exist anywhere in the workspace. There is no send_alert, so the webhook is a column nobody posts to. Separately and more seriously, update_spend has no production caller, so no request ever records spend and the budget check reads a value that live traffic never advances. RFC-0934 itself is carefully written and discloses its own limitations, which is what makes the mission's claims checkable."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0934
    - 0934-a-budget-spend-tracking
---

# Mission `the-budget-mission-checks-off-atomic-enforcement-the-code-does-not-have`

## Status

Deferred. Six delivery claims, a reachability gap, and a question of whether
archived mission checkboxes are ever re-verified. Nothing was actioned, and the
archived mission was **not** rewritten — historical records are not edited.

## Finding

RFC-0934 specifies budget enforcement. Its build mission,
`missions/archived/0934-a-budget-spend-tracking.md`, is archived as complete and
checks off six properties. **Five are not in the code, and the sixth is true only
in a way that does not protect anything.**

The RFC is not the problem here. It is one of the better-specified documents in
the corpus, and its care is what made the mission's claims checkable.

## What the mission claims against what the code does

| Archived mission checkbox                                                                    | Reality                                                                                                                                      |
| -------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `[x]` Atomic UPDATE with WHERE clause to avoid race conditions (check THEN record)           | The only UPDATE is `current_spend = current_spend + $1 WHERE entity_id = $2 AND entity_type = $3` — **no guard clause**. Nothing is checked. |
| `[x]` Pre-request budget check using single atomic UPDATE with CASE (reset period if needed) | `check_budget` contains **no SQL at all**. It calls `get_spend()` and compares in Rust.                                                      |
| `[x]` Budget reset on period boundary (atomic, part of `check_budget`)                       | `reset_budget` has **zero callers** and is not part of `check_budget`.                                                                       |
| `[x]` Use `query_optional()` for budget lookups                                              | `query_optional` **does not exist anywhere in the workspace**.                                                                               |
| `[x]` Alert webhook when `soft_limit_percentage` exceeded                                    | `alert_webhook` is a column and a struct field. There is **no `send_alert`**. Nothing posts.                                                 |
| `[x]` Return `BudgetError::KeyBudgetExceeded` when hard limit exceeded                       | Two variants exist; the check path returns `KeyError::BudgetExceeded`, a different enum.                                                     |

## The one that matters most: nothing records spend

This is worse than any of the six.

`update_spend` is the function that increments `current_spend`. Its only call
sites in the entire workspace are inside `#[cfg(test)]` modules — including the
one in `proxy.rs`, which sits below that file's test boundary. The parallel
`record_spend` path is the same: every caller is a test.

So in production, `current_spend` is never advanced by a request. And the proxy's
budget check reads it:

```rust
match storage.get_budget(&team_id.to_string(), "team") {
    Ok(Some(budget)) => {
        if budget.current_spend >= budget.budget_limit { reject }
    }
```

**The check compares a value that live traffic never moves.** Per-team budget
enforcement is inert, and no test would reveal it because every test writes spend
explicitly before checking.

This is the same shape as the reachability finding from the marketplace sweep —
correct, tested components with no production caller — appearing in a
security-relevant path rather than an economic one. The difference is that
`task_market` being unreachable wastes work, while a budget ceiling that is never
advanced under-bills rather than over-bills. **It fails in the direction that
looks safe**, which is precisely why nothing flagged it.

## The RFC is the good document here

Worth stating at length, because it is what makes this a mission about the
mission rather than about RFC-0934.

RFC-0934 anticipates its own problems and says so in the prose:

- **On the atomicity, twice and in bold terms.** "CHECK FIRST, THEN RECORD —
  avoid charging for blocked requests", "Atomic check-and-increment: only
  increments if budget allows", "Single atomic statement … Avoids TOCTOU race
  between check and reset."
- **On its own limitation:** "Each entity can have only ONE budget row … A key
  cannot have both a daily budget AND a monthly budget simultaneously."
- **On approximation drift:** the fixed 30-day month is called out as
  accumulating error and justified against LiteLLM rather than glossed.
- **On deferral:** `User` and `Team` are marked "Reserved — deferred to future
  RFC" rather than implied to work.
- **On type choice:** i64 microdollars, with the reason given ("avoids
  floating-point precision issues") and the existing `ApiKey::budget_limit` type
  cited as the constraint to match.

A reader could take this document and implement it correctly. The gap between
the specification and the delivery is not a specification gap.

## Where the implementation genuinely matches

Not everything is missing, and the parts that landed are close to the RFC:

- **`BudgetPeriod { Daily, Weekly, Monthly, Total }` and `EntityType { Key, User,
Team }`** exist with **exactly** the variants specified.
- The `budgets` table exists with the RFC's primary key `(entity_id,
entity_type)` and its `soft_limit_pct` and `alert_webhook` columns.
- Four production write paths exist against it: create, update limit, increment,
  reset.

## Three schema divergences worth recording

| RFC-0934 specifies                           | Implemented                                         |
| -------------------------------------------- | --------------------------------------------------- |
| `max_budget BIGINT NOT NULL`                 | `budget_limit INTEGER NOT NULL`                     |
| `period_start`, `period_end` BIGINT NOT NULL | replaced by `last_reset INTEGER NOT NULL`           |
| `soft_limit_pct INTEGER NOT NULL DEFAULT 80` | `soft_limit_pct INTEGER` — **nullable, no default** |

The nullable `soft_limit_pct` matters more than it looks: RFC-0934's soft-limit
arithmetic is `max_budget * soft_limit_pct / 100`, and the RFC reads that value
from the row. A nullable column with no default makes "no soft limit set"
indistinguishable from "80 percent" to a naive reader, and the RFC never says
what None means.

The table's own comment attributes it to a different RFC: `// budgets table
(RFC-0914, Mission 0914-a)`. RFC-0934 is not named in the schema it specifies.

## One small attribution slip in the RFC

RFC-0934 says `compute_cost_from_pricing_table` lives in `pricing.rs` in one
place and `keys/mod.rs` in two. It is in `keys/mod.rs`. The archived mission got
this right, so the RFC is the document that drifted.

## Why this was not caught

The six boxes were checked against the _mission's_ reading of the RFC, not
against the tree. Three of them name mechanisms whose absence is trivially
visible — `query_optional` is a symbol that does not exist anywhere, and
`send_alert` is a function that does not exist anywhere. Neither required running
a test to establish.

**The pattern worth generalising:** a checkbox asserting a _mechanism_ is only as
good as someone reading the code. A checkbox asserting an _outcome_ — "a
concurrent pair of requests cannot exceed the limit" — would have forced the
check, because the only way to tick it is to demonstrate it.

## What the owner should do

- **Re-open the delivery question for RFC-0934**, without rewriting the archived
  mission. What shipped is a storage substrate and an inert check; what the
  mission describes is an enforcement mechanism.
- **Decide whether budget enforcement should be atomic at all.** The RFC says
  yes and the implementation says no. Either restore the guarded increment
  (`AND current_spend + $1 <= budget_limit`) or amend RFC-0934 to describe
  best-effort enforcement. Silently leaving both true is the one option not
  available.
- **Wire spend recording into the request path**, or state that budgets are
  advisory. Right now the number the check reads is never written in production.
- **Add one outcome-level test:** concurrent requests against a limit, asserting
  the total never exceeds it. That single test would have caught all three of
  the atomicity claims.
- **Settle the `soft_limit_pct` nullability**, and decide what `max_budget`
  versus `budget_limit` is meant to be.
- **Do not edit `missions/archived/0934-a`.** Its claims were true when written,
  or were believed to be, and the record of that belief is worth keeping.

## Reproduction

```bash
# CONTROL — the search reaches these files
grep -rn --include=*.rs 'pub struct Budget\b\|pub enum BudgetPeriod' crates/

# the enums match the RFC exactly
grep -rn --include=*.rs -A5 'pub enum BudgetPeriod' crates/quota-router-core/src/cache.rs
grep -rn --include=*.rs -A4 'pub enum EntityType' crates/quota-router-core/src/cache.rs

# CLAIM 1 — the atomic UPDATE has no guard clause
grep -rn --include=*.rs 'UPDATE budgets SET current_spend = current_spend' crates/
# ...compare against what RFC-0934 specifies
grep -n -A6 'Atomic check-and-increment' \
  rfcs/accepted/economics/0934-budget-management-spend-tracking.md

# CLAIM 2 — check_budget contains no SQL
grep -n -A16 'pub fn check_budget' crates/quota-router-core/src/middleware.rs

# CLAIM 3 — reset_budget has no callers
grep -rn --include=*.rs 'reset_budget' crates/

# CLAIM 4 — query_optional does not exist
grep -rn --include=*.rs 'fn query_optional' crates/
# empty

# CLAIM 5 — no send_alert
grep -rn --include=*.rs 'fn send_alert' crates/
# empty
grep -rn --include=*.rs 'alert_webhook' crates/quota-router-core/src/schema.rs

# THE BIG ONE — update_spend has no production caller
grep -rn --include=*.rs 'update_spend' crates/
# the only call site outside storage.rs sits past proxy.rs's test boundary
awk 'NR<=7774 && /^#\[cfg\(test\)\]/ {n=NR} END{print "last cfg(test) before that call = " n}'
# 3390, so it is inside tests
grep -rn --include=*.rs '\.record_spend(' crates/

# and the check that reads a value nobody writes
sed -n '804,816p' crates/quota-router-core/src/proxy.rs

# the three schema divergences
grep -n -A12 'CREATE TABLE IF NOT EXISTS budgets' \
  crates/quota-router-core/src/schema.rs
grep -n -A13 'CREATE TABLE budgets' \
  rfcs/accepted/economics/0934-budget-management-spend-tracking.md

# what the RFC promises, in its own words
grep -n 'CHECK FIRST, THEN RECORD\|Avoids TOCTOU\|only increments if budget allows' \
  rfcs/accepted/economics/0934-budget-management-spend-tracking.md

# and what the archived mission checked off
grep -nE '^- \[x\]' missions/archived/0934-a-budget-spend-tracking.md
```

## Cross-references

- Mission `the-marketplace-facade-has-no-production-caller` — the same shape,
  correct and tested with no production caller, but here in an enforcement path.
  Amended to v1.1 for `task_market`, which joined it for the same reason and went
  unnoticed for the same reason
- Mission `documentation-that-describes-a-mechanism-nobody-built` — RFC-0934 is
  the inverse case, and worth pairing: the description is careful and the
  delivery is not
- Mission `nine-accepted-rfcs-have-a-version-history-whose-only-row-is-that-the-table-was-added`
  — RFC-0934 is one of the nine, and this mission is the evidence that a
  placeholder Version History says nothing about the document's quality
- Mission `two-crates-define-spend-ledger-with-incompatible-schemas` — the other
  spend-tracking substrate, and a different problem in the same area
