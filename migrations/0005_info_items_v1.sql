-- Reusable information items. The asset owns identity, lifecycle and tags.
INSERT INTO module_metadata (module_id, schema_version) VALUES ('info', 1);

CREATE TABLE info_records (
    asset_id TEXT PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    info_type TEXT NOT NULL CHECK (info_type IN ('email', 'url', 'api_key', 'text')),
    value TEXT NOT NULL CHECK (length(trim(value)) > 0),
    notes TEXT
);
