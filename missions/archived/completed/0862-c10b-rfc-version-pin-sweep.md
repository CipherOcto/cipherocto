# Mission: RFC Version-Pin Prose Sweep (Round 3 Adversarial Review Defect 6)

## Status

**LANDED 2026-08-19 (Round-3 R1 fix).** Corrected sweep replaces
`RFC-XXXX vN.M` cross-RFC prose refs with bare `RFC-XXXX` per
CLAUDE.md §RFC Reference Conventions Reaffirmed (use only the
number in prose; only the RFC's own Version History + Status header
carry version info).

**Exemptions enforced by script:**
1. Lines inside `## Version History` / `## Revision History` sections
2. Markdown table rows (lines starting with `|`)
3. `**Revision:**` / `**Status:**` self-referencing lines
4. Self-refs (`RFC-XXXX vN.M` inside the file whose stem starts with
   `XXXX` — the pin is the file identity)

## RFC

Multi-RFC (no single RFC); governed by CLAUDE.md
§RFC Reference Conventions Reaffirmed.

## Summary

Re-land the Round-3 Defect 6 sweep with a corrected regex. Prior
sweep (`1a7a1c4b`) used `(RFC-\d{4,})\s+v(\d+)\.(\d+)` which left
`.X` patch digits dangling — `RFC-0862 v1.4.0` became
`RFC-0862.0`. Reverted (`726c52e8`) and re-applied with corrected
regex `(RFC-\d{4,})\s+v\d+\.\d+(?:\.\d+)?` consuming optional patch.

## Acceptance Criteria

- [x] Cross-RFC `RFC-XXXX vN.M` prose pins replaced with bare
      `RFC-XXXX` in 15 RFC files (54 sites, plus 2 phantom-variant
      fixes from same R1 round)
- [x] Version History tables preserved (4 remaining pins in 0958
      are all inside `| table |` rows)
- [x] Self-refs preserved (e.g., `RFC-0862 v1.4.0` title,
      `RFC-0862 v2.0.3 §SpendLedger` cross-refs inside 0965's
      Version History table)
- [x] Zero orphan `RFC-NNNN.X` patterns remain
- [x] No phantom DQA enum variants (`MantissaOverflow`,
      `ConversionLoss`) in RFC-0105 (R1 CRITICAL fix landed in
      same commit)

## Files + sites

| File | Sites |
|---|---|
| `proof-systems/0958-zk-capability-subclass.md` | 17 |
| `networking/0862-writer-election-bootstrap-v130.md` | 12 |
| `economics/0960-grand-design-vaults-capabilities-reservations.md` | 5 |
| `economics/0920-unified-python-sdk-dual-mode-compatibility.md` | 4 |
| `economics/0965-capability-extension-format.md` | 3 |
| `economics/0900-ai-quota-marketplace.md` | 2 |
| `networking/0871-specialized-node-protocol-envelope.md` | 2 |
| `networking/0970-forwarding-hop-auth-envelope.md` | 2 |
| `economics/0964-constraint-encoding-standard.md` | 2 |
| `economics/0959-ask-settlement-chain.md` | 1 |
| `economics/0961-ciphero-sql-language-spec.md` | 1 |
| `economics/0962-consensus-session-protocol.md` | 1 |
| `economics/0963-resource-shard-routing.md` | 1 |
| `economics/0967-policy-object-graph.md` | 1 |

Plus 2 phantom-variant fixes in `numeric/0105-deterministic-quant-arithmetic.md`.

## Verification

```bash
grep -rEn 'RFC-[0-9]{4,}\.[0-9]' rfcs/accepted/  # zero hits
grep -rEn 'MantissaOverflow|ConversionLoss' rfcs/accepted/  # zero hits
```

## Reference

- CLAUDE.md §RFC Reference Conventions Reaffirmed
- Round-3 R1 finding (regex bug; HIGH severity)
