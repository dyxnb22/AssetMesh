//! One snapshot, bounded rows, private staging, and atomic replacement.
use super::budget::{
    MAX_BUNDLE_BYTES, MAX_BUNDLE_FILE_BYTES, MAX_BUNDLE_RECORDS, MAX_JSONL_LINE_BYTES,
    MAX_MANIFEST_BYTES,
};
use super::bundle_io::{
    ensure_replaceable, fs_error, private_directory, private_output_file, recover_interrupted_swap,
    swap_dir, with_bundle_lock,
};
use super::format::{
    json_error, ts_to_wire, ExportFile, ModuleVersion, PortableBundle, PortableManifest,
    PortableRow, EXPORT_FORMAT, EXPORT_VERSION, MEDIA_SCHEMA_VERSION, SERVICES_SCHEMA_VERSION,
    SOFTWARE_SCHEMA_VERSION, V1_FILE_PATHS,
};
use crate::application::SharedClock;
use crate::domain::info::SCHEMA_VERSION as INFO_SCHEMA_VERSION;
use crate::ports::uow::UnitOfWorkFactory;
use crate::{AppError, AppResult};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

pub struct PortableExportService<F: UnitOfWorkFactory> {
    factory: F,
    clock: SharedClock,
}

enum OutputFile {
    Memory(Vec<u8>),
    Disk(BufWriter<fs::File>),
}
impl Write for OutputFile {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Memory(buffer) => buffer.write(bytes),
            Self::Disk(file) => file.write(bytes),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Memory(buffer) => buffer.flush(),
            Self::Disk(file) => file.flush(),
        }
    }
}

struct Output {
    files: BTreeMap<&'static str, (OutputFile, usize)>,
    counts: BTreeMap<String, usize>,
    total: usize,
    records: usize,
}

impl Output {
    fn new(directory: Option<&Path>) -> AppResult<Self> {
        let mut files = BTreeMap::new();
        for path in V1_FILE_PATHS {
            let output = if let Some(directory) = directory {
                OutputFile::Disk(BufWriter::new(
                    private_output_file(&directory.join(path)).map_err(fs_error)?,
                ))
            } else {
                OutputFile::Memory(Vec::new())
            };
            files.insert(path, (output, 0));
        }
        let counts = [
            "assets",
            "media",
            "software",
            "services",
            "info",
            "external_refs",
            "activity",
            "tags",
            "asset_tags",
            "relations",
        ]
        .into_iter()
        .map(|name| (name.into(), 0))
        .collect();
        let mut output = Self {
            files,
            counts,
            total: 0,
            records: 0,
        };
        output.write("tags.json", b"[")?;
        Ok(output)
    }
    fn write(&mut self, path: &'static str, bytes: &[u8]) -> AppResult<()> {
        let (file, size) = self.files.get_mut(path).unwrap();
        if size.saturating_add(bytes.len()) > MAX_BUNDLE_FILE_BYTES
            || self.total.saturating_add(bytes.len()) > MAX_BUNDLE_BYTES - MAX_MANIFEST_BYTES
        {
            return Err(AppError::validation(
                "portable export exceeds the 32 MiB file or 128 MiB total size limit",
            ));
        }
        file.write_all(bytes).map_err(fs_error)?;
        *size += bytes.len();
        self.total += bytes.len();
        Ok(())
    }
    fn row(&mut self, row: PortableRow) -> AppResult<()> {
        if self.records == MAX_BUNDLE_RECORDS {
            return Err(AppError::validation(
                "portable export exceeds the 100000 record limit",
            ));
        }
        let (section, path) = row.section();
        let bytes = serde_json::to_vec(&row).map_err(json_error)?;
        if bytes.len() > MAX_JSONL_LINE_BYTES {
            return Err(AppError::validation(
                "portable export record exceeds the 1 MiB line size limit",
            ));
        }
        if path == "tags.json" && self.counts[section] > 0 {
            self.write(path, b",")?;
        }
        self.write(path, &bytes)?;
        if path != "tags.json" {
            self.write(path, b"\n")?;
        }
        *self.counts.get_mut(section).unwrap() += 1;
        self.records += 1;
        Ok(())
    }
    fn finish(&mut self) -> AppResult<()> {
        self.write("tags.json", b"]")?;
        for (file, _) in self.files.values_mut() {
            if let OutputFile::Disk(file) = file {
                file.flush().map_err(fs_error)?;
                file.get_ref().sync_all().map_err(fs_error)?;
            }
        }
        Ok(())
    }
}

impl<F: UnitOfWorkFactory> PortableExportService<F> {
    pub fn new(factory: F, clock: SharedClock) -> Self {
        Self { factory, clock }
    }

    /// Complete recovery representation, retained for in-memory consumers.
    pub fn export(&mut self, app_version: &str) -> AppResult<PortableBundle> {
        let mut output = Output::new(None)?;
        let manifest = self.export_rows(app_version, &mut output, true)?;
        let files = V1_FILE_PATHS
            .into_iter()
            .map(|path| {
                let (OutputFile::Memory(bytes), _) = output.files.remove(path).unwrap() else {
                    unreachable!()
                };
                ExportFile {
                    path: path.into(),
                    content: String::from_utf8(bytes).unwrap(),
                }
            })
            .collect();
        Ok(PortableBundle { manifest, files })
    }

    /// Streams production exports without collecting tables or JSONL files.
    pub fn export_to_directory(
        &mut self,
        app_version: &str,
        target: &Path,
        include_api_keys: bool,
    ) -> AppResult<PortableManifest> {
        with_bundle_lock(target, || {
            recover_interrupted_swap(target)?;
            ensure_replaceable(target)?;
            let swap = swap_dir(target);
            private_directory(&swap)?;
            let new = swap.join("new");
            private_directory(&new)?;
            private_directory(&new.join("modules"))?;
            let result = (|| {
                let mut output = Output::new(Some(&new))?;
                let manifest = self.export_rows(app_version, &mut output, include_api_keys)?;
                let bytes = serde_json::to_vec_pretty(&manifest).map_err(json_error)?;
                if bytes.len() > MAX_MANIFEST_BYTES {
                    return Err(AppError::validation(
                        "portable manifest exceeds the 256 KiB limit",
                    ));
                }
                let mut file = private_output_file(&new.join("manifest.json")).map_err(fs_error)?;
                file.write_all(&bytes).map_err(fs_error)?;
                file.sync_all().map_err(fs_error)?;
                drop(file);
                // No reader lock or output handle survives the directory swap.
                drop(output);
                if target.exists() {
                    fs::rename(target, swap.join("previous")).map_err(fs_error)?;
                }
                fs::rename(&new, target).map_err(fs_error)?;
                Ok(manifest)
            })();
            if result.is_ok() {
                let _ = fs::remove_dir_all(&swap);
            } else {
                recover_interrupted_swap(target)?;
            }
            result
        })
    }

    fn export_rows(
        &mut self,
        app_version: &str,
        output: &mut Output,
        include_api_keys: bool,
    ) -> AppResult<PortableManifest> {
        self.factory.read(&mut |query| {
            let excluded = if include_api_keys {
                HashSet::new()
            } else {
                query.private_asset_ids()?
            };
            query.visit_portable(&mut |row| {
                if !include_api_keys && row.touches(&excluded) {
                    return Ok(());
                }
                output.row(row)
            })
        })?;
        output.finish()?;
        Ok(PortableManifest {
            format: EXPORT_FORMAT.into(),
            version: EXPORT_VERSION,
            created_at: ts_to_wire(self.clock.now()),
            app_version: app_version.into(),
            modules: [
                ("media", MEDIA_SCHEMA_VERSION),
                ("software", SOFTWARE_SCHEMA_VERSION),
                ("services", SERVICES_SCHEMA_VERSION),
                ("info", INFO_SCHEMA_VERSION),
            ]
            .into_iter()
            .map(|(name, schema_version)| (name.into(), ModuleVersion { schema_version }))
            .collect(),
            record_counts: output.counts.clone(),
        })
    }
}
