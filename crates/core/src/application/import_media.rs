//! Legacy media import pipeline (docs/08 import contract, ADR 0005).
//!
//! parse → validate/normalize → match → plan → review (dry run) → commit →
//! report. Matching precedence:
//!
//! 1. canonical AssetMesh ID (for AssetMesh-native data);
//! 2. exact namespaced external reference;
//! 3. normalized title + media type + year — reported as a **potential
//!    duplicate conflict**, never auto-applied (ADR 0005 classifies
//!    title/type/year matching as heuristic; only canonical IDs and exact
//!    external references may update canonical data in Media V1);
//! 4. heuristic (normalized title + media type) — same: reported, never
//!    written.
//!
//! External references are kept ownerless until commit and re-pointed at the
//! matched/created asset on both the create and update paths, so planning and
//! commit stay symmetric. The commit phase writes in bounded transaction
//! batches; if a batch fails, the report discloses exactly what was already
//! committed. Parsing and matching happen entirely outside write
//! transactions.

mod apply;
mod matching;
mod normalize;

use crate::application::import_parse::{
    detect_format, parse_csv, parse_json, ImportCandidate, ImportFormat, ParsedRow,
};
use crate::application::{SharedClock, SharedIdGenerator};
use crate::domain::ids::AssetId;
use crate::ports::uow::UnitOfWorkFactory;
use crate::AppResult;
use matching::PlannedAction;
use normalize::normalize_candidate;
use serde::Serialize;

/// Records per committed transaction batch (ADR 0007: bounded batches).
const BATCH_SIZE: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RejectedRecord {
    pub index: usize,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportConflict {
    pub index: usize,
    pub title: String,
    pub candidate_asset_id: Option<AssetId>,
    pub reason: String,
}

/// Disclosure of a partial import: batches already committed stay committed,
/// the remaining records were not applied.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportFailure {
    pub error: String,
    pub records_committed: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportReport {
    pub input: usize,
    pub valid: usize,
    pub create: usize,
    pub update: usize,
    pub unchanged: usize,
    pub potential_duplicates: usize,
    pub rejected: usize,
    pub conflicts: Vec<ImportConflict>,
    pub rejected_records: Vec<RejectedRecord>,
    pub dry_run: bool,
    /// Set when a commit batch failed mid-run: earlier batches are already
    /// committed (see `records_committed`), later batches were not applied.
    pub failed: Option<ImportFailure>,
}

#[derive(Debug, Clone)]
pub enum ImportFormatHint {
    Auto,
    Json,
    Csv,
}

pub struct MediaImportService<F: UnitOfWorkFactory> {
    factory: F,
    clock: SharedClock,
    ids: SharedIdGenerator,
}

impl<F: UnitOfWorkFactory> MediaImportService<F> {
    pub fn new(factory: F, clock: SharedClock, ids: SharedIdGenerator) -> Self {
        MediaImportService {
            factory,
            clock,
            ids,
        }
    }

    /// Imports media records from file content. With `dry_run` nothing is
    /// written and the report shows what *would* happen.
    pub fn import(
        &mut self,
        content: &str,
        format: ImportFormatHint,
        dry_run: bool,
    ) -> AppResult<ImportReport> {
        let format = match format {
            ImportFormatHint::Auto => detect_format(content, None),
            ImportFormatHint::Json => ImportFormat::Json,
            ImportFormatHint::Csv => ImportFormat::Csv,
        };

        // Parse — outside any transaction.
        let parsed = match format {
            ImportFormat::Json => parse_json(content)?,
            ImportFormat::Csv => parse_csv(content)?,
        };

        let now = self.clock.now();
        let ids = self.ids.clone();

        // Validate + normalize.
        let mut candidates: Vec<ImportCandidate> = Vec::new();
        let mut rejected_records = Vec::new();
        for row in parsed {
            match row {
                ParsedRow::Malformed { index, reason } => {
                    rejected_records.push(RejectedRecord { index, reason });
                }
                ParsedRow::Raw { index, raw } => match normalize_candidate(*raw, index) {
                    Ok(candidate) => candidates.push(candidate),
                    Err(reason) => rejected_records.push(RejectedRecord { index, reason }),
                },
            }
        }

        // Match + plan — read-only, outside any write transaction. Assets
        // created or updated by this run are recorded in the match index as
        // they are planned, so later rows resolve against them consistently.
        let mut candidates = candidates.into_iter();
        let (mut plan, conflicts) = self
            .factory
            .read(&mut |q| matching::plan(q, &mut candidates, &ids))?;

        let create = plan
            .iter()
            .filter(|(_, a)| matches!(a, PlannedAction::Create))
            .count();
        let update = plan
            .iter()
            .filter(|(_, a)| matches!(a, PlannedAction::Update { .. }))
            .count();

        let mut report = ImportReport {
            input: plan.len() + rejected_records.len(),
            valid: plan.len(),
            create,
            update,
            unchanged: 0,
            potential_duplicates: conflicts.len(),
            rejected: rejected_records.len(),
            conflicts,
            rejected_records,
            dry_run,
            failed: None,
        };

        if dry_run {
            return Ok(report);
        }

        // Commit in bounded batches. If a batch fails, earlier committed
        // batches remain and the report discloses that state explicitly
        // instead of hiding partial work behind a bare error.
        let mut committed = (0usize, 0usize, 0usize);
        for chunk in plan.chunks_mut(BATCH_SIZE) {
            match apply::commit_chunk(&mut self.factory, &self.ids, chunk, now) {
                Ok((c, u, n)) => committed = (committed.0 + c, committed.1 + u, committed.2 + n),
                Err(error) => {
                    report.failed = Some(ImportFailure {
                        error: error.to_string(),
                        records_committed: committed.0 + committed.1 + committed.2,
                    });
                    break;
                }
            }
        }

        report.create = committed.0;
        report.update = committed.1;
        report.unchanged = committed.2;
        Ok(report)
    }
}
