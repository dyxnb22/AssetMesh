//! Portable export/import and native folder selection.

use std::path::Path;

use assetmesh_core::application::portable::{
    read_bundle_from_directory, EXPORT_FORMAT, EXPORT_VERSION, V1_FILE_PATHS,
};
use assetmesh_providers::{CommandRunner, SystemCommandRunner};
use tauri::Manager;

use crate::dto::{ExportReceiptDto, ImportPreviewDto, ImportReceiptDto, ImportReportDto};
use crate::error::DesktopError;
use crate::state::DesktopState;

#[tauri::command]
pub async fn pick_directory(prompt: Option<String>) -> Result<Option<String>, DesktopError> {
    tauri::async_runtime::spawn_blocking(move || {
        pick_directory_impl(
            prompt,
            &SystemCommandRunner::with_timeout(std::time::Duration::from_secs(300)),
        )
    })
    .await
    .map_err(|error| DesktopError::internal(error.to_string()))?
}

/// Opens the native directory chooser. `runner` is the seam around the
/// `osascript` child process: without it any test that reaches this code pops
/// a real modal dialog and blocks until a human dismisses it.
pub fn pick_directory_impl(
    prompt: Option<String>,
    runner: &dyn CommandRunner,
) -> Result<Option<String>, DesktopError> {
    #[cfg(target_os = "macos")]
    {
        // Try osascript choose folder on macOS
        let prompt_text = prompt.unwrap_or_else(|| "Select directory".to_string());
        let script = format!(
            r#"POSIX path of (choose folder with prompt "{}")"#,
            prompt_text.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let output = runner.run("osascript", &["-e", &script]).map_err(|_| {
            DesktopError::unavailable("Folder selection failed or exceeded its time limit")
        })?;
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(Some(path));
            }
        }
        if String::from_utf8_lossy(&output.stderr).contains("(-128)") {
            return Ok(None);
        }
        Err(DesktopError::unavailable(
            "Could not open the folder chooser",
        ))
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (prompt, runner);
        Ok(None)
    }
}

#[tauri::command]
pub async fn portable_export<R: tauri::Runtime>(
    target_dir: String,
    include_api_keys: Option<bool>,
    app: tauri::AppHandle<R>,
) -> Result<ExportReceiptDto, DesktopError> {
    tauri::async_runtime::spawn_blocking(move || {
        portable_export_options_impl(
            &target_dir,
            include_api_keys.unwrap_or(false),
            &app.state::<DesktopState>(),
        )
    })
    .await
    .map_err(|error| DesktopError::internal(error.to_string()))?
}

pub fn portable_export_impl(
    target_dir: &str,
    state: &DesktopState,
) -> Result<ExportReceiptDto, DesktopError> {
    portable_export_options_impl(target_dir, false, state)
}

pub fn portable_export_options_impl(
    target_dir: &str,
    include_api_keys: bool,
    state: &DesktopState,
) -> Result<ExportReceiptDto, DesktopError> {
    if target_dir.trim().is_empty() {
        return Err(DesktopError::invalid_input(
            "Target directory cannot be empty",
        ));
    }

    state.with_modules(|modules| {
        let mut service = modules.portable_export();
        let manifest = service.export_to_directory(
            env!("CARGO_PKG_VERSION"),
            Path::new(target_dir),
            include_api_keys,
        )?;
        let files = V1_FILE_PATHS.into_iter().map(str::to_string).collect();

        Ok(ExportReceiptDto {
            target_dir: target_dir.to_string(),
            format: manifest.format,
            version: manifest.version,
            app_version: manifest.app_version,
            created_at: manifest.created_at,
            record_counts: manifest.record_counts,
            files,
        })
    })
}

#[tauri::command]
pub async fn portable_import_preview<R: tauri::Runtime>(
    source_dir: String,
    app: tauri::AppHandle<R>,
) -> Result<ImportPreviewDto, DesktopError> {
    tauri::async_runtime::spawn_blocking(move || {
        portable_import_preview_impl(&source_dir, &app.state::<DesktopState>())
    })
    .await
    .map_err(|error| DesktopError::internal(error.to_string()))?
}

pub fn portable_import_preview_impl(
    source_dir: &str,
    state: &DesktopState,
) -> Result<ImportPreviewDto, DesktopError> {
    if source_dir.trim().is_empty() {
        return Err(DesktopError::invalid_input(
            "Source directory cannot be empty",
        ));
    }

    let source_path = Path::new(source_dir);
    let bundle = match read_bundle_from_directory(source_path) {
        Ok(b) => b,
        Err(err) => {
            return Ok(ImportPreviewDto {
                valid: false,
                source_dir: source_dir.to_string(),
                format: "unknown".to_string(),
                version: 0,
                app_version: "unknown".to_string(),
                created_at: String::new(),
                record_counts: Default::default(),
                modules: Vec::new(),
                dispositions: ImportReportDto::default(),
                fingerprint: String::new(),
                errors: vec![DesktopError::from(err).message],
            });
        }
    };

    let fingerprint = bundle.fingerprint().map_err(DesktopError::from)?;
    let manifest = bundle.manifest.clone();
    let mut errors = Vec::new();

    if bundle.manifest.format != EXPORT_FORMAT {
        errors.push(format!(
            "Unsupported format {:?}, expected {:?}",
            bundle.manifest.format, EXPORT_FORMAT
        ));
    }

    if bundle.manifest.version != EXPORT_VERSION {
        errors.push(format!(
            "Unsupported export version {}, supported version is {}",
            bundle.manifest.version, EXPORT_VERSION
        ));
    }

    let modules = manifest.modules.keys().cloned().collect();

    // If format or version invalid, return immediately without preflight
    if !errors.is_empty() {
        return Ok(ImportPreviewDto {
            valid: false,
            source_dir: source_dir.to_string(),
            format: manifest.format,
            version: manifest.version,
            app_version: manifest.app_version,
            created_at: manifest.created_at,
            record_counts: manifest.record_counts,
            modules,
            dispositions: ImportReportDto::default(),
            fingerprint,
            errors,
        });
    }

    state.with_modules(|dm| {
        let mut service = dm.portable_import();
        match service.import_bundle(&bundle, true) {
            Ok(report) => Ok(ImportPreviewDto {
                valid: true,
                source_dir: source_dir.to_string(),
                format: manifest.format,
                version: manifest.version,
                app_version: manifest.app_version,
                created_at: manifest.created_at,
                record_counts: manifest.record_counts,
                modules,
                dispositions: ImportReportDto::from(report),
                fingerprint,
                errors: Vec::new(),
            }),
            Err(err) => Ok(ImportPreviewDto {
                valid: false,
                source_dir: source_dir.to_string(),
                format: manifest.format,
                version: manifest.version,
                app_version: manifest.app_version,
                created_at: manifest.created_at,
                record_counts: manifest.record_counts,
                modules,
                dispositions: ImportReportDto::default(),
                fingerprint,
                errors: vec![DesktopError::from(err).message],
            }),
        }
    })
}

#[tauri::command]
pub async fn portable_import_apply<R: tauri::Runtime>(
    source_dir: String,
    expected_fingerprint: String,
    app: tauri::AppHandle<R>,
) -> Result<ImportReceiptDto, DesktopError> {
    tauri::async_runtime::spawn_blocking(move || {
        portable_import_apply_impl(
            &source_dir,
            &expected_fingerprint,
            &app.state::<DesktopState>(),
        )
    })
    .await
    .map_err(|error| DesktopError::internal(error.to_string()))?
}

pub fn portable_import_apply_impl(
    source_dir: &str,
    expected_fingerprint: &str,
    state: &DesktopState,
) -> Result<ImportReceiptDto, DesktopError> {
    if source_dir.trim().is_empty() {
        return Err(DesktopError::invalid_input(
            "Source directory cannot be empty",
        ));
    }

    if expected_fingerprint.trim().is_empty() {
        return Err(DesktopError::invalid_input(
            "Expected bundle fingerprint cannot be empty",
        ));
    }

    let source_path = Path::new(source_dir);
    let bundle = read_bundle_from_directory(source_path).map_err(DesktopError::from)?;

    let actual_fingerprint = bundle.fingerprint().map_err(DesktopError::from)?;
    if actual_fingerprint != expected_fingerprint {
        return Err(DesktopError::conflict(format!(
            "Bundle content changed since preview; expected fingerprint {}, found {}",
            expected_fingerprint, actual_fingerprint
        )));
    }

    if bundle.manifest.format != EXPORT_FORMAT {
        return Err(DesktopError::invalid_input(format!(
            "Unsupported format {:?}, expected {:?}",
            bundle.manifest.format, EXPORT_FORMAT
        )));
    }

    if bundle.manifest.version != EXPORT_VERSION {
        return Err(DesktopError::invalid_input(format!(
            "Unsupported export version {}, supported version is {}",
            bundle.manifest.version, EXPORT_VERSION
        )));
    }

    state.with_service_operation(|| {
        if state
            .service_runtime()
            .statuses()
            .iter()
            .any(|status| state.service_runtime().is_active(&status.asset_id))
        {
            return Err(DesktopError::conflict(
                "stop the local services before importing a library",
            ));
        }
        state.with_modules(|modules| {
            let mut service = modules.portable_import();
            state.recovery_point("before_import")?;
            let report = service.import_bundle(&bundle, false)?;

            Ok(ImportReceiptDto {
                success: true,
                source_dir: source_dir.to_string(),
                applied_at: chrono::Utc::now().to_rfc3339(),
                report: ImportReportDto::from(report),
            })
        })
    })
}
