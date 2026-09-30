//! Create and edit reusable information items.

use crate::application::projection::project_info;
use crate::application::shared::{check_asset_revision, load_active_asset, normalize_tags};
use crate::application::{SharedClock, SharedIdGenerator};
use crate::domain::activity::{actors, event_types, ActivityEvent};
use crate::domain::asset::{Asset, AssetKind};
use crate::domain::ids::AssetId;
use crate::domain::info::{InfoRecord, InfoType};
use crate::ports::uow::{UnitOfWork, UnitOfWorkFactory};
use crate::{AppError, AppResult};
use serde_json::json;

#[derive(Debug, Clone)]
pub struct CreateInfo {
    pub name: String,
    pub info_type: InfoType,
    pub value: String,
    pub notes: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub asset_id: AssetId,
    pub expected_revision: i64,
    pub name: String,
    pub info_type: InfoType,
    pub value: String,
    pub notes: Option<String>,
    /// None preserves memberships for callers that do not edit tags.
    pub tags: Option<Vec<String>>,
}

pub struct InfoService<F: UnitOfWorkFactory> {
    factory: F,
    clock: SharedClock,
    ids: SharedIdGenerator,
}

impl<F: UnitOfWorkFactory> InfoService<F> {
    pub fn new(factory: F, clock: SharedClock, ids: SharedIdGenerator) -> Self {
        Self {
            factory,
            clock,
            ids,
        }
    }

    pub fn create(&mut self, input: CreateInfo) -> AppResult<Asset> {
        let now = self.clock.now();
        let id = AssetId::from_uuid(self.ids.new_id());
        let asset = Asset::new(id, AssetKind::InfoItem, input.name.trim(), None, now)?;
        let mut record = InfoRecord {
            asset_id: id,
            info_type: input.info_type,
            value: input.value,
            notes: input.notes,
        };
        record.validate()?;
        let tag_names = normalize_tags(&input.tags);

        self.factory.transact(&mut |uow| {
            uow.assets().insert(&asset)?;
            uow.info().upsert(&record)?;
            for name in &tag_names {
                let tag = uow.tags().ensure(name)?;
                uow.tags().attach(id, tag.id)?;
            }
            uow.activity().append(&ActivityEvent::new(
                event_types::ASSET_CREATED,
                Some(id),
                actors::USER,
                json!({ "kind": asset.kind.as_str(), "name": asset.name }),
                now,
            ))?;
            refresh_info_projection(uow, &asset, &record)?;
            Ok(asset.clone())
        })
    }

    /// Creates a bounded batch atomically. Every item is validated before any
    /// database write, and the transaction rolls back if a write fails.
    pub fn create_many(&mut self, inputs: Vec<CreateInfo>) -> AppResult<Vec<Asset>> {
        if inputs.is_empty() || inputs.len() > 500 {
            return Err(AppError::validation(
                "information import must contain 1 to 500 items",
            ));
        }
        let now = self.clock.now();
        let mut prepared = Vec::with_capacity(inputs.len());
        for input in inputs {
            let id = AssetId::from_uuid(self.ids.new_id());
            let asset = Asset::new(id, AssetKind::InfoItem, input.name.trim(), None, now)?;
            let mut record = InfoRecord {
                asset_id: id,
                info_type: input.info_type,
                value: input.value,
                notes: input.notes,
            };
            record.validate()?;
            prepared.push((asset, record, normalize_tags(&input.tags)));
        }
        self.factory.transact(&mut |uow| {
            let mut created = Vec::with_capacity(prepared.len());
            for (asset, record, tag_names) in &prepared {
                uow.assets().insert(asset)?;
                uow.info().upsert(record)?;
                for name in tag_names {
                    let tag = uow.tags().ensure(name)?;
                    uow.tags().attach(asset.id, tag.id)?;
                }
                uow.activity().append(&ActivityEvent::new(
                    event_types::ASSET_CREATED,
                    Some(asset.id),
                    actors::USER,
                    json!({ "kind": asset.kind.as_str(), "name": asset.name }),
                    now,
                ))?;
                refresh_info_projection(uow, asset, record)?;
                created.push(asset.clone());
            }
            Ok(created)
        })
    }

    pub fn update(&mut self, input: UpdateInfo) -> AppResult<Asset> {
        let now = self.clock.now();
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::validation("information name must not be empty"));
        }
        let mut record = InfoRecord {
            asset_id: input.asset_id,
            info_type: input.info_type,
            value: input.value,
            notes: input.notes,
        };
        record.validate()?;
        let tag_names = input.tags.as_ref().map(|tags| normalize_tags(tags));

        self.factory.transact(&mut |uow| {
            let mut asset = load_active_asset(uow, input.asset_id)?;
            check_asset_revision(&asset, input.expected_revision)?;
            if asset.kind != AssetKind::InfoItem {
                return Err(AppError::conflict("asset is not an information item"));
            }
            if uow.info().get(input.asset_id)?.is_none() {
                return Err(AppError::not_found("information details", input.asset_id));
            }
            asset.name = name.clone();
            asset.touch(now);
            asset.validate()?;
            uow.assets().update(&asset)?;
            uow.info().upsert(&record)?;
            if let Some(tag_names) = &tag_names {
                let existing = uow.tags().list_for_asset(asset.id)?;
                for tag in existing {
                    if !tag_names
                        .iter()
                        .any(|name| name.eq_ignore_ascii_case(&tag.name))
                    {
                        uow.tags().detach(asset.id, tag.id)?;
                    }
                }
                for name in tag_names {
                    let tag = uow.tags().ensure(name)?;
                    uow.tags().attach(asset.id, tag.id)?;
                }
            }
            uow.activity().append(&ActivityEvent::new(
                "info.updated",
                Some(asset.id),
                actors::USER,
                json!({ "kind": asset.kind.as_str() }),
                now,
            ))?;
            refresh_info_projection(uow, &asset, &record)?;
            Ok(asset)
        })
    }
}

pub(crate) fn refresh_info_projection(
    uow: &mut dyn UnitOfWork,
    asset: &Asset,
    record: &InfoRecord,
) -> AppResult<()> {
    let tags = uow.tags().list_for_asset(asset.id)?;
    let refs = uow.external_refs().list_for_asset(asset.id)?;
    uow.search_index()
        .upsert(&project_info(asset, record, &tags, &refs))
}
