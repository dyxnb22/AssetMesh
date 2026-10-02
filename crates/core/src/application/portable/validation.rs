//! Preflight: manifest, bounded decoding, identity uniqueness, and graph integrity.
use super::budget::{validate_bundle_budget, MAX_BUNDLE_RECORDS, MAX_JSONL_LINE_BYTES};
use super::format::{
    id_from_wire, json_error, AssetTagRow, PortableActivityEventV1, PortableAssetV1,
    PortableBundle, PortableExternalRefV1, PortableInfoRecordV1, PortableManifest,
    PortableMediaRecordV1, PortableRelationV1, PortableServiceRecordV1, PortableSoftwareRecordV1,
    PortableTagV1, EXPORT_FORMAT, EXPORT_VERSION, LEGACY_SERVICES_SCHEMA_VERSION,
    MEDIA_SCHEMA_VERSION, SERVICES_SCHEMA_VERSION, SOFTWARE_SCHEMA_VERSION, V1_CORE_FILE_PATHS,
    V1_INFO_FILE_PATH, V1_MEDIA_FILE_PATH, V1_RELATIONS_FILE_PATH, V1_SERVICES_FILE_PATH,
    V1_SOFTWARE_FILE_PATH,
};
use crate::domain::activity::ActivityEvent;
use crate::domain::asset::{Asset, AssetKind};
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::{AssetId, TagId};
use crate::domain::info::{InfoRecord, SCHEMA_VERSION as INFO_SCHEMA_VERSION};
use crate::domain::media::MediaRecord;
use crate::domain::relation::Relation;
use crate::domain::service::ServiceRecord;
use crate::domain::software::SoftwareRecord;
use crate::domain::tag::Tag;
use crate::{AppError, AppResult};
use std::collections::{HashMap, HashSet};

pub(super) struct DecodedBundle {
    pub(super) assets: Vec<Asset>,
    pub(super) media: Vec<MediaRecord>,
    /// Empty when the bundle predates the Software module (undeclared).
    pub(super) software: Vec<SoftwareRecord>,
    /// Empty when the bundle predates the Services module (undeclared).
    pub(super) services: Vec<ServiceRecord>,
    pub(super) info: Vec<InfoRecord>,
    pub(super) refs: Vec<AssetExternalRef>,
    pub(super) activity: Vec<ActivityEvent>,
    pub(super) tags: Vec<Tag>,
    pub(super) asset_tags: Vec<AssetTagRow>,
    /// Empty when the bundle predates Relations (undeclared).
    pub(super) relations: Vec<Relation>,
    /// True when the manifest declares the software module. Declared
    /// sections are authoritative on import (reconciliation applies);
    /// undeclared sections are legacy compatibility and leave destination
    /// state untouched.
    pub(super) software_declared: bool,
    /// Same contract for the Services module (docs/10 "Portable data"): a
    /// declared section is authoritative, an undeclared one means the bundle
    /// predates Services and must not erase destination service data.
    pub(super) services_declared: bool,
    pub(super) info_declared: bool,
    pub(super) relations_declared: bool,
}

#[derive(Clone, Copy)]
struct DeclaredSections {
    software_declared: bool,
    services_declared: bool,
    info_declared: bool,
    relations_declared: bool,
}

pub(super) fn preflight(bundle: &PortableBundle) -> AppResult<DecodedBundle> {
    validate_bundle_budget(bundle)?;
    let sections = validate_manifest(&bundle.manifest)?;
    let contents = read_sections(bundle, sections)?;
    let decoded = decode_records(&bundle.manifest, sections, contents)?;
    let index = validate_identities(&decoded)?;
    validate_graph(&decoded, &index)?;
    Ok(decoded)
}

fn validate_manifest(manifest: &PortableManifest) -> AppResult<DeclaredSections> {
    if manifest.format != EXPORT_FORMAT {
        return Err(AppError::unsupported_schema_version(
            "portable export format",
            manifest.format.as_str(),
            EXPORT_FORMAT,
        ));
    }
    if manifest.version != EXPORT_VERSION {
        return Err(AppError::unsupported_schema_version(
            "portable export format",
            manifest.version,
            format!("version {EXPORT_VERSION}"),
        ));
    }
    match manifest.modules.get("media") {
        Some(module) if module.schema_version == MEDIA_SCHEMA_VERSION => {}
        Some(module) => {
            return Err(AppError::unsupported_schema_version(
                "media module",
                module.schema_version,
                format!("schema_version {MEDIA_SCHEMA_VERSION}"),
            ))
        }
        None => {
            return Err(AppError::validation(
                "bundle is missing the media module data",
            ))
        }
    }

    // Software module: declared → required and authoritative; undeclared →
    // a legacy (Phase 1) bundle predating Software, whose section is empty.
    let software_declared = match manifest.modules.get("software") {
        Some(module) if module.schema_version == SOFTWARE_SCHEMA_VERSION => true,
        Some(module) => {
            return Err(AppError::unsupported_schema_version(
                "software module",
                module.schema_version,
                format!("schema_version {SOFTWARE_SCHEMA_VERSION}"),
            ))
        }
        None => false,
    };
    // Services module: same declaration contract (docs/10 "Portable data").
    // Declared → required, count-checked, and authoritative; undeclared → a
    // bundle predating Services, which must not erase destination service
    // state; a section file without the declaration is corruption.
    // Schema version 1 is accepted beside the current version: the launch
    // fields added in version 2 are optional and defaulted on read, so a
    // legacy bundle is a strict subset of the current wire shape.
    let services_declared = match manifest.modules.get("services") {
        Some(module)
            if module.schema_version == SERVICES_SCHEMA_VERSION
                || module.schema_version == 2
                || module.schema_version == LEGACY_SERVICES_SCHEMA_VERSION =>
        {
            true
        }
        Some(module) => {
            return Err(AppError::unsupported_schema_version(
                "services module",
                module.schema_version,
                format!("schema_version {SERVICES_SCHEMA_VERSION}"),
            ))
        }
        None => false,
    };
    let info_declared = match manifest.modules.get("info") {
        Some(module) if module.schema_version == INFO_SCHEMA_VERSION => true,
        Some(module) => {
            return Err(AppError::unsupported_schema_version(
                "info module",
                module.schema_version,
                format!("schema_version {INFO_SCHEMA_VERSION}"),
            ))
        }
        None => false,
    };
    let relations_declared = manifest.record_counts.contains_key("relations");

    Ok(DeclaredSections {
        software_declared,
        services_declared,
        info_declared,
        relations_declared,
    })
}

fn read_sections(
    bundle: &PortableBundle,
    sections: DeclaredSections,
) -> AppResult<HashMap<&str, &str>> {
    let DeclaredSections {
        software_declared,
        services_declared,
        info_declared,
        relations_declared,
    } = sections;
    // Core files: a v1 bundle declares its full core shape; missing files
    // are corruption, not empty collections. Undeclared module sections
    // must NOT have a file present.
    let mut file_content = HashMap::new();
    for path in V1_CORE_FILE_PATHS {
        let content = bundle.file(path).ok_or_else(|| {
            AppError::validation(format!("bundle is missing required file {path:?}"))
        })?;
        file_content.insert(path, content);
    }
    let media_content = bundle.file(V1_MEDIA_FILE_PATH).ok_or_else(|| {
        AppError::validation(format!(
            "bundle is missing required file {V1_MEDIA_FILE_PATH:?}"
        ))
    })?;
    file_content.insert(V1_MEDIA_FILE_PATH, media_content);
    if software_declared {
        let content = bundle.file(V1_SOFTWARE_FILE_PATH).ok_or_else(|| {
            AppError::validation(format!(
                "bundle declares the software module but is missing required file \
                 {V1_SOFTWARE_FILE_PATH:?}"
            ))
        })?;
        file_content.insert(V1_SOFTWARE_FILE_PATH, content);
    } else if bundle.file(V1_SOFTWARE_FILE_PATH).is_some() {
        return Err(AppError::validation(
            "bundle contains modules/software.jsonl but its manifest does not declare \
             the software module",
        ));
    }
    if services_declared {
        let content = bundle.file(V1_SERVICES_FILE_PATH).ok_or_else(|| {
            AppError::validation(format!(
                "bundle declares the services module but is missing required file \
                 {V1_SERVICES_FILE_PATH:?}"
            ))
        })?;
        file_content.insert(V1_SERVICES_FILE_PATH, content);
    } else if bundle.file(V1_SERVICES_FILE_PATH).is_some() {
        return Err(AppError::validation(
            "bundle contains modules/services.jsonl but its manifest does not declare \
             the services module",
        ));
    }
    if info_declared {
        let content = bundle.file(V1_INFO_FILE_PATH).ok_or_else(|| {
            AppError::validation("bundle declares info but is missing modules/info.jsonl")
        })?;
        file_content.insert(V1_INFO_FILE_PATH, content);
    } else if bundle.file(V1_INFO_FILE_PATH).is_some() {
        return Err(AppError::validation(
            "bundle contains modules/info.jsonl without declaring info",
        ));
    }
    if relations_declared {
        let content = bundle.file(V1_RELATIONS_FILE_PATH).ok_or_else(|| {
            AppError::validation(format!(
                "bundle declares relations but is missing required file \
                 {V1_RELATIONS_FILE_PATH:?}"
            ))
        })?;
        file_content.insert(V1_RELATIONS_FILE_PATH, content);
    } else if bundle.file(V1_RELATIONS_FILE_PATH).is_some() {
        return Err(AppError::validation(
            "bundle contains relations.jsonl but its manifest does not declare a \
             relations count",
        ));
    }

    Ok(file_content)
}

fn decode_records(
    manifest: &PortableManifest,
    sections: DeclaredSections,
    file_content: HashMap<&str, &str>,
) -> AppResult<DecodedBundle> {
    let DeclaredSections {
        software_declared,
        services_declared,
        info_declared,
        relations_declared,
    } = sections;
    // Decode + verify declared record counts (catches silent truncation).
    let count_of = |name: &str| -> AppResult<usize> {
        manifest.record_counts.get(name).copied().ok_or_else(|| {
            AppError::validation(format!("manifest is missing record count for {name:?}"))
        })
    };

    let asset_rows: Vec<PortableAssetV1> =
        decode_jsonl(file_content["assets.jsonl"], "assets.jsonl")?;
    if asset_rows.len() != count_of("assets")? {
        return Err(AppError::validation(format!(
            "bundle count mismatch: manifest declares {} assets, found {}",
            count_of("assets")?,
            asset_rows.len()
        )));
    }
    let media_rows: Vec<PortableMediaRecordV1> =
        decode_jsonl(file_content["modules/media.jsonl"], "modules/media.jsonl")?;
    if media_rows.len() != count_of("media")? {
        return Err(AppError::validation(format!(
            "bundle count mismatch: manifest declares {} media records, found {}",
            count_of("media")?,
            media_rows.len()
        )));
    }
    let software_rows: Vec<PortableSoftwareRecordV1> = if software_declared {
        let rows = decode_jsonl(
            file_content["modules/software.jsonl"],
            "modules/software.jsonl",
        )?;
        if rows.len() != count_of("software")? {
            return Err(AppError::validation(format!(
                "bundle count mismatch: manifest declares {} software records, found {}",
                count_of("software")?,
                rows.len()
            )));
        }
        rows
    } else {
        Vec::new()
    };
    let service_rows: Vec<PortableServiceRecordV1> = if services_declared {
        let rows = decode_jsonl(
            file_content["modules/services.jsonl"],
            "modules/services.jsonl",
        )?;
        if rows.len() != count_of("services")? {
            return Err(AppError::validation(format!(
                "bundle count mismatch: manifest declares {} service records, found {}",
                count_of("services")?,
                rows.len()
            )));
        }
        rows
    } else {
        Vec::new()
    };
    let info_rows: Vec<PortableInfoRecordV1> = if info_declared {
        let rows = decode_jsonl(file_content[V1_INFO_FILE_PATH], V1_INFO_FILE_PATH)?;
        if rows.len() != count_of("info")? {
            return Err(AppError::validation(
                "bundle count mismatch for info records",
            ));
        }
        rows
    } else {
        Vec::new()
    };
    let relation_rows: Vec<PortableRelationV1> = if relations_declared {
        let rows = decode_jsonl(file_content["relations.jsonl"], "relations.jsonl")?;
        if rows.len() != count_of("relations")? {
            return Err(AppError::validation(format!(
                "bundle count mismatch: manifest declares {} relations, found {}",
                count_of("relations")?,
                rows.len()
            )));
        }
        rows
    } else {
        Vec::new()
    };
    let ref_rows: Vec<PortableExternalRefV1> =
        decode_jsonl(file_content["external_refs.jsonl"], "external_refs.jsonl")?;
    if ref_rows.len() != count_of("external_refs")? {
        return Err(AppError::validation(format!(
            "bundle count mismatch: manifest declares {} external refs, found {}",
            count_of("external_refs")?,
            ref_rows.len()
        )));
    }
    let activity_rows: Vec<PortableActivityEventV1> =
        decode_jsonl(file_content["activity.jsonl"], "activity.jsonl")?;
    if activity_rows.len() != count_of("activity")? {
        return Err(AppError::validation(format!(
            "bundle count mismatch: manifest declares {} activity events, found {}",
            count_of("activity")?,
            activity_rows.len()
        )));
    }
    let tag_rows: Vec<PortableTagV1> =
        decode_json_array(file_content["tags.json"], count_of("tags")?)?;
    if tag_rows.len() != count_of("tags")? {
        return Err(AppError::validation(format!(
            "bundle count mismatch: manifest declares {} tags, found {}",
            count_of("tags")?,
            tag_rows.len()
        )));
    }
    let asset_tag_rows: Vec<AssetTagRow> =
        decode_jsonl(file_content["asset_tags.jsonl"], "asset_tags.jsonl")?;
    if asset_tag_rows.len() != count_of("asset_tags")? {
        return Err(AppError::validation(format!(
            "bundle count mismatch: manifest declares {} asset tags, found {}",
            count_of("asset_tags")?,
            asset_tag_rows.len()
        )));
    }

    // Convert to domain and validate each row.
    let mut assets = Vec::with_capacity(asset_rows.len());
    for row in asset_rows {
        let asset = row.into_domain()?;
        asset
            .validate()
            .map_err(|e| AppError::validation(format!("bundle asset {}: {e}", asset.id)))?;
        assets.push(asset);
    }
    let mut media = Vec::with_capacity(media_rows.len());
    for row in media_rows {
        let record = row.into_domain()?;
        record.validate().map_err(|e| {
            AppError::validation(format!("bundle media record {}: {e}", record.asset_id))
        })?;
        media.push(record);
    }
    let mut software = Vec::with_capacity(software_rows.len());
    for row in software_rows {
        let mut record = row.into_domain()?;
        // Normalize in place: trims free text and rejects control characters
        // (including user-owned purpose/notes) before the value is stored, so
        // a bundle cannot persist raw unnormalized text into canonical state.
        record.validate().map_err(|e| {
            AppError::validation(format!("bundle software record {}: {e}", record.asset_id))
        })?;
        software.push(record);
    }
    let mut services = Vec::with_capacity(service_rows.len());
    for row in service_rows {
        let mut record = row.into_domain()?;
        // The same single canonicalization point as every other write path:
        // text normalization, URL shape, domain-name rules, money pairing, and
        // currency form are all enforced here, before any mutation.
        record.validate().map_err(|e| {
            AppError::validation(format!("bundle service record {}: {e}", record.asset_id))
        })?;
        services.push(record);
    }
    let mut info = Vec::with_capacity(info_rows.len());
    for row in info_rows {
        let mut record = row.into_domain()?;
        record.validate().map_err(|e| {
            AppError::validation(format!("bundle info record {}: {e}", record.asset_id))
        })?;
        info.push(record);
    }
    let mut relations = Vec::with_capacity(relation_rows.len());
    for row in relation_rows {
        let relation = row.into_domain()?;
        relation
            .validate()
            .map_err(|e| AppError::validation(format!("bundle relation {}: {e}", relation.id)))?;
        // Normalize to canonical storage form so a fact has exactly one
        // representation (mirrors the application-layer write path): inverse
        // pair types collapse onto their primary direction and symmetric
        // endpoints are ordered. The later duplicate-triple check therefore
        // also rejects bundles carrying both statements of one fact.
        let relation = relation.canonical_form();
        relations.push(relation);
    }
    let mut refs = Vec::with_capacity(ref_rows.len());
    for row in ref_rows {
        let reference = row.into_domain()?;
        reference.validate().map_err(|e| {
            AppError::validation(format!("bundle external ref {}: {e}", reference.id))
        })?;
        refs.push(reference);
    }
    let mut activity = Vec::with_capacity(activity_rows.len());
    for row in activity_rows {
        let event = row.into_domain()?;
        event.validate()?;
        activity.push(event);
    }
    let mut tags = Vec::with_capacity(tag_rows.len());
    for row in tag_rows {
        let tag = row.into_domain()?;
        tag.validate()
            .map_err(|e| AppError::validation(format!("bundle tag {}: {e}", tag.id)))?;
        tags.push(tag);
    }

    Ok(DecodedBundle {
        assets,
        media,
        software,
        services,
        info,
        refs,
        activity,
        tags,
        asset_tags: asset_tag_rows,
        relations,
        software_declared,
        services_declared,
        info_declared,
        relations_declared,
    })
}
struct BundleIndex<'a> {
    assets: HashMap<AssetId, &'a Asset>,
    tags: HashSet<TagId>,
}

fn validate_identities(decoded: &DecodedBundle) -> AppResult<BundleIndex<'_>> {
    let DecodedBundle {
        assets,
        media,
        software,
        services,
        info,
        refs,
        activity,
        tags,
        relations,
        ..
    } = decoded;
    // Uniqueness of identities.
    let mut asset_ids = HashSet::new();
    for asset in assets {
        if !asset_ids.insert(asset.id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate asset id {}",
                asset.id
            )));
        }
    }
    let mut media_ids = HashSet::new();
    for record in media {
        if !media_ids.insert(record.asset_id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate media record for asset {}",
                record.asset_id
            )));
        }
    }
    let mut software_ids = HashSet::new();
    for record in software {
        if !software_ids.insert(record.asset_id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate software record for asset {}",
                record.asset_id
            )));
        }
    }
    let mut service_ids = HashSet::new();
    for record in services {
        if !service_ids.insert(record.asset_id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate service record for asset {}",
                record.asset_id
            )));
        }
    }
    let mut info_ids = HashSet::new();
    for record in info {
        if !info_ids.insert(record.asset_id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate info record for asset {}",
                record.asset_id
            )));
        }
    }
    let mut relation_ids = HashSet::new();
    let mut relation_triples = HashSet::new();
    for relation in relations {
        if !relation_ids.insert(relation.id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate relation id {}",
                relation.id
            )));
        }
        let triple = (
            relation.source_asset_id,
            relation.target_asset_id,
            relation.relation_type,
        );
        if !relation_triples.insert(triple) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate relation {} between {} and {}",
                relation.relation_type, relation.source_asset_id, relation.target_asset_id
            )));
        }
    }
    let mut ref_keys = HashSet::new();
    let mut ref_ids = HashSet::new();
    for reference in refs {
        if !ref_keys.insert((reference.namespace.clone(), reference.external_id.clone())) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate external ref {}:{}",
                reference.namespace, reference.external_id
            )));
        }
        if !ref_ids.insert(reference.id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate external ref id {}",
                reference.id
            )));
        }
    }
    let mut event_ids = HashSet::new();
    for event in activity {
        if !event_ids.insert(event.id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate activity event id {}",
                event.id
            )));
        }
    }
    let mut tag_ids = HashSet::new();
    let mut tag_names = HashSet::new();
    for tag in tags {
        if !tag_ids.insert(tag.id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate tag id {}",
                tag.id
            )));
        }
        if !tag_names.insert(tag.name.clone()) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate tag name {:?}",
                tag.name
            )));
        }
    }

    Ok(BundleIndex {
        assets: assets.iter().map(|asset| (asset.id, asset)).collect(),
        tags: tag_ids,
    })
}

fn validate_graph(decoded: &DecodedBundle, index: &BundleIndex<'_>) -> AppResult<()> {
    let DecodedBundle {
        media,
        software,
        services,
        info,
        refs,
        activity,
        relations,
        asset_tags: asset_tag_rows,
        ..
    } = decoded;
    // Cross-record graph invariants.
    for record in media {
        let asset = index.assets.get(&record.asset_id).ok_or_else(|| {
            AppError::validation(format!(
                "bundle media record references missing asset {}",
                record.asset_id
            ))
        })?;
        if asset.kind != record.media_type.asset_kind() {
            return Err(AppError::validation(format!(
                "bundle asset {} has kind {} but its media record is a {}",
                asset.id, asset.kind, record.media_type
            )));
        }
    }
    for record in software {
        let asset = index.assets.get(&record.asset_id).ok_or_else(|| {
            AppError::validation(format!(
                "bundle software record references missing asset {}",
                record.asset_id
            ))
        })?;
        if asset.kind != record.category.asset_kind() {
            return Err(AppError::validation(format!(
                "bundle asset {} has kind {} but its software record is a {}",
                asset.id, asset.kind, record.category
            )));
        }
    }
    for record in services {
        let asset = index.assets.get(&record.asset_id).ok_or_else(|| {
            AppError::validation(format!(
                "bundle service record references missing asset {}",
                record.asset_id
            ))
        })?;
        // Kind ↔ type is 1:1 (docs/10): a bundle cannot attach a saas record
        // to a service.api asset, or any service record to a media/software
        // asset. The repository boundary enforces this too, but preflight must
        // fail identically for dry-run and commit, before any mutation.
        if asset.kind != record.service_type.asset_kind() {
            return Err(AppError::validation(format!(
                "bundle asset {} has kind {} but its service record is a {}",
                asset.id, asset.kind, record.service_type
            )));
        }
        // A merged identity carries no module detail (docs/10): the portable
        // contract has no "re-home this row onto the winner" rule, so a service
        // row on a tombstone is bundle corruption — not something import may
        // silently keep (stranding detail on a dead identity) or silently drop
        // (losing a fact the caller meant to move). The merge use case always
        // removes the loser's record, so a well-formed export never emits one.
        if asset.lifecycle_state == crate::domain::asset::LifecycleState::Merged {
            return Err(AppError::validation(format!(
                "bundle service record {} is attached to merged asset {}; a merged identity \
                 carries no service detail",
                record.asset_id, asset.id
            )));
        }
    }
    for record in info {
        let asset = index.assets.get(&record.asset_id).ok_or_else(|| {
            AppError::validation(format!(
                "bundle info record references missing asset {}",
                record.asset_id
            ))
        })?;
        if asset.kind != AssetKind::InfoItem
            || asset.lifecycle_state == crate::domain::asset::LifecycleState::Merged
        {
            return Err(AppError::validation(format!(
                "bundle info record {} has incompatible asset kind or lifecycle",
                record.asset_id
            )));
        }
    }
    for relation in relations {
        if !index.assets.contains_key(&relation.source_asset_id) {
            return Err(AppError::validation(format!(
                "bundle relation {} references missing source asset {}",
                relation.id, relation.source_asset_id
            )));
        }
        if !index.assets.contains_key(&relation.target_asset_id) {
            return Err(AppError::validation(format!(
                "bundle relation {} references missing target asset {}",
                relation.id, relation.target_asset_id
            )));
        }
    }
    for reference in refs {
        if !index.assets.contains_key(&reference.asset_id) {
            return Err(AppError::validation(format!(
                "bundle external ref {}:{} references missing asset {}",
                reference.namespace, reference.external_id, reference.asset_id
            )));
        }
    }
    for event in activity {
        if let Some(asset_id) = event.asset_id {
            if !index.assets.contains_key(&asset_id) {
                return Err(AppError::validation(format!(
                    "bundle activity event {} references missing asset {}",
                    event.id, asset_id
                )));
            }
        }
    }
    for pair in asset_tag_rows {
        let asset_id = AssetId::from_uuid(id_from_wire(&pair.asset_id, "asset_tags")?);
        let tag_id = TagId::from_uuid(id_from_wire(&pair.tag_id, "asset_tags")?);
        if !index.assets.contains_key(&asset_id) {
            return Err(AppError::validation(format!(
                "bundle asset_tags references missing asset {}",
                pair.asset_id
            )));
        }
        if !index.tags.contains(&tag_id) {
            return Err(AppError::validation(format!(
                "bundle asset_tags references missing tag {}",
                pair.tag_id
            )));
        }
    }

    validate_redirects(&decoded.assets, &index.assets)
}

fn validate_redirects(assets: &[Asset], index: &HashMap<AssetId, &Asset>) -> AppResult<()> {
    for asset in assets {
        if let Some(target) = asset.merged_into {
            if target == asset.id {
                return Err(AppError::validation(format!(
                    "bundle asset {} is merged into itself",
                    asset.id
                )));
            }
            if !index.contains_key(&target) {
                return Err(AppError::validation(format!(
                    "bundle asset {} is merged into missing asset {}",
                    asset.id, target
                )));
            }
        }
    }
    // Every accepted path is marked once. Shared tails are never walked again.
    let mut checked = HashSet::new();
    for asset in assets {
        let mut visiting = HashSet::new();
        let mut cursor = asset.id;
        while !checked.contains(&cursor) {
            if !visiting.insert(cursor) {
                return Err(AppError::validation(format!(
                    "bundle contains a merge redirect cycle at asset {}",
                    cursor
                )));
            }
            match index.get(&cursor).and_then(|asset| asset.merged_into) {
                Some(next) => cursor = next,
                None => break,
            }
        }
        checked.extend(visiting);
    }
    Ok(())
}

fn decode_jsonl<T: serde::de::DeserializeOwned>(text: &str, path: &str) -> AppResult<Vec<T>> {
    let mut rows = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if rows.len() >= MAX_BUNDLE_RECORDS || line.len() > MAX_JSONL_LINE_BYTES {
            return Err(AppError::validation(format!(
                "bundle file {path} exceeds the record or line size limit"
            )));
        }
        rows.push(serde_json::from_str(line).map_err(|e| {
            AppError::validation(format!(
                "bundle file {path} line {} is invalid: {e}",
                line_no + 1
            ))
        })?);
    }
    Ok(rows)
}

fn decode_json_array<T: serde::de::DeserializeOwned>(
    text: &str,
    limit: usize,
) -> AppResult<Vec<T>> {
    struct BoundedArray<T> {
        limit: usize,
        item: std::marker::PhantomData<T>,
    }
    impl<'de, T: serde::de::DeserializeOwned> serde::de::Visitor<'de> for BoundedArray<T> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an array within the declared record count")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut rows = Vec::new();
            while rows.len() < self.limit {
                let Some(row) = sequence.next_element()? else {
                    return Ok(rows);
                };
                rows.push(row);
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom(
                    "tag array exceeds the declared record count",
                ));
            }
            Ok(rows)
        }
    }
    let mut decoder = serde_json::Deserializer::from_str(text);
    let rows = serde::Deserializer::deserialize_seq(
        &mut decoder,
        BoundedArray {
            limit,
            item: std::marker::PhantomData,
        },
    )
    .map_err(json_error)?;
    decoder.end().map_err(json_error)?;
    Ok(rows)
}
