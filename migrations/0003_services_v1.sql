-- AssetMesh database migration 0003: Services module V1.
--
-- Adds the Services module's typed details (module schema version 1,
-- recorded in module_metadata): durable service inventory plus subscription
-- lifecycle metadata — plan, integer-minor-unit cost, currency, billing
-- cadence, renewal/expiry, and auto-renew — as typed detail of a shared
-- Asset (docs/10, ADR 0010). A subscription is NOT a separate asset.
--
-- Existing Media/Software/Relation rows are untouched. The relations
-- stored-type CHECK is deliberately unchanged here: `hosted_on`/`points_to`
-- belong to the Phase 3C service-behavior batch and the SQL CHECK must stay
-- in lockstep with the Rust relation registry (docs/10).

CREATE TABLE service_records (
    asset_id TEXT PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    -- Kind ↔ type is 1:1 (service.saas ↔ saas, ...); the CHECK set must stay
    -- in sync with domain::service::ServiceType.
    service_type TEXT NOT NULL CHECK (service_type IN ('saas', 'api', 'vps', 'domain', 'local')),
    provider TEXT,
    account_label TEXT,
    endpoint_url TEXT,
    dashboard_url TEXT,
    domain_name TEXT,
    plan TEXT,
    -- Money is integer minor units, never floating point (ADR 0010).
    cost_minor INTEGER CHECK (cost_minor IS NULL OR cost_minor >= 0),
    -- Trimmed uppercase ASCII, exactly three letters.
    currency TEXT CHECK (currency IS NULL OR (length(currency) = 3 AND currency GLOB '[A-Z][A-Z][A-Z]')),
    billing_cadence TEXT CHECK (billing_cadence IS NULL OR billing_cadence IN ('monthly', 'quarterly', 'yearly', 'usage_based', 'one_time', 'other')),
    renews_at TEXT,
    expires_at TEXT,
    -- NULL means unknown; 0/1 means explicitly off/on.
    auto_renew INTEGER CHECK (auto_renew IS NULL OR auto_renew IN (0, 1)),
    notes TEXT,
    -- cost_minor and currency are a pair: both present or both absent.
    CHECK ((cost_minor IS NULL) = (currency IS NULL))
);

CREATE INDEX idx_service_records_type ON service_records(service_type);

INSERT INTO module_metadata (module_id, schema_version) VALUES ('services', 1);
