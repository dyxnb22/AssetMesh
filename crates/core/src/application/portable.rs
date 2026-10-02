//! Portable export format V1 (docs/05-storage-and-portability).
//!
//! The portable bundle is a product contract, independent of the physical
//! SQLite schema AND of the Rust domain structs: dedicated `*V1` DTOs freeze
//! the wire format so internal refactors cannot silently change version 1.
//! Derived state (search projection), discovery snapshots, provider cache,
//! are excluded. Full recovery bundles include information values, including
//! API keys, and must be kept private. Ordinary exports omit API key assets
//! unless their inclusion is explicitly requested.
//!
//! Layout:
//!
//! ```text
//! assetmesh-export/
//! ├── manifest.json
//! ├── assets.jsonl
//! ├── external_refs.jsonl
//! ├── activity.jsonl
//! ├── tags.json
//! ├── asset_tags.jsonl
//! ├── relations.jsonl
//! └── modules/
//!     ├── media.jsonl
//!     ├── software.jsonl
//!     └── services.jsonl
//! ```
//!
//! Module sections are governed by manifest declaration (ADR 0008): a
//! bundle exported by a build that manages a module declares it in
//! `manifest.modules` (and its count in `record_counts`), and the section is
//! then authoritative — including reconciliation of destination state the
//! bundle no longer carries. A Phase 1 bundle predates Software and
//! Relations; a Phase 2 bundle predates Services: the absent section means
//! "contains no such state", imports cleanly, and leaves any pre-existing
//! destination data of that kind untouched. A section file present WITHOUT
//! its manifest declaration is bundle corruption and fails loudly.
//!
//! Every import (dry-run or commit) runs the same preflight: declared files
//! must be present, decoded row counts must match the manifest, identities
//! must be unique, and the reference graph (module details, refs,
//! memberships, activity, merge redirects, relations) must be internally
//! consistent — a damaged bundle can never restore "successfully" while
//! omitting records. Dry-run additionally inspects the destination, so it
//! fails exactly when commit would.

mod budget;
mod bundle_io;
mod destination;
mod format;
mod import;
mod streaming;
mod validation;

pub use budget::{MAX_BUNDLE_BYTES, MAX_BUNDLE_FILE_BYTES, MAX_BUNDLE_RECORDS, MAX_MANIFEST_BYTES};
pub use bundle_io::{
    lock_path, read_bundle_from_directory, with_bundle_lock, write_bundle_to_directory, BundleLock,
};
pub use format::{
    AssetTagRow, ExportFile, ModuleVersion, PortableActivityEventV1, PortableAssetV1,
    PortableBundle, PortableExternalRefV1, PortableInfoRecordV1, PortableManifest,
    PortableMediaRecordV1, PortableRelationV1, PortableRow, PortableServiceRecordV1,
    PortableSoftwareRecordV1, PortableTagV1, EXPORT_FORMAT, EXPORT_VERSION, MEDIA_SCHEMA_VERSION,
    SERVICES_SCHEMA_VERSION, SOFTWARE_SCHEMA_VERSION, V1_CORE_FILE_PATHS, V1_FILE_PATHS,
    V1_INFO_FILE_PATH, V1_MEDIA_FILE_PATH, V1_RELATIONS_FILE_PATH, V1_SERVICES_FILE_PATH,
    V1_SOFTWARE_FILE_PATH,
};
pub use import::{PortableImportReport, PortableImportService};
pub use streaming::PortableExportService;
