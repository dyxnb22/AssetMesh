//! AssetMesh discovery providers (docs/06 provider design).
//!
//! Concrete Software discovery sources for Phase 2: macOS applications,
//! Homebrew, and global CLI tool registries (npm, pipx). Providers are
//! read-only: they stage [`assetmesh_core::ports::providers::SoftwareCandidate`]s
//! for review; only an explicit application-layer adoption turns a candidate
//! into canonical data.
//!
//! Every environment interaction sits behind a small seam (filesystem roots,
//! a command runner) so provider logic is testable with fixtures and no test
//! depends on the host machine's installed software.

pub mod cli_tools;
pub mod homebrew;
pub mod macos_apps;

pub use cli_tools::CliToolsProvider;
pub use homebrew::HomebrewProvider;
pub use macos_apps::MacosApplicationsProvider;

use std::fmt::Debug;
use std::process::Output;

/// Why an external program could not be executed. Typed so callers can
/// distinguish "tool not installed" (an advisory skip for optional sources)
/// from real execution failures (a surfaced error).
#[derive(Debug)]
pub enum CommandError {
    /// The program does not exist / is not on PATH.
    NotFound,
    /// The program exists but could not be executed.
    Failed(std::io::Error),
}

impl CommandError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, CommandError::NotFound)
    }
}

/// Executes an external program. A seam around `std::process::Command` so
/// provider output parsing is testable with fixture responses.
pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, CommandError>;
}

/// Production runner.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, CommandError> {
        std::process::Command::new(program)
            .args(args)
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    CommandError::NotFound
                } else {
                    CommandError::Failed(e)
                }
            })
    }
}
