---
name: 0948-b1-dispatch-hook-admin-rate-limiter-has-no-call-site-and-cites-an-rfc-section-that-does-not-exist
description: "crates/quota-router-core/src/admin_rate_limit.rs is a complete admin rate limiter with 10 passing unit tests and zero call sites outside its own module — nothing outside lib.rs names AdminRateLimiter, check_admin_rate_limit, or policy_for. The admin API therefore ships with no rate limiting. Its module header cites RFC-0933 §Admin endpoints, but RFC-0933 contains no occurrence of the word admin and has no such section. The owning mission 0948-b1 is closed with its dispatch-hook acceptance criterion unchecked and deferred to two follow-on missions that were never filed, and 0949-b defers its own auth-endpoint rate limiting to that closed mission."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0933
---

# Mission `0948-b1-dispatch-hook-admin-rate-limiter-has-no-call-site-and-cites-an-rfc-section-that-does-not-exist`

## Status

Open. Nothing has been changed on the admin rate-limiting path. The limiter
substrate is untouched and its 10 unit tests still pass.

## The finding

`crates/quota-router-core/src/admin_rate_limit.rs` is a finished piece of work:

- `AdminRateLimiter` with `with_default_policies`, `set_enabled`,
  `is_enabled`, `policy_for`, `check`, and `reset_all`
- `AdminRateLimitPolicySet` covering 11 admin routes, including
  `POST /auth/token/introspect`, `POST /auth/logout`, and the prompt CRUD group
- `AdminIdentity` with per-IP, per-user, and per-API-key-key resolution
- `check_admin_rate_limit` as the HTTP-agnostic entry point
- 10 in-module unit tests, all passing

**Nothing calls it.** A search across the workspace for `AdminRateLimiter`,
`check_admin_rate_limit`, and `policy_for` returns hits only inside
`admin_rate_limit.rs` itself and in the module's own documentation. The single
external reference in the whole repository is the declaration
`pub mod admin_rate_limit;` in `lib.rs`.

The admin API consequently serves every route with no rate limiting, no `429`,
and no `Retry-After` — on the same surface that mission 0870-b finds has no
authorization either. Both controls are absent from the shipped admin API, and
the missing rate limiter was built specifically for it.

## Two defects in the record that made this hard to see

### The only stated justification is a citation that does not resolve

The module header reads:

```rust
//! Admin endpoint rate limiting (RFC-0933 §Admin endpoints).
```

RFC-0933 contains no occurrence of the word _admin_, and none of its sections is
titled anything like _Admin endpoints_. Its sections are Rate Limit Check
Middleware, Rate Limit Headers, Scope Hierarchy, Persistence, Error Response,
and Configuration.

So the module that exists to rate limit admin endpoints cites, as its sole
authority, a section of an RFC that never discusses admin endpoints. The
implementation tracks RFC-0933's generic machinery and extends it to a surface
RFC-0933 does not cover — which is a reasonable thing to have built, and is not
what the header says happened.

This matters beyond tidiness. RFC-0933 being accepted means the middleware,
header, scope-hierarchy, persistence, and error-response shapes it specifies are
project requirements — for the surface it actually covers. The admin surface has
neither an implementation nor a requirement behind it. There is no RFC that says
the admin API must be rate limited, and the one artifact claiming otherwise
points at a section that does not exist.

### The deferral chain dead-ends in missions that were never filed

`missions/claimed/0948-b1-admin-rate-limiting.md` is marked _Closed 2026-08-13_
with the substrate landed. Its dispatch-hook acceptance criterion is unchecked
and annotated as deferred to a follow-on, `0948-b1-dispatch-hook`, and the
`ServerConfig` plumbing to `0948-b1-server-config`. Its v0.2 history row claims
_3 follow-ons filed_.

Neither named follow-on exists. A search of `missions/` for those identifiers
returns no file; the only mention of `dispatch-hook` anywhere is inside
`0948-b1` itself. Two are named and three are claimed, so the history row
overstates even the deferrals it records.

One hop further out, `missions/claimed/0949-b-sso-oauth2-oidc.md` defers rate
limiting on the auth endpoints to `0948-b1` and lists it under _Already filed_.
That mission is closed. So a live, claimed mission records its rate-limiting
obligation as discharged by work that has not happened.

The net effect is that the admin rate limiter is orphaned code with a broken
citation and no owner, and the RFC that would have required it does not.

## What is NOT claimed

- No claim that the limiter is wrong. It was built, reviewed, and tested; the
  defect is that it is not connected to anything.
- No claim that the owning mission was closed in error. Deferring a dispatch
  hook behind a substrate landing is a reasonable sequence. The failure is that
  the deferral has no destination.
- No claim that RFC-0933 _should_ cover the admin surface. That is the
  specification decision this mission exists to route, not to make.

## Acceptance Criteria

- [ ] The `RFC-0933 §Admin endpoints` citation in the module header resolves, or
      is replaced with whatever does authorise an admin rate limiter, or
      removed — a citation to a section that does not exist is worse than no
      citation, because it reads as a discharged obligation
- [ ] `check_admin_rate_limit` has a call site in `admin.rs::handle_request`,
      carrying the acceptance criterion `0948-b1` closed with deferred
- [ ] The blocked path produces `429` with `Retry-After`, per the response shape
      `0948-b1` specifies and does not deliver
- [ ] `AdminRateLimiter::set_enabled` is reachable from server configuration
      rather than runtime-only, so the limiter can be turned off deliberately
      rather than being permanently inert
- [ ] `0949-b`'s deferral of auth-endpoint rate limiting points at an open
      mission instead of a closed one
- [ ] A decision is recorded on whether the admin surface requires rate
      limiting, since RFC-0933 does not cover it

## Why this is filed rather than fixed

Wiring the limiter changes runtime behaviour of all 57 admin route arms: routes
that answer instantly today begin returning `429` under load, and the identity
resolution decides who shares a bucket. That is a deployable behaviour change
with a real failure mode, not a mechanical connection, and it belongs to whoever
owns the admin API's posture — the same decision mission 0870-b is routing.

## Related

Mission 0870-b covers authorization on this same router and finds no handler
that authorizes the caller. Together the two mean the shipped admin API has
neither authentication nor rate limiting, and in both cases a specification
exists somewhere in the RFC set for the model that is missing.

Mission 0949-d covers the `/auth/*` stubs on this router, including
`POST /auth/token/introspect` and `POST /auth/logout`, the two routes for which
`admin_rate_limit` already carries a policy.

## Method note

Every claim above was measured against the tree rather than read off the
citations. The zero-call-site claim is a workspace-wide symbol search; the
citation claim is a section-heading list of RFC-0933; the missing-follow-on
claim is an identifier search of the whole `missions/` tree. No part of this
mission rests on a document asserting that the work was done.
