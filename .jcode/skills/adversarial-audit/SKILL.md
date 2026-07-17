---
name: adversarial-audit
description: Cross-reference an adversarial code review document against actual source code to determine which findings are fixed vs still open. Use when the user asks to "audit", "verify review", "check findings", or "cross-reference review against code".
keywords: audit, adversarial, review, code review, cross-reference, findings, verify, check
---

# Adversarial Code Audit

Cross-references an adversarial review document against the actual source code to produce a structured status report of each finding.

## Quick Usage

```
adversarial-audit <path-to-review-doc>
```

## Full Procedure

### Step 1: Read the review document

Read the entire review document. Extract:
- Each finding ID (e.g. M-NEW-1, L-NEW-3, M2, L8)
- Severity (MEDIUM, LOW, CRITICAL)
- File paths referenced
- Description of the issue
- Any previous status (e.g. "from R1", "unfixed")

### Step 2: Read all referenced source files

In parallel, read every source file referenced in the review. Use the Read tool with the full paths from the review document. This gives you the current state of the code.

### Step 3: Cross-reference each finding

For each finding in the review:
1. Locate the specific code location mentioned
2. Check if the issue still exists in the current code
3. Determine status: **FIXED**, **STILL OPEN**, or **CHANGED** (partially fixed or fix introduced new issue)
4. Record evidence: the specific line(s) and what they show

### Step 4: Compile structured report

Output a table with columns:

| Finding | Severity | Status | Evidence |
|---------|----------|--------|----------|
| M-NEW-1 | MEDIUM | STILL OPEN | `main.rs:43` — `tracing::error!("{:#}", e)` uses Display only, drops inner() |
| L-NEW-1 | LOW | FIXED | `logging.rs:30` — uses `lower == k` not `lower.contains(k)` |

Include:
- Summary counts: X fixed, Y still open, Z changed
- Any patterns (e.g. "all credential-related findings still open")
- Recommendations for which findings to prioritize

### Step 5: Report to user

Present the full status table and ask if they want to:
- Fix the remaining open findings
- Generate an updated review document (R3)
- Focus on specific findings

## Review Document Format

Adversarial reviews in this project follow this pattern:
- Located in `docs/reviews/` (scratchpad, never committed)
- Named like `octo-<component>-impl-adversarial-review-r<N>.md`
- Contain findings prefixed with `M-` (MEDIUM) or `L-` (LOW) severity
- Numbered like `M-NEW-1`, `L-NEW-2` (new in this round) or `M2`, `L8` (carried from earlier)

## Project Rules

From MEMORY.md:
- `docs/reviews/` are scratchpads — **NEVER committed to git**
- Findings use severity prefixes: `M-` = MEDIUM, `L-` = LOW, `CRIT-` = CRITICAL

## Tips

- Read all source files in parallel (multiple Read calls in one response) for speed
- When a finding references multiple locations, check all of them
- If the code changed since the review, note what changed even if the finding is "fixed"
- For `#[allow(dead_code)]` findings, check if the function is actually called anywhere
- For "missing" functionality, verify by searching the codebase (Grep), not just reading the file
