# Design Principles

The engineering principles this codebase is built on. Code comments cite these
as `§<section name>` — for example "per §No parallel abstractions".

These were previously only reachable through a private working-notes file, which
meant 137 doc comments across the tree cited something no reader of the
repository could open. They live here now so that a citation resolves.

A principle earns its place by being **falsifiable**: it names a decision a
reviewer can disagree with. Where a principle has an exception, the exception is
written down next to it rather than left to folklore.

## The thirteen

### Separation of concerns

Each module owns **one** thing. Do not conflate parsing, verification, decision,
dispatch and policy in the same place — the bugs this workspace has actually
shipped come from exactly those boundaries being blurred.

_Audible as:_ a module whose name contains "and", or a function that both decides
and performs.

### Stable Abstractions Principle

Abstractions depend on stable things, not the reverse. Primitives are stable;
business logic churns. When a choice must be made, put it where the churn is not.

_Audible as:_ a new enum variant or option appearing in Layer A code.

### No premature coupling

A caller should not reach into a callee's dependencies — its storage, its
business state. Go through a protocol boundary. A type that exposes a callee's
internals is a type that has to change when the callee does.

### Open/Closed

Open to extension, closed to modification. New variants must land without a
central edit. If adding a case requires touching a `match` in another crate, the
abstraction is in the wrong place.

### Dependency Inversion

Depend on abstractions (traits). The side that owns the data owns the
implementation. A trait defined next to its only implementation is not yet an
abstraction; it is a deferred rename.

### Interface Segregation

Small focused traits beat god-traits. A trait with fifteen methods, of which each
caller needs two, is fifteen contracts where two were needed.

### No god-objects

Split multi-concern structs early, before they reach roughly three thousand
lines. The threshold is a smell detector, not a quota.

### Composition over inheritance

Prefer `Vec<TypedVariant>` with logical-AND semantics over subclassing.
Inheritance is upgrade-hostile: every new shape becomes a base-class change plus
a review of every subclass. Composition lets new variants land without touching
the core type.

### Push complexity to the edges

Keep the core simple and let boundaries absorb environment messiness. Parsing,
transport quirks and storage dialects belong at the edge, not in the middle of
the domain logic.

### Discipline at first call site pays off

Do not bypass an abstraction "just once". The second bypass becomes a pattern and
the third makes the abstraction dead code — at which point it is being paid for
and never used.

### No parallel abstractions

Use the existing primitive — the general transport, the general codec, the
general identity. Do not invent a second one. Parallel abstractions duplicate
adapter code and end up with two health-check systems, one of which nobody
watches.

### Storage is not a protocol

Direct storage coupling forces business rules to be replicated at every read
site. Go through a typed query or response boundary.

### Attenuation invariants cross boundaries

A type-level invariant asserted inside one module is still the caller's
obligation. If module B consumes module A's output, B is responsible for the
invariant A advertised. A proof that stops at the module boundary is a comment.

## Layer stability

Match crate stability to design depth.

| Layer | Scope                                                    | Stability                     | Evolves when              |
| ----- | -------------------------------------------------------- | ----------------------------- | ------------------------- |
| **A** | Crypto primitives, canonical encoding, semantic policies | RFC-frozen, semver-major only | PQC migration (years)     |
| **B** | Identity substrate, transport, cable, wallet-core        | RFC-driven, additive only     | A new RFC adds a feature  |
| **C** | Specialized nodes, one per node role                     | Per-RFC                       | New node type = new RFC   |
| **D** | Transport adapters (BLE, USB, TCP, QUIC, HID, …)         | Per-adapter                   | New adapter = new crate   |
| **E** | User extensions and capability variants                  | Per-extension                 | New extension = new crate |

**Direction is A → B → C → D/E, never the reverse.** The audit question for any
new crate or dependency is: which layer, and does the dependency respect the
direction?

Layer B depends on A because A is stable substrate. Layer D depends on B because
B owns the transport trait. Layer E registers into B and does not depend on it.

Crypto and identity survive ten-year migrations; business logic churns monthly
without breaking crypto. Isolating the blast radius of a curve transition to
Layer A is the whole point of the model.

## Extension over enumeration

For types with an unbounded extension surface, use a **typed discriminator plus a
raw escape hatch** — not a central enum. The discriminator is a 128-bit UUID
allocated from an RFC namespace with a user-extension range.

Enums are upgrade-hostile: every new type becomes a central edit plus a
cross-crate review, which is precisely the coupling §Open/Closed forbids. Old
code fails closed on an unknown discriminator, which is the safe default — a
refusal, not a wrong answer.

## Substrate truth

Multi-concern modules are split early (§No god-objects). The substrate owns the
concrete implementation; the abstraction is what callers depend on. When a
sealed trait exists, only the substrate crate implements it, so no downstream
crate can invent a parallel path around it.

## Reference conventions

- **Section refs, not line refs.** Prose refers to other documents by
  `§<section name>` or by symbol anchor, never `file.rs:123`. Line numbers rot
  on the next edit and cannot be clicked.
- **RFCs are cited by number alone.** No status, no version pin, no metadata in
  prose, cross-references or changelogs. `RFC-0909`, never
  `RFC-0903 (Accepted v63)`. A reference that carries version state is a
  reference that will be stale in the same commit that adds the note.
- **Diagrams are Mermaid, never ASCII art.** They render in GitHub and VS Code,
  they diff, and they are consistent across documents.

## See also

- [`docs/BLUEPRINT.md`](../BLUEPRINT.md) — the RFC and mission process these
  principles are applied through.
- [`docs/architecture/stoolap-integration.md`](./stoolap-integration.md) — the
  boundary rules for the embedded Stoolap fork, cited as
  `[[stoolap-general-purpose-db]]` throughout the tree.
- [`docs/05-process/engineering-rules.md`](../05-process/engineering-rules.md) —
  workflow rules, cited by name throughout the tree.
