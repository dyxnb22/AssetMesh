//! Local launch timings. Records contain stage names and durations only.
use crate::error::DesktopError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrontendStage {
    FrontendLoaded,
    ShellVisible,
    WorkspaceMounted,
    FirstListReady,
}

impl FrontendStage {
    fn name(self) -> &'static str {
        match self {
            Self::FrontendLoaded => "frontend_loaded",
            Self::ShellVisible => "shell_visible",
            Self::WorkspaceMounted => "workspace_mounted",
            Self::FirstListReady => "first_list_ready",
        }
    }
}

#[derive(Clone, Serialize)]
struct StageTiming {
    stage: &'static str,
    native_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    web_ms: Option<f64>,
}

#[derive(Serialize)]
struct Report {
    app_version: &'static str,
    started_at: String,
    stages: Vec<StageTiming>,
}

struct Inner {
    started: Instant,
    report: Mutex<Report>,
    output: Mutex<Option<PathBuf>>,
    writer: Mutex<()>,
    trace: bool,
}

#[derive(Clone)]
pub struct StartupTimings(Arc<Inner>);

impl Default for StartupTimings {
    fn default() -> Self {
        let timings = Self(Arc::new(Inner {
            started: Instant::now(),
            report: Mutex::new(Report {
                app_version: env!("CARGO_PKG_VERSION"),
                started_at: chrono::Utc::now().to_rfc3339(),
                stages: Vec::new(),
            }),
            output: Mutex::new(None),
            writer: Mutex::new(()),
            trace: std::env::var_os("ASSETMESH_STARTUP_TRACE").is_some(),
        }));
        timings.record("native_start", None);
        timings
    }
}

impl StartupTimings {
    pub fn set_directory(&self, directory: &Path) {
        if let Ok(mut output) = self.0.output.lock() {
            *output = Some(directory.join("startup-performance.json"));
        }
    }

    pub fn native(&self, stage: &'static str) {
        self.record(stage, None);
        if stage == "database_failed" {
            self.persist_in_background();
        }
    }

    pub fn frontend(&self, stage: FrontendStage, web_ms: f64) -> Result<(), DesktopError> {
        if !web_ms.is_finite() || !(0.0..=86_400_000.0).contains(&web_ms) {
            return Err(DesktopError::invalid_input("Invalid startup duration"));
        }
        if self.record(stage.name(), Some(web_ms))
            && matches!(
                stage,
                FrontendStage::WorkspaceMounted | FrontendStage::FirstListReady
            )
        {
            self.persist_in_background();
        }
        Ok(())
    }

    fn record(&self, stage: &'static str, web_ms: Option<f64>) -> bool {
        let timing = StageTiming {
            stage,
            native_ms: self.0.started.elapsed().as_secs_f64() * 1000.0,
            web_ms,
        };
        let Ok(mut report) = self.0.report.lock() else {
            return false;
        };
        if report.stages.iter().any(|entry| entry.stage == stage) {
            return false;
        }
        report.stages.push(timing.clone());
        if self.0.trace {
            if let Ok(json) = serde_json::to_string(&timing) {
                eprintln!("[assetmesh-startup] {json}");
            }
        }
        true
    }

    fn persist_in_background(&self) {
        if self
            .0
            .output
            .lock()
            .map(|output| output.is_none())
            .unwrap_or(true)
        {
            return;
        }
        let timings = self.clone();
        std::thread::spawn(move || {
            let _ = timings.persist();
        });
    }

    // Serialize writers and read the newest report under that lock: an earlier
    // milestone's worker cannot replace the final first-list measurement.
    fn persist(&self) -> std::io::Result<()> {
        let _writer = self
            .0
            .writer
            .lock()
            .map_err(|_| std::io::Error::other("Startup writer lock failed"))?;
        let output = self
            .0
            .output
            .lock()
            .map_err(|_| std::io::Error::other("Startup output lock failed"))?
            .clone();
        let Some(output) = output else {
            return Ok(());
        };
        let report = self
            .0
            .report
            .lock()
            .map_err(|_| std::io::Error::other("Startup report lock failed"))?;
        let bytes = serde_json::to_vec_pretty(&*report)?;
        drop(report);
        let parent = output.parent().expect("startup report has a parent");
        std::fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(".startup-{}.json", uuid::Uuid::now_v7()));
        let result =
            std::fs::write(&temporary, bytes).and_then(|_| std::fs::rename(&temporary, &output));
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_list_timing_survives_later_navigation_and_is_saved_without_user_data() {
        let dir = std::env::temp_dir().join(format!("assetmesh-startup-{}", uuid::Uuid::now_v7()));
        let timings = StartupTimings::default();
        timings.native("database_ready");
        timings
            .frontend(FrontendStage::FirstListReady, 125.0)
            .unwrap();
        timings
            .frontend(FrontendStage::FirstListReady, 900.0)
            .unwrap();
        timings.set_directory(&dir);
        timings.persist().unwrap();
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("startup-performance.json")).unwrap())
                .unwrap();
        let stages = report["stages"].as_array().unwrap();
        let first: Vec<_> = stages
            .iter()
            .filter(|entry| entry["stage"] == "first_list_ready")
            .collect();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0]["web_ms"], 125.0);
        assert_eq!(report.as_object().unwrap().len(), 3);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_frontend_durations_are_rejected() {
        let timings = StartupTimings::default();
        for duration in [f64::NAN, f64::INFINITY, -1.0, 86_400_001.0] {
            assert!(timings
                .frontend(FrontendStage::FrontendLoaded, duration)
                .is_err());
        }
    }
}
