//! Portable export/import and app settings commands (P5-09).

use std::path::Path;

use assetmesh_core::application::library_service::AppCapabilities;
use assetmesh_core::application::portable::{
    read_bundle_from_directory, write_bundle_to_directory, EXPORT_FORMAT, EXPORT_VERSION,
};
use assetmesh_providers::{CommandRunner, MacosApplicationsProvider, SystemCommandRunner};
use tauri::State;

use crate::dto::{
    AppSettingsDto, ExportReceiptDto, ImportPreviewDto, ImportReceiptDto, ImportReportDto,
    ProviderStatusDto,
};
use crate::error::DesktopError;
use crate::state::{AppStatus, DesktopState};

#[tauri::command]
pub fn pick_directory(prompt: Option<String>) -> Result<Option<String>, DesktopError> {
    pick_directory_impl(prompt, &SystemCommandRunner)
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
            prompt_text.replace('"', "\\\"")
        );
        if let Ok(output) = runner.run("osascript", &["-e", &script]) {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Ok(Some(path));
                }
            }
            // User cancelled in AppleScript returns non-zero status
            return Ok(None);
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (prompt, runner);
    }

    Ok(None)
}

#[tauri::command]
pub fn portable_export(
    target_dir: String,
    state: State<'_, DesktopState>,
) -> Result<ExportReceiptDto, DesktopError> {
    portable_export_impl(&target_dir, &state)
}

pub fn portable_export_impl(
    target_dir: &str,
    state: &DesktopState,
) -> Result<ExportReceiptDto, DesktopError> {
    if target_dir.trim().is_empty() {
        return Err(DesktopError::invalid_input(
            "Target directory cannot be empty",
        ));
    }

    state.with_modules(|modules| {
        let mut service = modules.portable_export();
        let bundle = service.export(env!("CARGO_PKG_VERSION"))?;

        let target_path = Path::new(target_dir);
        write_bundle_to_directory(&bundle, target_path)?;

        let files = bundle.files.into_iter().map(|f| f.path).collect();

        Ok(ExportReceiptDto {
            target_dir: target_dir.to_string(),
            format: bundle.manifest.format,
            version: bundle.manifest.version,
            app_version: bundle.manifest.app_version,
            created_at: bundle.manifest.created_at,
            record_counts: bundle.manifest.record_counts,
            files,
        })
    })
}

#[tauri::command]
pub fn portable_import_preview(
    source_dir: String,
    state: State<'_, DesktopState>,
) -> Result<ImportPreviewDto, DesktopError> {
    portable_import_preview_impl(&source_dir, &state)
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

    if manifest.format != EXPORT_FORMAT {
        errors.push(format!(
            "Unsupported format {:?}, expected {:?}",
            manifest.format, EXPORT_FORMAT
        ));
    }

    if manifest.version != EXPORT_VERSION {
        errors.push(format!(
            "Unsupported export version {}, supported version is {}",
            manifest.version, EXPORT_VERSION
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
pub fn portable_import_apply(
    source_dir: String,
    expected_fingerprint: String,
    state: State<'_, DesktopState>,
) -> Result<ImportReceiptDto, DesktopError> {
    portable_import_apply_impl(&source_dir, &expected_fingerprint, &state)
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

    state.with_modules(|modules| {
        let mut service = modules.portable_import();
        let report = service.import_bundle(&bundle, false)?;

        Ok(ImportReceiptDto {
            success: true,
            source_dir: source_dir.to_string(),
            applied_at: chrono::Utc::now().to_rfc3339(),
            report: ImportReportDto::from(report),
        })
    })
}

#[tauri::command]
pub fn app_settings(state: State<'_, DesktopState>) -> Result<AppSettingsDto, DesktopError> {
    app_settings_impl(&state)
}

pub fn app_settings_impl(state: &DesktopState) -> Result<AppSettingsDto, DesktopError> {
    let status = state.get_status();
    let (db_path, db_status) = match status {
        AppStatus::Ready { db_path } => (Some(db_path), "Ready".to_string()),
        AppStatus::Loading => (None, "Loading".to_string()),
        AppStatus::SetupFailure { message } => (None, format!("Setup Failure: {message}")),
        AppStatus::CorruptFailure { message } => (None, format!("Corrupt Failure: {message}")),
    };

    let capabilities = AppCapabilities::current();
    let runner = SystemCommandRunner;

    // Check macos_applications
    let macos_status = if cfg!(target_os = "macos") {
        match MacosApplicationsProvider::system_default() {
            Ok(_) => ProviderStatusDto {
                name: "macos_applications".to_string(),
                display_name: "macOS Applications".to_string(),
                available: true,
                details: Some("/Applications, ~/Applications".to_string()),
            },
            Err(e) => ProviderStatusDto {
                name: "macos_applications".to_string(),
                display_name: "macOS Applications".to_string(),
                available: false,
                details: Some(e.to_string()),
            },
        }
    } else {
        ProviderStatusDto {
            name: "macos_applications".to_string(),
            display_name: "macOS Applications".to_string(),
            available: false,
            details: Some("Not supported on this OS".to_string()),
        }
    };

    // Check homebrew
    let brew_status = match runner.run("brew", &["--version"]) {
        Ok(out) if out.status.success() => ProviderStatusDto {
            name: "homebrew".to_string(),
            display_name: "Homebrew".to_string(),
            available: true,
            details: Some(
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("Installed")
                    .to_string(),
            ),
        },
        _ => ProviderStatusDto {
            name: "homebrew".to_string(),
            display_name: "Homebrew".to_string(),
            available: false,
            details: Some("brew command not found in PATH".to_string()),
        },
    };

    // Check cli_tools (npm & pipx)
    let npm_ok = runner
        .run("npm", &["--version"])
        .map(|o| o.status.success())
        .unwrap_or(false);
    let pipx_ok = runner
        .run("pipx", &["--version"])
        .map(|o| o.status.success())
        .unwrap_or(false);

    let cli_details = match (npm_ok, pipx_ok) {
        (true, true) => "npm and pipx detected in PATH".to_string(),
        (true, false) => "npm detected; pipx not found".to_string(),
        (false, true) => "pipx detected; npm not found".to_string(),
        (false, false) => "Neither npm nor pipx found in PATH".to_string(),
    };

    let cli_status = ProviderStatusDto {
        name: "cli_tools".to_string(),
        display_name: "CLI Tools (npm, pipx)".to_string(),
        available: npm_ok || pipx_ok,
        details: Some(cli_details),
    };

    Ok(AppSettingsDto {
        db_path,
        db_status,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        providers: vec![macos_status, brew_status, cli_status],
        capabilities,
    })
}
