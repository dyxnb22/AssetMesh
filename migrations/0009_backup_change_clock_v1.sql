-- Durable change identity for backups across processes and launches.
-- The clock is updated in the same transaction as canonical rows; rollbacks
-- roll it back too. Derived FTS rows do not create a second change.
CREATE TABLE assetmesh_change_clock (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    library_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision >= 0 AND typeof(revision) = 'integer')
);
INSERT INTO assetmesh_change_clock VALUES (1, lower(hex(randomblob(16))), 0);

CREATE TRIGGER change_clock_assets_insert
AFTER INSERT ON assets
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_assets_update
AFTER UPDATE ON assets
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_assets_delete
AFTER DELETE ON assets
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_media_records_insert
AFTER INSERT ON media_records
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_media_records_update
AFTER UPDATE ON media_records
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_media_records_delete
AFTER DELETE ON media_records
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_software_records_insert
AFTER INSERT ON software_records
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_software_records_update
AFTER UPDATE ON software_records
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_software_records_delete
AFTER DELETE ON software_records
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_service_records_insert
AFTER INSERT ON service_records
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_service_records_update
AFTER UPDATE ON service_records
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_service_records_delete
AFTER DELETE ON service_records
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_info_records_insert
AFTER INSERT ON info_records
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_info_records_update
AFTER UPDATE ON info_records
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_info_records_delete
AFTER DELETE ON info_records
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_external_refs_insert
AFTER INSERT ON external_refs
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_external_refs_update
AFTER UPDATE ON external_refs
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_external_refs_delete
AFTER DELETE ON external_refs
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_activity_events_insert
AFTER INSERT ON activity_events
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_activity_events_update
AFTER UPDATE ON activity_events
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_activity_events_delete
AFTER DELETE ON activity_events
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_tags_insert
AFTER INSERT ON tags
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_tags_update
AFTER UPDATE ON tags
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_tags_delete
AFTER DELETE ON tags
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_asset_tags_insert
AFTER INSERT ON asset_tags
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_asset_tags_update
AFTER UPDATE ON asset_tags
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_asset_tags_delete
AFTER DELETE ON asset_tags
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_relations_insert
AFTER INSERT ON relations
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_relations_update
AFTER UPDATE ON relations
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_relations_delete
AFTER DELETE ON relations
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_module_metadata_insert
AFTER INSERT ON module_metadata
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_module_metadata_update
AFTER UPDATE ON module_metadata
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;

CREATE TRIGGER change_clock_module_metadata_delete
AFTER DELETE ON module_metadata
BEGIN
    UPDATE assetmesh_change_clock SET revision = revision + 1 WHERE singleton = 1;
END;
