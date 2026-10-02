-- Extend the existing indexes with the stable page tie-breaker. Replacing
-- them avoids keeping redundant time-only indexes alongside paging indexes.
DROP INDEX idx_activity_time;
CREATE INDEX idx_activity_time ON activity_events(occurred_at, id);
DROP INDEX idx_activity_asset;
CREATE INDEX idx_activity_asset ON activity_events(asset_id, occurred_at, id);
