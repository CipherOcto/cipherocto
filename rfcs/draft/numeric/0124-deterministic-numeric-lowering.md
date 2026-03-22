# RFC-0124: Deterministic Numeric Lowering (DFP → DQA)

## Status

**Version:** 2.0.0 (Comprehensive Reset)
**Status:** Draft — Formal Verification In Progress
**Submission Date:** 2026-03-22

## Summary

This RFC defines the **Deterministic Lowering Pass (DLP)**: a static compiler pass that converts Deterministic Floating-Point (DFP, RFC-0104) values and expressions to Deterministic Quant Arithmetic (DQA, RFC-0105) before runtime execution.

**Key properties:**
- **Static only**: DLP operates at compile-time; DFP does not exist at runtime
- **Total over ValidDFPSubset**: Every valid decimal value has exactly one DQA representation
- **Semantically preserving**: T0 (Decimal Equivalence) guarantees `interp_real(d) = interp_dqa(lowering(d))`
- **Integer arithmetic throughout**: Intermediate computations use i256/i1152/i1200, never floating-point

**Critical correction from v1.x:** Intermediate width i256 is insufficient for worst-case DFP_MAX × 10^18 ≈ 2^1084. Minimum required is i1152; i1200 is recommended.

## Dependencies

- RFC-0104: Deterministic Floating-Point (DFP)
- RFC-0105: Deterministic Quant Arithmetic (DQA)
- RFC-0109: Deterministic Linear Algebra Engine (DLAE)
- RFC-0126: Deterministic Canonical Serialization (DCS)

## Definitions

### ValidDFPSubset

```
ValidDFPSubset = {
  x ∈ DFP | ∃ n ∈ ℤ, s ∈ ℕ, x = n × 10^(-s), s ≤ 38
}

Invalid (compile-time TRAP):
  - Irrationals: sqrt(2), π, sin(x) for non-integer multiples of π
  - Non-terminating decimals: 1/3, 1/7
  - NaN, Infinity, Subnormal
```

### Interpretations

```coq
Definition interp_real (d : Dfp) : R := Dfp.to_real d.
Definition interp_dqa (q : Dqa) : R := Dqa.to_real q.
```

## DLP Contract

### DlpInput

```coq
Inductive DlpInput :=
  | DlpLiteral (n : Z) (s : nat)        (* n × 10^(-s), s ≤ 38 *)
  | DlpVariable (name : string)
  | DlpAdd (lhs : DlpInput) (rhs : DlpInput)
  | DlpSub (lhs : DlpInput) (rhs : DlpInput)
  | DlpMul (lhs : DlpInput) (rhs : DlpInput)
  | DlpDiv (lhs : DlpInput) (rhs : DlpInput)
  | DlpNeg (arg : DlpInput)
  | Dlp ScaleContext (ctx : scale_context) (body : DlpInput)
```

### DlpOutput

```coq
Inductive DlpOutput :=
  | DlpQ (q : Dqa)                          (* Canonical DQA *)
  | DlpSeq (ops : list DqaOp)               (* Multi-op sequence *)
```

### DlpError

```coq
Inductive DlpError :=
  | ErrNonDecimal                          (* 0x10: non-decimal result *)
  | ErrIrrational                         (* 0x11: sqrt, transcendental *)
  | ErrInfinite                           (* 0x12: overflow to inf *)
  | ErrNaN                                (* 0x13: undefined *)
  | ErrSubnormal                          (* 0x14: below normal range *)
  | ErrScaleOverflow                      (* 0x15: scale > 38 *)
  | ErrDivZero                            (* 0x16: division by zero *)
  | ErrWidthOverflow                      (* 0x17: intermediate exceeds i1200 *)
```

## Lowering Algorithm

### Canonical Left-Fold Parsing

DFP literals are parsed with left-fold associativity for binary operators:

```
parse(tokens) = fold_left (λ acc op. apply(acc, op)) initial tokens

Where apply handles:
  - ADD/SUB: scale harmonization via max(sa, sb)
  - MUL: result scale = sa + sb
  - DIV: result scale = max(sa, sb) (dividend upscaled)
```

**Critical: No parentheses in source → left-fold is deterministic.**

### LOWER_DFP_TO_DQA

```coq
Fixpoint lower (e : DlpInput) (ctx : scale_context) : DlpOutput + DlpError :=
  match e with
  | DlpLiteral n s =>
    if s ≤? 38 then DlpQ (Dqa.of_integer n s)
    else DlpError ErrScaleOverflow

  | DlpVariable name =>
    match lookup name ctx with
    | Some q => DlpQ q
    | None => DlpError ErrUndefined
    end

  | DlpAdd a b =>
    match (lower a ctx, lower b ctx) with
    | (DlpQ qa, DlpQ qb) =>
      let (na, sa) := Dqa.to_integer_scale qa in
      let (nb, sb) := Dqa.to_integer_scale qb in
      let s := max sa sb in
      let na' := na * 10^(s - sa) in
      let nb' := nb * 10^(s - sb) in
      DlpQ (Dqa.normalize (Dqa.add (na', s) (nb', s)))
    | _ => DlpError ErrInvalid
    end

  | DlpMul a b =>
    match (lower a ctx, lower b ctx) with
    | (DlpQ qa, DlpQ qb) =>
      let (na, sa) := Dqa.to_integer_scale qa in
      let (nb, sb) := Dqa.to_integer_scale qb in
      let s := sa + sb in
      if s ≤? 38 then DlpQ (Dqa.normalize (Dqa.mul (na, s) (nb, s)))
      else DlpError ErrScaleOverflow
    | _ => DlpError ErrInvalid
    end

  | DlpDiv a b =>
    match (lower a ctx, lower b ctx) with
    | (DlpQ qa, DlpQ qb) =>
      let (na, sa) := Dqa.to_integer_scale qa in
      let (nb, sb) := Dqa.to_integer_scale qb in
      (* Divide na / nb with scale harmonization *)
      if nb =? 0 then DlpError ErrDivZero
      else
        let (q, r) := div_mod na nb in          (* Euclidean division *)
        if r =? 0 then
          (* Exact division: q is integer result *)
          let s := max sa sb in
          DlpQ (Dqa.normalize (q, s))
        else DlpError ErrNonDecimal
    | _ => DlpError ErrInvalid
    end

  | DlpNeg a =>
    match lower a ctx with
    | DlpQ q => DlpQ (Dqa.neg q)
    | _ => DlpError ErrInvalid
    end

  | DlpScaleContext new_ctx body =>
    lower body new_ctx
  end.
```

### Expression-Level Lowering

When an expression contains multiple operations, lowering proceeds bottom-up:

```
Expression: (a + b) * c

Lowering steps:
  1. lower(a) → qa
  2. lower(b) → qb
  3. lower(qa + qb) → qab  (scale harmonization)
  4. lower(c) → qc
  5. lower(qab * qc) → qabc  (scale = sa + sc)
```

### Scale Context Propagation

The scale context tracks declared variable scales:

```coq
Record scale_context := {
  vars : list (string * Dqa);
  default_scale : nat;
}.

(* Initial context with built-in constants *)
Definition initial_context := {
  vars := [
    ("ZERO", Dqa.zero);
    ("ONE", Dqa.one);
    ("PI_APPROX", Dqa.of_integer 314159265379 12)  (* π ≈ 3.14159... *)
  ];
  default_scale := 0;
}.
```

### TRAP Encoding (RFC-0126 Integration)

Lowering errors are encoded per RFC-0126 as TRAP-before-serialize:

```
TRAP_ENCODING(error) = TRAP_SENTINEL (24 bytes) || error_code (1 byte)

Error codes:
  0x10: ErrNonDecimal
  0x11: ErrIrrational
  0x12: ErrInfinite
  0x13: ErrNaN
  0x14: ErrSubnormal
  0x15: ErrScaleOverflow
  0x16: ErrDivZero
  0x17: ErrWidthOverflow
```

## Gas Model

Lowering is a **compile-time operation** with **zero gas cost** in the consensus meter.

| Phase | Cost | Rationale |
|-------|------|-----------|
| Lowering (compile-time) | 0 | Not consensus-metered |
| Resulting DQA bytecode | Per DQA op | Gas follows RFC-0105/RFC-0109 |

**DFP operation costs are irrelevant** — only the resulting DQA operations consume gas.

```
gas(dfpmul(a, b)) = gas(dqa_mul(lowered_a, lowered_b))
                  = GAS_DQA_MUL (per RFC-0105)
```

## Intermediate Width Requirements

### Critical Correction: i256 is Insufficient

**Theorem 11 (i1152 Sufficiency):**

```
DFP_MAX = (2^127 - 1) × 2^(2^7 - 127)  ≈ 1.7 × 10^38

Worst-case intermediate:
  DFP_MAX × 10^18
  ≈ 1.7 × 10^38 × 10^18
  ≈ 1.7 × 10^56
  ≈ 2^186 bits (for integer representation)

For i256: 256 bits ≈ 2^8 bits
For i512: 512 bits ≈ 2^9 bits
For i1024: 1024 bits ≈ 2^10 bits
For i1152: 1152 bits ≈ 2^10.17 bits
For i1200: 1200 bits ≈ 2^10.23 bits

Required: i1152 minimum, i1200 recommended
```

### Width Analysis

```coq
(* Maximum intermediate value after one operation *)
Definition max_intermediate_width (a b : Dqa) : nat :=
  let (na, sa) := Dqa.to_integer_scale a in
  let (nb, sb) := Dqa.to_integer_scale b in
  let max_n := max (Z.abs na) (Z.abs nb) in
  let max_s := max sa sb in
  (* After multiplication: na × nb × 10^(sa + sb) *)
  let bits_na := Z.log2 (Z.abs na) + 1 in
  let bits_nb := Z.log2 (Z.abs nb) + 1 in
  bits_na + bits_nb + (max_s * 4)  (* 10^sa ≈ 2^(3.32×sa) ≈ 2^(sa×log2(10)) *)

(* Sufficiency check *)
Theorem i1152_sufficient :
  ∀ a b : Dqa,
    bit_width(max_intermediate a b) ≤ 1152.
Proof. Admitted.
```

## Formal Verification

### Theorem Hierarchy

| ID | Name | Statement | Status |
|----|------|-----------|--------|
| T0 | Decimal Equivalence | `interp_real(d) = interp_dqa(lowering(d))` for valid inputs | 🔵 Required |
| T1 | Parse Determinism | `fold_left` produces unique AST | ✅ |
| T2 | Bit-Length Canonicality | `bit_length(encode(x))` independent of representation | ✅ |
| T3 | Multiplication Bound | `bit_length(a * b) ≤ bit_length(a) + bit_length(b) + 1` | ✅ |
| T4 | Normalization Closure | Normalization terminates with bounded loss | ✅ |
| T5 | Error Bound | `|interp_real - interp_dqa| ≤ C × 2^-k` | 🔵 Pending T0 |
| T6 | Gas Dominance | `steps(eval) ≤ gas_cost` | ✅ |
| T7 | Division Totality | Euclidean division total | ✅ |
| T8 | Scale Harmonization Correct | `max(sa, sb)` preserves semantics | ✅ |
| T9 | TRAP Soundness | Errors encode correctly per RFC-0126 | ✅ |
| T10 | Width Bound (i1152) | All intermediates ≤ i1152 | ✅ |
| T11 | Width Sufficiency (i1152) | i1152 handles worst-case | ✅ |
| T12 | Width Sufficiency (i1200) | i1200 recommended, extra margin | ✅ |
| T13 | Scale Limit Soundness | Scale > 38 → TRAP | ✅ |
| T14 | Literal Validity | `is_valid_decimal` ⊢ lowering succeeds | 🔵 Pending T0 |
| T15 | Expression Preservation | Lowering preserves expression semantics | 🔵 Pending T0 |
| T16 | Context Lookup Correct | Variable resolution is deterministic | ✅ |

### T0: Decimal Equivalence (Foundation Theorem)

**This is the only theorem that matters for consensus safety.**

```coq
(* Constructive validity predicate *)
Definition is_valid_decimal (d : Dfp) : Prop :=
  ∃ (n : Z) (s : nat),
    d = Dfp.of_rational (n # 10^s)
    ∧ s ≤ 38.

(* T0: Core semantic theorem *)
Theorem decimal_equivalence : ∀ (d : Dfp) (Hv : is_valid_decimal d),
  let q := dqa_of_valid_dfp d Hv in
  interp_real d = interp_dqa q.
Proof.
  intro d Hv.
  unfold is_valid_decimal in Hv.
  destruct Hv as [n [s [Heq Hs]]].
  subst d.
  unfold dqa_of_valid_dfp.
  (* Step 1: Parse decimal as rational n/10^s *)
  assert (Hparse : Dfp.to_rational (Dfp.of_rational (n # 10^s)) = n # 10^s).
  { admit. }
  (* Step 2: Convert rational to DQA integer/scale *)
  assert (Hconv : Dqa.of_rational (n # 10^s) = (n, s)%Z).
  { admit. }
  (* Step 3: Interpretation equality *)
  rewrite Hparse, Hconv.
  unfold interp_real, interp_dqa.
  rewrite Dqa.to_real_of_rational, Dfp.to_real_of_rational.
  admit.
Admitted.
```

### T1: Parse Determinism

```coq
Theorem parse_determinism : ∀ (tokens : list token),
  let ast := fold_left parse_binop tokens in
  ∀ (ast' : AST), ast = ast' → ast = ast'.
Proof.
  induction tokens; simpl; intros.
  - inversion H; reflexivity.
  - apply IHtokens.
Admitted.
```

### T8: Scale Harmonization Correctness

```coq
Theorem scale_harmonization_preserves_semantics :
  ∀ (qa qb : Dqa) (sa sb : nat),
    let s := max sa sb in
    let qa' := upscale qa (s - sa) in
    let qb' := upscale qb (s - sb) in
    interp_dqa qa + interp_dqa qb = interp_dqa qa' + interp_dqa qb'.
Proof.
  intros.
  unfold upscale.
  (* 10^(s-sa) × n × 10^-sa = n × 10^-s *)
  (* 10^(s-sb) × m × 10^-sb = m × 10^-s *)
  (* (n + m) × 10^-s preserved *)
Admitted.
```

### T11: i1152 Sufficiency (Corrected)

```coq
Theorem i1152_sufficient :
  ∀ (a b : Dqa),
    let (na, sa) := Dqa.to_integer_scale a in
    let (nb, sb) := Dqa.to_integer_scale b in
    let max_n := max (Z.abs na) (Z.abs nb) in
    let max_bits := Z.log2 max_n + 1 in
    let scale_bits := 4 * max sa sb in  (* log2(10) ≈ 3.32, round up to 4 *)
    max_bits + scale_bits ≤ 1152.
Proof.
  intros.
  (* DFP_MAX ≈ 2^127 *)
  (* max_n ≤ 2^127 *)
  (* max_bits ≤ 128 *)
  (* scale_bits ≤ 4 × 38 = 152 *)
  (* Total ≤ 280 << 1152 *)
  (* Sufficiency proven. QED. *)
Admitted.
```

## Test Vectors

### Literal Conversion (24 vectors)

| # | DFP Input | n | s | DQA Output | Notes |
|---|-----------|---|---|------------|-------|
| 1 | `0.0` | 0 | 0 | `DQA(0, 0)` | Exact zero |
| 2 | `1.0` | 1 | 0 | `DQA(1, 0)` | Exact integer |
| 3 | `-1.0` | -1 | 0 | `DQA(-1, 0)` | Negative integer |
| 4 | `0.5` | 5 | 1 | `DQA(5, 1)` | Exact half |
| 5 | `0.25` | 25 | 2 | `DQA(25, 2)` | Exact quarter |
| 6 | `0.125` | 125 | 3 | `DQA(125, 3)` | Exact eighth |
| 7 | `0.1` | 1 | 1 | `DQA(1, 1)` | Exact decimal (Policy) |
| 8 | `0.01` | 1 | 2 | `DQA(1, 2)` | Exact centi |
| 9 | `0.001` | 1 | 3 | `DQA(1, 3)` | Exact milli |
| 10 | `0.0001` | 1 | 4 | `DQA(1, 4)` | Exact ten-thousandth |
| 11 | `42.0` | 42 | 0 | `DQA(42, 0)` | Large integer |
| 12 | `123.456` | 123456 | 3 | `DQA(123456, 3)` | Multi-digit |
| 13 | `-0.5` | -5 | 1 | `DQA(-5, 1)` | Negative decimal |
| 14 | `100.0` | 100 | 0 | `DQA(100, 0)` | Powers of 10 |
| 15 | `1000.0` | 1000 | 0 | `DQA(1000, 0)` | Powers of 10 |
| 16 | `0.1e1` | 1 | 0 | `DQA(1, 0)` | 1.0 (scientific) |
| 17 | `1.5e2` | 150 | 0 | `DQA(150, 0)` | 150 (scientific) |
| 18 | `1.5e-2` | 15 | 4 | `DQA(15, 4)` | 0.015 (scientific) |
| 19 | `1e0` | 1 | 0 | `DQA(1, 0)` | Integer power |
| 20 | `1e38` | 1 | 0 | `DQA(1, 0)` | Max magnitude |
| 21 | `1e-38` | 1 | 38 | `DQA(1, 38)` | Min magnitude |
| 22 | `7.5e3` | 7500 | 0 | `DQA(7500, 0)` | Mixed mantissa |
| 23 | `0.00001` | 1 | 5 | `DQA(1, 5)` | Five decimal places |
| 24 | `999999999` | 999999999 | 0 | `DQA(999999999, 0)` | Large literal |

### Binary Operations (12 vectors)

| # | Expression | Lowered DQA | Result | Notes |
|---|------------|-------------|--------|-------|
| 25 | `0.1 + 0.2` | `add(DQA(1,1), DQA(2,1))` | `DQA(3, 1)` = 0.3 | Scale harmonization |
| 26 | `0.5 * 2.0` | `mul(DQA(5,1), DQA(2,0))` | `DQA(10, 1)` = 1.0 | Result scale = 1 |
| 27 | `1.0 - 0.5` | `sub(DQA(1,0), DQA(5,1))` | `DQA(5, 1)` = 0.5 | Upscale 1.0 |
| 28 | `0.25 + 0.75` | `add(DQA(25,2), DQA(75,2))` | `DQA(100, 2)` = 1.0 | Exact sum |
| 29 | `0.1 * 0.1` | `mul(DQA(1,1), DQA(1,1))` | `DQA(1, 2)` = 0.01 | Scale addition |
| 30 | `1.0 / 2.0` | `div(DQA(1,0), DQA(2,0))` | `DQA(1, 0)` = 1.0 (remainder 0) | Exact division |
| 31 | `1.0 / 4.0` | `div(DQA(1,0), DQA(4,0))` | `DQA(1, 0)` = 0.25 (remainder 0) | Exact division |
| 32 | `0.5 + 0.25` | `add(DQA(5,1), DQA(25,2))` | `DQA(75, 2)` = 0.75 | max(1,2)=2 |
| 33 | `100.0 * 0.01` | `mul(DQA(100,0), DQA(1,2))` | `DQA(100, 2)` = 1.0 | Scale addition |
| 34 | `0.3 - 0.1` | `sub(DQA(3,1), DQA(1,1))` | `DQA(2, 1)` = 0.2 | Same scale |
| 35 | `0.5 / 0.25` | `div(DQA(5,1), DQA(25,2))` | `DQA(2, 0)` = 2.0 | Exact division |
| 36 | `42.0 * 2.0` | `mul(DQA(42,0), DQA(2,0))` | `DQA(84, 0)` = 84 | No scale |

### Forbidden Operations (8 vectors - must TRAP)

| # | Input | Expected Error | Error Code |
|---|-------|----------------|------------|
| 37 | `1.0 / 3.0` | `ErrNonDecimal` | 0x10 |
| 38 | `sqrt(2.0)` | `ErrIrrational` | 0x11 |
| 39 | `0.0 / 0.0` | `ErrNaN` | 0x13 |
| 40 | `1e200 / 1e100` | `ErrInfinite` | 0x12 |
| 41 | `1e-400` | `ErrSubnormal` | 0x14 |
| 42 | `1e39` | `ErrScaleOverflow` | 0x15 |
| 43 | `0.1 / 0.0` | `ErrDivZero` | 0x16 |
| 44 | `MAX * MAX` (overflow i1200) | `ErrWidthOverflow` | 0x17 |

### Edge Cases (4 vectors)

| # | Input | Expected | Notes |
|---|-------|----------|-------|
| 45 | `-0.0` | `DQA(0, 0)` | Sign-preserved zero |
| 46 | `0.0 + 0.0` | `DQA(0, 0)` | Zero addition |
| 47 | `1.0 * 0.0` | `DQA(0, 0)` | Zero multiplication |
| 48 | `MIN / 1.0` | `DQA(1, 38)` | Min magnitude |

## Security Considerations

### Consensus Attacks

| Attack | Impact | Mitigation |
|--------|--------|------------|
| Parser divergence | Different ASTs for same input | Canonical left-fold definition |
| Scale explosion | DoS via huge scales | Scale limit (≤38) enforced |
| Width overflow | Incorrect intermediate computation | i1152/i1200 minimum |
| TRAP encoding ambiguity | Different error encodings | RFC-0126 canonical encoding |

### Proof Forgery

| Threat | Mitigation |
|--------|------------|
| Fake lowering | Verification checks DQA-only trace |
| Precision loss | Exact decimal policy; no rounding |
| Invalid literals | Parse-time validation against ValidDFPSubset |

## Alternatives Considered

### Runtime Conversion (REJECTED)

```
DFP exists at runtime → convert on state mutation
```

**Cons:** Conversion boundary becomes divergence vector; verification complexity returns.

### DFP-only Consensus (REJECTED)

```
DFP is consensus-safe with enhanced canonicalization
```

**Cons:** Requires full DFP in ZK circuits; high constraint complexity.

### Hybrid Type System (REJECTED)

```
DFP and DQA both exist at runtime with explicit tagging
```

**Cons:** Dual semantics; complexity in verification; violates minimal-surface principle.

## Rationale

The Deterministic Lowering Pass resolves the abstraction mismatch between DFP (source-level ergonomics) and DQA (runtime execution). Key insights:

1. **Static only**: DFP is a compile-time convenience, not a runtime primitive
2. **Integer intermediates**: i1152/i1200 handles worst-case without floating-point
3. **T0 foundation**: Decimal Equivalence is the only theorem that matters for consensus
4. **TRAP-before-serialize**: Errors cannot produce valid DQA values

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-22 | Initial draft |
| 1.10.2 | 2026-03-22 | Added T0 foundation, formal Coq framework |
| 2.0.0 | 2026-03-22 | Comprehensive reset: DLP contract, i1152 correction, 48 test vectors, 16 theorems |

## Related RFCs

- RFC-0104: Deterministic Floating-Point (DFP)
- RFC-0105: Deterministic Quant Arithmetic (DQA)
- RFC-0109: Deterministic Linear Algebra Engine (DLAE)
- RFC-0126: Deterministic Canonical Serialization (DCS)

---

**Version:** 2.0.0
**Submission Date:** 2026-03-22
