use crate::format::{print_graph_nodes, print_neighbor_views, print_relation_views};
use assetmesh_core::application::relation_query_service::{
    RelationQueryService, TraversalDirection, TraversalOptions,
};
use assetmesh_core::application::relation_service::RelationService;
use assetmesh_core::domain::ids::RelationId;
use assetmesh_core::domain::relation::{RelationProvenance, RelationType};
use assetmesh_core::{AppError, SharedClock, SharedIdGenerator};
use clap::{Subcommand, ValueEnum};

use crate::commands::asset::resolve_asset_id;
use crate::commands::SharedFactory;

#[derive(Subcommand)]
pub(crate) enum RelationCommand {
    /// Attach a relation between two assets.
    Add {
        source: String,
        /// Relation type, e.g. depends_on, uses, hosted_on, points_to, related_to.
        relation_type: String,
        target: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// List relations touching an asset (inverse semantics resolved).
    List { asset: String },
    /// Remove a relation by its id (see `relation list`).
    Remove { relation_id: String },
    /// Every directly connected asset, both directions (Phase 4B).
    Neighbors {
        asset: String,
        /// Restrict to relation types, e.g. depends_on (repeatable).
        #[arg(long = "type")]
        relation_types: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// What this asset depends on, transitively (Phase 4B).
    Dependencies {
        asset: String,
        /// Hops to follow.
        #[arg(long, default_value_t = 8)]
        depth: usize,
        #[arg(long)]
        json: bool,
    },
    /// What depends on this asset, transitively (Phase 4B).
    Dependents {
        asset: String,
        /// Hops to follow.
        #[arg(long, default_value_t = 8)]
        depth: usize,
        #[arg(long)]
        json: bool,
    },
    /// What could be affected if this asset went away, with paths (Phase 4B).
    Impact {
        asset: String,
        /// Hops to follow.
        #[arg(long, default_value_t = 8)]
        depth: usize,
        #[arg(long)]
        json: bool,
    },
    /// Bounded graph traversal in any direction (Phase 4B).
    Traverse {
        asset: String,
        #[arg(long, value_enum, default_value_t = CliTraversalDirection::Outgoing)]
        direction: CliTraversalDirection,
        /// Restrict to relation types, e.g. depends_on (repeatable).
        #[arg(long = "type")]
        relation_types: Vec<String>,
        /// Hops to follow.
        #[arg(long, default_value_t = 8)]
        depth: usize,
        /// Include archived assets as nodes.
        #[arg(long)]
        include_archived: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliTraversalDirection {
    Outgoing,
    Incoming,
    Both,
}

impl From<CliTraversalDirection> for TraversalDirection {
    fn from(value: CliTraversalDirection) -> Self {
        match value {
            CliTraversalDirection::Outgoing => TraversalDirection::Outgoing,
            CliTraversalDirection::Incoming => TraversalDirection::Incoming,
            CliTraversalDirection::Both => TraversalDirection::Both,
        }
    }
}

/// Traversal options with only the depth set; the service supplies the rest.
pub(crate) fn bounded_options(depth: usize) -> TraversalOptions {
    TraversalOptions {
        max_depth: depth,
        ..TraversalOptions::default()
    }
}

pub(crate) fn parse_relation_types(
    raw: &[String],
) -> Result<Vec<assetmesh_core::domain::relation::RelationType>, AppError> {
    raw.iter()
        .map(|value| {
            assetmesh_core::domain::relation::RelationType::parse(value).ok_or_else(|| {
                AppError::validation(format!(
                    "unknown relation type {value:?}; expected one of: depends_on, dependency_of, \
                     uses, used_by, installed_via, installs, hosted_on, hosts, points_to, \
                     pointed_to_by, related_to"
                ))
            })
        })
        .collect()
}

pub(crate) fn run_relation(
    factory: SharedFactory,
    clock: SharedClock,
    ids: SharedIdGenerator,
    cmd: RelationCommand,
) -> Result<(), AppError> {
    // Phase 4B graph queries are read-only and need neither clock nor ids, but
    // the write/listing commands below do.
    let mut relations = RelationService::new(factory.clone(), clock, ids);

    match cmd {
        RelationCommand::Neighbors {
            asset,
            relation_types,
            json,
        } => {
            let asset_id = resolve_asset_id(&factory, &asset)?;
            let options = TraversalOptions {
                relation_types: parse_relation_types(&relation_types)?,
                ..TraversalOptions::default()
            };
            let views = RelationQueryService::new(factory).neighbors(asset_id, &options)?;
            print_neighbor_views(&views, json);
        }
        RelationCommand::Dependencies { asset, depth, json } => {
            let asset_id = resolve_asset_id(&factory, &asset)?;
            let view = RelationQueryService::new(factory)
                .dependencies(asset_id, &bounded_options(depth))?;
            print_graph_nodes(&view, json);
        }
        RelationCommand::Dependents { asset, depth, json } => {
            let asset_id = resolve_asset_id(&factory, &asset)?;
            let view =
                RelationQueryService::new(factory).dependents(asset_id, &bounded_options(depth))?;
            print_graph_nodes(&view, json);
        }
        RelationCommand::Impact { asset, depth, json } => {
            let asset_id = resolve_asset_id(&factory, &asset)?;
            let view =
                RelationQueryService::new(factory).impact(asset_id, &bounded_options(depth))?;
            print_graph_nodes(&view, json);
        }
        RelationCommand::Traverse {
            asset,
            direction,
            relation_types,
            depth,
            include_archived,
            json,
        } => {
            let asset_id = resolve_asset_id(&factory, &asset)?;
            let options = TraversalOptions {
                direction: direction.into(),
                relation_types: parse_relation_types(&relation_types)?,
                max_depth: depth,
                include_archived,
            };
            let view = RelationQueryService::new(factory).traverse(asset_id, &options)?;
            print_graph_nodes(&view, json);
        }
        RelationCommand::Add {
            source,
            relation_type,
            target,
            note,
        } => {
            let source_id = resolve_asset_id(&factory, &source)?;
            let target_id = resolve_asset_id(&factory, &target)?;
            let relation_type = RelationType::parse(&relation_type).ok_or_else(|| {
                AppError::validation(format!(
                    "unknown relation type {relation_type:?}; expected one of: depends_on, \
                     dependency_of, uses, used_by, installed_via, installs, hosted_on, hosts, \
                     points_to, pointed_to_by, related_to"
                ))
            })?;
            let relation = relations.attach(
                source_id,
                relation_type,
                target_id,
                note,
                RelationProvenance::Manual,
            )?;
            println!(
                "attached {} {} → {}",
                relation.relation_type, relation.source_asset_id, relation.target_asset_id
            );
        }
        RelationCommand::List { asset } => {
            let asset_id = resolve_asset_id(&factory, &asset)?;
            let views = relations.list_for_asset(asset_id)?;
            print_relation_views(&views);
        }
        RelationCommand::Remove { relation_id } => {
            let id = RelationId::from_uuid(
                uuid::Uuid::parse_str(relation_id.trim())
                    .map_err(|_| AppError::validation("relation id must be a UUID"))?,
            );
            relations.remove(id)?;
            println!("removed {id}");
        }
    }
    Ok(())
}
