//! SQLite implementation of `ServiceRepository`.

use crate::repos::{row_result, ts_from_string, ts_to_string, uuid_from_string, uuid_to_string};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::service::{BillingCadence, ServiceEntry, ServiceRecord, ServiceType};
use assetmesh_core::ports::repos::{
    ServiceFilter, ServiceListRow, ServiceReader, ServiceRepository, ServiceSort,
};
use assetmesh_core::{AppError, AppResult};
use rusqlite::Connection;

pub struct SqliteServiceRepo<'conn> {
    pub(crate) conn: &'conn Connection,
}

const ASSET_COLS: &str =
    "a.id, a.kind, a.name, a.summary, a.lifecycle_state, a.revision, a.created_at, \
     a.updated_at, a.archived_at, a.merged_into_asset_id";
pub(crate) const SERVICE_COLS: &str =
    "s.service_type, s.provider, s.account_label, s.endpoint_url, s.dashboard_url, \
     s.domain_name, s.plan, s.cost_minor, s.currency, s.billing_cadence, s.renews_at, \
     s.expires_at, s.auto_renew, s.notes";

fn col<T: rusqlite::types::FromSql>(row: &rusqlite::Row, idx: usize) -> AppResult<T> {
    row.get(idx).map_err(crate::map_error)
}

pub(crate) fn parse_record(asset_id: AssetId, row: &rusqlite::Row, offset: usize) -> AppResult<ServiceRecord> {
    let service_type: String = col(row, offset)?;
    let provider: Option<String> = col(row, offset + 1)?;
    let account_label: Option<String> = col(row, offset + 2)?;
    let endpoint_url: Option<String> = col(row, offset + 3)?;
    let dashboard_url: Option<String> = col(row, offset + 4)?;
    let domain_name: Option<String> = col(row, offset + 5)?;
    let plan: Option<String> = col(row, offset + 6)?;
    let cost_minor: Option<i64> = col(row, offset + 7)?;
    let currency: Option<String> = col(row, offset + 8)?;
    let billing_cadence: Option<String> = col(row, offset + 9)?;
    let renews_at: Option<String> = col(row, offset + 10)?;
    let expires_at: Option<String> = col(row, offset + 11)?;
    // Stored as NULL / 0 / 1 per the migration CHECK.
    let auto_renew: Option<i64> = col(row, offset + 12)?;
    let notes: Option<String> = col(row, offset + 13)?;

    Ok(ServiceRecord {
        asset_id,
        service_type: ServiceType::parse(&service_type).ok_or_else(|| {
            AppError::storage(format!("unknown stored service type: {service_type}"))
        })?,
        provider,
        account_label,
        endpoint_url,
        dashboard_url,
        domain_name,
        plan,
        cost_minor,
        currency,
        billing_cadence: billing_cadence
            .as_deref()
            .map(|raw| {
                BillingCadence::parse(raw).ok_or_else(|| {
                    AppError::storage(format!("unknown stored billing cadence: {raw}"))
                })
            })
            .transpose()?,
        renews_at: renews_at.as_deref().map(ts_from_string).transpose()?,
        expires_at: expires_at.as_deref().map(ts_from_string).transpose()?,
        auto_renew: auto_renew.map(|value| value != 0),
        notes,
    })
}

fn record_params(record: &ServiceRecord) -> Vec<Box<dyn rusqlite::ToSql>> {
    vec![
        Box::new(uuid_to_string(record.asset_id.as_uuid())),
        Box::new(record.service_type.as_str().to_string()),
        Box::new(record.provider.clone()),
        Box::new(record.account_label.clone()),
        Box::new(record.endpoint_url.clone()),
        Box::new(record.dashboard_url.clone()),
        Box::new(record.domain_name.clone()),
        Box::new(record.plan.clone()),
        Box::new(record.cost_minor),
        Box::new(record.currency.clone()),
        Box::new(record.billing_cadence.map(|c| c.as_str().to_string())),
        Box::new(record.renews_at.map(ts_to_string)),
        Box::new(record.expires_at.map(ts_to_string)),
        Box::new(record.auto_renew.map(|value| value as i64)),
        Box::new(record.notes.clone()),
    ]
}

impl SqliteServiceRepo<'_> {
    /// Enforces service_type ↔ asset-kind compatibility at the storage
    /// boundary so even direct UnitOfWork writes cannot attach a service
    /// record to an incompatible asset (a SaaS record on a `service.api`
    /// asset, or any service record on a media/software asset). An asset that
    /// does not exist yet is left to the foreign key.
    fn ensure_type_matches_asset(&self, record: &ServiceRecord) -> AppResult<()> {
        let kind: Option<String> = self
            .conn
            .query_row(
                "SELECT kind FROM assets WHERE id = ?1",
                [uuid_to_string(record.asset_id.as_uuid())],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .map_err(crate::map_error)?;
        if let Some(kind) = kind {
            let expected = record.service_type.asset_kind().as_str();
            if kind != expected {
                return Err(AppError::conflict(format!(
                    "service record type {} requires asset kind {expected}, but asset {} has kind {kind}",
                    record.service_type, record.asset_id
                )));
            }
        }
        Ok(())
    }
}

impl ServiceReader for SqliteServiceRepo<'_> {
    fn get(&mut self, asset_id: AssetId) -> AppResult<Option<ServiceRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT s.service_type, s.provider, s.account_label, s.endpoint_url, \
                      s.dashboard_url, s.domain_name, s.plan, s.cost_minor, s.currency, \
                      s.billing_cadence, s.renews_at, s.expires_at, s.auto_renew, s.notes \
                      FROM service_records s WHERE s.asset_id = ?1",
            )
            .map_err(crate::map_error)?;
        let mut rows = stmt
            .query_map([uuid_to_string(asset_id.as_uuid())], |row| {
                crate::repos::app_row(parse_record(asset_id, row, 0))
            })
            .map_err(crate::map_error)?;
        match rows.next() {
            Some(row) => Ok(Some(row_result(row)?)),
            None => Ok(None),
        }
    }

    fn list(&mut self, filter: &ServiceFilter) -> AppResult<Vec<ServiceListRow>> {
        let mut sql = format!(
            "SELECT {ASSET_COLS}, {SERVICE_COLS} FROM service_records s \
             JOIN assets a ON a.id = s.asset_id WHERE 1=1"
        );
        let mut params: Vec<String> = Vec::new();

        if let Some(service_type) = filter.service_type {
            params.push(service_type.as_str().to_string());
            sql.push_str(&format!(" AND s.service_type = ?{}", params.len()));
        }
        if let Some(provider) = &filter.provider {
            // Substring match with escaped wildcards so a provider name
            // containing `%` or `_` matches literally.
            let escaped = provider
                .to_lowercase()
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            params.push(format!("%{escaped}%"));
            sql.push_str(&format!(
                " AND lower(s.provider) LIKE ?{} ESCAPE '\\'",
                params.len()
            ));
        }
        if let Some(tag) = &filter.tag {
            params.push(tag.to_lowercase());
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM asset_tags ft JOIN tags t ON t.id = ft.tag_id \
                 WHERE ft.asset_id = a.id AND lower(t.name) = ?{})",
                params.len()
            ));
        }

        match filter.sort {
            ServiceSort::UpdatedDesc => sql.push_str(" ORDER BY a.updated_at DESC, a.id DESC"),
            ServiceSort::TitleAsc => sql.push_str(" ORDER BY a.name COLLATE NOCASE ASC, a.id ASC"),
            // Services without a renewal date sort last so the upcoming
            // renewals come first.
            ServiceSort::RenewsAsc => sql.push_str(
                " ORDER BY s.renews_at IS NULL, s.renews_at ASC, a.name COLLATE NOCASE ASC",
            ),
        }

        let mut stmt = self.conn.prepare(&sql).map_err(crate::map_error)?;
        let refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        let rows = stmt
            .query_map(refs.as_slice(), |row| {
                let asset = crate::repos::asset::row_to_asset(row)?;
                // asset columns consumed 0..10; service columns start at 10
                let record = crate::repos::app_row(parse_record(asset.id, row, 10))?;
                Ok((asset, record))
            })
            .map_err(crate::map_error)?;

        let mut rows_with_assets = Vec::new();
        for row in rows {
            let (asset, record) = row.map_err(crate::map_error)?;
            rows_with_assets.push((asset, record));
        }

        // Tags for the whole page in one query (see `repos::batch_tags`): a
        // per-row query here made the list O(rows) round-trips.
        let asset_ids: Vec<AssetId> = rows_with_assets.iter().map(|(a, _)| a.id).collect();
        let tags_by_asset = crate::repos::batch_tags(self.conn, &asset_ids)?;

        let mut out = Vec::with_capacity(rows_with_assets.len());
        for (asset, record) in rows_with_assets {
            let tags = tags_by_asset.get(&asset.id).cloned().unwrap_or_default();
            out.push(ServiceListRow {
                entry: ServiceEntry { asset, record },
                tags,
            });
        }
        Ok(out)
    }

    fn list_all(&mut self) -> AppResult<Vec<ServiceRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT s.asset_id, s.service_type, s.provider, s.account_label, s.endpoint_url, \
                      s.dashboard_url, s.domain_name, s.plan, s.cost_minor, s.currency, \
                      s.billing_cadence, s.renews_at, s.expires_at, s.auto_renew, s.notes \
                      FROM service_records s ORDER BY s.asset_id",
            )
            .map_err(crate::map_error)?;
        let rows = stmt
            .query_map([], |row| {
                let asset_id_str: String = row.get(0)?;
                let asset_id = AssetId::from_uuid(
                    uuid_from_string(&asset_id_str).map_err(crate::repos::box_app_error)?,
                );
                crate::repos::app_row(parse_record(asset_id, row, 1))
            })
            .map_err(crate::map_error)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row_result(row)?);
        }
        Ok(records)
    }
}

impl ServiceRepository for SqliteServiceRepo<'_> {
    fn upsert(&mut self, record: &ServiceRecord) -> AppResult<()> {
        // Enforce the invariants at the repository boundary too: a direct
        // UnitOfWork write must not bypass normalization. The port takes the
        // record by shared reference, so the normalized copy is what is
        // stored while callers keep their own (already canonical) value.
        let mut normalized = record.clone();
        normalized.validate()?;
        self.ensure_type_matches_asset(&normalized)?;
        let params = record_params(&normalized);
        let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        self.conn
            .execute(
                "INSERT INTO service_records (asset_id, service_type, provider, account_label, \
                 endpoint_url, dashboard_url, domain_name, plan, cost_minor, currency, \
                 billing_cadence, renews_at, expires_at, auto_renew, notes) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15) \
                 ON CONFLICT(asset_id) DO UPDATE SET service_type=?2, provider=?3, \
                 account_label=?4, endpoint_url=?5, dashboard_url=?6, domain_name=?7, plan=?8, \
                 cost_minor=?9, currency=?10, billing_cadence=?11, renews_at=?12, \
                 expires_at=?13, auto_renew=?14, notes=?15",
                refs.as_slice(),
            )
            .map_err(crate::map_error)?;
        Ok(())
    }

    fn delete(&mut self, asset_id: AssetId) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM service_records WHERE asset_id = ?1",
                [uuid_to_string(asset_id.as_uuid())],
            )
            .map_err(crate::map_error)?;
        Ok(())
    }
}
