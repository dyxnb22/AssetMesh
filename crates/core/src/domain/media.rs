//! Media module domain: typed details owned by the Media module (ADR 0003).
//!
//! A `MediaRecord` is typed detail of a shared `Asset`, not a competing
//! top-level identity. Media-specific semantics (status vocabulary, progress
//! structure) are isolated here; the kernel does not know them.

use crate::domain::asset::AssetKind;
use crate::domain::ids::AssetId;
use crate::domain::Timestamp;
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Media module data schema version (ADR 0008). Owned by the Media module;
/// independent of the database migration version and the portable export
/// format version.
pub const SCHEMA_VERSION: i64 = 1;

/// Initial media types for Media V1. Additional types can be added later
/// without redesigning the base Asset model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Movie,
    Tv,
    Anime,
    Game,
}

impl MediaType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            MediaType::Movie => "movie",
            MediaType::Tv => "tv",
            MediaType::Anime => "anime",
            MediaType::Game => "game",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "movie" | "film" => Some(MediaType::Movie),
            "tv" | "series" | "show" => Some(MediaType::Tv),
            "anime" => Some(MediaType::Anime),
            "game" | "video_game" | "videogame" => Some(MediaType::Game),
            _ => None,
        }
    }

    /// Human-oriented label used in search subtitles ("Anime · 2023").
    pub const fn label(&self) -> &'static str {
        match self {
            MediaType::Movie => "Movie",
            MediaType::Tv => "TV",
            MediaType::Anime => "Anime",
            MediaType::Game => "Game",
        }
    }

    /// The asset kind that owns this media type. Kind and media type must
    /// stay compatible (e.g. a `media.game` asset cannot hold a movie record).
    pub const fn asset_kind(&self) -> AssetKind {
        match self {
            MediaType::Movie => AssetKind::MediaMovie,
            MediaType::Tv => AssetKind::MediaTv,
            MediaType::Anime => AssetKind::MediaAnime,
            MediaType::Game => AssetKind::MediaGame,
        }
    }

    pub fn from_asset_kind(kind: AssetKind) -> Option<Self> {
        match kind {
            AssetKind::MediaMovie => Some(MediaType::Movie),
            AssetKind::MediaTv => Some(MediaType::Tv),
            AssetKind::MediaAnime => Some(MediaType::Anime),
            AssetKind::MediaGame => Some(MediaType::Game),
            _ => None,
        }
    }
}

impl fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Normalized status vocabulary shared by all media types. UI labels may
/// differ ("Watching"/"Playing") but storage stays normalized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaStatus {
    Planned,
    InProgress,
    Completed,
    Paused,
    Dropped,
}

impl MediaStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            MediaStatus::Planned => "planned",
            MediaStatus::InProgress => "in_progress",
            MediaStatus::Completed => "completed",
            MediaStatus::Paused => "paused",
            MediaStatus::Dropped => "dropped",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "planned" | "backlog" | "want" => Some(MediaStatus::Planned),
            "in_progress" | "inprogress" | "watching" | "playing" | "reading" | "active" => {
                Some(MediaStatus::InProgress)
            }
            "completed" | "complete" | "finished" | "done" => Some(MediaStatus::Completed),
            "paused" | "on_hold" | "hold" => Some(MediaStatus::Paused),
            "dropped" | "abandoned" => Some(MediaStatus::Dropped),
            _ => None,
        }
    }

    /// Small, explicit transition matrix. The rules exist to keep state
    /// explainable, not to prevent legitimate personal data: anything can be
    /// re-opened, and only a re-started item returns to `planned`.
    pub fn can_transition_to(&self, to: MediaStatus) -> bool {
        use MediaStatus::*;
        matches!(
            (self, to),
            (Planned, InProgress)
                | (Planned, Completed)
                | (Planned, Dropped)
                | (InProgress, Planned)
                | (InProgress, Paused)
                | (InProgress, Completed)
                | (InProgress, Dropped)
                | (Paused, Planned)
                | (Paused, InProgress)
                | (Paused, Completed)
                | (Paused, Dropped)
                | (Dropped, Planned)
                | (Dropped, InProgress)
                | (Dropped, Paused)
                | (Dropped, Completed)
                | (Completed, InProgress)
                | (Completed, Paused)
                | (Completed, Dropped)
        )
    }
}

impl fmt::Display for MediaStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Structured progress. Never stored as a single formatted string.
///
/// Numbers may be omitted entirely (common for games with unknown progress);
/// a unit without any number is meaningless and rejected.
#[derive(Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Progress {
    pub current: Option<f64>,
    pub total: Option<f64>,
    pub unit: Option<String>,
}

impl Progress {
    pub const fn is_empty(&self) -> bool {
        self.current.is_none() && self.total.is_none() && self.unit.is_none()
    }
}

/// Media typed details owned by the Media module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaRecord {
    pub asset_id: AssetId,
    pub media_type: MediaType,
    pub status: MediaStatus,
    /// Rating on a 0..=10 scale.
    pub rating: Option<f64>,
    pub year: Option<i32>,
    pub platform: Option<String>,
    pub progress: Progress,
    pub notes: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

impl MediaRecord {
    pub fn new(asset_id: AssetId, media_type: MediaType, status: MediaStatus) -> Self {
        MediaRecord {
            asset_id,
            media_type,
            status,
            rating: None,
            year: None,
            platform: None,
            progress: Progress::default(),
            notes: None,
            started_at: None,
            completed_at: None,
        }
    }

    /// Record-level invariants. These apply to every write path, including
    /// import; interactive status changes additionally go through the
    /// transition matrix in [`MediaRecord::transition_to`].
    pub fn validate(&self) -> AppResult<()> {
        if let Some(rating) = self.rating {
            if !rating.is_finite() {
                return Err(AppError::validation(format!(
                    "rating must be a finite number, got {rating}"
                )));
            }
            if !(0.0..=10.0).contains(&rating) {
                return Err(AppError::validation(format!(
                    "rating must be between 0 and 10, got {rating}"
                )));
            }
        }

        if let Some(year) = self.year {
            if !(1850..=2100).contains(&year) {
                return Err(AppError::validation(format!(
                    "year must be between 1850 and 2100, got {year}"
                )));
            }
        }

        let Progress {
            current,
            total,
            unit,
        } = &self.progress;

        // Non-finite values would not survive SQLite/JSON round trips, so
        // they are rejected before any range comparison (NaN comparisons are
        // always false and would otherwise slip through).
        if let Some(value) = current {
            if !value.is_finite() {
                return Err(AppError::validation(format!(
                    "progress_current must be a finite number, got {value}"
                )));
            }
            if *value < 0.0 {
                return Err(AppError::validation(
                    "progress_current must not be negative",
                ));
            }
        }
        if let Some(value) = total {
            if !value.is_finite() {
                return Err(AppError::validation(format!(
                    "progress_total must be a finite number, got {value}"
                )));
            }
            if *value <= 0.0 {
                return Err(AppError::validation("progress_total must be positive"));
            }
        }
        if unit.is_some() && current.is_none() && total.is_none() {
            return Err(AppError::validation(
                "progress_unit requires at least one progress value",
            ));
        }
        if let (Some(cur), Some(tot)) = (current, total) {
            if cur > tot {
                return Err(AppError::validation(
                    "progress_current must not exceed progress_total",
                ));
            }
        }

        if self.completed_at.is_some() && self.status != MediaStatus::Completed {
            return Err(AppError::validation(
                "completed_at may only be set when status is completed",
            ));
        }
        if self.started_at.is_some() && self.status == MediaStatus::Planned {
            return Err(AppError::validation(
                "planned media must not have started_at set",
            ));
        }
        if let (Some(started), Some(completed)) = (self.started_at, self.completed_at) {
            if completed < started {
                return Err(AppError::validation(
                    "completed_at must not precede started_at",
                ));
            }
        }
        Ok(())
    }

    /// Applies an interactive status change, maintaining the timestamp fields
    /// consistently with the transition matrix.
    ///
    /// `started_at` records when the item was last (re-)started; returning to
    /// `planned` clears it. Completing sets `completed_at`; leaving
    /// `completed` clears it so the invariant "completed_at only when
    /// completed" holds.
    pub fn transition_to(&mut self, to: MediaStatus, now: Timestamp) -> AppResult<()> {
        if self.status == to {
            return Err(AppError::validation(format!("media is already {to}")));
        }
        if !self.status.can_transition_to(to) {
            return Err(AppError::validation(format!(
                "cannot transition media from {} to {to}",
                self.status
            )));
        }

        match to {
            MediaStatus::Planned => {
                self.started_at = None;
            }
            MediaStatus::InProgress => {
                if self.started_at.is_none() {
                    self.started_at = Some(now);
                }
            }
            MediaStatus::Completed => {
                self.completed_at = Some(now);
                if self.started_at.is_none() {
                    self.started_at = Some(now);
                }
            }
            MediaStatus::Paused | MediaStatus::Dropped => {}
        }

        if self.status == MediaStatus::Completed && to != MediaStatus::Completed {
            self.completed_at = None;
        }

        self.status = to;
        Ok(())
    }
}

/// An asset together with its media details — the joined view the Media
/// repository returns for list/detail queries.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaEntry {
    pub asset: crate::domain::asset::Asset,
    pub record: MediaRecord,
}

impl fmt::Debug for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.current, &self.total, &self.unit) {
            (Some(c), Some(t), Some(u)) => write!(f, "{c}/{t} {u}"),
            (Some(c), None, Some(u)) => write!(f, "{c} {u}"),
            (None, Some(t), Some(u)) => write!(f, "?/{t} {u}"),
            _ => f.write_str("(no progress)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(status: MediaStatus) -> MediaRecord {
        MediaRecord::new(AssetId::generate(), MediaType::Anime, status)
    }

    #[test]
    fn rating_range_is_validated() {
        let mut r = record(MediaStatus::Planned);
        r.rating = Some(10.0);
        assert!(r.validate().is_ok());
        r.rating = Some(-0.1);
        assert!(r.validate().is_err());
        r.rating = Some(10.5);
        assert!(r.validate().is_err());
    }

    #[test]
    fn progress_must_not_be_negative_or_exceed_total() {
        let mut r = record(MediaStatus::InProgress);
        r.progress = Progress {
            current: Some(-1.0),
            total: Some(28.0),
            unit: Some("episode".into()),
        };
        assert!(r.validate().is_err());

        r.progress = Progress {
            current: Some(29.0),
            total: Some(28.0),
            unit: Some("episode".into()),
        };
        assert!(r.validate().is_err());

        r.progress = Progress {
            current: Some(18.0),
            total: Some(28.0),
            unit: Some("episode".into()),
        };
        assert!(r.validate().is_ok());
    }

    #[test]
    fn non_finite_progress_and_rating_are_rejected() {
        let mut r = record(MediaStatus::InProgress);
        r.progress = Progress {
            current: Some(f64::NAN),
            total: None,
            unit: Some("episode".into()),
        };
        assert!(r.validate().is_err(), "NaN must be rejected");

        r.progress = Progress {
            current: Some(f64::INFINITY),
            total: None,
            unit: Some("episode".into()),
        };
        assert!(r.validate().is_err(), "infinity must be rejected");

        r.progress = Progress {
            current: None,
            total: Some(f64::NEG_INFINITY),
            unit: Some("episode".into()),
        };
        assert!(r.validate().is_err(), "-infinity must be rejected");

        r.rating = Some(f64::NAN);
        r.progress = Progress::default();
        assert!(r.validate().is_err(), "NaN rating must be rejected");
    }

    #[test]
    fn progress_numbers_may_be_absent_but_unit_alone_is_rejected() {
        let mut r = record(MediaStatus::InProgress);
        // Games may omit numeric progress entirely.
        r.progress = Progress::default();
        assert!(r.validate().is_ok());

        r.progress = Progress {
            current: None,
            total: None,
            unit: Some("percent".into()),
        };
        assert!(r.validate().is_err());
    }

    #[test]
    fn completed_at_must_not_precede_started_at() {
        let mut r = record(MediaStatus::Completed);
        let t0 = chrono::Utc::now();
        r.started_at = Some(t0);
        r.completed_at = Some(t0 - chrono::Duration::seconds(1));
        assert!(r.validate().is_err());

        r.completed_at = Some(t0 + chrono::Duration::seconds(1));
        assert!(r.validate().is_ok());
    }

    #[test]
    fn status_transition_matrix_is_enforced() {
        let now = chrono::Utc::now();
        let mut r = record(MediaStatus::Planned);
        r.transition_to(MediaStatus::InProgress, now).unwrap();
        assert_eq!(r.status, MediaStatus::InProgress);
        assert!(r.started_at.is_some());

        // Same-status transition is a no-op error.
        assert!(r.transition_to(MediaStatus::InProgress, now).is_err());

        r.transition_to(MediaStatus::Completed, now).unwrap();
        assert!(r.completed_at.is_some());

        // Re-opening clears completed_at.
        r.transition_to(MediaStatus::InProgress, now).unwrap();
        assert!(r.completed_at.is_none());

        // Completed -> Planned is not allowed.
        r.transition_to(MediaStatus::Completed, now).unwrap();
        assert!(r.transition_to(MediaStatus::Planned, now).is_err());

        // Back to planned clears started_at.
        r.transition_to(MediaStatus::Paused, now).unwrap();
        r.transition_to(MediaStatus::Planned, now).unwrap();
        assert!(r.started_at.is_none());
        assert!(r.validate().is_ok());
    }

    #[test]
    fn planned_media_must_not_have_started_at() {
        let mut r = record(MediaStatus::Planned);
        r.started_at = Some(chrono::Utc::now());
        assert!(r.validate().is_err());
    }

    #[test]
    fn media_type_and_asset_kind_stay_compatible() {
        assert_eq!(MediaType::Anime.asset_kind(), AssetKind::MediaAnime);
        assert_eq!(
            MediaType::from_asset_kind(AssetKind::MediaGame),
            Some(MediaType::Game)
        );
    }

    #[test]
    fn status_parsing_accepts_common_aliases() {
        assert_eq!(
            MediaStatus::parse("Watching"),
            Some(MediaStatus::InProgress)
        );
        assert_eq!(
            MediaStatus::parse("Completed"),
            Some(MediaStatus::Completed)
        );
        assert_eq!(MediaStatus::parse("nonsense"), None);
        assert_eq!(MediaType::parse("TV"), Some(MediaType::Tv));
        assert_eq!(MediaType::parse("game"), Some(MediaType::Game));
    }
}
