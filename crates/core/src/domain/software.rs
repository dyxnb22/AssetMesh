//! Software module domain: typed details owned by the Software module
//! (ADR 0003). A `SoftwareRecord` is typed detail of a shared `Asset`, not a
//! competing top-level identity. Discovery-specific data lives in candidates
//! (application layer) and is promoted here only through explicit adoption.

use crate::domain::asset::AssetKind;
use crate::domain::ids::AssetId;
use crate::domain::validation::{bounded, optional_text};
use crate::domain::Timestamp;
use crate::AppResult;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Software module data schema version (ADR 0008). Owned by the Software
/// module; independent of the database migration version and the portable
/// export format version.
pub const SCHEMA_VERSION: i64 = 1;

/// Small normalized category vocabulary. Additional categories can be added
/// later without redesigning the base Asset model; do not grow a package
/// manager taxonomy here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoftwareCategory {
    Application,
    Cli,
    Package,
    Runtime,
    Tool,
}

impl SoftwareCategory {
    pub const fn as_str(&self) -> &'static str {
        match self {
            SoftwareCategory::Application => "application",
            SoftwareCategory::Cli => "cli",
            SoftwareCategory::Package => "package",
            SoftwareCategory::Runtime => "runtime",
            SoftwareCategory::Tool => "tool",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "application" | "app" | "gui" => Some(SoftwareCategory::Application),
            "cli" | "command_line" | "commandline" => Some(SoftwareCategory::Cli),
            "package" => Some(SoftwareCategory::Package),
            "runtime" => Some(SoftwareCategory::Runtime),
            "tool" | "utility" => Some(SoftwareCategory::Tool),
            _ => None,
        }
    }

    /// Human-oriented label used in search subtitles ("CLI Tool · 14.1.0").
    pub const fn label(&self) -> &'static str {
        match self {
            SoftwareCategory::Application => "Application",
            SoftwareCategory::Cli => "CLI Tool",
            SoftwareCategory::Package => "Package",
            SoftwareCategory::Runtime => "Runtime",
            SoftwareCategory::Tool => "Tool",
        }
    }

    /// The asset kind that owns this category. Kind and category must stay
    /// compatible (e.g. a `software.cli` asset cannot hold an application
    /// record).
    pub const fn asset_kind(&self) -> AssetKind {
        match self {
            SoftwareCategory::Application => AssetKind::SoftwareApplication,
            SoftwareCategory::Cli => AssetKind::SoftwareCli,
            SoftwareCategory::Package => AssetKind::SoftwarePackage,
            SoftwareCategory::Runtime => AssetKind::SoftwareRuntime,
            SoftwareCategory::Tool => AssetKind::SoftwareTool,
        }
    }

    pub fn from_asset_kind(kind: AssetKind) -> Option<Self> {
        match kind {
            AssetKind::SoftwareApplication => Some(SoftwareCategory::Application),
            AssetKind::SoftwareCli => Some(SoftwareCategory::Cli),
            AssetKind::SoftwarePackage => Some(SoftwareCategory::Package),
            AssetKind::SoftwareRuntime => Some(SoftwareCategory::Runtime),
            AssetKind::SoftwareTool => Some(SoftwareCategory::Tool),
            _ => None,
        }
    }
}

impl fmt::Display for SoftwareCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How the software is installed, where known. A small closed set covering
/// the Phase 2 discovery sources plus durable manual/system concepts. Raw
/// provider payloads never enter canonical state through this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallSource {
    MacosApp,
    HomebrewFormula,
    HomebrewCask,
    NpmGlobal,
    Pipx,
    Manual,
    System,
    Unknown,
}

impl InstallSource {
    pub const fn as_str(&self) -> &'static str {
        match self {
            InstallSource::MacosApp => "macos_app",
            InstallSource::HomebrewFormula => "homebrew_formula",
            InstallSource::HomebrewCask => "homebrew_cask",
            InstallSource::NpmGlobal => "npm_global",
            InstallSource::Pipx => "pipx",
            InstallSource::Manual => "manual",
            InstallSource::System => "system",
            InstallSource::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "macos_app" | "mac_app" | "macos application" => Some(InstallSource::MacosApp),
            "homebrew_formula" | "brew_formula" | "formula" => Some(InstallSource::HomebrewFormula),
            "homebrew_cask" | "brew_cask" | "cask" => Some(InstallSource::HomebrewCask),
            "npm_global" | "npm" => Some(InstallSource::NpmGlobal),
            "pipx" => Some(InstallSource::Pipx),
            "manual" => Some(InstallSource::Manual),
            "system" => Some(InstallSource::System),
            "unknown" => Some(InstallSource::Unknown),
            _ => None,
        }
    }

    pub const fn label(&self) -> &'static str {
        match self {
            InstallSource::MacosApp => "macOS App",
            InstallSource::HomebrewFormula => "Homebrew Formula",
            InstallSource::HomebrewCask => "Homebrew Cask",
            InstallSource::NpmGlobal => "npm Global",
            InstallSource::Pipx => "pipx",
            InstallSource::Manual => "Manual",
            InstallSource::System => "System",
            InstallSource::Unknown => "Unknown",
        }
    }
}

impl fmt::Display for InstallSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

const fn default_install_source() -> InstallSource {
    InstallSource::Unknown
}

/// Software typed details owned by the Software module.
///
/// `purpose` and `notes` are user-owned: discovery/adoption must never
/// overwrite them (docs/09). Provider-specific raw fields stay in candidate
/// metadata, never here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoftwareRecord {
    pub asset_id: AssetId,
    pub category: SoftwareCategory,
    pub install_source: InstallSource,
    pub version: Option<String>,
    pub install_location: Option<String>,
    pub executable_path: Option<String>,
    /// Why this software exists in the inventory. User-controlled.
    pub purpose: Option<String>,
    pub notes: Option<String>,
    /// When discovery first observed the software, if it was discovered.
    pub discovered_at: Option<Timestamp>,
    /// Only when reliably known (package receipts); never guessed.
    pub installed_at: Option<Timestamp>,
    pub architecture: Option<String>,
}

impl SoftwareRecord {
    pub fn new(asset_id: AssetId, category: SoftwareCategory) -> Self {
        SoftwareRecord {
            asset_id,
            category,
            install_source: default_install_source(),
            version: None,
            install_location: None,
            executable_path: None,
            purpose: None,
            notes: None,
            discovered_at: None,
            installed_at: None,
            architecture: None,
        }
    }

    /// Record-level invariants, enforced and normalized on every write path
    /// (services, portable import, repository upsert).
    ///
    /// This is the single canonicalization point for optional text: every
    /// free-text field is trimmed (whitespace-only collapses to `None`) and
    /// control characters are rejected — including the user-owned `purpose`
    /// and `notes`. Callers pass a mutable record so the normalized values are
    /// what gets stored; a read-only check that discarded the trimmed text
    /// would let raw unnormalized values through behind it.
    pub fn validate(&mut self) -> AppResult<()> {
        self.version = optional_text(&self.version, "version")?;
        if let Some(text) = self.version.as_deref() {
            bounded(text, 128, "version")?;
        }
        for (value, name, max) in [
            (&mut self.install_location, "install_location", 1024),
            (&mut self.executable_path, "executable_path", 1024),
            (&mut self.architecture, "architecture", 1024),
            (&mut self.purpose, "purpose", 1024),
            (&mut self.notes, "notes", 1024),
        ] {
            *value = optional_text(value, name)?;
            if let Some(text) = value.as_deref() {
                bounded(text, max, name)?;
            }
        }
        Ok(())
    }
}

/// An asset together with its software details — the joined view the
/// Software repository returns for list/detail queries.
#[derive(Debug, Clone, PartialEq)]
pub struct SoftwareEntry {
    pub asset: crate::domain::asset::Asset,
    pub record: SoftwareRecord,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_parsing_and_kind_mapping() {
        assert_eq!(
            SoftwareCategory::parse("Application"),
            Some(SoftwareCategory::Application)
        );
        assert_eq!(SoftwareCategory::parse("CLI"), Some(SoftwareCategory::Cli));
        assert_eq!(SoftwareCategory::parse("nonsense"), None);
        assert_eq!(
            SoftwareCategory::Application.asset_kind(),
            AssetKind::SoftwareApplication
        );
        assert_eq!(
            SoftwareCategory::from_asset_kind(AssetKind::SoftwareRuntime),
            Some(SoftwareCategory::Runtime)
        );
        assert_eq!(
            SoftwareCategory::from_asset_kind(AssetKind::MediaGame),
            None
        );
    }

    #[test]
    fn install_source_parsing() {
        assert_eq!(
            InstallSource::parse("homebrew_cask"),
            Some(InstallSource::HomebrewCask)
        );
        assert_eq!(
            InstallSource::parse("macos_app"),
            Some(InstallSource::MacosApp)
        );
        assert_eq!(InstallSource::parse("whatever"), None);
    }

    #[test]
    fn validation_rejects_overlong_fields() {
        let mut record = SoftwareRecord::new(AssetId::generate(), SoftwareCategory::Cli);
        assert!(record.validate().is_ok());

        record.version = Some("v".repeat(129));
        assert!(record.validate().is_err());
        record.version = Some("1.2.3".into());
        assert!(record.validate().is_ok());

        record.install_location = Some("/".repeat(1025));
        assert!(record.validate().is_err());
        record.install_location = Some("/Applications/Foo.app".into());
        assert!(record.validate().is_ok());
    }

    #[test]
    fn optional_text_trims_and_rejects_control_characters() {
        assert_eq!(optional_text(&None, "x").unwrap(), None);
        assert_eq!(optional_text(&Some("  ".into()), "x").unwrap(), None);
        assert_eq!(
            optional_text(&Some(" hi ".into()), "x").unwrap(),
            Some("hi".into())
        );
        assert!(optional_text(&Some("bad\u{0}value".into()), "x").is_err());
    }

    #[test]
    fn validation_normalizes_in_place_and_covers_user_owned_fields() {
        // validate() must store the trimmed values, not merely inspect them,
        // and must cover purpose/notes — the fields a portable bundle can
        // otherwise fill with raw unnormalized text.
        let mut record = SoftwareRecord::new(AssetId::generate(), SoftwareCategory::Cli);
        record.version = Some("  1.2.3  ".into());
        record.install_location = Some("  /opt/x  ".into());
        record.purpose = Some("  why installed  ".into());
        record.notes = Some("   ".into());

        assert!(record.validate().is_ok());
        assert_eq!(record.version.as_deref(), Some("1.2.3"));
        assert_eq!(record.install_location.as_deref(), Some("/opt/x"));
        assert_eq!(record.purpose.as_deref(), Some("why installed"));
        // Whitespace-only collapses to None instead of an empty string.
        assert_eq!(record.notes, None);
    }

    #[test]
    fn validation_rejects_control_characters_in_user_owned_fields() {
        let mut record = SoftwareRecord::new(AssetId::generate(), SoftwareCategory::Cli);
        record.purpose = Some("why\u{0}installed".into());
        assert!(record.validate().is_err());

        let mut record = SoftwareRecord::new(AssetId::generate(), SoftwareCategory::Cli);
        record.notes = Some("line\nbreak".into());
        assert!(record.validate().is_err());
    }
}
