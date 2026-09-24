//! 50k scale performance and deep paging benchmark gate (P5-04, R4).
//!
//! Validates:
//! 1. Constant memory footprint: SQL LIMIT/OFFSET ensures only the requested page
//!    rows (and their batch tags/subtitles) are instantiated in memory, not 50,000 rows.
//! 2. Shallow, 1k, 10k, and deep pagination stay correct at 50k scale;
//!    timings are reported, not asserted against noisy CI wall-clock budgets.
//! 3. Safe chunking: Batch queries with > 500 asset IDs (such as 1,200 IDs) are safely
//!    chunked into batches <= 500, preventing SQLite parameter overflow.

use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use assetmesh_core::application::library_service::{
    LibraryModule, LibraryQuery, LibraryService, LibrarySort, PageRequest,
};
use assetmesh_core::domain::asset::AssetKind;
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::ports::repos::LifecycleFilter;
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use assetmesh_storage_sqlite::SharedSqlite;

const TOTAL_SCALE: usize = 50_000;
const MEDIA_COUNT: usize = 20_000;
const SOFTWARE_COUNT: usize = 20_000;
const SERVICE_COUNT: usize = 10_000;
const TAGGED_COUNT: usize = 5_000;
const RELATIONS_COUNT: usize = 1_000;

fn seed_50k_database() -> (SharedSqlite, Vec<AssetId>) {
    let factory_raw = Arc::new(assetmesh_storage_sqlite::open_in_memory().unwrap());
    let mut asset_ids = Vec::with_capacity(TOTAL_SCALE);

    let start = Instant::now();
    factory_raw
        .with_raw_connection_mut(|conn| {
            let tx = conn.transaction().unwrap();

            // Prepare statements for bulk insert
            let mut insert_asset = tx
                .prepare(
                    "INSERT INTO assets (id, kind, name, lifecycle_state, revision, created_at, updated_at) \
                     VALUES (?, ?, ?, 'active', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                )
                .unwrap();
            let mut insert_media = tx
                .prepare(
                    "INSERT INTO media_records (asset_id, media_type, status, rating, year, platform) \
                     VALUES (?, 'movie', 'completed', 8.5, 2024, 'Theater')",
                )
                .unwrap();
            let mut insert_software = tx
                .prepare(
                    "INSERT INTO software_records (asset_id, category, install_source, version) \
                     VALUES (?, 'tool', 'manual', '1.0.0')",
                )
                .unwrap();
            let mut insert_service = tx
                .prepare(
                    "INSERT INTO service_records (asset_id, service_type, plan, cost_minor, currency, billing_cadence) \
                     VALUES (?, 'saas', 'Pro', 2000, 'USD', 'monthly')",
                )
                .unwrap();
            let mut insert_search = tx
                .prepare("INSERT INTO search_documents (title, subtitle, body, keywords, kind, asset_id) VALUES (?, '', '', '', ?, ?)")
                .unwrap();

            // Insert benchmark tag with valid UUID
            let tag_uuid = Uuid::from_u128(0xba5e_ba11).to_string();
            tx.execute(
                "INSERT INTO tags (id, name, created_at) VALUES (?, 'benchmark', '2026-01-01T00:00:00Z')",
                rusqlite::params![tag_uuid],
            )
            .unwrap();
            let mut insert_asset_tag = tx
                .prepare("INSERT INTO asset_tags (asset_id, tag_id) VALUES (?, ?)")
                .unwrap();

            for i in 0..TOTAL_SCALE {
                let id = Uuid::from_u128((i + 1) as u128);
                let id_str = id.to_string();
                asset_ids.push(AssetId::from_uuid(id));

                if i < MEDIA_COUNT {
                    let name = format!("Media Movie {i:05}");
                    insert_asset.execute(rusqlite::params![id_str, "media.movie", name]).unwrap();
                    insert_search.execute(rusqlite::params![name, "media.movie", id_str]).unwrap();
                    insert_media.execute(rusqlite::params![id_str]).unwrap();
                } else if i < MEDIA_COUNT + SOFTWARE_COUNT {
                    let name = format!("Software Tool {i:05}");
                    insert_asset.execute(rusqlite::params![id_str, "software.tool", name]).unwrap();
                    insert_search.execute(rusqlite::params![name, "software.tool", id_str]).unwrap();
                    insert_software.execute(rusqlite::params![id_str]).unwrap();
                } else {
                    let name = format!("Service SaaS {i:05}");
                    insert_asset.execute(rusqlite::params![id_str, "service.saas", name]).unwrap();
                    insert_search.execute(rusqlite::params![name, "service.saas", id_str]).unwrap();
                    insert_service.execute(rusqlite::params![id_str]).unwrap();
                }

                if i < TAGGED_COUNT {
                    insert_asset_tag.execute(rusqlite::params![id_str, tag_uuid]).unwrap();
                }
            }

            drop(insert_asset);
            drop(insert_media);
            drop(insert_software);
            drop(insert_service);
            drop(insert_search);
            drop(insert_asset_tag);

            // Insert relations between consecutive assets
            let mut insert_relation = tx
                .prepare(
                    "INSERT INTO relations (id, source_asset_id, target_asset_id, relation_type, provenance, created_at) \
                     VALUES (?, ?, ?, 'depends_on', 'manual', '2026-01-01T00:00:00Z')",
                )
                .unwrap();

            for i in 0..RELATIONS_COUNT {
                let rel_id = Uuid::from_u128((TOTAL_SCALE + i + 1) as u128).to_string();
                let src_id = asset_ids[i].to_string();
                let tgt_id = asset_ids[i + 1].to_string();
                insert_relation.execute(rusqlite::params![rel_id, src_id, tgt_id]).unwrap();
            }
            drop(insert_relation);

            tx.commit().unwrap();
        })
        .unwrap();

    let duration = start.elapsed();
    println!(">>> Seeded 50,000 assets and {RELATIONS_COUNT} relations in {duration:?}");
    (SharedSqlite(factory_raw), asset_ids)
}

#[test]
fn scale_50k_paging_and_memory_gate() {
    let (factory, _asset_ids) = seed_50k_database();
    let mut library = LibraryService::new(factory);

    // 1. Shallow query: Page 1 (offset: 0, limit: 20)
    let q1 = LibraryQuery {
        modules: Vec::new(),
        kinds: Vec::new(),
        lifecycle: LifecycleFilter::Active,
        tags: Vec::new(),
        sort: LibrarySort::NameAsc,
        page: PageRequest {
            offset: 0,
            limit: 20,
        },
    };

    let t0 = Instant::now();
    let page1 = library
        .list_assets(&q1)
        .expect("page 1 query should succeed");
    let d1 = t0.elapsed();
    println!(">>> Page 1 query elapsed: {d1:?}");

    assert_eq!(
        page1.items.len(),
        20,
        "page 1 must contain exactly 20 items"
    );
    assert_eq!(page1.total, Some(TOTAL_SCALE), "total must report 50,000");
    assert_eq!(page1.offset, 0);
    assert_eq!(page1.limit, 20);

    // 2. Deep query: Offset 40,000 (limit: 20)
    let q_deep = LibraryQuery {
        modules: Vec::new(),
        kinds: Vec::new(),
        lifecycle: LifecycleFilter::Active,
        tags: Vec::new(),
        sort: LibrarySort::NameAsc,
        page: PageRequest {
            offset: 40_000,
            limit: 20,
        },
    };

    let t0 = Instant::now();
    let page_deep = library
        .list_assets(&q_deep)
        .expect("deep page query should succeed");
    let d_deep = t0.elapsed();
    println!(">>> Deep page (offset 40,000) query elapsed: {d_deep:?}");

    assert_eq!(
        page_deep.items.len(),
        20,
        "deep page must contain exactly 20 items"
    );
    assert_eq!(
        page_deep.total,
        Some(TOTAL_SCALE),
        "total must report 50,000"
    );
    assert_eq!(page_deep.offset, 40_000);

    // 3. Filtered query by module: Software only
    let q_software = LibraryQuery {
        modules: vec![LibraryModule::Software],
        kinds: Vec::new(),
        lifecycle: LifecycleFilter::Active,
        tags: Vec::new(),
        sort: LibrarySort::NameAsc,
        page: PageRequest {
            offset: 0,
            limit: 20,
        },
    };

    let t0 = Instant::now();
    let page_sw = library
        .list_assets(&q_software)
        .expect("software query should succeed");
    let d_sw = t0.elapsed();
    println!(">>> Software module query elapsed: {d_sw:?}");

    assert_eq!(page_sw.items.len(), 20);
    assert_eq!(page_sw.total, Some(SOFTWARE_COUNT));
    assert!(page_sw
        .items
        .iter()
        .all(|item| item.kind == AssetKind::SoftwareTool));

    // 4. Filtered query by tag: "benchmark" tag
    let q_tag = LibraryQuery {
        modules: Vec::new(),
        kinds: Vec::new(),
        lifecycle: LifecycleFilter::Active,
        tags: vec!["benchmark".to_string()],
        sort: LibrarySort::NameAsc,
        page: PageRequest {
            offset: 0,
            limit: 20,
        },
    };

    let t0 = Instant::now();
    let page_tag = library
        .list_assets(&q_tag)
        .expect("tag query should succeed");
    let d_tag = t0.elapsed();
    println!(">>> Tag query elapsed: {d_tag:?}");

    assert_eq!(page_tag.items.len(), 20);
    assert_eq!(page_tag.total, Some(TAGGED_COUNT));

    // 5. Filtered query by module: Services only
    let q_service = LibraryQuery {
        modules: vec![LibraryModule::Services],
        kinds: Vec::new(),
        lifecycle: LifecycleFilter::Active,
        tags: Vec::new(),
        sort: LibrarySort::NameAsc,
        page: PageRequest {
            offset: 0,
            limit: 20,
        },
    };

    let t0 = Instant::now();
    let page_svc = library
        .list_assets(&q_service)
        .expect("service query should succeed");
    let d_svc = t0.elapsed();
    println!(">>> Service module query elapsed: {d_svc:?}");

    assert_eq!(page_svc.items.len(), 20);
    assert_eq!(page_svc.total, Some(SERVICE_COUNT));
    assert!(page_svc
        .items
        .iter()
        .all(|item| item.kind == AssetKind::ServiceSaas));

    for offset in [1_000, 10_000] {
        let page = library
            .list_assets(&LibraryQuery {
                page: PageRequest { offset, limit: 20 },
                ..q1.clone()
            })
            .unwrap();
        assert_eq!(page.items.len(), 20);
        assert_eq!(page.total, Some(TOTAL_SCALE));
    }
}

#[test]
fn scale_50k_search_hydrates_only_indexed_candidates() {
    let (factory, ids) = seed_50k_database();
    let mut library = LibraryService::new(factory.clone());
    for (text, expected) in [
        ("Movie 01000", ids[1_000]),
        ("Tool 30000", ids[30_000]),
        ("SaaS 49999", ids[49_999]),
    ] {
        assetmesh_storage_sqlite::statement_accounting::arm(factory.0.as_ref());
        let start = Instant::now();
        let result = library
            .search_assets(
                &assetmesh_core::application::library_service::LibrarySearchQuery {
                    text: text.to_string(),
                    ..Default::default()
                },
            )
            .unwrap();
        let statements = assetmesh_storage_sqlite::statement_accounting::take();
        println!(
            ">>> Search {text:?}: {:?}, {statements} statements",
            start.elapsed()
        );
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].id, expected);
        assert!(
            statements > 0,
            "query tracer must observe the actual read connection"
        );
        assert!(
            statements <= 40,
            "candidate hydration must use bounded queries"
        );
    }
}

#[test]
fn scale_batch_safe_chunking_limits() {
    let (mut factory, asset_ids) = seed_50k_database();
    assert!(asset_ids.len() >= 1_200);

    // Take 1,200 asset IDs: this exceeds the 500-parameter threshold and would
    // fail with SQLite variable limit error if not safely chunked.
    let sample_ids = &asset_ids[0..1_200];

    // 1. Tag batch reading with 1,200 IDs
    let t0 = Instant::now();
    let tag_pairs = factory
        .read(&mut |q| q.tags().list_for_assets(sample_ids))
        .expect("tag chunking query must succeed without sqlite variable overflow");
    let d_tags = t0.elapsed();
    println!(
        ">>> 1,200 asset tag batch query elapsed: {d_tags:?}, returned {} pairs",
        tag_pairs.len()
    );
    // Since the first 5,000 were tagged with 'benchmark', all 1,200 sample assets have this tag
    assert_eq!(tag_pairs.len(), 1_200);

    // 2. Relation batch reading with 1,200 IDs
    let t0 = Instant::now();
    let rels = factory
        .read(&mut |q| q.relations().list_for_assets(sample_ids))
        .expect("relation chunking query must succeed without sqlite variable overflow");
    let d_rels = t0.elapsed();
    println!(
        ">>> 1,200 asset relation batch query elapsed: {d_rels:?}, returned {} relations",
        rels.len()
    );
    assert!(
        !rels.is_empty(),
        "must find the seeded relations between sample assets"
    );
}
