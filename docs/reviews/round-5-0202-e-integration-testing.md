# Round 5 Adversarial Review: Mission 0202-e (Integration Testing and Verification)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-e-bigint-decimal-integration-testing.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 5

---

## Status of Prior Issues

All round 4 fixes are correctly applied:
- **C1** (canonical zero note) — Fixed. AC-5 now includes the RFC-0110 §10.2 note about potential rejection.
- **C2** (DECIMAL within-negative test vectors) — Fixed. AC-10 now includes explicit test vectors with sign-flip explanation.
- **C3** (per RFC §8 ambiguity) — Fixed. Line 53 now reads "per RFC-0202-A §8 estimates."

No prior round 4 issues remain open.

---

## ACCEPTED ISSUES

### C1 · LOW: AC-5 "verify determin crate behavior" is not itself an acceptance criterion

**Severity:** LOW
**Section:** AC-5 (Canonical zero verification), line 33

**Problem:** AC-5 ends with: "Verify determin crate behavior before writing tests."

This is an instruction to the implementer but is not listed as a checkbox item. If the implementer skips this step (or does it but draws the wrong conclusion), the AC will be marked complete despite having unverified assumptions. The "verify" step has no deliverable and no pass/fail criterion.

**Required fix:** Either:
1. Add a checkbox: `- [ ] Confirm determin crate BigInt::from_str("-0") behavior (returns Error or canonical bytes)`; or
2. Remove the "verify before writing tests" instruction and replace with a definitive statement of expected behavior (e.g., "RFC-0110 §10.2 requires rejection; confirm and update AC accordingly").

---

### C2 · LOW: AC-1 and AC-2 do not specify Merkle verification procedure

**Severity:** LOW
**Section:** AC-1 (BIGINT Merkle root), AC-2 (DECIMAL Merkle root), lines 17-27

**Problem:** Both ACs say "Verify Merkle root of test vector outputs matches RFC-0110 §Test Vectors Merkle root" without specifying:
1. Where to find the expected Merkle root hash in the RFC
2. How to compute the Merkle root from the 56/57 test vector outputs
3. What format the test vector outputs take (raw values? serialized bytes? something else?)

An implementer must reverse-engineer the Merkle verification process. This is not trivial — Merkle tree construction varies (which hash function? what pairing order? are intermediate nodes included?).

**Required fix:** Add procedure note to AC-1 and AC-2:
```
- Compute Merkle root of all 56 test vector outputs using SHA-256 (per RFC-0110 §Test Vectors)
- Compare computed root against expected root: [insert expected root here]
- Document: pass/fail and computed root hash
```

If the expected root is too long to inline, reference the specific RFC section and paragraph.

---

### C3 · LOW: Cross-type comparison AC execution gated on Phase 3 with no follow-through

**Severity:** LOW
**Section:** AC-4 (Cross-type comparison tests), line 34

**Problem:** AC-4 says tests "**execute only after Phase 3 (mission 0202-d) is complete**" and explains they will panic in Phase 1-2. This is good warning. But there is no corresponding note in mission 0202-d that says "Phase 3 must implement safe cross-type comparison dispatch to avoid the panic described in 0202-e AC-4."

If 0202-d is implemented without addressing this panic, the tests in 0202-e still cannot run, and the dependency relationship between the missions is unenforceable.

**Required fix:** Add a note to Dependencies section or inline in AC-4:
```
- Phase 3 (0202-d) MUST implement safe cross-type comparison dispatch that avoids as_float64().unwrap() panic, otherwise AC-4 cross-type comparison tests cannot be executed.
```

---

### C4 · LOW: Serialization ACs do not specify which serialize/deserialize API to use

**Severity:** LOW
**Section:** AC-7 (BIGINT serialization), AC-8 (DECIMAL serialization), lines 38-49

**Problem:** AC-7 says "BIGINT '1' serializes to `[13]01000000010000000100000000000000`" and "BIGINT → serialize → deserialize → same value." It does not name the specific function. AC-8 similarly says "DECIMAL → serialize → deserialize."

If an implementer uses a different serialization path than the one that produces the specified wire bytes, the test will fail — but not because the type is wrong, because the API choice is wrong. Different serialization paths (e.g., `Serialize` trait vs internal `to_bytes` method) can produce different encodings for the same logical value.

**Required fix:** Name the specific API:
```
- BIGINT: `BigInt::serialize()` → wire format (as specified above); `BigInt::deserialize()` → round-trip
- DECIMAL: `decimal_to_bytes()` → wire format (as specified above); `decimal_from_bytes()` → round-trip
```

---

## QUESTIONS

**Q1:** Where in RFC-0110 §Test Vectors and RFC-0111 §Test Vectors is the expected Merkle root hash? The mission references these sections but an implementer would need to search the RFC for the specific root value. Is the root hash inline in the RFC, or should it be extracted from the test vector table via a script?

**Q2:** Does mission 0202-d (bigint-decimal-vm) explicitly include implementing safe cross-type comparison dispatch that avoids the `as_float64().unwrap()` panic? If not, who owns that fix?

**Q3:** What hash function is used for Merkle root computation in the test vectors — SHA-256, Keccak-256, or something else? This is critical for implementing the verification correctly.

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action |
|----|----------|-------|-----------------|
| C1 | LOW | "Verify determin crate behavior" has no deliverable | Convert to explicit checklist item or bake into AC text |
| C2 | LOW | Merkle verification procedure unspecified | Add expected root hash reference and computation procedure |
| C3 | LOW | Phase 3 panic warning unenforceable without 0202-d coordination | Add cross-reference to 0202-d requiring the fix |
| C4 | LOW | Serialization API unspecified | Name the specific function (serialize/deserialize) |

---

## Verdict

**Ready to start** after C1–C4 are addressed. All are LOW severity. C1 is a documentation gap (floating instruction not tied to a checkbox). C2 is a procedure gap (Merkle verification is non-trivial without a worked example). C3 is a cross-mission dependency that should be explicit in both missions. C4 is an API naming omission. No MODERATE or CRITICAL issues found.