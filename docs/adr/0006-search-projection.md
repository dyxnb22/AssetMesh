# ADR 0006: Shared search projection

- Status: Accepted
- Date: 2026-09-19

## Context

AssetMesh will contain heterogeneous modules such as media, software, services, projects, and knowledge. If each UI queries module tables directly, global search will become a collection of special-case joins and duplicated ranking logic.

Search also needs aliases, external identifiers, notes, tags, and localized names without forcing all searchable fields into the base `assets` table.

## Decision

Search is a projection derived from canonical module data.

Each module maps an asset into a shared search document:

```text
SearchDocument
- asset_id
- kind
- title
- subtitle?
- body?
- keywords[]
- updated_at
```

The canonical source remains the normal asset/module tables. Search indexes are rebuildable projections and may be deleted/recreated without losing user data.

### Responsibilities

Modules own the transformation from typed details to `SearchDocument`.

Examples:

```text
Media
  title: canonical title
  subtitle: Anime · 2023
  body: notes + platform
  keywords: alternate titles, provider aliases, tags

Software
  title: application name
  subtitle: AI Client
  body: purpose + install source + location
  keywords: bundle id, Homebrew name, aliases
```

### V1 storage

Start with SQLite-backed search.

Preferred progression:

1. indexed normalized columns for exact/common filters;
2. SQLite FTS5 for text search;
3. trigram or equivalent substring support where language/tokenization requirements justify it;
4. no external search service in V1.

Search implementation details belong in infrastructure. Application code depends on a `SearchPort`/`SearchRepository` style contract.

### Consistency

Canonical write and search projection update should be transactionally consistent where practical. If later indexing becomes asynchronous, the system must expose reindex/rebuild semantics and tolerate temporary lag explicitly.

### Query model

Search returns asset identities and presentation-oriented search fields. It does not return arbitrary module database rows as a substitute for application queries.

Structured filtering remains typed application behavior, for example media status or software install source. Full-text search should not become the only query mechanism.

## Consequences

- Global search does not require cross-module SQL knowledge in the UI.
- Search indexes are disposable and rebuildable.
- New modules can join global search by implementing one projection contract.
- Localization and aliases can evolve without bloating `Asset`.

## Non-goals

- Elasticsearch/Meilisearch in V1;
- semantic/vector search as canonical infrastructure;
- using the search index as the source of truth.
