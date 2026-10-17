---
name: prefix-resolver-already-exists-proposal-withdrawn
description: "WITHDRAWN. A mission proposed extending the anchors linter candidates function with a trailing-qualifier prefix resolver, claiming it would resolve 799 findings with zero single-word masking. On review the resolver already exists. resolves() performs a bidirectional word-boundary prefix fallback, added earlier in the same session by commit 9496e190, and it already refuses to resolve down to a single word. The proposal was redundant and its measurements were produced by a simulation that did not model the linters own numeric-anchor gating."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `prefix-resolver-already-exists-proposal-withdrawn`

## Status

**Withdrawn. The proposal is redundant and its measurements are wrong.**

Kept rather than deleted because the error is the useful record: a plausible,
carefully-measured proposal for a change the codebase already contained.

## What was proposed

Extend `candidates` so a trailing qualifier is dropped, claiming:

- 1,237 findings on the gated tree, reduced to 438
- 799 findings resolved
- 242 baselined entries made dead
- zero cases resolved by a single-word heading, presented as the check that
  made it safe

## Why it is withdrawn

**The resolver already exists.** `resolves` does not merely call
`candidates`. It falls back in both directions:

```python
longest = candidates(name)[-1]
if longest and any(k.startswith(longest + " ") for k in known):
    return True
words = longest.split()
for cut in range(len(words) - 1, 1, -1):
    if " ".join(words[:cut]) in known:
        return True
```

The first block handles a heading that extends the anchor. The second handles
the direction this proposal claimed to add: an anchor carrying words the
heading does not. And the `range(len(words) - 1, 1, -1)` bound is precisely the
"never resolve down to a single word" constraint that was presented as this
proposal's key safety property.

The commit that added it is `9496e190`, "fix: resolve anchors whose heading is
a word-prefix", whose message cites the same corpus examples this proposal
rediscovered: `Implementation Phases Phase 1` against `Implementation Phases`,
and `Adversarial Review Process Dry Run` against `Adversarial Review Process`.

The proposal was made after that commit, in the same session.

## The measurements were also wrong

Every figure above came from a simulation that reimplemented the resolution
loop. The simulation missed two properties of the real linter:

1. **Numeric anchors are gated.** A numeric reference is only checked when its
   line also cites a project RFC, so that external specifications such as
   `RFC 8032 §5.1.6` are not reported. The simulation applied no such gate.
2. **`resolves` was replaced by a naive membership test.** The simulation used
   `form in known` across a locally extended candidate list, discarding the
   real bidirectional fallback.

The consequence: the simulation reported 1,237 findings where the live linter
reports **599**. Every downstream figure — 799 resolved, 242 dead, 401 after
the terminator, 15 new names — inherits the error.

A related claim in the parent mission is corrected by the same discovery. That
mission measured `docs/` and `missions/` with the same style of simulation and
reported 827 qualified citations needing a resolver. A substantial part of
that number is already handled by the existing fallback.

## What remains true

The terminator change and the scan-scope question are unaffected, because
their measurements were taken against the real extraction and classifier
rather than this simulation. Only the resolver figures are void.

## Note

Three claims in this area have now been wrong in the same way: an aggregate
read as a conclusion, a bulk classification read as a set of facts, and a
proposal for machinery that already existed. The common move was reasoning
forward from reading the code without checking whether the behaviour was
already implemented.

The check that would have caught this one is the cheapest available: `git log -S`
on the distinctive string, or reading the function that is called rather than
the function one is looking at.
