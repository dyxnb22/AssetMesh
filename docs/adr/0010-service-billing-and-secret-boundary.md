# ADR 0010 — Service billing representation and secret boundary

Status: **Accepted**

## Context

Phase 3 introduces Services whose durable metadata may include provider, plan, cost,
renewal/expiry dates, endpoint URLs, and account labels. Those assets are adjacent to
credentials and commercial lifecycle data, which creates two long-lived risks if the
representation is not fixed early:

1. floating-point money or implicit calendar arithmetic would make canonical state
   ambiguous and hard to round-trip safely;
2. treating API keys/passwords/tokens as ordinary Service fields would leak secrets
   into SQLite, search, activity, logs, portable exports, and future UI transports.

AssetMesh is local-first, but local-first does not mean secret material should become
ordinary canonical library data. The architecture already separates domain/application
rules from infrastructure adapters and leaves OS secure storage as an optional adapter.

## Decision

### Subscription identity

A subscription is not a separate top-level Asset in V1. Commercial/lifecycle metadata
belongs to the Service asset it describes.

This avoids competing identities for one durable thing (for example a SaaS Service and
a second Subscription asset representing the same SaaS entitlement).

### Money

Canonical recurring/plan cost is represented as:

```text
cost_minor: integer
currency:   three-letter uppercase code
```

Money is never stored as floating point.

`cost_minor` and `currency` are paired: both present or both absent. V1 does not
perform exchange-rate conversion, tax calculation, invoice accounting, or inferred
provider billing.

Adapters may accept decimal user input, but must parse it deterministically into minor
units before invoking application services.

### Renewal and expiry

`renews_at` records the next expected commercial renewal/charge boundary.
`expires_at` records the known end of entitlement/service unless extended.

These are distinct facts and neither automatically changes shared Asset lifecycle.
AssetMesh does not guess the next billing date. Renewal operations record explicit
caller/provider-known dates so month length, timezone, provider policy, and usage-based
billing rules are not silently invented.

### Credentials and secrets

Canonical Service records, Search Projection, Activity payloads, external references,
and portable export must not contain secret material such as:

- API keys;
- passwords;
- access/refresh tokens;
- cookies/session credentials;
- private SSH keys;
- authorization headers.

Non-secret account labels, provider names, dashboard/endpoint URLs, and stable provider
resource IDs are allowed.

Phase 3 does not introduce `credential_ref` speculatively. If a concrete workflow later
requires credentials, the application/domain boundary will depend on an inward-defined
secure-reference/`SecretStore` port and an infrastructure adapter such as the OS
Keychain. Portable export will still exclude secret values.

## Consequences

### Positive

- portable data round-trips commercial values without floating-point drift;
- Service assets remain the single canonical identity for subscriptions;
- renewal behavior is deterministic and explainable;
- SQLite/search/export do not become accidental credential stores;
- a future Keychain/secure-store implementation can be introduced without changing the
  meaning of existing Service records;
- CLI/Desktop/HTTP/MCP adapters share the same application semantics.

### Costs

- Phase 3 cannot directly operate services that require credentials;
- account discovery and authenticated provider integrations remain future work;
- V1's user-facing decimal parsing policy is intentionally narrower than a full
  multi-currency accounting system;
- cancellation/expiry does not automatically archive an Asset and must remain an
  explicit lifecycle decision.

## Rejected alternatives

### Store cost as floating point

Rejected because binary floating-point values are unsuitable as canonical monetary
representation and make deterministic import/export harder.

### Create a separate Subscription asset for every paid service

Rejected because V1 has no independent subscription behavior that justifies a second
identity; it would create duplication and merge/linking ambiguity.

### Store encrypted credentials directly in Service records

Rejected because encryption-at-rest alone does not prevent secret propagation through
search, export, activity, debug output, or transport DTOs. Secret ownership belongs
behind a dedicated secure-store boundary when a real workflow requires it.

### Automatically calculate the next renewal date

Rejected because billing cadence alone is insufficient to reproduce provider-specific
calendar, timezone, grace-period, and usage-based policies reliably.

## Related

- ADR 0001 — local-first and UI-independent core
- ADR 0003 — shared Asset identity plus module-owned typed details
- ADR 0005 — external references and merge semantics
- ADR 0008 — independent module schema versioning
- `docs/10-services-subscriptions-v1.md`
