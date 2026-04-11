# Round 6 Adversarial Review: Mission 0202-d (Expression VM Support)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-d-bigint-decimal-vm.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 6

---

## Status of Prior Issues (Round 5)

| ID | Severity | Issue | Status |
|----|----------|-------|--------|
| D1 | LOW | A3 (stale round-3 review) still unfixed | **NOT FIXED** |
| D2 | LOW | NEG/ABS status unresolved — not VM-dispatchable or missing from AC? | **NOT FIXED** |
| D3 | LOW | DECIMAL AVG gas ambiguity — SUM gas billed separately or combined? | **NOT FIXED** |

---

## ACCEPTED ISSUES

### E1 · CRITICAL: BITLEN opcode missing entirely — not implemented anywhere

**Severity:** CRITICAL
**Section:** AC (BIGINT operation dispatch), ops.rs, vm.rs

**Problem:** AC item 1 lists BITLEN as a required BIGINT operation dispatch. The `Op` enum has no `BITLEN` variant, and vm.rs has no `Op::BitLen` dispatch case. RFC-0110 §8 v2.14 specifies BITLEN gas as `10 + limbs` (worst case 74 gas for 64 limbs).

**Required fix:** Add `Op::BitLen` to the Op enum and implement dispatch in vm.rs executing `bigint_bitlen(input)`, metering `10 + limbs` gas.

---

### E2 · CRITICAL: SHL/SHR in VM operate on INTEGER values — no BIGINT-specific implementation

**Severity:** CRITICAL
**Section:** vm.rs lines 1202–1228

**Problem:** The mission AC requires BIGINT SHL/SHR with pre-flight bounds check `(0 <= shift < 8 * num_limbs)`. The current `Op::Shl`/`Op::Shr` at vm.rs:1202-1228 operate on `Value::Integer` using `wrapping_shl`/`wrapping_shr` — these are plain INTEGER bit-shift operations with no bounds check, no gas metering, and no BIGINT limb validation.

**Required fix:** Add dedicated BIGINT SHL/SHR opcodes (e.g., `Op::BigIntShl`, `Op::BigIntShr`) that:
1. Extract limb count from the BIGINT value
2. Perform pre-flight bounds check: `if shift >= 8 * num_limbs { return Error::invalid_argument("shift out of bounds") }`
3. Consume 10 gas for pre-flight check; consume full `10 + limbs` gas on success

---

### E3 · MODERATE: DfpDiv division-by-zero silently returns NULL instead of error

**Severity:** MODERATE
**Section:** vm.rs lines 1100–1128

**Problem:** The AC requires that division by zero return `Error::invalid_argument("division by zero")`. The current implementation checks `if dfp_b.to_f64() == 0.0` and returns `Value::Null(DataType::DeterministicFloat)` — silently, with no error.

**Required fix:** If divisor is zero, consume 10 gas (pre-flight only) and return `Error::invalid_argument("division by zero")`. Do not execute `dfp_div`.

---

### E4 · LOW: DqaDiv uses wrong error variant

**Severity:** LOW
**Section:** vm.rs line 3825–3826

**Problem:** `arithmetic_op_quant` maps `DqaError::DivisionByZero` to `crate::core::Error::internal("DQA division by zero")`. The AC explicitly requires `Error::invalid_argument("division by zero")`.

**Required fix:** Change to `crate::core::Error::invalid_argument("division by zero")`.

---

### E5 · LOW: DfpDiv division-by-zero has no gas metering

**Severity:** LOW
**Section:** Gas metering, vm.rs lines 1100–1128

**Problem:** Even if E3 is fixed, the current DfpDiv has no gas metering at all. The AC requires pre-flight bounds check (10 gas) for division by zero.

**Required fix:** Add gas metering — pre-flight check (10 gas) if divisor is zero; full `50 + 3 * scale_a * scale_b` gas on successful execution.

---

### E6 · LOW: DfpSqrt has no gas metering

**Severity:** LOW
**Section:** Gas metering, vm.rs lines 1001–1020

**Problem:** AC requires SQRT gas of `100 + 5 * scale`. The `Op::DfpSqrt` dispatch has no gas metering.

**Required fix:** Add gas metering for DfpSqrt: `100 + 5 * scale` where scale is extracted from the input DFP.

---

### E7 · LOW: NEG/ABS opcodes exist but not in AC or Reference

**Severity:** LOW
**Section:** Reference section, ops.rs line 928–929, vm.rs lines 955–981

**Problem:** `Op::DqaNeg` and `Op::DqaAbs` exist in ops.rs and are implemented in vm.rs. But the AC's BIGINT operation list does not include NEG or ABS, and the Reference section does not clarify whether they are VM-dispatchable. D2 from round-5 requested clarification.

**Required fix (per D2 resolution):** Add to Reference section:
> - RFC-0110 §7 (BigInt NEG, ABS defined but NOT VM-dispatchable via 0202-d; excluded from this mission — `Op::DqaNeg`/`Op::DqaAbs` exist for future use)

---

### E8 · LOW: AVG blocking references non-existent RFC-0202-B

**Severity:** LOW
**Section:** AC (BIGINT aggregate dispatch), mission line 27

**Problem:** The mission states: "AVG: blocked — returns `Error::NotSupported("AVG on BIGINT requires RFC-0202-B")`." There is no RFC-0202-B in the repository.

**Required fix:** Either remove the RFC-0202-B reference or update with a valid RFC identifier.

---

### E9 · MODERATE: Dependency on 0202-c may be overstated

**Severity:** MODERATE
**Section:** Dependencies, mission line 56

**Problem:** The mission states 0202-d is blocked by 0202-c (persistence layer). However, the VM expression operations (DQA/DFP arithmetic, aggregates) do not require persistence — they operate on in-memory `Value` types. The dependency may be conservatively stated but is not technically accurate for expression evaluation.

**Required fix:** Clarify which specific AC items depend on 0202-c persistence completion, or remove the blocking dependency if VM ops are independently implementable.

---

### E10 · LOW: Integer-to-DQA promotion path has confirmed bug (line 3870)

**Severity:** LOW
**Section:** vm.rs lines 3862–3890

**Problem:** Test comment at line 6281 states: "test_dqa_integer_promotion removed - the Integer + DQA path in arithmetic_op_quant has a bug (wrong variable on line 3870)." When `a` is Integer and `b` is Extension, the same variable `i` is used for both branches, corrupting the computation.

**Required fix:** Fix the bug (second `int_to_dqa(*i)` at line 3870 should use the other variable) or explicitly reject mixed-type path with a type check that returns Null.

---

## UNRESOLVED PRIOR ISSUES (Carry-forward)

### D1 · LOW: Round-3 review still has no resolution note

**Status:** NOT FIXED. Round-3 review at `docs/reviews/round-3-0202-d-vm.md` still has no resolution note.

**Required fix:** Add to top of `docs/reviews/round-3-0202-d-vm.md`:
> **Resolution:** Issues C1 (SQRT under BigInt), C2 (MIN/MAX gas missing), C3 (BITLEN placeholder) were resolved in mission update commit dceb19c (2026-04-11). See [round-4 review](round-4-0202-d-vm.md) for details.

---

### D2 · LOW: NEG/ABS status still unresolved

**Status:** NOT FIXED. Still no clarification in the mission Reference or AC about NEG/ABS.

**Required fix:** Add to Reference section:
> - RFC-0110 §7 (BigInt NEG, ABS defined but NOT VM-dispatchable via 0202-d; excluded from this mission)

---

### D3 · LOW: DECIMAL AVG gas ambiguity still unresolved

**Status:** NOT FIXED. The DECIMAL AVG AC does not clarify whether SUM gas is billed separately.

**Required fix:** Add clarifying note:
> "AVG gas (15 + 3 × scale) includes sum computation; do not bill SUM gas separately. If sum exceeds ±(10^36 − 1), return DecimalError::Overflow before computing average."

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action |
|----|----------|-------|-----------------|
| E1 | CRITICAL | BITLEN opcode missing | Add `Op::BitLen` to Op enum and implement dispatch |
| E2 | CRITICAL | SHL/SHR operate on INTEGER, not BIGINT | Add BIGINT-specific SHL/SHR with bounds check |
| E3 | MODERATE | DfpDiv division-by-zero returns NULL | Return `Error::invalid_argument("division by zero")` |
| E4 | LOW | DqaDiv wrong error variant | Use `Error::invalid_argument("division by zero")` |
| E5 | LOW | DfpDiv has no gas metering | Add pre-flight (10) + operation gas |
| E6 | LOW | DfpSqrt has no gas metering | Add `100 + 5 * scale` gas |
| E7 | LOW | NEG/ABS opcodes exist but not in AC/Reference | Add NEG/ABS excluded-from-VM note |
| E8 | LOW | AVG blocking references non-existent RFC-0202-B | Update or remove reference |
| E9 | MODERATE | Dependency on 0202-c may be overstated | Clarify which items actually depend on persistence |
| E10 | LOW | Integer+DQA promotion bug at line 3870 | Fix the bug or reject mixed-type path |
| D1 | LOW | Round-3 review still no resolution note | Add resolution note |
| D2 | LOW | NEG/ABS status still unresolved | Add NEG/ABS excluded-from-VM note |
| D3 | LOW | DECIMAL AVG gas ambiguity | Add clarifying note |

---

## Verdict

**Not ready to start.** This mission has 2 CRITICAL issues (E1, E2) that represent missing implementation — BITLEN and BIGINT-specific SHL/SHR with bounds checking are not implemented despite being listed in the AC. E3-E10 and D1-D3 are additional issues. The core concern is that many AC items are described in the mission but not found in the codebase.

Before re-review, the implementer should either:
1. Confirm these AC items are deferred to a later mission (and update the mission), or
2. Provide code showing these are implemented under different names/patterns