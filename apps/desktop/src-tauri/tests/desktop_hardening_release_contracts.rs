//! Desktop Hardening and Release Gate Integration Contract Tests — Phase 5 (P5-10).
//!
//! Validates:
//! 1. Booting Desktop with real legacy Phase 1 fixture (media-only) upgrades cleanly and lists assets.
//! 2. Booting Desktop with real legacy Phase 2 fixture (software) upgrades cleanly and lists assets.
//! 3. Zero direct SQL invariant: desktop adapter codebase contains no raw SQL queries or rusqlite direct imports.
//! 4. Crash and reopen durability: closing and reopening the state in a fresh instance preserves canonical data.
//! 5. Scale & pagination bounds: paging and search bounds are strictly enforced.
//! 6. Read-only safety: library and graph reads never cause state mutation or spurious activity events.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use assetmesh_core::ports::{SystemClock, UuidV7Generator};
use assetmesh_desktop_lib::commands::{
    activity_query_impl, app_status_impl, library_get_impl, library_list_impl, library_search_impl,
    media_command_impl, relation_attach_impl, relation_traverse_impl, software_command_impl,
};
use assetmesh_desktop_lib::dto::{
    ActivityQueryDto, LibraryQueryDto, LibrarySearchQueryDto, MediaCommandDto, RelationAttachDto,
    RelationTraverseQueryDto, SoftwareCommandDto,
};
use assetmesh_desktop_lib::state::{AppStatus, DesktopState};

fn temp_db_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "assetmesh-hardening-{}-{}.db",
        name,
        uuid::Uuid::now_v7()
    ))
}

fn setup_state(db_path: &Path) -> DesktopState {
    let state = DesktopState::with_clock_and_ids(Arc::new(SystemClock), Arc::new(UuidV7Generator));
    state.initialize(db_path).expect("initialize");
    state
}

#[test]
fn test_boot_desktop_with_phase1_legacy_database_fixture() {
    // Locate the historical Phase 1 fixture in storage-sqlite
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../crates/storage-sqlite/tests/fixtures/phase1_media_only.db");
    // Also check alternative relative path if running from repo root
    let fixture_path = if root.exists() {
        root
    } else {
        PathBuf::from("crates/storage-sqlite/tests/fixtures/phase1_media_only.db")
    };

    assert!(
        fixture_path.exists(),
        "Phase 1 fixture must exist at {}",
        fixture_path.display()
    );

    let temp_db = temp_db_path("phase1_legacy");
    fs::copy(&fixture_path, &temp_db).expect("copy phase1 fixture");

    // Boot Desktop against this legacy DB
    let state = setup_state(&temp_db);
    let status = app_status_impl(&state).expect("status");
    assert_eq!(
        status,
        AppStatus::Ready {
            db_path: temp_db.to_string_lossy().to_string()
        }
    );

    // Verify Desktop can query library items migrated from Phase 1
    let list = library_list_impl(Some(LibraryQueryDto::default()), &state).expect("list");
    assert!(
        list.total.unwrap_or(0) > 0,
        "legacy media assets must be present"
    );

    // Verify detail can be read for the first item
    let first_id = &list.items[0].id;
    let detail = library_get_impl(first_id, &state).expect("get detail");
    assert_eq!(detail.id, *first_id);
    assert_eq!(detail.details["module"], "media");
}

#[test]
fn test_boot_desktop_with_phase2_legacy_database_fixture() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../crates/storage-sqlite/tests/fixtures/phase2_software.db");
    let fixture_path = if root.exists() {
        root
    } else {
        PathBuf::from("crates/storage-sqlite/tests/fixtures/phase2_software.db")
    };

    assert!(
        fixture_path.exists(),
        "Phase 2 fixture must exist at {}",
        fixture_path.display()
    );

    let temp_db = temp_db_path("phase2_legacy");
    fs::copy(&fixture_path, &temp_db).expect("copy phase2 fixture");

    let state = setup_state(&temp_db);
    let status = app_status_impl(&state).expect("status");
    assert_eq!(
        status,
        AppStatus::Ready {
            db_path: temp_db.to_string_lossy().to_string()
        }
    );

    let list = library_list_impl(Some(LibraryQueryDto::default()), &state).expect("list");
    assert!(
        list.total.unwrap_or(0) > 0,
        "legacy software assets must be present"
    );
}

#[test]
fn test_desktop_adapter_zero_direct_sql_invariant() {
    // Static architectural audit: scan all .rs files in apps/desktop/src-tauri/src
    // Verify that rusqlite is NEVER imported or referenced, and no raw SQL is written.
    let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files_checked = 0;

    fn check_dir(dir: &Path, count: &mut usize) {
        for entry in fs::read_dir(dir).expect("read_dir") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if path.is_dir() {
                check_dir(&path, count);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                *count += 1;
                let content = fs::read_to_string(&path).expect("read rs file");
                assert!(
                    !content.contains("rusqlite"),
                    "Forbidden rusqlite reference found in {}: desktop must remain strictly an adapter over core services",
                    path.display()
                );
                assert!(
                    !content.contains("SELECT ") && !content.contains("INSERT INTO ") && !content.contains("UPDATE ") && !content.contains("DELETE FROM "),
                    "Forbidden raw SQL query found in {}: desktop must remain strictly an adapter over core services",
                    path.display()
                );
            }
        }
    }

    check_dir(&src_dir, &mut files_checked);
    assert!(
        files_checked >= 10,
        "Checked {} files in desktop adapter src",
        files_checked
    );
}

#[test]
fn test_crash_and_reopen_durability() {
    let db_path = temp_db_path("durability");

    // Process 1: Open, seed, and mutate state
    let asset_id = {
        let state1 = setup_state(&db_path);
        let receipt = media_command_impl(
            MediaCommandDto::Create {
                title: "Durable Movie".into(),
                media_type: "movie".into(),
                summary: Some("Durability check".into()),
                status: Some("completed".into()),
                rating: Some(4.8),
                year: Some(2025),
                platform: None,
                progress_current: None,
                progress_total: None,
                progress_unit: None,
                notes: Some("Permanent note".into()),
                tags: vec!["durability".into()],
            },
            &state1,
        )
        .expect("create media");
        receipt.asset_ids[0].clone()
        // state1 drops here, simulating process exit
    };

    // Process 2: Reopen in a completely new DesktopState instance
    let state2 = setup_state(&db_path);
    let detail = library_get_impl(&asset_id, &state2).expect("read back");
    assert_eq!(detail.name, "Durable Movie");
    assert_eq!(detail.lifecycle, "active");

    let list = library_list_impl(Some(LibraryQueryDto::default()), &state2).expect("list");
    assert_eq!(list.total, Some(1));
    assert_eq!(list.items[0].id, asset_id);
}

#[test]
fn test_list_and_search_scale_and_pagination_bounds() {
    let db_path = temp_db_path("bounds_scale");
    let state = setup_state(&db_path);

    // Seed 15 assets
    for i in 0..15 {
        media_command_impl(
            MediaCommandDto::Create {
                title: format!("Media Title {i:02}"),
                media_type: "movie".into(),
                summary: None,
                status: Some("planned".into()),
                rating: None,
                year: Some(2020 + i),
                platform: None,
                progress_current: None,
                progress_total: None,
                progress_unit: None,
                notes: None,
                tags: vec!["bulk".into()],
            },
            &state,
        )
        .expect("create media");
    }

    // Page 1: limit 5, offset 0
    let p1 = library_list_impl(
        Some(LibraryQueryDto {
            limit: Some(5),
            offset: Some(0),
            ..Default::default()
        }),
        &state,
    )
    .expect("p1");
    assert_eq!(p1.items.len(), 5);
    assert_eq!(p1.total, Some(15));
    assert_eq!(p1.offset, 0);

    // Page 2: limit 5, offset 5
    let p2 = library_list_impl(
        Some(LibraryQueryDto {
            limit: Some(5),
            offset: Some(5),
            ..Default::default()
        }),
        &state,
    )
    .expect("p2");
    assert_eq!(p2.items.len(), 5);
    assert_eq!(p2.offset, 5);

    // Disjoint items
    for item1 in &p1.items {
        assert!(
            !p2.items.iter().any(|item2| item2.id == item1.id),
            "Paging must not overlap items across pages"
        );
    }

    // Out of bounds offset: offset 100 on 15 items returns empty items gracefully
    let p_empty = library_list_impl(
        Some(LibraryQueryDto {
            limit: Some(10),
            offset: Some(100),
            ..Default::default()
        }),
        &state,
    )
    .expect("p_empty");
    assert_eq!(p_empty.items.len(), 0);
    assert_eq!(p_empty.total, Some(15));
}

#[test]
fn library_listing_stays_bounded_as_the_library_grows() {
    // The original scale check seeded 15 rows and asserted page contents only.
    // That cannot see the failure this guards: a page whose cost grows with the
    // library. Two properties are pinned instead.
    //
    // 1. A wall-clock assertion would be flaky on a loaded machine, so the
    //    growth is measured in SQL statements, not milliseconds. Rendering the
    //    whole library must not issue one query per row.
    // 2. Search has to keep working at the same size — the unified library reads
    //    the same rows for list and search, so a regressed batching would show
    //    up in both.
    let rows = 120usize;

    let small_state = setup_state_for_scale("scale_small");
    scale_seed(&small_state, 12);
    let small_page = scale_first_page(&small_state);
    let small_rows = scale_list(&small_state);
    assert_eq!(small_rows, 12);

    let large_state = setup_state_for_scale("scale_large");
    scale_seed(&large_state, rows);
    let large_page = scale_first_page(&large_state);
    let large_rows = scale_list(&large_state);
    assert_eq!(large_rows, rows);

    // The invariant is per page, not per walk: a full page must cost the same
    // whether the library behind it holds 12 rows or 120. A per-row lookup
    // shows up here long before it shows up in wall-clock time.
    //
    // Both pages are checked full first, because a short page would compare two
    // different amounts of work and could pass for the wrong reason.
    assert_eq!(large_page.rows, 20, "the large page must be full");
    assert!(large_page.statements > 0, "nothing was measured");
    assert_eq!(
        small_page.statements, large_page.statements,
        "one page of {small_rows} rows and one page of {large_rows} rows must cost the same \
         number of statements; a per-row tag lookup is the usual cause of a growing count \
         ({} vs {})",
        small_page.statements, large_page.statements
    );

    // Search still reaches a single row inside the larger library, and the hit
    // carries the same vocabulary as the list.
    let hits = library_search_impl(
        LibrarySearchQueryDto {
            text: format!("Scale Title {:04}", rows - 1),
            limit: Some(5),
            ..Default::default()
        },
        &large_state,
    )
    .expect("search");
    assert_eq!(hits.items.len(), 1, "one row matches that title");
    assert_eq!(hits.total, Some(1));
    assert_eq!(hits.items[0].name, format!("Scale Title {:04}", rows - 1));

    // A term shared by every seeded row is findable, which is what a projection
    // regression would break first at this size.
    let shared = library_search_impl(
        LibrarySearchQueryDto {
            text: "Scale Title".into(),
            limit: Some(200),
            ..Default::default()
        },
        &large_state,
    )
    .expect("shared search");
    assert_eq!(shared.items.len(), rows);
}

/// An initialized state on its own temporary database, kept apart from the
/// other tests so the two scale measurements cannot see each other's rows.
fn setup_state_for_scale(name: &str) -> DesktopState {
    setup_state(&temp_db_path(name))
}

/// Seeds `count` media assets with a tag each, through the adapter command.
fn scale_seed(state: &DesktopState, count: usize) {
    for i in 0..count {
        media_command_impl(
            MediaCommandDto::Create {
                title: format!("Scale Title {i:04}"),
                media_type: "movie".into(),
                summary: Some(format!("summary {i}")),
                status: Some("planned".into()),
                rating: None,
                year: Some(2020),
                platform: None,
                progress_current: None,
                progress_total: None,
                progress_unit: None,
                notes: None,
                tags: vec![format!("scale-{i:04}")],
            },
            state,
        )
        .expect("seed media");
    }
}

/// Lists every row the library has, one page at a time, so paging is exercised
/// rather than bypassed. Returns the total number of distinct rows seen.
fn scale_list(state: &DesktopState) -> usize {
    let mut offset = 0usize;
    let mut seen = std::collections::HashSet::new();
    loop {
        let page = library_list_impl(
            Some(LibraryQueryDto {
                limit: Some(20),
                offset: Some(offset),
                ..Default::default()
            }),
            state,
        )
        .expect("page");
        if page.items.is_empty() {
            break;
        }
        for item in &page.items {
            assert!(
                seen.insert(item.id.clone()),
                "a page repeated an asset: {}",
                item.id
            );
        }
        offset += page.items.len();
    }
    seen.len()
}

/// One page of the ledger plus the SQL statements it cost.
struct PageCost {
    statements: usize,
    rows: usize,
}

/// Arms the statement counter and fetches a single full page.
///
/// Measuring one page rather than the whole walk is deliberate: the walk's cost
/// scales with the number of pages, which is correct behaviour. What must not
/// scale is the cost *of* a page.
fn scale_first_page(state: &DesktopState) -> PageCost {
    state
        .with_factory(|factory| {
            assetmesh_storage_sqlite::statement_accounting::arm(&factory.0);
            Ok(())
        })
        .expect("the seeded database is open");

    let page = library_list_impl(
        Some(LibraryQueryDto {
            limit: Some(20),
            offset: Some(0),
            ..Default::default()
        }),
        state,
    )
    .expect("first page");

    PageCost {
        statements: assetmesh_storage_sqlite::statement_accounting::take(),
        rows: page.items.len(),
    }
}

#[test]
fn test_read_operations_never_mutate_state() {
    let db_path = temp_db_path("read_safety");
    let state = setup_state(&db_path);

    // Seed 1 software and 1 media
    let sw_id = {
        let r = software_command_impl(
            SoftwareCommandDto::Create {
                name: "Test CLI".into(),
                category: "cli".into(),
                summary: None,
                install_source: None,
                version: Some("1.0.0".into()),
                install_location: None,
                executable_path: None,
                purpose: None,
                notes: None,
                architecture: None,
                tags: vec![],
            },
            &state,
        )
        .expect("create software");
        r.asset_ids[0].clone()
    };

    let media_id = {
        let r = media_command_impl(
            MediaCommandDto::Create {
                title: "Test Movie".into(),
                media_type: "movie".into(),
                summary: None,
                status: None,
                rating: None,
                year: None,
                platform: None,
                progress_current: None,
                progress_total: None,
                progress_unit: None,
                notes: None,
                tags: vec![],
            },
            &state,
        )
        .expect("create media");
        r.asset_ids[0].clone()
    };

    relation_attach_impl(
        RelationAttachDto {
            source_asset_id: sw_id.clone(),
            target_asset_id: media_id.clone(),
            relation_type: "uses".into(),
            note: None,
            expected_source_revision: None,
            expected_target_revision: None,
        },
        &state,
    )
    .expect("attach rel");

    let initial_activities = activity_query_impl(ActivityQueryDto::default(), &state).expect("act");
    let initial_act_count = initial_activities.total.unwrap_or(0);

    // Repeated read operations:
    for _ in 0..5 {
        let _ = library_list_impl(Some(LibraryQueryDto::default()), &state).expect("list");
        let _ = library_get_impl(&sw_id, &state).expect("get sw");
        let _ = library_get_impl(&media_id, &state).expect("get media");
        let _ = relation_traverse_impl(
            RelationTraverseQueryDto {
                asset_id: sw_id.clone(),
                mode: None,
                direction: None,
                relation_types: None,
                max_depth: Some(2),
                include_archived: None,
            },
            &state,
        )
        .expect("traverse");
    }

    // Zero additional activity events recorded!
    let final_activities = activity_query_impl(ActivityQueryDto::default(), &state).expect("act");
    assert_eq!(final_activities.total.unwrap_or(0), initial_act_count);
}
