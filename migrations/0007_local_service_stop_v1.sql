-- Optional explicit stop command; existing services keep process-tree stopping.
ALTER TABLE service_records ADD COLUMN stop_command TEXT;

UPDATE module_metadata SET schema_version = 3 WHERE module_id = 'services';
