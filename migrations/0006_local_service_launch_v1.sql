-- AssetMesh database migration 0006: local-service launch metadata.
--
-- Adds the two launch fields of a local service (docs/10): the project
-- directory the command runs in, and the command the user starts manually.
-- Both are optional module-owned text on the existing service_records row,
-- so every pre-existing record (saas/api/vps/domain/local) keeps its data
-- unchanged and simply reads back with both fields absent.
--
-- The launch fields are typed metadata of a service_type = 'local' record
-- (domain validation rejects them on other types); the access address and
-- notes keep using endpoint_url / notes. Runtime state (running or not,
-- PID, logs) is deliberately NOT persisted: it is temporary process
-- information, never recoverable business state.
--
-- Services module schema version moves 1 -> 2 here. Old records are valid
-- version 2 records with both fields NULL.

ALTER TABLE service_records ADD COLUMN project_dir TEXT;
ALTER TABLE service_records ADD COLUMN start_command TEXT;

UPDATE module_metadata SET schema_version = 2 WHERE module_id = 'services';
