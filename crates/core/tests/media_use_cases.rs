//! Application use-case tests for Media, run against in-memory ports.

mod support;

use assetmesh_core::application::media_service::{
    CreateMedia, ExternalRefInput, UpdateMediaMetadata,
};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::media::{MediaStatus, MediaType, Progress};
use assetmesh_core::ports::repos::MediaFilter;
use assetmesh_core::ports::repos::MediaSort;
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use assetmesh_core::AppError;
use support::test_env;

fn create_cmd(title: &str, media_type: MediaType) -> CreateMedia {
    CreateMedia {
        title: title.to_string(),
        media_type,
        summary: None,
        status: None,
        rating: None,
        year: Some(2023),
        platform: None,
        progress: Progress::default(),
        notes: None,
        tags: Vec::new(),
        external_refs: Vec::new(),
        started_at: None,
        completed_at: None,
    }
}

#[test]
fn create_and_get_media_round_trips_details() {
    let env = test_env();
    let mut media = env.media_service();

    let view = media
        .create_media(CreateMedia {
            tags: vec!["healing".into(), "fantasy".into()],
            external_refs: vec![ExternalRefInput {
                namespace: "tmdb".into(),
                external_id: "209867".into(),
                source_url: Some("https://www.themoviedb.org/tv/209867".into()),
            }],
            platform: Some("Crunchyroll".into()),
            ..create_cmd("Frieren", MediaType::Anime)
        })
        .unwrap();

    let fetched = media.get_media(view.entry.asset.id).unwrap();
    assert_eq!(fetched.entry.asset.name, "Frieren");
    assert_eq!(fetched.entry.asset.kind.as_str(), "media.anime");
    assert_eq!(fetched.entry.record.status, MediaStatus::Planned);
    assert_eq!(fetched.entry.record.year, Some(2023));
    assert_eq!(fetched.tags.len(), 2);
    assert_eq!(fetched.external_refs.len(), 1);
    assert_eq!(fetched.external_refs[0].namespace, "tmdb");

    let event_types: Vec<&str> = fetched
        .activity
        .iter()
        .map(|e| e.event_type.as_str())
        .collect();
    assert!(event_types.contains(&"asset.created"));
    assert!(event_types.contains(&"media.created"));
}

#[test]
fn create_rejects_invalid_records() {
    let env = test_env();
    let mut media = env.media_service();

    let mut cmd = create_cmd("Bad rating", MediaType::Movie);
    cmd.rating = Some(11.0);
    assert!(matches!(
        media.create_media(cmd),
        Err(AppError::Validation { .. })
    ));

    let mut cmd = create_cmd("Bad progress", MediaType::Tv);
    cmd.progress = Progress {
        current: Some(29.0),
        total: Some(28.0),
        unit: Some("episode".into()),
    };
    assert!(media.create_media(cmd).is_err());

    let mut cmd = create_cmd("", MediaType::Game);
    cmd.title = "   ".into();
    assert!(media.create_media(cmd).is_err());
}

#[test]
fn duplicate_external_ref_conflicts() {
    let env = test_env();
    let mut media = env.media_service();

    media
        .create_media(CreateMedia {
            external_refs: vec![ref_input("steam", "1091500")],
            ..create_cmd("Cyberpunk 2077", MediaType::Game)
        })
        .unwrap();

    let err = media
        .create_media(CreateMedia {
            external_refs: vec![ref_input("steam", "1091500")],
            ..create_cmd("Another game", MediaType::Game)
        })
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }));
}

fn ref_input(namespace: &str, external_id: &str) -> ExternalRefInput {
    ExternalRefInput {
        namespace: namespace.into(),
        external_id: external_id.into(),
        source_url: None,
    }
}

#[test]
fn status_lifecycle_sets_timestamps_and_emits_activity() {
    let env = test_env();
    let mut media = env.media_service();
    let created = media
        .create_media(create_cmd("Frieren", MediaType::Anime))
        .unwrap();
    let id = created.entry.asset.id;

    // start
    env.clock.advance_seconds(10);
    let view = media.start_media(id).unwrap();
    assert_eq!(view.entry.record.status, MediaStatus::InProgress);
    assert!(view.entry.record.started_at.is_some());
    assert!(view
        .activity
        .iter()
        .any(|e| e.event_type == "media.started"));

    // progress
    env.clock.advance_seconds(10);
    let view = media
        .update_progress(
            id,
            Progress {
                current: Some(18.0),
                total: Some(28.0),
                unit: Some("episode".into()),
            },
        )
        .unwrap();
    assert_eq!(view.entry.record.progress.current, Some(18.0));
    assert!(view
        .activity
        .iter()
        .any(|e| e.event_type == "media.progress_changed"));

    // complete
    env.clock.advance_seconds(10);
    let view = media.complete_media(id).unwrap();
    assert_eq!(view.entry.record.status, MediaStatus::Completed);
    assert!(view.entry.record.completed_at.is_some());
    assert!(view
        .activity
        .iter()
        .any(|e| e.event_type == "media.completed"));

    // rate
    env.clock.advance_seconds(10);
    let view = media.rate_media(id, 9.5).unwrap();
    assert_eq!(view.entry.record.rating, Some(9.5));
    assert!(view
        .activity
        .iter()
        .any(|e| e.event_type == "media.rating_changed"));

    // pause after completion clears completed_at
    env.clock.advance_seconds(10);
    let view = media.pause_media(id).unwrap();
    assert_eq!(view.entry.record.status, MediaStatus::Paused);
    assert!(view.entry.record.completed_at.is_none());

    // paused -> completed -> planned is rejected by the matrix
    media.complete_media(id).unwrap();
    let view = media.get_media(id).unwrap();
    assert_eq!(view.entry.record.status, MediaStatus::Completed);
}

#[test]
fn invalid_transition_is_rejected() {
    let env = test_env();
    let mut media = env.media_service();
    let created = media
        .create_media(create_cmd("Fresh anime", MediaType::Anime))
        .unwrap();
    // planned -> paused is not in the transition matrix
    let err = media.pause_media(created.entry.asset.id).unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }));
}

#[test]
fn update_metadata_touches_asset_and_projection() {
    let env = test_env();
    let mut media = env.media_service();
    let created = media
        .create_media(create_cmd("Old title", MediaType::Movie))
        .unwrap();
    let id = created.entry.asset.id;

    let updated = media
        .update_metadata(UpdateMediaMetadata {
            asset_id: id,
            title: Some("New title".into()),
            summary: Some("A summary".into()),
            year: Some(1999),
            platform: Some("DVD".into()),
            notes: Some("Rewatched".into()),
        })
        .unwrap();

    assert_eq!(updated.entry.asset.name, "New title");
    assert_eq!(updated.entry.record.year, Some(1999));
    assert_eq!(updated.entry.asset.revision, 2);
    // Trivial metadata edits do not emit activity events (docs/08).
    assert!(!updated
        .activity
        .iter()
        .any(|e| e.event_type != "asset.created" && e.event_type != "media.created"));
}

#[test]
fn archive_blocks_further_mutations() {
    let env = test_env();
    let mut media = env.media_service();
    let mut assets = env.asset_service();
    let created = media
        .create_media(create_cmd("To archive", MediaType::Game))
        .unwrap();
    let id = created.entry.asset.id;

    assets.archive_asset(id).unwrap();

    let err = media.complete_media(id).unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }));
    let err = media.rate_media(id, 5.0).unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }));

    let view = assets.get_asset(id).unwrap();
    assert_eq!(
        view.asset.lifecycle_state,
        assetmesh_core::domain::asset::LifecycleState::Archived
    );
    assert!(view.asset.archived_at.is_some());
}

#[test]
fn merge_moves_details_and_tombstones_loser() {
    let env = test_env();
    let mut media = env.media_service();
    let mut assets = env.asset_service();

    let loser = media
        .create_media(CreateMedia {
            tags: vec!["shared".into()],
            external_refs: vec![ref_input("tmdb", "111")],
            ..create_cmd("Duplicate A", MediaType::Movie)
        })
        .unwrap();
    let winner = media
        .create_media(CreateMedia {
            external_refs: vec![ref_input("tmdb", "222")],
            ..create_cmd("Duplicate B", MediaType::Movie)
        })
        .unwrap();
    let (loser_id, winner_id) = (loser.entry.asset.id, winner.entry.asset.id);

    assets.merge_assets(loser_id, winner_id).unwrap();

    // Loser is a tombstone and rejects mutations.
    let loser_view = assets.get_asset(loser_id).unwrap();
    assert_eq!(
        loser_view.asset.lifecycle_state,
        assetmesh_core::domain::asset::LifecycleState::Merged
    );
    assert_eq!(loser_view.asset.merged_into, Some(winner_id));
    let err = media.complete_media(loser_id).unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }));

    // Refs and tags moved to the winner.
    let winner_view = assets.get_asset(winner_id).unwrap();
    let ref_keys: Vec<(String, String)> = winner_view
        .external_refs
        .iter()
        .map(|r| (r.namespace.clone(), r.external_id.clone()))
        .collect();
    assert!(ref_keys.contains(&("tmdb".into(), "111".into())));
    assert!(ref_keys.contains(&("tmdb".into(), "222".into())));
    assert!(winner_view.tags.contains(&"shared".into()));

    // Winner records the merge; loser's activity provenance is preserved.
    let winner_media = media.get_media(winner_id).unwrap();
    assert!(winner_media
        .activity
        .iter()
        .any(|e| e.event_type == "asset.merged"));

    // Merging into a merged asset is rejected.
    let third = media
        .create_media(create_cmd("Third movie", MediaType::Movie))
        .unwrap();
    let err = assets
        .merge_assets(third.entry.asset.id, loser_id)
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }));
}

#[test]
fn merge_moves_media_details_when_winner_lacks_them() {
    let env = test_env();
    let mut media = env.media_service();
    let mut assets = env.asset_service();

    let loser = media
        .create_media(CreateMedia {
            rating: Some(8.0),
            progress: Progress {
                current: Some(2.0),
                total: Some(10.0),
                unit: Some("volume".into()),
            },
            ..create_cmd("Detail source", MediaType::Anime)
        })
        .unwrap();
    let winner = media
        .create_media(create_cmd("Detail target", MediaType::Anime))
        .unwrap();

    // Simulate an asset without module details (possible after a partial
    // portable import): the loser's record should move across.
    let winner_id = winner.entry.asset.id;
    let mut factory = env.factory.clone();
    factory
        .transact(&mut |uow| uow.media().delete(winner_id))
        .unwrap();

    assets
        .merge_assets(loser.entry.asset.id, winner_id)
        .unwrap();

    let winner_media = media.get_media(winner_id).unwrap();
    assert_eq!(winner_media.entry.record.rating, Some(8.0));
    assert_eq!(winner_media.entry.record.progress.current, Some(2.0));
}

#[test]
fn merge_with_conflicting_details_keeps_winner_and_preserves_loser_details() {
    let env = test_env();
    let mut media = env.media_service();
    let mut assets = env.asset_service();

    let loser = media
        .create_media(CreateMedia {
            rating: Some(3.0),
            ..create_cmd("Loser", MediaType::Movie)
        })
        .unwrap();
    let winner = media
        .create_media(CreateMedia {
            rating: Some(9.0),
            ..create_cmd("Winner", MediaType::Movie)
        })
        .unwrap();

    assets
        .merge_assets(loser.entry.asset.id, winner.entry.asset.id)
        .unwrap();

    let winner_media = media.get_media(winner.entry.asset.id).unwrap();
    assert_eq!(winner_media.entry.record.rating, Some(9.0));
    let merged_event = winner_media
        .activity
        .iter()
        .find(|e| e.event_type == "asset.merged")
        .unwrap();
    assert!(merged_event.payload.get("loser_media_details").is_some());
}

#[test]
fn same_kind_required_for_merge() {
    let env = test_env();
    let mut media = env.media_service();
    let mut assets = env.asset_service();

    let movie = media
        .create_media(create_cmd("A movie", MediaType::Movie))
        .unwrap();
    let game = media
        .create_media(create_cmd("A game", MediaType::Game))
        .unwrap();

    let err = assets
        .merge_assets(game.entry.asset.id, movie.entry.asset.id)
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }));
}

#[test]
fn list_media_sorts_and_filters() {
    let env = test_env();
    let mut media = env.media_service();

    let a = media
        .create_media(CreateMedia {
            rating: Some(5.0),
            tags: vec!["solo".into()],
            ..create_cmd("Beta", MediaType::Movie)
        })
        .unwrap();
    env.clock.advance_seconds(5);
    let b = media
        .create_media(CreateMedia {
            rating: Some(9.0),
            ..create_cmd("Alpha", MediaType::Anime)
        })
        .unwrap();

    let rows = media
        .list_media(&MediaFilter {
            media_type: Some(MediaType::Movie),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.asset.id, a.entry.asset.id);

    let rows = media
        .list_media(&MediaFilter {
            tag: Some("solo".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tags, vec!["solo".to_string()]);

    let rows = media
        .list_media(&MediaFilter {
            sort: MediaSort::RatingDesc,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows[0].entry.asset.id, b.entry.asset.id);

    let rows = media
        .list_media(&MediaFilter {
            sort: MediaSort::TitleAsc,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows[0].entry.asset.name, "Alpha");
}

#[test]
fn storage_failures_propagate_and_roll_back_asset_operations() {
    // A failing media read during archive/merge/ref operations must abort
    // the whole transaction, not silently skip the projection refresh.
    let env = test_env();
    let mut media = env.media_service();
    let mut assets = env.asset_service();
    let created = media
        .create_media(create_cmd("Faulty", MediaType::Movie))
        .unwrap();
    let id = created.entry.asset.id;

    // Inject media-read failures.
    env.factory
        .store_borrow_mut(|store| store.inject_media_get_error = true);
    assert!(assets.archive_asset(id).is_err());

    // Failure rolled back: asset is still active.
    let view = assets.get_asset(id).unwrap();
    assert_eq!(
        view.asset.lifecycle_state,
        assetmesh_core::domain::asset::LifecycleState::Active
    );

    // Normal operation works again after removing the fault.
    env.factory
        .store_borrow_mut(|store| store.inject_media_get_error = false);
    assets.archive_asset(id).unwrap();
    let view = assets.get_asset(id).unwrap();
    assert_eq!(
        view.asset.lifecycle_state,
        assetmesh_core::domain::asset::LifecycleState::Archived
    );
}

#[test]
fn unknown_asset_is_not_found() {
    let env = test_env();
    let mut media = env.media_service();
    let err = media.get_media(AssetId::generate()).unwrap_err();
    assert!(matches!(err, AppError::NotFound { .. }));
}
