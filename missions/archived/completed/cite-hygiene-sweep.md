# cite-hygiene-sweep — strip RFC version pins + line refs from doc-comments

**Status:** claimed (2026-08-27)
**Substrate:** `cipherocto-design-principles` §No line refs anywhere + CLAUDE.md §RFC Reference Conventions
**Parent:** R3 review follow-on (cite hygiene finding — mechanical sed sweep)
**Depends on:** (none — pure mechanical edit)

## Motivation

R3 review surfaced a cite-hygiene finding: doc-comments across the workspace carry RFC version pins (`RFC-0960 v3.7`, `RFC-0960 (Accepted v3.7)`, `RFC-0960-v37`) and line refs (`L474-516`, `L130`, `L748-752`) that violate the §RFC Reference Conventions rule (RFC number only in prose) and the §No line refs anywhere rule (§section_name / symbol names, no file:line).

The canonical rules:
- **CLAUDE.md §RFC Reference Conventions:** "When referencing RFCs in prose, cross-references, changelogs, and approval criteria — use only the number. Never include status, version pins, or metadata."
- **memory/no-line-refs-anywhere:** "§section_name / symbol names. Code exempt."

Current scope of the violation (verified via grep at mission creation):

- 588 RFC version pins across `crates/` — all form `RFC-XXXX vN.M` (zero `(Accepted vN.M)` / `(Draft vN.M)` / `RFC-XXXX-vNM` matches; the additional regex branches are defensive in case patterns appear later)
- 77 line refs total across `crates/` (`§X.Y L<num>` union `L<num>-<num>` regex) — split as 53 `§X.Y L<num>` + 57 bare `L<num>-<num>`, with 33 lines carrying BOTH patterns (53 + 57 − 33 = 77 unique). Both patterns MUST be stripped; both land at the §-section citation.

**R2 substrate-fidelity catch:** earlier draft claimed 486 pins + 97 line refs (off by 102 pins and arithmetic error on the union). Re-count at mission creation is mandatory; numbers updated here.

The mission is mechanical sed-grade cleanup. No semantic changes.

## Scope

Strip RFC version pins + line refs from all doc-comments (`///` and `//!`) and inline `//` comments in `crates/`. Touch ONLY comment text — preserve all code, attributes, and string literals (which may legitimately carry version strings as data).

### Sub-steps

1. **RFC version pin patterns** — strip these patterns from comment text:
   - `RFC-XXXX vN.M` → `RFC-XXXX` (where N, M are digits)
   - `RFC-XXXX (Accepted vN.M)` → `RFC-XXXX`
   - `RFC-XXXX (Draft vN.M)` → `RFC-XXXX`
   - `RFC-XXXX-vNM` → `RFC-XXXX`
   - Exception: legitimate version pins in CODE (e.g., `"cipherocto/burn/v1/"` domain string — that is data, not a citation) MUST be preserved.

2. **Line ref patterns** — strip these patterns from comment text:
   - `§X.Y L<num>-<num>` → `§X.Y`
   - `§X.Y L<num>` → `§X.Y`
   - `L<num>-<num>` (when standalone, not a §-prefixed ref) → remove
   - Exception: legitimate code references like `Vec::with_capacity(N)` are NOT line refs and MUST be preserved.

3. **Mechanical sed sweep** — script:
   ```bash
   # Pre-flight: count occurrences
   grep -rhE 'RFC-[0-9]{4}\s+v[0-9]+\.[0-9]+|RFC-[0-9]{4}\s*\([Aa]ccepted v[0-9]+\.[0-9]+|RFC-[0-9]{4}\s*\([Dd]raft v[0-9]+\.[0-9]+|RFC-[0-9]{4}-v[0-9]+\.[0-9]+' crates/ | wc -l
   # expect: 588 (current, all `RFC-XXXX vN.M` form); 0 after sweep

   grep -rhE '§[0-9]+\.[0-9]+ L[0-9]+|L[0-9]+-[0-9]+' crates/ | wc -l
   # expect: 77 (union regex; 53 §X.Y L<num> + 57 bare L<num>-<num> − 33 overlap = 77); 0 after sweep

   # Sweep (sed in-place over *.rs files; restrict to comment lines)
   # Note: requires comment-line-aware sed (or per-file manual review)
   ```

4. **Manual review pass** — after sed sweep, `git diff` MUST show ONLY comment-text changes. Any change to code / strings / attributes is a sed-regex overreach and MUST be reverted. Split such occurrences into a follow-up. **Self-exemption clause:** the illustrative examples carried by THIS mission body (line 10 + lines 16-19 + lines 36-37 + lines 41-42 — `RFC-0960 v3.7`, `L474-516`, `Vec::with_capacity(N)` exception, `L<num>-<num>` pattern templates, etc.) are exempt from sub-step 1/2 patterns. The sweep targets `crates/` only (per Out-of-Scope). The exemption is documented here so the sweep operator does not waste a review pass on the mission's own examples.

5. **Validate cite script** — `bash scripts/validate_cites.sh` (if exists) returns 0 INVALID + 0 STALE + 0 PHANTOM after sweep. If the script does not exist, create it (per `rfc-referencing-convention` memory card pattern).

## Out of Scope

- Touching `docs/` markdown — separate sweep mission (different file class, different sed rules)
- Touching `missions/` YAML — cite hygiene there tracked by `no-phantom-mission-pointers` discipline
- Touching `rfcs/` — RFCs legitimately carry their own VH version metadata
- Touching `Cargo.toml` version strings — those are semver, not citation pins
- Touching string literals — `"cipherocto/burn/v1/"` is data, not citation

## Test Vectors

- TV-CH-1: `grep -rhE 'RFC-[0-9]{4}\s+v[0-9]+\.[0-9]+' crates/` returns 0 matches after sweep
- TV-CH-2: `grep -rhE 'RFC-[0-9]{4}\s*\([Aa]ccepted|RFC-[0-9]{4}\s*\([Dd]raft' crates/` returns 0 matches
- TV-CH-3: `grep -rhE 'RFC-[0-9]{4}-v[0-9]+\.[0-9]+' crates/` returns 0 matches
- TV-CH-4: `grep -rhE '§[0-9]+\.[0-9]+ L[0-9]+|L[0-9]+-[0-9]+' crates/` returns 0 matches in doc-comments (line refs in code MAY remain)
- TV-CH-5: `git diff --stat` shows 0 changes to `.rs` code lines; only `///` / `//!` / `//` comment text changed
- TV-CH-6: `cargo clippy --workspace --all-targets --features full -- -D warnings` zero warnings (no semantic change)
- TV-CH-7: `cargo test --workspace --lib` green (no semantic change)
- TV-CH-8: `cargo doc --no-deps --workspace` no broken links (citations unchanged at the §-section level)

## Layer direction (per `cipherocto-design-principles`)

- All crates — comment-text edits only
- No semantic change → no layer-direction impact
- All cross-references (e.g., `RFC-0105 §5 (overflow handling)` → `RFC-0105 §5`) preserved at the §-section level — the section anchor survives even when version pins above it are stripped

## Validation

```bash
# Pre-merge
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features full -- -D warnings
cargo test --workspace --lib

# Cite hygiene gates
grep -rhE 'RFC-[0-9]{4}\s+v[0-9]+\.[0-9]+|RFC-[0-9]{4}\s*\([Aa]ccepted|RFC-[0-9]{4}\s*\([Dd]raft|RFC-[0-9]{4}-v[0-9]+\.[0-9]+' crates/ | wc -l
# expect: 0

grep -rhE '§[0-9]+\.[0-9]+ L[0-9]+|L[0-9]+-[0-9]+' crates/ | wc -l
# expect: 0 (in doc-comments)
```

## Backward compat

- ZERO. Pure mechanical sweep of comment text. No code, no signatures, no wire form, no API surface affected.

## Risk

- HIGH: regex overreach can corrupt code — a single bad sed hunk in a mechanical sweep can break `cargo build --workspace` workspace-wide. Mitigation: comment-line-aware sed pattern (`^\s*///` / `^\s*//!` / `^\s*//\s*[A-Za-z]`); mandatory `cargo build --workspace --features full` gate after each commit; branch protection requires green build.
- MEDIUM: legitimate version pins in string literals (`"cipherocto/burn/v1/"`) get corrupted if sed is not string-aware. Mitigation: AST-aware sed via `syn::parse_str` to walk the Rust AST before sed, OR manual review of every hunk touching a string literal.
- LOW: legitimate L<num> references in code (rare but possible) get corrupted. Mitigation: skip code lines via AST-aware pass.

## Cross-references

- `cipherocto-design-principles` §No line refs anywhere
- CLAUDE.md §RFC Reference Conventions Reaffirmed
- `rfc-referencing-convention` memory card
- `no-line-refs-anywhere` memory card
- R3 review cite-hygiene finding

## Claimant

@mmacedoeu

## Pull Request

#