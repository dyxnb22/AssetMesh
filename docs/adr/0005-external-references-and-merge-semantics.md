# ADR 0005: External references and asset merge semantics

- Status: Accepted
- Date: 2026-09-19

## Context

AssetMesh has its own stable asset identity, but many assets also have identities in external systems: Steam app IDs, IGDB IDs, bundle identifiers, Homebrew formula/cask names, Git repository URLs, provider account IDs, and similar identifiers.

Using provider IDs as AssetMesh primary keys would couple canonical identity to external systems. Matching only by display name would make discovery and repeated imports unreliable.

The system also needs a safe answer for the case where two AssetMesh assets are later discovered to represent the same logical asset.

## Decision

AssetMesh owns canonical asset IDs. External identifiers are modeled separately as aliases/references.

Suggested model:

```text
AssetExternalRef
- id
- asset_id
- namespace
- external_id
- source_url?
- metadata?
- created_at
- updated_at

UNIQUE(namespace, external_id)
```

Examples:

```text
steam:1091500
igdb:1877
bundle_id:com.microsoft.VSCode
homebrew_cask:visual-studio-code
github_repo:dyxnb22/AssetMesh
```

Namespaces are stable machine identifiers, not UI labels.

### Matching order

Discovery/import matching should prefer, in order:

1. exact canonical asset ID when importing AssetMesh data;
2. exact external-reference match;
3. deterministic module-specific keys, if explicitly defined;
4. heuristic similarity such as normalized title/type/year.

Heuristic matches must not silently merge canonical assets.

### Merge semantics

Asset merges are explicit application operations, not repository side effects.

A merge selects one survivor asset and redirects the duplicate into it:

```text
merge(source, target)

1. validate kind/module compatibility
2. resolve typed-detail conflicts
3. move or deduplicate external refs
4. move relations, tags, collection membership, and attachments
5. preserve activity provenance
6. record an asset.merged activity event
7. mark source as merged/redirected rather than immediately erasing identity
8. commit atomically
```

The initial implementation may use a `merged_into_asset_id` or equivalent tombstone/redirect field. Hard deletion of the source is not required.

### Conflict policy

Module-specific fields are never merged by a generic SQL overwrite. The owning module decides how conflicts are resolved. V1 may require explicit user choice for ambiguous conflicts.

## Consequences

- Provider changes do not change AssetMesh identity.
- Repeated discovery becomes idempotent when stable external IDs exist.
- Imports can detect known assets reliably.
- Merge history remains explainable.
- More tables and application logic are required, but identity stays durable.

## Non-goals

- automatic fuzzy deduplication of the whole library;
- globally standardized namespaces across the internet;
- distributed identity or cross-device sync in V1.
