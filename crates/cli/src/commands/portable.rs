//! Portable bundle commands: the whole-bundle export and import paths.

use crate::commands::SharedFactory;
use assetmesh_core::application::portable::{
    read_bundle_from_directory, PortableExportService, PortableImportService,
};
use assetmesh_core::{AppError, SharedClock};
use std::path::PathBuf;

pub(crate) fn run_export(
    factory: SharedFactory,
    clock: SharedClock,
    dir: PathBuf,
    include_api_keys: bool,
) -> Result<(), AppError> {
    let mut service = PortableExportService::new(factory, clock);
    let manifest =
        service.export_to_directory(env!("CARGO_PKG_VERSION"), &dir, include_api_keys)?;
    if include_api_keys {
        eprintln!("This export includes API key values and is not encrypted. Keep it on encrypted storage.");
    }
    println!("exported portable bundle to {}", dir.display());
    for (name, count) in &manifest.record_counts {
        println!("  {name}: {count}");
    }
    Ok(())
}

pub(crate) fn run_import(
    factory: SharedFactory,
    path: PathBuf,
    dry_run: bool,
) -> Result<(), AppError> {
    let bundle = read_bundle_from_directory(&path)?;
    let mut service = PortableImportService::new(factory);
    let report = service.import_bundle(&bundle, dry_run)?;
    if dry_run {
        println!("dry run — nothing written");
    }
    println!(
        "assets: {} created, {} updated",
        report.assets_created, report.assets_updated
    );
    println!(
        "media: {} created, {} updated",
        report.media_created, report.media_updated
    );
    println!(
        "software: {} created, {} updated",
        report.software_created, report.software_updated
    );
    println!(
        "services: {} created, {} updated",
        report.services_created, report.services_updated
    );
    println!(
        "relations: {} created, {} updated",
        report.relations_created, report.relations_updated
    );
    println!(
        "external refs: {} created, {} deduplicated",
        report.external_refs_created, report.external_refs_deduplicated
    );
    println!(
        "activity: {} events, tags: {}",
        report.activity_created, report.tags_created
    );
    Ok(())
}
