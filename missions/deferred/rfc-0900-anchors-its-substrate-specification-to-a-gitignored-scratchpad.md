---
name: rfc-0900-anchors-its-substrate-specification-to-a-gitignored-scratchpad
description: "RFC-0900 section Slash Ledger Substrate specifies the chain-aware slash ledger substrate, and twice anchors it to section 20.3 Model B. RFC-0900 has no numbered sections at all, so that anchor cannot resolve inside the document it appears in. Section 20.3 belongs to a storage-layer restructuring review under docs/reviews, which .gitignore excludes in its entirety and which BLUEPRINT mandates be ephemeral and uncommitted. The anchor is therefore unresolvable for anyone but a reader who happens to have that scratchpad on their machine: no clone, no CI run and no reviewer will ever see it. The substance is correct, since the review really does define PRIMARY KEY of chain_id, owner_did and asset_id with chain_id NOT NULL, which is the pattern the RFC claims to mirror. That same pattern is what the landed v015 migration deviates from, by delivering a nullable chain_id inside a unique index rather than a primary key, so the RFC cites an unciteable pattern and then does not follow it. The anchor also propagated into the migration SQL header, where no linter can reach it."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
---

# Mission `rfc-0900-anchors-its-substrate-specification-to-a-gitignored-scratchpad`

## Status

Deferred. Re-anchoring a normative section is an RFC edit, and whether the
slash ledger should carry a real primary key is a schema decision. Nothing was
actioned and no file was moved.

## Finding

RFC-0900 §Slash Ledger Substrate is the document's most carefully specified
section, and it is anchored twice to a target that no reader of the repository
can reach:

> RFC-0900 v2.0 … `slash_ledger` PK promoted from `(row_id, provider_id
UNIQUE)` to `(chain_id, provider_id)` **per §20.3 Model B** (parallel to vault
> v013 PK).

> **Substrate status:** … specifies the storage + in-memory substrate shape that
> backs §Slashing Model. **Per §20.3 Model B**, slash ledger mirrors the vault
> substrate's chain-aware PK lattice.

Three things are wrong with that anchor, and each alone would be tolerable.

**RFC-0900 has no numbered sections.** Every heading in the document is named —
`§State Machines`, `§Observability`, `§Open Questions`. A grep for a numbered
heading returns nothing. So `§20.3` cannot resolve inside the document that
cites it, and nothing in the surrounding text names a different document.

**§20.3 belongs to a review scratchpad.** It is
`docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`, and its
heading is `## 20.3 Final vault model — Model B with chain_id in PK`.

**That directory is gitignored.** `.gitignore` excludes `docs/reviews/`
outright; `git ls-files docs/reviews/` returns nothing; the file is present in
this working tree and untracked. BLUEPRINT is explicit about why:

> **Review files go in `docs/reviews/`** — ephemeral scratchpads, NOT committed
> to git

So the normative anchor for an Accepted RFC's substrate specification is a file
that project policy says must not exist in the repository. I resolved it only
because the scratchpad happens to sit on this machine. A fresh clone, a CI run
and any reviewer see an RFC citing a section that appears nowhere.

The citation gives no filename, so even a reader who suspects the target is
elsewhere has nothing to search for.

## The substance is right, which is what makes it worth fixing

The review document really does define what the RFC says it mirrors:

```sql
chain_id  BLOB(32) NOT NULL,
owner_did TEXT NOT NULL,
asset_id  BLOB(32) NOT NULL,
...
PRIMARY KEY (chain_id, owner_did, asset_id)
```

That is the `(chain_id, owner_did, asset_id)` lattice RFC-0900 names, and it is
the pattern the slash ledger was promoted to mirror. **The RFC is right and
unciteable**, which is a documentation problem rather than a specification
problem — the cheapest kind to fix and the easiest to leave standing.

## And the implementation follows the pattern less closely than the RFC does

The pattern the RFC anchors to declares `chain_id NOT NULL` inside a real
`PRIMARY KEY`. The landed migration delivers something weaker:

```sql
ALTER TABLE slash_ledger ADD COLUMN chain_id BLOB;
UPDATE slash_ledger SET chain_id = CAST('0000...0000' AS BLOB) WHERE chain_id IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS slash_ledger_chain_provider_idx
    ON slash_ledger (chain_id, provider_id);
```

`chain_id` is nullable, and the uniqueness is a UNIQUE index rather than a
primary key. In a SQLite-derived engine NULLs compare distinct inside a unique
index, so two rows with a NULL `chain_id` and the same `provider_id` would both
be admitted — which is the cross-chain collapse the whole migration exists to
prevent.

No code path reaches it: every `SlashLedgerRow` binds a `[u8; 32]`, and the
insert statement binds it explicitly. So this is a schema weaker than both the
RFC's stated invariant and the pattern the RFC points at, not a live defect.

It is worth noting the direction of the gap. The RFC is _more_ careful than its
implementation here, and both are more careful than the target it cites is
reachable.

## The anchor propagated into code

`v015__chain_aware_slash_ledger.sql` carries it in three places:

```sql
-- diverges from §20.3 Model B chain-aware vault substrate (PK =
-- NOTE on §20.3 + RFC-0960 §Vault Substrate (CROSS-REF 0900-d):
--    asset_id`) per §20.3 Model B lattice.
```

So a SQL file in the crate, on a migration that has already run, points at a
review scratchpad. No Markdown linter reaches it, and the section-cite linter
does not read `.sql` in any case.

## What the owner should do

- **Re-anchor to something in the repository.** Either promote the vault PK
  pattern into an RFC that can be cited, or state the pattern inline in
  RFC-0900 with the vault table quoted — which is what the RFC already does for
  the `chain_id` type and the amount columns.
- **Decide whether the slash ledger gets a real primary key.** If the
  `(chain_id, provider_id)` lattice is normative, `NOT NULL` and a PK
  constraint are how it is expressed. If the nullable-column form is
  deliberate, say so and drop the "mirrors Model B" framing, which currently
  promises more than the schema delivers.
- **Sweep the same anchor pattern.** A gitignored review file is a plausible
  source for citations in any RFC written from one, and nothing in the
  toolchain can see it.

## Reproduction

```bash
# RFC-0900 has no numbered sections at all
grep -nE '^#+ [0-9]+\.' rfcs/accepted/economics/0900-ai-quota-marketplace.md \
  || echo "NONE — every heading is named"

# the anchor, twice in the RFC
grep -n '20\.3' rfcs/accepted/economics/0900-ai-quota-marketplace.md

# and three more times in the migration
grep -n '20\.3' crates/quota-router-storage/migrations/v015__chain_aware_slash_ledger.sql

# the target is a review scratchpad...
grep -rnE '^#+ *20\.3' --include='*.md' docs/reviews/

# ...in a directory the project excludes from version control entirely
grep -n '^docs/reviews/' .gitignore          # line 43, the rule
git check-ignore -v docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md
git ls-files docs/reviews/ | wc -l           # 0 tracked
grep -n 'Review files go in' docs/BLUEPRINT.md

# and the pattern it defines is NOT NULL inside a real primary key
grep -n 'PRIMARY KEY (chain_id, owner_did, asset_id)' \
  docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md

# against what v015 actually delivers
grep -n 'ADD COLUMN chain_id\|CREATE UNIQUE INDEX' \
  crates/quota-router-storage/migrations/v015__chain_aware_slash_ledger.sql
```

## Cross-references

- Mission `rfc-0900-specifies-a-listing-lifecycle-that-does-not-exist-anywhere`
  — the same document; its §Slash Ledger Substrate praise is qualified by this
  finding
- Mission `the-section-cite-linter-checks-token-shape-and-never-resolution` —
  the class this extends. Five instances of unresolvable citations were found
  across the RFC-0959 documents; this is a sixth, and the only one whose target
  actually exists
- Mission `rfc-0959-v27-checks-its-own-substrate-claims-and-the-other-three-do-not`
  — v27's discipline includes marking what is pending substrate confirmation,
  which is the practice that would have surfaced this
- Mission `documentation-that-describes-a-mechanism-nobody-built` — the wider
  class, where a document describes something the repository cannot show
