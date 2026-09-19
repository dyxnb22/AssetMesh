# ADR 0008: Module-owned schema versioning

- Status: Accepted
- Date: 2026-09-19

## Context

AssetMesh uses a shared Asset identity with module-owned typed details. Media, software, services, projects, and future modules will evolve at different speeds. Treating the entire application as one implicit schema version makes portable imports, module migrations, and backward compatibility harder to reason about.

Database migration version, portable export format version, and module data version are related but distinct concerns.

## Decision

Each module owns an explicit data schema version for its canonical typed data.

Conceptually:

```text
ModuleDescriptor
- module_id          # media, software, services
- current_schema_version
- supported_import_versions
```

Canonical typed records may store or derive the schema version necessary for migration. The exact physical representation may be per-row, per-table, or module metadata depending on the module's needs; this ADR does not require version columns on every row.

### Three independent version axes

AssetMesh must distinguish:

1. **Database schema version** — implementation detail for the local SQLite layout.
2. **Portable export format version** — top-level interchange contract for an AssetMesh export bundle.
3. **Module data schema version** — semantic version of module-owned exported/imported data.

Example manifest fragment:

```json
{
  "format": "assetmesh-portable-export",
  "version": 1,
  "modules": {
    "media": { "schema_version": 1 },
    "software": { "schema_version": 2 }
  }
}
```

### Migration ownership

- Core migrations own shared AssetMesh tables and invariants.
- Each module owns migrations for its typed details and module-specific exported representation.
- A module migration must not reach into another module's private tables directly.
- Cross-module changes go through shared core contracts or explicit coordinated migrations.

### Import compatibility

Importers should be able to:

- identify module version before mutation;
- reject unsupported future versions with a clear error;
- migrate supported older versions through explicit steps;
- preserve unknown optional metadata when feasible;
- run migrations against fixtures in tests.

### Forward evolution

Prefer additive changes. Breaking semantic changes require a schema-version increment and migration path.

## Consequences

- Modules can evolve independently without turning every change into a global format break.
- Portable exports remain understandable over time.
- Tests can pin real historical fixtures to exact module versions.
- Some extra version metadata and migration code is required.

## Non-goals

- independent binary/plugin versioning in V1;
- semantic-version strings for every database migration;
- guaranteeing import of arbitrarily newer module versions.
