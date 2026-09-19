//! Legacy import pipeline tests: parse/validate/normalize/match/plan/
//! commit/report behavior including dry-run and conflict semantics.

mod support;

use assetmesh_core::application::import_media::ImportFormatHint;
use assetmesh_core::application::media_service::CreateMedia;
use assetmesh_core::domain::media::{MediaType, Progress};
use support::test_env;

fn json_input() -> String {
    r#"[
        {
            "title": "Frieren",
            "media_type": "anime",
            "status": "completed",
            "year": 2023,
            "rating": 9.5,
            "platform": "Crunchyroll",
            "progress_current": 28,
            "progress_total": 28,
            "progress_unit": "episode",
            "tags": ["healing", "fantasy"],
            "external_refs": {"tmdb": "209867"}
        },
        {
            "name": "Cyberpunk 2077",
            "type": "game",
            "status": "playing",
            "progress_current": 45,
            "progress_total": 100,
            "progress_unit": "percent"
        }
    ]"#
    .to_string()
}

#[test]
fn json_import_creates_valid_records() {
    let env = test_env();
    let mut importer = env.import_service();

    let report = importer
        .import(&json_input(), ImportFormatHint::Json, false)
        .unwrap();

    assert_eq!(report.input, 2);
    assert_eq!(report.valid, 2);
    assert_eq!(report.create, 2);
    assert_eq!(report.rejected, 0);
    assert_eq!(report.potential_duplicates, 0);

    let mut media = env.media_service();
    let rows = media.list_media(&Default::default()).unwrap();
    assert_eq!(rows.len(), 2);

    let frieren = rows
        .iter()
        .find(|r| r.entry.asset.name == "Frieren")
        .unwrap();
    assert_eq!(
        frieren.entry.record.status,
        assetmesh_core::domain::media::MediaStatus::Completed
    );
    assert_eq!(frieren.entry.record.progress.current, Some(28.0));
    assert!(frieren.tags.contains(&"healing".to_string()));

    let view = media.get_media(frieren.entry.asset.id).unwrap();
    assert!(view
        .external_refs
        .iter()
        .any(|r| r.namespace == "tmdb" && r.external_id == "209867"));
    // import actor is recorded on activity
    assert!(view
        .activity
        .iter()
        .any(|e| e.event_type == "media.created" && e.actor == "import"));
}

#[test]
fn csv_import_maps_headers_and_ignores_bad_rows() {
    let env = test_env();
    let mut importer = env.import_service();

    let csv = "title,media_type,status,year,rating\n\
               Frieren,anime,completed,2023,9.5\n\
               Bad Row,movie,completed,yesterday,8\n\
               No Type,movie\n";
    let report = importer.import(csv, ImportFormatHint::Csv, false).unwrap();

    assert_eq!(report.input, 3);
    assert_eq!(report.valid, 2);
    assert_eq!(report.rejected, 1);
    assert_eq!(report.create, 2);
}

#[test]
fn dry_run_writes_nothing() {
    let env = test_env();
    let mut importer = env.import_service();

    let report = importer
        .import(&json_input(), ImportFormatHint::Json, true)
        .unwrap();
    assert_eq!(report.create, 2);
    assert!(report.dry_run);

    let mut media = env.media_service();
    assert!(media.list_media(&Default::default()).unwrap().is_empty());
}

#[test]
fn rerun_is_idempotent_via_external_refs() {
    let env = test_env();
    let mut importer = env.import_service();

    importer
        .import(&json_input(), ImportFormatHint::Json, false)
        .unwrap();

    // Re-run with slightly different progress: the ref-carrying record
    // matches by external ref and updates. The ref-less Cyberpunk row shares
    // title/type/year with the existing asset — an uncertain normalized-key
    // match, reported as a potential duplicate instead of being applied.
    let changed = json_input().replace(r#""progress_current": 45"#, r#""progress_current": 60"#);
    let report = importer
        .import(&changed, ImportFormatHint::Json, false)
        .unwrap();

    assert_eq!(report.create, 0);
    assert_eq!(report.update + report.unchanged, 1);
    assert_eq!(report.potential_duplicates, 1);

    let mut media = env.media_service();
    let rows = media.list_media(&Default::default()).unwrap();
    let cyberpunk = rows
        .iter()
        .find(|r| r.entry.asset.name == "Cyberpunk 2077")
        .unwrap();
    assert_eq!(
        cyberpunk.entry.record.progress.current,
        Some(45.0),
        "uncertain match must not mutate"
    );
}

#[test]
fn rerun_with_identical_data_reports_unchanged() {
    let env = test_env();
    let mut importer = env.import_service();

    importer
        .import(&json_input(), ImportFormatHint::Json, false)
        .unwrap();
    let report = importer
        .import(&json_input(), ImportFormatHint::Json, false)
        .unwrap();

    assert_eq!(report.create, 0);
    assert_eq!(report.unchanged, 1, "ref-carrying record dedupes exactly");
    assert_eq!(report.update, 0);
    assert_eq!(
        report.potential_duplicates, 1,
        "ref-less record needs identifiers to dedupe"
    );
}

#[test]
fn deterministic_key_matches_same_title_type_year() {
    // ADR 0005: title/type/year matching is heuristic. Two distinct releases
    // can share the tuple, so a normalized-key match is surfaced as a
    // potential duplicate and must never auto-update the matched asset.
    let env = test_env();
    let mut importer = env.import_service();
    let mut media = env.media_service();

    let first = r#"[{"title":"Perfect Blue","media_type":"movie","year":1997,"rating":8.0}]"#;
    let second = r#"[{"title":"perfect   blue","media_type":"movie","year":1997,"rating":8.5,"notes":"different release"}]"#;

    importer
        .import(first, ImportFormatHint::Json, false)
        .unwrap();
    let report = importer
        .import(second, ImportFormatHint::Json, false)
        .unwrap();

    assert_eq!(report.create, 0);
    assert_eq!(report.update, 0);
    assert_eq!(report.potential_duplicates, 1);
    assert!(report.conflicts[0]
        .reason
        .contains("normalized-key match requires review"));

    // Canonical state untouched by the uncertain match.
    let rows = media.list_media(&Default::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.record.rating, Some(8.0));
    assert!(rows[0].entry.record.notes.is_none());
}

#[test]
fn heuristic_title_match_is_reported_not_merged() {
    let env = test_env();
    let mut media = env.media_service();
    let mut importer = env.import_service();

    media
        .create_media(CreateMedia {
            year: Some(2023),
            ..create_cmd()
        })
        .unwrap();

    // Same normalized title + type, different year: heuristic candidate only.
    let report = importer
        .import(
            r#"[{"title":"FRIEREN","media_type":"anime","year":2024,"rating":7.0}]"#,
            ImportFormatHint::Json,
            false,
        )
        .unwrap();

    assert_eq!(report.create, 0);
    assert_eq!(report.update, 0);
    assert_eq!(report.potential_duplicates, 1);
    assert!(report.conflicts[0].candidate_asset_id.is_some());
    assert!(report.conflicts[0].reason.contains("potential duplicate"));

    // Existing record untouched.
    let rows = media.list_media(&Default::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.record.rating, None);
}

fn create_cmd() -> CreateMedia {
    CreateMedia {
        title: "Frieren".into(),
        media_type: MediaType::Anime,
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
fn ambiguous_external_refs_conflict() {
    let env = test_env();
    let mut media = env.media_service();
    let mut importer = env.import_service();

    media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            external_refs: vec![
                assetmesh_core::application::media_service::ExternalRefInput {
                    namespace: "steam".into(),
                    external_id: "1".into(),
                    source_url: None,
                },
            ],
            ..create_cmd()
        })
        .unwrap();
    media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            external_refs: vec![
                assetmesh_core::application::media_service::ExternalRefInput {
                    namespace: "igdb".into(),
                    external_id: "2".into(),
                    source_url: None,
                },
            ],
            ..create_cmd()
        })
        .unwrap();

    // One record whose refs point at two different assets: ambiguous.
    let report = importer
        .import(
            r#"[{"title":"Whatever","media_type":"anime","external_refs":{"steam":"1","igdb":"2"}}]"#,
            ImportFormatHint::Json,
            false,
        )
        .unwrap();
    assert_eq!(report.potential_duplicates, 1);
    assert!(report.conflicts[0].reason.contains("different assets"));
}

#[test]
fn canonical_id_for_nonexistent_asset_is_conflict() {
    let env = test_env();
    let mut importer = env.import_service();

    let missing_id = uuid::Uuid::now_v7();
    let report = importer
        .import(
            &format!(r#"[{{"title":"Ghost","media_type":"movie","asset_id":"{missing_id}"}}]"#),
            ImportFormatHint::Json,
            false,
        )
        .unwrap();
    assert_eq!(report.potential_duplicates, 1);
    assert!(report.conflicts[0]
        .reason
        .contains(missing_id.to_string().as_str()));
}

#[test]
fn duplicates_within_one_batch_are_deduplicated() {
    // Records carrying the same external reference resolve to one asset:
    // the first plans a create, the second matches the planned ref.
    let env = test_env();
    let mut importer = env.import_service();

    let batch = r#"[
        {"title":"Twin Peaks","media_type":"tv","year":1990,"external_refs":{"tmdb":"111"}},
        {"title":"twin peaks","media_type":"tv","year":1990,"rating":9.0,"external_refs":{"tmdb":"111"}}
    ]"#;
    let report = importer
        .import(batch, ImportFormatHint::Json, false)
        .unwrap();

    assert_eq!(report.create, 1);
    assert_eq!(report.update + report.unchanged, 1);
    assert_eq!(report.potential_duplicates, 0);

    let mut media = env.media_service();
    let rows = media.list_media(&Default::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.record.rating, Some(9.0));
}

#[test]
fn duplicates_without_refs_in_one_batch_conflict() {
    // Without identifiers, two rows with the same title/type/year are
    // uncertain: one create plus one reviewable conflict.
    let env = test_env();
    let mut importer = env.import_service();

    let batch = r#"[
        {"title":"Twin Peaks","media_type":"tv","year":1990},
        {"title":"twin peaks","media_type":"tv","year":1990,"rating":9.0}
    ]"#;
    let report = importer
        .import(batch, ImportFormatHint::Json, false)
        .unwrap();

    assert_eq!(report.create, 1);
    assert_eq!(report.update, 0);
    assert_eq!(report.potential_duplicates, 1);

    let mut media = env.media_service();
    let rows = media.list_media(&Default::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].entry.record.rating, None,
        "conflict must not mutate"
    );
}

#[test]
fn planned_ref_across_batch_boundary_resolves_to_one_asset() {
    // Row 1 (chunk 1) creates an asset with ref steam:1; row 201 (chunk 2)
    // re-uses it: the planned-ref index must resolve it to the same asset
    // even though the create committed in an earlier batch.
    let env = test_env();
    let mut importer = env.import_service();

    let mut records = vec![
        r#"{"title":"Batch One","media_type":"movie","year":2000,"external_refs":{"steam":"1"}}"#
            .to_string(),
    ];
    for i in 1..201 {
        records.push(format!(
            r#"{{"title":"Filler {i}","media_type":"game","year":2001}}"#
        ));
    }
    records.push(
        r#"{"title":"Batch One Followup","media_type":"movie","year":2000,"rating":7.0,"external_refs":{"steam":"1"}}"#
            .to_string(),
    );
    let content = format!("[{}]", records.join(","));

    let report = importer
        .import(&content, ImportFormatHint::Json, false)
        .unwrap();

    assert_eq!(report.create, 201);
    assert_eq!(report.update + report.unchanged, 1);
    assert_eq!(report.failed, None);

    let mut media = env.media_service();
    let rows = media.list_media(&Default::default()).unwrap();
    // The followup row updates the originally created asset (the update
    // policy includes the title), so it now carries the followup title and
    // rating — proof both rows resolved to ONE asset across the boundary.
    let batch_one = rows
        .iter()
        .find(|r| r.entry.asset.name == "Batch One Followup")
        .unwrap();
    assert_eq!(batch_one.entry.record.rating, Some(7.0));
    assert!(rows
        .iter()
        .all(|r| r.entry.record.rating != Some(7.0) || r.entry.asset.name == "Batch One Followup"));
}

#[test]
fn update_enrichment_attaches_new_refs_with_correct_owner() {
    // A matched update carrying a brand-new external ref must insert it
    // owned by the matched asset — never a placeholder ID (FK-safe).
    let env = test_env();
    let mut importer = env.import_service();
    let mut media = env.media_service();

    media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            external_refs: vec![
                assetmesh_core::application::media_service::ExternalRefInput {
                    namespace: "tmdb".into(),
                    external_id: "123".into(),
                    source_url: None,
                },
            ],
            ..create_cmd_movie()
        })
        .unwrap();

    let report = importer
        .import(
            r#"[{"title":"Ref owner","media_type":"movie","year":2020,"external_refs":{"tmdb":"123","igdb":"9"}}]"#,
            ImportFormatHint::Json,
            false,
        )
        .unwrap();

    assert_eq!(report.failed, None);
    assert_eq!(report.update + report.unchanged, 1);

    let rows = media.list_media(&Default::default()).unwrap();
    assert_eq!(rows.len(), 1);
    let id = rows[0].entry.asset.id;
    let view = media.get_media(id).unwrap();
    let refs: Vec<(String, String)> = view
        .external_refs
        .iter()
        .map(|r| (r.namespace.clone(), r.external_id.clone()))
        .collect();
    assert!(refs.contains(&("tmdb".into(), "123".into())));
    assert!(
        refs.contains(&("igdb".into(), "9".into())),
        "new ref attached: {refs:?}"
    );
}

#[test]
fn duplicate_ref_claims_in_one_batch_resolve_to_one_asset() {
    // Two rows in the same chunk plan the same NEW ref: both must resolve
    // to the same planned create instead of colliding on the unique index.
    let env = test_env();
    let mut importer = env.import_service();

    let batch = r#"[
        {"title":"Claim owner","media_type":"game","year":2021,"external_refs":{"steam":"77"}},
        {"title":"claim owner","media_type":"game","year":2021,"external_refs":{"steam":"77"},"rating":6.0}
    ]"#;
    let report = importer
        .import(batch, ImportFormatHint::Json, false)
        .unwrap();
    assert_eq!(report.failed, None);
    assert_eq!(report.create, 1);
    assert_eq!(report.update + report.unchanged, 1);
}

fn create_cmd_movie() -> assetmesh_core::application::media_service::CreateMedia {
    assetmesh_core::application::media_service::CreateMedia {
        title: "Ref owner".into(),
        media_type: MediaType::Movie,
        summary: None,
        status: None,
        rating: None,
        year: Some(2020),
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
fn partial_batch_failure_is_disclosed() {
    // Batch 1 (200 records) commits; batch 2 contains a record that only
    // fails at commit time. The report must disclose what was committed
    // instead of hiding partial work behind a bare error.
    let env = test_env();
    let mut media = env.media_service();
    let mut importer = env.import_service();

    // Pre-create the update target: in_progress, ref fail:1.
    media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            status: Some(assetmesh_core::domain::media::MediaStatus::InProgress),
            external_refs: vec![
                assetmesh_core::application::media_service::ExternalRefInput {
                    namespace: "fail".into(),
                    external_id: "1".into(),
                    source_url: None,
                },
            ],
            ..create_cmd_movie()
        })
        .unwrap();

    let mut records: Vec<String> = (0..200)
        .map(|i| format!(r#"{{"title":"Partial {i}","media_type":"anime","year":2002}}"#))
        .collect();
    // Update record whose completed_at lands on an in_progress target:
    // valid at plan time, fails record validation at commit.
    records.push(
        r#"{"title":"Ref owner","media_type":"movie","year":2020,"completed_at":"2024-01-01","external_refs":{"fail":"1"}}"#
            .to_string(),
    );
    records.push(r#"{"title":"Never applied","media_type":"anime","year":2003}"#.to_string());
    let content = format!("[{}]", records.join(","));

    let report = importer
        .import(&content, ImportFormatHint::Json, false)
        .unwrap();

    let failure = report.failed.expect("partial failure must be disclosed");
    assert!(
        failure.records_committed >= 200,
        "batch 1 stayed committed: {failure:?}"
    );
    assert_eq!(report.create, 200);
    assert_eq!(report.unchanged, 0);

    // Committed batch is durable; later records were not applied.
    let rows = media.list_media(&Default::default()).unwrap();
    assert!(rows.iter().any(|r| r.entry.asset.name == "Partial 199"));
    assert!(!rows.iter().any(|r| r.entry.asset.name == "Never applied"));
    assert!(
        !rows
            .iter()
            .any(|r| r.entry.asset.name == "Ref owner" && r.entry.record.completed_at.is_some()),
        "failed batch rolled back"
    );
}

#[test]
fn non_finite_progress_is_rejected_by_import() {
    // CSV "inf" parses as a float but must be rejected as non-finite.
    let env = test_env();
    let mut importer = env.import_service();

    let csv = "title,media_type,progress_current,progress_total,progress_unit
               Frieren,anime,inf,28,episode
";
    let report = importer.import(csv, ImportFormatHint::Csv, false).unwrap();
    assert_eq!(report.rejected, 1);
    assert_eq!(report.valid, 0);
}

#[test]
fn kind_mismatch_conflicts_instead_of_updating() {
    let env = test_env();
    let mut media = env.media_service();
    let mut importer = env.import_service();

    media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            external_refs: vec![
                assetmesh_core::application::media_service::ExternalRefInput {
                    namespace: "steam".into(),
                    external_id: "7".into(),
                    source_url: None,
                },
            ],
            ..create_cmd()
        })
        .unwrap();

    // The steam:7 ref belongs to an anime asset; record claims to be a movie.
    let report = importer
        .import(
            r#"[{"title":"Wrong kind","media_type":"movie","external_refs":{"steam":"7"}}]"#,
            ImportFormatHint::Json,
            false,
        )
        .unwrap();
    assert_eq!(report.potential_duplicates, 1);
    assert!(report.conflicts[0].reason.contains("kind"));
}

// ---------------------------------------------------------------------------
// Identity-path regressions: dry-run must be authoritative for every path
// that could fail at commit (second review round).
// ---------------------------------------------------------------------------

#[test]
fn canonical_id_with_foreign_ref_conflicts_in_dry_run() {
    // Row names asset B's canonical ID but carries a ref owned by asset A:
    // planning must reject it instead of letting the commit hit the unique
    // index after a successful dry-run.
    let env = test_env();
    let mut media = env.media_service();
    let mut importer = env.import_service();

    let a = media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            external_refs: vec![
                assetmesh_core::application::media_service::ExternalRefInput {
                    namespace: "tmdb".into(),
                    external_id: "1".into(),
                    source_url: None,
                },
            ],
            ..create_cmd_movie()
        })
        .unwrap();
    let b = media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            title: "Asset B".into(),
            ..create_cmd_movie()
        })
        .unwrap();

    let content = format!(
        r#"[{{"title":"Whatever","media_type":"movie","year":2020,"asset_id":"{}","external_refs":{{"tmdb":"1"}}}}]"#,
        b.entry.asset.id
    );

    // Dry-run already surfaces the conflict — no successful preview followed
    // by a failed commit.
    let dry = importer
        .import(&content, ImportFormatHint::Json, true)
        .unwrap();
    assert_eq!(dry.potential_duplicates, 1);
    assert_eq!(dry.update, 0);
    assert!(dry.conflicts[0].reason.contains("belongs to asset"));
    assert_eq!(dry.conflicts[0].candidate_asset_id, Some(a.entry.asset.id));

    let real = importer
        .import(&content, ImportFormatHint::Json, false)
        .unwrap();
    assert_eq!(real.potential_duplicates, 1);
    assert_eq!(real.update, 0);
    assert_eq!(real.failed, None);
}

#[test]
fn canonical_id_with_planned_foreign_ref_conflicts() {
    // The ref is claimed by an earlier row of the same run (planned create),
    // while this row names a different canonical ID.
    let env = test_env();
    let mut media = env.media_service();
    let mut importer = env.import_service();

    let b = media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            title: "Asset B".into(),
            ..create_cmd_movie()
        })
        .unwrap();

    let content = format!(
        r#"[
            {{"title":"Claimer","media_type":"movie","year":2020,"external_refs":{{"steam":"9"}}}},
            {{"title":"Impostor","media_type":"movie","year":2020,"asset_id":"{}","external_refs":{{"steam":"9"}}}}
        ]"#,
        b.entry.asset.id
    );

    let report = importer
        .import(&content, ImportFormatHint::Json, false)
        .unwrap();

    assert_eq!(report.create, 1);
    assert_eq!(
        report.update, 0,
        "canonical-ID row must not claim a foreign planned ref"
    );
    assert_eq!(report.potential_duplicates, 1);
    assert!(report.conflicts[0].reason.contains("belongs to asset"));
    assert_eq!(report.failed, None);
}

#[test]
fn ref_owned_by_archived_or_merged_asset_conflicts() {
    // Ref owners outside the active set are invisible to update matching but
    // still own their ref: importing that ref must be a reviewable conflict,
    // not a silent create that would collide with the unique index.
    let env = test_env();
    let mut media = env.media_service();
    let mut assets = env.asset_service();
    let mut importer = env.import_service();

    let owned = media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            external_refs: vec![
                assetmesh_core::application::media_service::ExternalRefInput {
                    namespace: "tmdb".into(),
                    external_id: "42".into(),
                    source_url: None,
                },
            ],
            ..create_cmd_movie()
        })
        .unwrap();
    assets.archive_asset(owned.entry.asset.id).unwrap();

    let content = r#"[{"title":"Fresh import","media_type":"movie","year":2020,"external_refs":{"tmdb":"42"}}]"#;

    let dry = importer
        .import(content, ImportFormatHint::Json, true)
        .unwrap();
    assert_eq!(dry.create, 0, "dry-run must see the inactive owner");
    assert_eq!(dry.potential_duplicates, 1);
    assert!(dry.conflicts[0].reason.contains("non-active"));

    let real = importer
        .import(content, ImportFormatHint::Json, false)
        .unwrap();
    assert_eq!(real.create, 0);
    assert_eq!(real.potential_duplicates, 1);
    assert_eq!(real.failed, None);
}

#[test]
fn identity_conflict_after_batch_boundary_is_disclosed_not_failed() {
    // Row 201 names a canonical ID while carrying a ref owned by another
    // asset: it conflicts at plan time, so earlier batches stay committed
    // and the run completes without a batch failure.
    let env = test_env();
    let mut media = env.media_service();
    let mut importer = env.import_service();

    let a = media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            external_refs: vec![
                assetmesh_core::application::media_service::ExternalRefInput {
                    namespace: "tmdb".into(),
                    external_id: "1".into(),
                    source_url: None,
                },
            ],
            ..create_cmd_movie()
        })
        .unwrap();
    let b = media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            title: "Asset B".into(),
            ..create_cmd_movie()
        })
        .unwrap();

    let mut records: Vec<String> = (0..200)
        .map(|i| format!(r#"{{"title":"Filler {i}","media_type":"anime","year":2005}}"#))
        .collect();
    records.push(format!(
        r#"{{"title":"Row 201","media_type":"movie","year":2020,"asset_id":"{}","external_refs":{{"tmdb":"1"}}}}"#,
        b.entry.asset.id
    ));
    let content = format!("[{}]", records.join(","));

    let report = importer
        .import(&content, ImportFormatHint::Json, false)
        .unwrap();

    assert_eq!(report.failed, None, "conflicts plan out; no batch fails");
    assert_eq!(report.create, 200);
    assert_eq!(report.potential_duplicates, 1);
    assert_eq!(
        report.conflicts[0].candidate_asset_id,
        Some(a.entry.asset.id)
    );
}

#[test]
fn report_indexes_track_physical_rows() {
    // Malformed rows before valid/conflicting ones must not shift the
    // reported indexes: they are physical source positions.
    let env = test_env();
    let mut importer = env.import_service();

    let content = r#"[
        {"title":"Good 0","media_type":"movie","year":2000},
        {"title":123},
        {"title":"Bad status","media_type":"movie","status":"nonsense"},
        {"title":"Good 2","media_type":"movie","year":2000},
        {"title":"good 0","media_type":"movie","year":2000}
    ]"#;
    let report = importer
        .import(content, ImportFormatHint::Json, true)
        .unwrap();

    let rejected_indexes: Vec<usize> = report.rejected_records.iter().map(|r| r.index).collect();
    assert_eq!(rejected_indexes, vec![1, 2], "physical row indexes");
    assert!(report.rejected_records[0].reason.contains("record 1"));
    assert!(report.rejected_records[1].reason.contains("record 2"));

    let conflict_indexes: Vec<usize> = report.conflicts.iter().map(|c| c.index).collect();
    assert_eq!(
        conflict_indexes,
        vec![4],
        "conflict points at physical row 4"
    );
    // Valid candidates are physical rows 0, 3 and 4 (row 4 conflicts).
    assert_eq!(report.valid, 3);
    assert_eq!(report.create, 2);
}
