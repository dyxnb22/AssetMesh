# Services and Subscriptions V1

Phase 3 delivers the **Services and Subscriptions** vertical slice: a third first-party
asset domain proving that the AssetMesh kernel can support durable digital services,
commercial lifecycle metadata, and cross-domain relationships without introducing
UI-specific assumptions, secret storage, runtime monitoring, or cloud-account
orchestration.

Headless by design: all canonical behavior in this document is exposed through
application services and exercised by the CLI/test harness. Desktop presentation
remains Phase 5; runtime state, health checks, process/container inspection, and
operational actions remain Phase 6.

This phase deliberately treats subscriptions as lifecycle/commercial properties of a
Service asset rather than as a second competing top-level identity.

## Goals

Phase 3 must prove that Media, Software, and Services can coexist on the same shared
contracts for:

- Asset identity and lifecycle;
- namespaced external references;
- explicit merge semantics;
- shared relations;
- activity;
- rebuildable search projection;
- SQLite migrations and repository boundaries;
- portable export/import;
- deterministic application services independent of CLI/UI transports.

A successful Phase 3 makes the shared kernel credible across three substantially
different first-party modules before AssetMesh commits to broader extension points or
a desktop presentation architecture.

## Explicit non-goals

Phase 3 does **not** implement:

- desktop UI or graph visualization;
- Docker/OrbStack/process/port monitoring;
- endpoint health checks or log collection;
- start/stop/restart/install/uninstall actions;
- SSH/VPS administration;
- cloud-provider account discovery;
- automatic SaaS/API account discovery;
- automatic renewal or purchasing;
- invoice ingestion or accounting/expense reporting;
- exchange-rate conversion or tax calculation;
- a password manager or credential vault;
- API-key/password/token/cookie/SSH-key persistence in canonical Service data;
- a durable job queue or mandatory local daemon;
- a separate Subscription asset/module;
- a separate Account asset/module.

These boundaries are intentional. Phase 3 owns durable service inventory and
subscription lifecycle metadata, not runtime operations or credential management.

## Canonical model

A service is a shared `Asset` plus module-owned typed details.

```text
ServiceRecord
- asset_id             # canonical identity; Asset owns name/summary/lifecycle
- service_type         # saas | api | vps | domain | local
- provider?            # human-readable provider name
- account_label?       # non-secret label, e.g. "Personal" or "Work"
- endpoint_url?        # service/API endpoint; metadata, never identity
- dashboard_url?       # non-secret management/dashboard URL
- domain_name?         # canonical domain text when service_type = domain
- plan?                # e.g. Plus, Pro, Basic VPS
- cost_minor?          # integer minor units, never floating point
- currency?            # ISO 4217-style uppercase code, e.g. USD, HKD
- billing_cadence?     # monthly | quarterly | yearly | usage_based | one_time | other
- renews_at?           # next expected renewal/charge boundary
- expires_at?          # service/right ceases after this time unless extended
- auto_renew?          # user-known setting; null means unknown
- notes?               # user-owned notes, never secrets
```

Module schema version is `services schema_version = 1`, independent of SQLite
migration version and portable export format version (ADR 0008).

### Asset kinds and type compatibility

Service type maps 1:1 to Asset kind:

```text
service.saas   <-> saas
service.api    <-> api
service.vps    <-> vps
service.domain <-> domain
service.local  <-> local
```

Every write path, including direct repository/UnitOfWork writes, must reject a
`ServiceRecord` whose `service_type` is incompatible with the owning Asset kind.
Changing an Asset kind must not be allowed to strand incompatible module-owned state,
and import/merge preflight must enforce the same invariant before mutation.

## Subscription semantics

A subscription is **not** a separate Asset in V1.

For example, "ChatGPT Plus", "Google AI Pro", a paid API plan, a domain renewal, or
a VPS rental is represented by the Service asset itself. Commercial/lifecycle
attributes (`plan`, `cost_minor`, `currency`, `billing_cadence`, `renews_at`,
`expires_at`, `auto_renew`) describe that service.

This avoids duplicate identities such as one Asset for the service and another Asset
for the subscription to the same thing.

A future billing-history or account module may reference Service assets if a concrete
need appears; Phase 3 does not reserve or pre-build that model.

## Money representation

Money must never use floating point in canonical state or portable wire DTOs.

```text
cost_minor = 1999
currency   = "USD"
```

means USD 19.99.

Rules:

- `cost_minor` is a non-negative signed integer in the currency's minor unit;
- `currency`, when present, is trimmed uppercase ASCII and must be exactly three
  alphabetic characters;
- `cost_minor` and `currency` are either both present or both absent;
- AssetMesh does not infer exchange rates, convert currencies, split taxes, or claim
  accounting precision beyond the user-provided recurring/plan cost;
- `usage_based` may have no `cost_minor` when no stable amount exists;
- CLI decimal input is parsed deterministically into minor units and rejects values
  that cannot be represented without rounding under the selected currency policy.

V1 uses two decimal minor units for CLI entry/display unless/until AssetMesh gains a
currency metadata table. The canonical field remains integer minor units so this UI
policy can evolve without changing stored meaning.

## Time and lifecycle semantics

`renews_at` and `expires_at` are deliberately different:

- `renews_at`: the next expected commercial renewal/charge boundary;
- `expires_at`: the known end of entitlement/service if no extension occurs.

Neither field changes the shared Asset lifecycle automatically.

Examples:

- a monthly SaaS plan may have `renews_at` and no `expires_at`;
- a domain registration may have both, or only `expires_at` if auto-renew status is
  unknown;
- a cancelled subscription may set `auto_renew = false` while retaining the future
  `expires_at` date;
- an already expired service may remain an active historical Asset until the user
  explicitly archives it.

Application services must not guess calendar arithmetic for future renewals. A
renewal command records the supplied renewal fact and the caller provides the next
renewal/expiry boundary when known. This avoids ambiguous month-length, timezone,
provider-policy, and usage-based billing rules.

All stored timestamps use the same canonical timestamp representation as the rest of
AssetMesh.

## Renewal use case

Services need a domain-specific behavior beyond CRUD: `record_renewal`.

Conceptually:

```text
record_renewal(service_id,
               charged_cost_minor?,
               currency?,
               renewed_at,
               next_renews_at?,
               next_expires_at?)
```

The command:

1. loads and validates the Service asset in one short write transaction;
2. applies explicitly supplied next-boundary/cost metadata, never deriving a
   boundary from the billing cadence;
3. appends one `service.renewed` activity event containing only non-secret factual
   renewal data;
4. updates the Search Projection only if visible canonical fields changed;
5. always appends the event, because `renewed_at` is a required argument — a
   renewal command that carries no renewal moment is a caller error, not a
   silent no-op. Money is optional but must be supplied as a pair
   (`charged_cost_minor` together with `currency`) whenever either is present.

Phase 3 does not create an invoice ledger. The activity event is historical provenance,
not a general accounting subsystem.

## Cancellation and archival

Phase 3 does not introduce a separate subscription status enum.

- stopping future renewal is represented by `auto_renew = false` when known;
- a known end-of-service date belongs in `expires_at`;
- the durable Asset remains available for history/search until explicitly archived via
  the shared Asset lifecycle;
- cancellation may emit `service.updated` only if Phase 3 adopts meaningful-change
  activity for service lifecycle fields; metadata-only edits should remain conservative
  and avoid activity spam.

The implementation must not automatically archive/delete a Service merely because
`expires_at` is in the past.

## Text and URL invariants

All user-owned text fields follow the established repository-boundary discipline:
trim input, reject control characters, enforce bounded lengths, and validate again at
the repository seam so portable/import/direct writes cannot persist raw invalid text.

Recommended V1 limits:

```text
provider        <= 256 chars
account_label   <= 256 chars
endpoint_url    <= 2048 chars
 dashboard_url  <= 2048 chars
 domain_name     <= 253 chars
plan            <= 256 chars
notes           <= 8192 chars
```

URLs are metadata, not external identity. V1 validation should require syntactically
reasonable absolute `http`/`https` URLs where a URL field is used, without performing
network I/O.

`domain_name` is normalized for comparison/storage (trimmed, lowercased ASCII form
where safely available) but Phase 3 must not invent DNS ownership or resolution facts.

## Secret boundary

Canonical Service data must not contain credentials.

Forbidden canonical/export/search values include:

- API keys;
- passwords;
- access or refresh tokens;
- cookies/session material;
- private SSH keys;
- raw authorization headers;
- provider secret payloads.

`account_label`, provider name, endpoint URL, dashboard URL, external resource IDs,
and non-secret notes are allowed.

Phase 3 does not implement `credential_ref` unless a concrete workflow requires it.
If credential support is added later, it must use a dedicated inward-defined
`SecretStore`/secure-reference port with an OS secure-store adapter; portable export
must never serialize secret material.

Tests must include explicit assertions that Services portable export and Search
Projection contain no credential fields and that no domain/application DTO offers a
field that encourages secret persistence.

See ADR 0010.

## External references

External systems provide aliases, never AssetMesh primary identity (ADR 0005).

Examples of acceptable namespaces when the concrete provider/use case exists:

```text
domain_name:assetmesh.dev
cloudflare_zone:<zone-id>
vercel_project:<project-id>
digitalocean_droplet:<id>
aws_resource:<id>
provider_resource:<stable-provider-id>
```

Rules:

- namespace strings obey the existing kernel validation rules;
- `(namespace, external_id)` remains globally unique;
- exact external-reference ownership is authoritative for deterministic matching;
- names/URLs/provider labels may suggest duplicates but never silently merge assets;
- API keys/tokens are never external references;
- endpoint/dashboard URLs are metadata, not identity aliases by default.

Phase 3 does not require automatic provider discovery. Refs can be added manually or
through future provider adapters while preserving the same matching contract.

## Relations

Services create two concrete relation needs in addition to the Phase 2 registry:

```text
hosted_on <-> hosts
points_to <-> pointed_to_by
```

Representative facts:

```text
AssetMesh API --hosted_on--> RackNerd VPS
assetmesh.dev --points_to--> AssetMesh API
Cursor --uses--> OpenAI API
Project --depends_on--> Cloudflare
```

The registry after Phase 3 therefore supports primary stored types:

```text
depends_on
uses
installed_via
hosted_on
points_to
related_to
```

with inverse types derived at view time and `related_to` remaining symmetric.

All Phase 2 canonicalization guarantees remain mandatory:

- inverse facts normalize to one primary stored representation;
- symmetric facts normalize endpoint order;
- self-relations are rejected;
- one fact has exactly one canonical row;
- service attach/remove paths do not bypass the shared Relation service/repository;
- merges re-point, canonicalize, deduplicate, and remove self-loops;
- the SQL CHECK constraint and Rust registry must evolve together in migration 0004.

Phase 4 owns general relation traversal/impact query services as part of the broader
Unified Library Core contract (`docs/11-unified-library-core.md`). Graph visualization
and other desktop presentation remain Phase 5.
Phase 3 only adds relation semantics required by real Service use cases.

## CRUD application services

Phase 3 provides application-layer use cases equivalent in discipline to Media and
Software:

```text
create_service
get_service
list_services
update_service
record_renewal
archive_service (through shared Asset lifecycle)
```

Commands operate on stable DTOs and never expose repositories directly to the CLI.
External/network I/O, if any future provider adapter appears, must occur outside write
transactions.

Updates are explicit patch semantics. A caller can distinguish "leave unchanged" from
"clear this optional field"; adapters must not accidentally erase canonical data
because an omitted CLI/UI value was deserialized as null.

No service discovery/adoption pipeline is required in Phase 3 because no concrete
read-only provider is necessary to prove the vertical slice. Add provider candidates
later only when a real source exists; do not invent a generic cloud scanner abstraction
now.

## Merge semantics

Asset merge remains explicit and uses one surviving Asset identity (ADR 0005).

Service-specific merge rules:

1. merge preflight rejects service/non-service or incompatible ServiceType detail
   combinations unless the caller has first resolved the type conflict explicitly;
2. if only one side has a ServiceRecord, it may move to the winner when winner kind is
   compatible;
3. if both sides have ServiceRecords of the same type, conflicting non-empty typed
   fields are **not** silently chosen;
4. equal normalized values deduplicate safely;
5. empty-vs-present values may fill the winner's missing field;
6. any remaining conflicts are returned as reviewable merge conflicts before mutation;
7. external refs and relations follow shared merge semantics atomically with module
   detail resolution;
8. no rule prefers the newest value merely by timestamp, and provider metadata never
   overrides user-owned canonical fields automatically.

A merge must leave no ServiceRecord attached to a loser tombstone and must preserve the
explainable redirect/merged identity behavior of the shared kernel.

## Activity policy

Keep activity meaningful and conservative.

Required events:

- `asset.created` + `service.created` for manual creation;
- `service.renewed` for an explicit renewal recording;
- shared relation lifecycle events for relation creation/removal;
- shared merge activity according to existing merge semantics.

Ordinary metadata edits do not need an event unless they represent a meaningful
service lifecycle change that the application deliberately models. Do not emit an
event for every field edit.

No secret-bearing value may ever appear in an activity payload.

## Search projection

Services participate in the shared rebuildable Search Projection.

Recommended mapping:

```text
title:    Asset name
subtitle: service type label + provider + plan when useful
body:     non-secret lifecycle/commercial summary + endpoint/domain + notes
keywords: tags + safe external aliases + provider/domain labels
```

Examples:

```text
ChatGPT Plus
SaaS · OpenAI · Plus
USD 20.00/month · renews 2026-10-20 · Personal

assetmesh.dev
Domain · Cloudflare
expires 2027-09-20
```

Search output must never include credential values or opaque raw provider payloads.
The projection remains disposable and rebuildable entirely from canonical state.

## SQLite layout and migration 0003

Migration `0003_services_v1.sql` should add approximately:

```text
service_records
- asset_id PK -> assets ON DELETE CASCADE
- service_type CHECK
- provider
- account_label
- endpoint_url
- dashboard_url
- domain_name
- plan
- cost_minor
- currency
- billing_cadence CHECK
- renews_at
- expires_at
- auto_renew CHECK (NULL/0/1)
- notes
```

Storage-level CHECK constraints should enforce representation-level invariants where
practical, including enum values and paired money fields when SQLite semantics permit;
domain/repository validation remains authoritative for richer invariants.

Migration 0004 (not 0003) updates the relations-table stored-type CHECK to include the
new canonical primary relation types (`hosted_on`, `points_to`) while preserving existing
rows and uniqueness/canonical-row guarantees. Migration 0003 already shipped with the
Phase 3 core and is applied by every existing database, and the checksum guard makes an
already-applied migration immutable, so the CHECK change had to arrive as a new migration
rather than as an edit to 0003.

`module_metadata` gains `('services', 1)`.

Required migration tests:

- a real Phase 2 database produced by the Phase 2 binary migrates to Phase 3;
- existing Media/Software/Relation data is unchanged;
- repeated open is idempotent;
- changed checksum for an already-applied migration fails loudly;
- module schema versions are validated independently;
- old DBs cannot end in partially migrated relation/service state.

## Portable data

Portable format remains top-level version 1 unless an actually incompatible bundle
change requires otherwise.

Phase 3 adds:

```text
modules/services.jsonl
```

using dedicated `ServiceV1` wire DTOs rather than serializing domain structs directly.
Existing `relations.jsonl` naturally carries the new relation types after validation.

Declaration compatibility follows Phase 2:

- a declared Services section must exist, match manifest count/schema version, and be
  fully preflighted before mutation;
- a declared Services section is authoritative for bundled Service assets;
- an absent Services declaration means the bundle predates Phase 3 and must not erase
  pre-existing destination Service data;
- a `modules/services.jsonl` file present without declaration is corruption;
- dry-run and commit use the same validation/conflict classification;
- re-import is deterministic/idempotent;
- Search Projection, caches, and secrets are never exported.

### Import conflict policy

Portable import never silently resolves Service field conflicts.

Before mutation it validates:

- Asset kind <-> ServiceType compatibility;
- enum/text/URL/time/money invariants;
- external-ref uniqueness;
- relation canonicalizability and endpoint existence;
- duplicate Service detail rows;
- no Service detail on merged/tombstone identities: the portable contract has no
  "re-home this row onto the winner" rule, so a service row on a tombstone is a
  preflight error for dry-run and commit alike (a merge always removes the
  loser's record, so a well-formed export never emits one);
- declared authoritative reconciliation does not create cross-module stranded state.

For the bundled canonical identity, declared Services data is authoritative in the
same sense as the existing module contracts: fields absent/cleared in the wire record
are reconciled accordingly. This is different from heuristic legacy import or
provider adoption, which must never overwrite canonical truth implicitly.

## CLI surface

Exact argument spelling may follow the existing CLI conventions, but Phase 3 should
cover all application services through commands equivalent to:

```bash
assetmesh service add \
  --name "Google AI Pro" \
  --type saas \
  --provider Google \
  --plan "AI Pro" \
  --cost 5.00 \
  --currency USD \
  --billing monthly \
  --renews-at 2026-10-18 \
  --auto-renew

assetmesh service get <asset-id>
assetmesh service list [filters]
assetmesh service update <asset-id> [...patch fields...]

assetmesh service renew <asset-id> \
  --cost 5.00 \
  --currency USD \
  --renewed-at 2026-10-18 \
  --next-renewal 2026-11-18

assetmesh relation attach <source> hosted_on <target>
assetmesh relation attach <source> points_to <target>
```

The CLI is an adapter only. Parsing currency decimals, optional-field clear/set intent,
and timestamps must map into application command DTOs without reimplementing domain
policy.

## Testing requirements

Phase 3 is not complete with happy-path CRUD alone.

At minimum cover:

### Domain/repository invariants

- every ServiceType <-> Asset kind pair;
- direct repository writes cannot attach incompatible details;
- text normalization/control-character rejection at service and repository seams;
- valid/invalid money pairs and negative costs;
- valid/invalid currency forms;
- lifecycle timestamp combinations without auto-archival;
- secret-like fields do not exist in canonical Service DTO/wire/search contracts.

### Application behavior

- create/get/list/update/clear optional fields;
- renewal event and explicit next-date semantics;
- no guessed renewal arithmetic;
- no metadata edit activity spam;
- search projection update/rebuild;
- relation attach/remove with both new inverse pairs;
- merge fill-missing vs conflict behavior.

### SQLite contracts

- migration from the actual Phase 2 fixture;
- repository parity with in-memory/test doubles;
- relation SQL CHECK stays aligned with registry;
- transaction rollback leaves no partial Asset/Service/activity/search state;
- concurrent readers/writers retain ADR 0007 guarantees.

### Portability

- Phase 1 and Phase 2 bundles still import;
- Services declaration authoritative behavior;
- undeclared Services absence preserves destination state;
- undeclared Services file is rejected;
- malformed money/type/URL/ref/relation state fails preflight and commit identically;
- deterministic export and idempotent re-import;
- merge/tombstone reconciliation;
- explicit assertions that exported/searchable data contains no credentials.

### CLI end-to-end

Exercise at least SaaS, VPS, domain, and API examples, renewal, relation creation, export,
import, and search through the compiled binary rather than only unit-level command
parsing.

## Delivery batches

### Phase 3A — Contract

- this document;
- ADR 0010 for billing representation and credential boundary;
- Roadmap/ADR index alignment.

Exit: implementation can proceed without inventing service identity, subscription,
money, time, secret, merge, or import semantics.

### Phase 3B — Canonical core

- `domain/service.rs` and schema version;
- Service repository ports and application service;
- migration 0003 and SQLite repository;
- CRUD and patch semantics;
- ServiceType/kind, text, URL, money, and lifecycle invariants;
- search projection integration.

Exit: canonical Services are durable/searchable with repository-boundary enforcement.

### Phase 3C — Service-specific behavior

- `record_renewal` (implemented: `RecordRenewal` in `application/service_service.rs`);
- `hosted_on`/`hosts` and `points_to`/`pointed_to_by` registry + SQL support;
- activity policy;
- merge behavior;
- CLI coverage for CRUD/renewal/relations.

Exit: Services demonstrate real domain behavior beyond generic CRUD.

### Phase 3D — Portability and hardening

- `ServiceV1` portable wire DTO and `modules/services.jsonl`;
- import/export/dry-run/preflight;
- Phase 2 -> Phase 3 real migration fixture;
- compatibility with older bundles;
- E2E tests;
- focused review for secret leakage, money precision, relation canonicalization,
  migration safety, cross-module merge, and authoritative restore.

Exit: Phase 3 satisfies the full vertical-slice contract. All four batches (3A-3D) are
implemented and covered by unit, repository, and CLI end-to-end tests.

## Exit criteria

Phase 3 is complete when:

1. SaaS, API, VPS, domain, and local-service assets can be created, edited, archived,
   searched, related, exported, imported, and rebuilt headlessly;
2. subscription cost/renewal metadata is deterministic and uses integer minor units;
3. renewals can be explicitly recorded without guessed provider/calendar behavior;
4. no credential material is represented in canonical Service data, Search Projection,
   activity payloads, or portable export;
5. new service relation types preserve the one-row canonical relation invariant at
   domain, repository, import, merge, and SQL boundaries;
6. service merge/import conflicts are reviewable and never silently choose competing
   canonical values;
7. a real Phase 2 database migrates safely and historical Phase 1/2 portable bundles
   remain readable;
8. Media, Software, and Services coexist on shared identity/search/activity/relation/
   migration/portability contracts without module-to-module private-table coupling;
9. the entire slice works without Desktop, daemon, cloud discovery, or runtime
   monitoring.
