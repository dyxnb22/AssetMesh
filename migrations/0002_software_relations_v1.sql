-- AssetMesh database migration 0002: Software module V1 + shared relations.
--
-- Adds the Software module's typed details (module schema version 1,
-- recorded in module_metadata) and the shared relations table used to
-- connect assets across modules (docs/03). Existing Media rows are untouched.

CREATE TABLE software_records (
    asset_id TEXT PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    category TEXT NOT NULL CHECK (category IN ('application', 'cli', 'package', 'runtime', 'tool')),
    install_source TEXT NOT NULL CHECK (install_source IN ('macos_app', 'homebrew_formula', 'homebrew_cask', 'npm_global', 'pipx', 'manual', 'system', 'unknown')),
    version TEXT,
    install_location TEXT,
    executable_path TEXT,
    purpose TEXT,
    notes TEXT,
    discovered_at TEXT,
    installed_at TEXT,
    architecture TEXT
);

INSERT INTO module_metadata (module_id, schema_version) VALUES ('software', 1);

-- Shared relations: one row represents the relation between two shared
-- assets, and every fact has exactly ONE row representation. Inverse types
-- (dependency_of, used_by, installs) are view-time derivations of their
-- primary (depends_on, uses, installed_via) and are never stored — a fact
-- stated with an inverse type is stored as its primary with the endpoints
-- swapped. Symmetric `related_to` is stored in canonical (source, target)
-- order. Together with UNIQUE(source, target, relation_type) this makes
-- duplicate inverse representations unrepresentable at the storage level;
-- the CHECK set must stay in sync with the relation registry (docs/03, 09).
CREATE TABLE relations (
    id TEXT PRIMARY KEY,
    source_asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    target_asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL CHECK (relation_type IN ('depends_on', 'uses', 'installed_via', 'related_to')),
    note TEXT,
    provenance TEXT NOT NULL CHECK (provenance IN ('manual', 'discovered', 'imported')),
    created_at TEXT NOT NULL,
    CHECK (source_asset_id <> target_asset_id),
    -- Symmetric types store the lexicographically smaller endpoint as
    -- source (see Relation::canonical_form). Without this, UNIQUE(source,
    -- target, type) would still permit both A->B and B->A related_to rows
    -- and one fact would have two representations even at the SQL level.
    -- The TEXT order of canonical hyphenated lowercase UUIDs matches the
    -- byte order the domain compares, so the two agree exactly.
    CHECK (relation_type <> 'related_to' OR source_asset_id < target_asset_id),
    UNIQUE (source_asset_id, target_asset_id, relation_type)
);

CREATE INDEX idx_relations_source ON relations(source_asset_id);
CREATE INDEX idx_relations_target ON relations(target_asset_id);
