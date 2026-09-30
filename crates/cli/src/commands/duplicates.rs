use crate::format::print_duplicate_candidates;
use assetmesh_core::application::duplicate_review_service::{
    DuplicateQuery, DuplicateReviewService,
};
use assetmesh_core::application::library_service::PageRequest;
use assetmesh_core::domain::asset::AssetKind;
use assetmesh_core::{AppError, AppResult};
use clap::Subcommand;

use crate::commands::SharedFactory;

#[derive(Subcommand)]
pub(crate) enum DuplicatesCommand {
    /// Review likely duplicate pairs. Never merges anything.
    List {
        /// Restrict to asset kinds such as software.cli (repeatable).
        #[arg(long = "kind")]
        kinds: Vec<String>,
        /// Exclude archived assets from the review.
        #[arg(long)]
        active_only: bool,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long)]
        json: bool,
    },
}

/// Duplicate review commands (Phase 4C): review only, never a merge.
pub(crate) fn run_duplicates(
    factory: SharedFactory,
    cmd: DuplicatesCommand,
) -> Result<(), AppError> {
    let mut service = DuplicateReviewService::new(factory.clone());
    match cmd {
        DuplicatesCommand::List {
            kinds,
            active_only,
            limit,
            offset,
            json,
        } => {
            let kinds = kinds
                .iter()
                .map(|kind| {
                    AssetKind::parse(kind)
                        .ok_or_else(|| AppError::validation(format!("unknown asset kind {kind:?}")))
                })
                .collect::<AppResult<Vec<_>>>()?;
            let query = DuplicateQuery {
                kinds,
                include_archived: !active_only,
                page: PageRequest::new(limit, offset),
            };
            let page = service.candidates(&query)?;
            print_duplicate_candidates(&page, json);
        }
    }
    Ok(())
}
