-- AssetMesh database migration 0004: service relation types.
--
-- Extends the shared relations table's stored-type CHECK with the two
-- canonical primary relation types Services needs (docs/10 "Relations"):
--
--   hosted_on  <-> hosts         (AssetMesh API --hosted_on--> RackNerd VPS)
--   points_to  <-> pointed_to_by (assetmesh.dev --points_to--> AssetMesh API)
--
-- The inverse types (`hosts`, `pointed_to_by`) stay view-time derivations and
-- are NOT storable, exactly like the Phase 2 inverses, so one fact still has
-- exactly one row.
--
-- Why a new migration instead of editing 0003: an already-applied migration is
-- immutable — its checksum is recorded in assetmesh_migrations and an edit
-- would make every database that already ran it fail to open (docs/05 schema
-- migration rules). The Phase 3B core already shipped 0003, so the CHECK
-- change has to arrive as its own migration.
--
-- SQLite cannot alter a CHECK constraint in place, so the table is rebuilt:
-- create the new shape, copy every row verbatim, drop the old table, rename.
-- Existing Media/Software/Service/Relation rows are untouched.

CREATE TABLE relations_v4 (
    id TEXT PRIMARY KEY,
    source_asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    target_asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    -- Canonical primary types only; the set must stay in lockstep with
    -- domain::relation::RelationType::primary() (docs/03, 10).
    relation_type TEXT NOT NULL CHECK (relation_type IN ('depends_on', 'uses', 'installed_via', 'hosted_on', 'points_to', 'related_to')),
    note TEXT,
    provenance TEXT NOT NULL CHECK (provenance IN ('manual', 'discovered', 'imported')),
    created_at TEXT NOT NULL,
    CHECK (source_asset_id <> target_asset_id),
    -- Symmetric types store the lexicographically smaller endpoint as source
    -- (see Relation::canonical_form). Without this, UNIQUE(source, target,
    -- type) would still permit both A->B and B->A related_to rows and one fact
    -- would have two representations even at the SQL level. The TEXT order of
    -- canonical hyphenated lowercase UUIDs matches the byte order the domain
    -- compares, so the two agree exactly.
    CHECK (relation_type <> 'related_to' OR source_asset_id < target_asset_id),
    UNIQUE (source_asset_id, target_asset_id, relation_type)
);

INSERT INTO relations_v4 (id, source_asset_id, target_asset_id, relation_type, note, provenance, created_at)
    SELECT id, source_asset_id, target_asset_id, relation_type, note, provenance, created_at
    FROM relations;

DROP TABLE relations;
ALTER TABLE relations_v4 RENAME TO relations;

CREATE INDEX idx_relations_source ON relations(source_asset_id);
CREATE INDEX idx_relations_target ON relations(target_asset_id);
