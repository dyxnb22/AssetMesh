-- AssetMesh database migration 0001: shared kernel tables + Media module V1.
--
-- Version axes are independent (ADR 0008):
--   * this file is database migration version 1;
--   * the portable export format version lives in the application layer;
--   * the media module data schema version is recorded in module_metadata.

CREATE TABLE assets (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (length(kind) > 0),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    summary TEXT,
    lifecycle_state TEXT NOT NULL CHECK (lifecycle_state IN ('active', 'archived', 'merged')),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    archived_at TEXT,
    merged_into_asset_id TEXT REFERENCES assets(id)
);

CREATE INDEX idx_assets_kind ON assets(kind);
CREATE INDEX idx_assets_lifecycle ON assets(lifecycle_state);

CREATE TABLE module_metadata (
    module_id TEXT PRIMARY KEY,
    schema_version INTEGER NOT NULL
);

INSERT INTO module_metadata (module_id, schema_version) VALUES ('media', 1);

CREATE TABLE media_records (
    asset_id TEXT PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    media_type TEXT NOT NULL CHECK (media_type IN ('movie', 'tv', 'anime', 'game')),
    status TEXT NOT NULL CHECK (status IN ('planned', 'in_progress', 'completed', 'paused', 'dropped')),
    rating REAL CHECK (rating IS NULL OR (rating >= 0 AND rating <= 10)),
    year INTEGER CHECK (year IS NULL OR (year BETWEEN 1850 AND 2100)),
    platform TEXT,
    progress_current REAL CHECK (progress_current IS NULL OR progress_current >= 0),
    progress_total REAL CHECK (progress_total IS NULL OR progress_total > 0),
    progress_unit TEXT,
    notes TEXT,
    started_at TEXT,
    completed_at TEXT,
    -- a unit label without any numeric value is meaningless
    CHECK (progress_unit IS NULL OR progress_current IS NOT NULL OR progress_total IS NOT NULL),
    CHECK (progress_current IS NULL OR progress_total IS NULL OR progress_current <= progress_total)
);

CREATE TABLE external_refs (
    id TEXT PRIMARY KEY,
    asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    namespace TEXT NOT NULL,
    external_id TEXT NOT NULL,
    source_url TEXT,
    metadata TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (namespace, external_id)
);

CREATE INDEX idx_external_refs_asset ON external_refs(asset_id);

CREATE TABLE activity_events (
    id TEXT PRIMARY KEY,
    occurred_at TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (length(event_type) > 0),
    asset_id TEXT REFERENCES assets(id),
    actor TEXT NOT NULL CHECK (length(actor) > 0),
    payload TEXT NOT NULL
);

CREATE INDEX idx_activity_asset ON activity_events(asset_id, occurred_at);
CREATE INDEX idx_activity_time ON activity_events(occurred_at);

CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);

CREATE TABLE asset_tags (
    asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (asset_id, tag_id)
);

CREATE INDEX idx_asset_tags_tag ON asset_tags(tag_id);

-- Search projection (ADR 0006): an FTS5 table holding the derived
-- SearchDocument text. asset_id is UNINDEXED metadata used to address rows.
-- The whole table is rebuildable from canonical data and is never exported.
CREATE VIRTUAL TABLE search_documents USING fts5(
    title,
    subtitle,
    body,
    keywords,
    kind UNINDEXED,
    asset_id UNINDEXED,
    tokenize = 'unicode61'
);
