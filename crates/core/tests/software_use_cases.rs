//! Software application use-case tests over in-memory port doubles.

mod support;

use assetmesh_core::application::media_service::ExternalRefInput;
use assetmesh_core::application::software_discovery::{
    CandidateDisposition, CandidateRef, SoftwareCandidate,
};
use assetmesh_core::application::software_service::{
    AdoptOverrides, AdoptTarget, CreateSoftware, SoftwareService, UpdateSoftwareMetadata,
};
use assetmesh_core::ports::providers::SoftwareDiscoveryProvider;
use assetmesh_core::ports::repos::{SoftwareFilter, SoftwareSort};
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use assetmesh_core::{AppError, AppResult};
use support::test_env;

fn create_cmd(name: &str, category: SoftwareCategory) -> CreateSoftware {
    CreateSoftware {
        name: name.into(),
        category,
        summary: None,
        install_source: None,
        version: None,
        install_location: None,
        executable_path: None,
        purpose: None,
        notes: None,
        architecture: None,
        installed_at: None,
        tags: Vec::new(),
        external_refs: Vec::new(),
    }
}

use assetmesh_core::domain::software::{InstallSource, SoftwareCategory};

fn ref_input(namespace: &str, external_id: &str) -> ExternalRefInput {
    ExternalRefInput {
        namespace: namespace.into(),
        external_id: external_id.into(),
        source_url: None,
    }
}

fn candidate(
    provider: &str,
    name: &str,
    category: SoftwareCategory,
    refs: Vec<CandidateRef>,
) -> SoftwareCandidate {
    SoftwareCandidate {
        provider: provider.into(),
        display_name: name.into(),
        category,
        install_source: InstallSource::Unknown,
        version: None,
        install_location: None,
        executable_path: None,
        external_refs: refs,
        metadata: None,
    }
}

/// A provider double that records whether it was scanned; it can never write
/// canonical state because it has no ports at all.
#[derive(Debug)]
struct FakeProvider {
    candidates: Vec<SoftwareCandidate>,
    scanned: std::sync::atomic::AtomicBool,
    error: Option<AppError>,
}

impl FakeProvider {
    fn new(candidates: Vec<SoftwareCandidate>) -> Self {
        FakeProvider {
            candidates,
            scanned: std::sync::atomic::AtomicBool::new(false),
            error: None,
        }
    }

    fn failing(error: AppError) -> Self {
        FakeProvider {
            candidates: Vec::new(),
            scanned: std::sync::atomic::AtomicBool::new(false),
            error: Some(error),
        }
    }
}

impl SoftwareDiscoveryProvider for FakeProvider {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn description(&self) -> &'static str {
        "fake provider"
    }

    fn scan(&self) -> AppResult<Vec<SoftwareCandidate>> {
        self.scanned
            .store(true, std::sync::atomic::Ordering::SeqCst);
        match &self.error {
            Some(e) => Err(e.clone()),
            None => Ok(self.candidates.clone()),
        }
    }
}

#[test]
fn create_and_get_software_round_trips_details() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    let view = software
        .create_software(CreateSoftware {
            purpose: Some("Rust toolchain helper".into()),
            ..create_cmd("ripgrep", SoftwareCategory::Cli)
        })
        .unwrap();
    assert_eq!(view.entry.record.category, SoftwareCategory::Cli);
    assert_eq!(view.entry.record.install_source, InstallSource::Unknown);
    assert_eq!(
        view.entry.record.purpose.as_deref(),
        Some("Rust toolchain helper")
    );
    assert!(view.entry.asset.name == "ripgrep");
    assert_eq!(view.entry.asset.kind.as_str(), "software.cli");

    let fetched = software.get_software(view.entry.asset.id).unwrap();
    assert_eq!(fetched.entry.record, view.entry.record);
}

#[test]
fn create_rejects_empty_name_and_invalid_refs() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    let err = software
        .create_software(create_cmd("   ", SoftwareCategory::Cli))
        .unwrap_err();
    assert!(err.to_string().contains("name"), "{err}");

    let mut cmd = create_cmd("ripgrep", SoftwareCategory::Cli);
    cmd.external_refs = vec![ref_input("Bad Namespace", "x")];
    let err = software.create_software(cmd).unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err}");
}

#[test]
fn duplicate_external_ref_conflicts_across_modules() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    software
        .create_software(CreateSoftware {
            external_refs: vec![ref_input("bundle_id", "com.example.Foo")],
            ..create_cmd("Foo", SoftwareCategory::Application)
        })
        .unwrap();

    let err = software
        .create_software(CreateSoftware {
            external_refs: vec![ref_input("bundle_id", "com.example.Foo")],
            ..create_cmd("Foo clone", SoftwareCategory::Application)
        })
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err}");
}

#[test]
fn list_filters_by_category_install_source_and_tag() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    software
        .create_software(CreateSoftware {
            install_source: Some(InstallSource::HomebrewFormula),
            tags: vec!["dev".into()],
            ..create_cmd("ripgrep", SoftwareCategory::Cli)
        })
        .unwrap();
    software
        .create_software(CreateSoftware {
            install_source: Some(InstallSource::MacosApp),
            ..create_cmd("Safari", SoftwareCategory::Application)
        })
        .unwrap();

    let rows = software
        .list_software(&SoftwareFilter {
            category: Some(SoftwareCategory::Cli),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.record.category, SoftwareCategory::Cli);

    let rows = software
        .list_software(&SoftwareFilter {
            install_source: Some(InstallSource::MacosApp),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.asset.name, "Safari");

    let rows = software
        .list_software(&SoftwareFilter {
            tag: Some("dev".into()),
            sort: SoftwareSort::TitleAsc,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tags, vec!["dev".to_string()]);
}

#[test]
fn update_metadata_preserves_user_owned_fields_and_touches_projection() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    let view = software
        .create_software(CreateSoftware {
            purpose: Some("Java development".into()),
            external_refs: vec![ref_input("homebrew_formula", "openjdk")],
            ..create_cmd("OpenJDK", SoftwareCategory::Runtime)
        })
        .unwrap();
    let asset_id = view.entry.asset.id;

    let updated = software
        .update_metadata(UpdateSoftwareMetadata {
            asset_id,
            version: Some("21".into()),
            purpose: Some("Kept for one old project".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(updated.entry.record.version.as_deref(), Some("21"));
    assert_eq!(
        updated.entry.record.purpose.as_deref(),
        Some("Kept for one old project")
    );

    // Projection contains the new version keyword.
    let mut search = env.search_service();
    let hits = search.search("21", 10).unwrap();
    assert!(
        hits.iter().any(|h| h.asset_id == asset_id),
        "search: {hits:?}"
    );
}

#[test]
fn discovery_scan_never_writes_canonical_state() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    let provider = FakeProvider::new(vec![candidate(
        "fake",
        "Some App",
        SoftwareCategory::Application,
        vec![CandidateRef::new("bundle_id", "com.example.SomeApp").unwrap()],
    )]);
    let report = software.discover(&provider).unwrap();
    assert_eq!(report.provider, "fake");
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].disposition, CandidateDisposition::New);

    // Scan emitted no activity, no assets, no search documents.
    let store = env.factory.store();
    assert!(store.assets.is_empty(), "scan must not create assets");
    assert!(store.activity.is_empty(), "scan must not append activity");
    assert!(store.search_docs.is_empty(), "scan must not project search");
}

#[test]
fn failing_provider_scan_surfaces_typed_error() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    let provider = FakeProvider::failing(AppError::provider_unavailable("brew is not installed"));
    let err = software.discover(&provider).unwrap_err();
    assert!(matches!(err, AppError::ProviderUnavailable { .. }), "{err}");
}

#[test]
fn classify_exact_external_ref_match() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    let created = software
        .create_software(CreateSoftware {
            external_refs: vec![ref_input("bundle_id", "com.example.Existing")],
            ..create_cmd("Existing", SoftwareCategory::Application)
        })
        .unwrap();

    let provider = FakeProvider::new(vec![candidate(
        "fake",
        "Existing",
        SoftwareCategory::Application,
        vec![CandidateRef::new("bundle_id", "com.example.Existing").unwrap()],
    )]);
    let report = software.discover(&provider).unwrap();
    assert_eq!(
        report.candidates[0].disposition,
        CandidateDisposition::ExactMatch {
            asset_id: created.entry.asset.id
        }
    );
}

#[test]
fn classify_heuristic_name_match_is_review_only() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    let created = software
        .create_software(create_cmd(
            "Visual Studio Code",
            SoftwareCategory::Application,
        ))
        .unwrap();

    let provider = FakeProvider::new(vec![candidate(
        "fake",
        "visual   studio code",
        SoftwareCategory::Application,
        vec![CandidateRef::new("some_provider", "vscode").unwrap()],
    )]);
    let report = software.discover(&provider).unwrap();
    assert_eq!(
        report.candidates[0].disposition,
        CandidateDisposition::PotentialDuplicate {
            asset_ids: vec![created.entry.asset.id]
        }
    );
}

#[test]
fn classify_ref_owned_by_non_software_asset_is_conflict() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());
    let mut media = assetmesh_core::application::media_service::MediaService::new(
        env.factory.clone(),
        env.clock.clone(),
        env.ids.clone(),
    );

    media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            title: "Frieren".into(),
            media_type: assetmesh_core::domain::media::MediaType::Anime,
            external_refs: vec![ref_input("tmdb", "209867")],
            ..create_media_defaults()
        })
        .unwrap();

    let provider = FakeProvider::new(vec![candidate(
        "fake",
        "Frieren",
        SoftwareCategory::Application,
        vec![CandidateRef::new("tmdb", "209867").unwrap()],
    )]);
    let report = software.discover(&provider).unwrap();
    match &report.candidates[0].disposition {
        CandidateDisposition::Conflict { message } => {
            assert!(message.contains("not a software asset"), "{message}");
        }
        other => panic!("expected conflict, got {other:?}"),
    }
}

fn create_media_defaults() -> assetmesh_core::application::media_service::CreateMedia {
    assetmesh_core::application::media_service::CreateMedia {
        title: String::new(),
        media_type: assetmesh_core::domain::media::MediaType::Movie,
        summary: None,
        status: None,
        rating: None,
        year: None,
        platform: None,
        progress: assetmesh_core::domain::media::Progress::default(),
        notes: None,
        tags: Vec::new(),
        external_refs: Vec::new(),
        started_at: None,
        completed_at: None,
    }
}

#[test]
fn adoption_creates_canonical_asset_atomically() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    let outcome = software
        .adopt_candidate(
            SoftwareCandidate {
                version: Some("1.2.3".into()),
                install_location: Some("/Applications/Foo.app".into()),
                ..candidate(
                    "fake",
                    "Foo",
                    SoftwareCategory::Application,
                    vec![CandidateRef::new("bundle_id", "com.example.Foo").unwrap()],
                )
            },
            AdoptOverrides {
                purpose: Some("Required by AssetMesh".into()),
                tags: vec!["dev".into()],
                ..Default::default()
            },
        )
        .unwrap();
    assert!(outcome.created);
    assert_eq!(outcome.disposition, "new");

    let view = software.get_software(outcome.asset_id).unwrap();
    assert_eq!(view.entry.record.version.as_deref(), Some("1.2.3"));
    assert_eq!(
        view.entry.record.purpose.as_deref(),
        Some("Required by AssetMesh")
    );
    assert!(view.entry.record.discovered_at.is_some());
    assert_eq!(view.external_refs[0].namespace, "bundle_id");
    assert_eq!(view.tags, vec!["dev".to_string()]);

    // Activity: asset.created + software.adopted, no per-candidate noise.
    {
        let store = env.factory.store();
        let types: Vec<&str> = store
            .activity
            .iter()
            .filter(|e| e.asset_id == Some(outcome.asset_id))
            .map(|e| e.event_type.as_str())
            .collect();
        assert_eq!(
            types,
            vec!["asset.created", "software.adopted"],
            "{types:?}"
        );
    }

    // Searchable immediately.
    let mut search = env.search_service();
    let hits = search.search("bundle_id:com.example.Foo", 10).unwrap();
    assert!(hits.iter().any(|h| h.asset_id == outcome.asset_id));
}

#[test]
fn adoption_of_exact_match_fills_missing_fields_only() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    let created = software
        .create_software(CreateSoftware {
            purpose: Some("User wrote this".into()),
            notes: Some("User notes".into()),
            version: Some("1.0.0".into()),
            external_refs: vec![ref_input("bundle_id", "com.example.Bar")],
            ..create_cmd("Bar", SoftwareCategory::Application)
        })
        .unwrap();

    let outcome = software
        .adopt_candidate(
            SoftwareCandidate {
                version: Some("2.0.0".into()),
                install_location: Some("/Applications/Bar.app".into()),
                ..candidate(
                    "fake",
                    "Bar",
                    SoftwareCategory::Application,
                    vec![CandidateRef::new("bundle_id", "com.example.Bar").unwrap()],
                )
            },
            AdoptOverrides::default(),
        )
        .unwrap();
    assert!(!outcome.created);
    assert_eq!(outcome.disposition, "exact_match");
    assert_eq!(outcome.asset_id, created.entry.asset.id);

    let view = software.get_software(created.entry.asset.id).unwrap();
    // Existing version NOT overwritten; missing location filled.
    assert_eq!(view.entry.record.version.as_deref(), Some("1.0.0"));
    assert_eq!(
        view.entry.record.install_location.as_deref(),
        Some("/Applications/Bar.app")
    );
    // User-owned fields untouched by discovery.
    assert_eq!(
        view.entry.record.purpose.as_deref(),
        Some("User wrote this")
    );
    assert_eq!(view.entry.record.notes.as_deref(), Some("User notes"));
    assert_eq!(
        view.activity.len(),
        3,
        "asset.created + software.created + software.adopted"
    );
}

#[test]
fn adoption_update_can_still_write_purpose_via_explicit_override() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    let created = software
        .create_software(CreateSoftware {
            external_refs: vec![ref_input("homebrew_cask", "zoom")],
            ..create_cmd("Zoom", SoftwareCategory::Application)
        })
        .unwrap();

    software
        .adopt_candidate(
            candidate(
                "fake",
                "Zoom",
                SoftwareCategory::Application,
                vec![CandidateRef::new("homebrew_cask", "zoom").unwrap()],
            ),
            AdoptOverrides {
                purpose: Some("Meetings".into()),
                ..Default::default()
            },
        )
        .unwrap();

    let view = software.get_software(created.entry.asset.id).unwrap();
    assert_eq!(view.entry.record.purpose.as_deref(), Some("Meetings"));
}

#[test]
fn adoption_requires_explicit_target_for_potential_duplicates() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    let existing = software
        .create_software(create_cmd("Firefox", SoftwareCategory::Application))
        .unwrap()
        .entry
        .asset
        .id;

    // Auto target fails loudly instead of guessing.
    let err = software
        .adopt_candidate(
            candidate(
                "fake",
                "firefox",
                SoftwareCategory::Application,
                vec![CandidateRef::new("vendor", "ffx").unwrap()],
            ),
            AdoptOverrides::default(),
        )
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err}");
    assert!(err.to_string().contains("explicit target"), "{err}");

    // Explicit --new creates a separate record; no silent merge.
    let outcome = software
        .adopt_candidate(
            candidate(
                "fake",
                "firefox",
                SoftwareCategory::Application,
                vec![CandidateRef::new("vendor", "ffx").unwrap()],
            ),
            AdoptOverrides {
                target: AdoptTarget::CreateNew,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(outcome.created);
    assert_ne!(outcome.asset_id, existing);

    // Explicit --as updates the chosen asset.
    let outcome = software
        .adopt_candidate(
            candidate(
                "fake",
                "firefox",
                SoftwareCategory::Application,
                vec![CandidateRef::new("vendor", "ffx2").unwrap()],
            ),
            AdoptOverrides {
                target: AdoptTarget::Existing(existing),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!outcome.created);
    assert_eq!(outcome.asset_id, existing);
}

#[test]
fn adoption_is_idempotent_for_the_same_candidate() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    let make = || {
        candidate(
            "fake",
            "ripgrep",
            SoftwareCategory::Cli,
            vec![CandidateRef::new("homebrew_formula", "ripgrep").unwrap()],
        )
    };
    let first = software
        .adopt_candidate(make(), AdoptOverrides::default())
        .unwrap();
    let second = software
        .adopt_candidate(make(), AdoptOverrides::default())
        .unwrap();
    assert!(first.created);
    assert!(!second.created);
    assert_eq!(first.asset_id, second.asset_id);
}

#[test]
fn adoption_rejects_non_software_and_missing_targets() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());
    let mut media = assetmesh_core::application::media_service::MediaService::new(
        env.factory.clone(),
        env.clock.clone(),
        env.ids.clone(),
    );
    let media_view = media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            title: "A Movie".into(),
            ..create_media_defaults()
        })
        .unwrap();

    let err = software
        .adopt_candidate(
            candidate("fake", "X", SoftwareCategory::Cli, Vec::new()),
            AdoptOverrides {
                target: AdoptTarget::Existing(media_view.entry.asset.id),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("software assets"), "{err}");

    let missing = AdoptTarget::Existing(assetmesh_core::domain::ids::AssetId::generate());
    let err = software
        .adopt_candidate(
            candidate("fake", "X", SoftwareCategory::Cli, Vec::new()),
            AdoptOverrides {
                target: missing,
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound { .. }), "{err}");
}

#[test]
fn adoption_rolls_back_on_failure() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    // An existing asset owns the ref; force a conflict mid-adoption by
    // targeting a different existing asset (refs would collide).
    let owner = software
        .create_software(CreateSoftware {
            external_refs: vec![ref_input("bundle_id", "com.example.Owner")],
            ..create_cmd("Owner", SoftwareCategory::Application)
        })
        .unwrap()
        .entry
        .asset
        .id;
    let other = software
        .create_software(create_cmd("Other", SoftwareCategory::Application))
        .unwrap()
        .entry
        .asset
        .id;

    let err = software
        .adopt_candidate(
            candidate(
                "fake",
                "Owner",
                SoftwareCategory::Application,
                vec![CandidateRef::new("bundle_id", "com.example.Owner").unwrap()],
            ),
            AdoptOverrides {
                target: AdoptTarget::Existing(other),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err}");

    let store = env.factory.store();
    // "Other" asset unchanged, no tombstones, no partial refs.
    assert_eq!(store.assets.len(), 2);
    assert_eq!(store.software.len(), 2);
    assert!(store.refs.values().all(|r| r.asset_id == owner));
    assert_eq!(store.activity.len(), 4); // 2 per created asset only
}

#[test]
fn invalid_candidate_is_rejected() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    let err = software
        .adopt_candidate(
            candidate("fake", "   ", SoftwareCategory::Cli, Vec::new()),
            AdoptOverrides::default(),
        )
        .unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err}");
}

#[test]
fn scan_report_serializes_for_adapters() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());
    let provider = FakeProvider::new(vec![candidate(
        "fake",
        "App",
        SoftwareCategory::Application,
        Vec::new(),
    )]);
    let report = software.discover(&provider).unwrap();
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value["provider"], "fake");
    assert_eq!(value["candidates"][0]["disposition"], "new");
}

#[test]
fn stating_a_fact_via_its_inverse_type_is_a_duplicate_not_a_second_row() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());
    let a = software
        .create_software(create_cmd("AppA", SoftwareCategory::Application))
        .unwrap()
        .entry
        .asset
        .id;
    let b = software
        .create_software(create_cmd("AppB", SoftwareCategory::Runtime))
        .unwrap()
        .entry
        .asset
        .id;

    let mut relations = assetmesh_core::application::relation_service::RelationService::new(
        env.factory.clone(),
        env.clock.clone(),
        env.ids.clone(),
    );
    relations
        .attach(
            a,
            assetmesh_core::domain::relation::RelationType::DependsOn,
            b,
            None,
            assetmesh_core::domain::relation::RelationProvenance::Manual,
        )
        .unwrap();

    // The same fact stated from the other endpoint via the inverse type must
    // be a conflict: one canonical row per fact (docs/03, docs/09).
    let err = relations
        .attach(
            b,
            assetmesh_core::domain::relation::RelationType::DependencyOf,
            a,
            None,
            assetmesh_core::domain::relation::RelationProvenance::Manual,
        )
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err}");

    // Exactly one row exists and both endpoints resolve it correctly.
    let from_a = relations.list_for_asset(a).unwrap();
    assert_eq!(from_a.len(), 1);
    assert_eq!(
        from_a[0].relation_type,
        assetmesh_core::domain::relation::RelationType::DependsOn
    );
    let from_b = relations.list_for_asset(b).unwrap();
    assert_eq!(from_b.len(), 1);
    assert_eq!(
        from_b[0].relation_type,
        assetmesh_core::domain::relation::RelationType::DependencyOf
    );
}

#[test]
fn attaching_with_an_inverse_type_normalizes_to_the_primary_direction() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());
    let a = software
        .create_software(create_cmd("AppA", SoftwareCategory::Application))
        .unwrap()
        .entry
        .asset
        .id;
    let b = software
        .create_software(create_cmd("AppB", SoftwareCategory::Runtime))
        .unwrap()
        .entry
        .asset
        .id;

    let mut relations = assetmesh_core::application::relation_service::RelationService::new(
        env.factory.clone(),
        env.clock.clone(),
        env.ids.clone(),
    );
    // "B dependency_of A" is stored as "A depends_on B".
    let relation = relations
        .attach(
            b,
            assetmesh_core::domain::relation::RelationType::DependencyOf,
            a,
            None,
            assetmesh_core::domain::relation::RelationProvenance::Manual,
        )
        .unwrap();
    assert_eq!(
        relation.relation_type,
        assetmesh_core::domain::relation::RelationType::DependsOn
    );
    assert_eq!(relation.source_asset_id, a);
    assert_eq!(relation.target_asset_id, b);
}

#[test]
fn unchanged_readoption_is_a_true_noop() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());

    let make = || SoftwareCandidate {
        version: Some("1.2.3".into()),
        install_location: Some("/Applications/Foo.app".into()),
        ..candidate(
            "fake",
            "Foo",
            SoftwareCategory::Application,
            vec![CandidateRef::new("bundle_id", "com.example.Foo").unwrap()],
        )
    };
    let first = software
        .adopt_candidate(make(), AdoptOverrides::default())
        .unwrap();
    assert!(first.created);

    let (revision_before, updated_before, activity_before, software_len) = {
        let store = env.factory.store();
        let asset = store.assets.get(&first.asset_id.to_string()).unwrap();
        (
            asset.revision,
            asset.updated_at,
            store.activity.len(),
            store.software.len(),
        )
    };

    // Re-adopting the identical candidate changes nothing: no revision bump,
    // no timestamp churn, no extra activity (docs/09 idempotent adoption).
    let second = software
        .adopt_candidate(make(), AdoptOverrides::default())
        .unwrap();
    assert!(!second.created);
    assert_eq!(second.asset_id, first.asset_id);
    assert!(second.updated_fields.is_empty());

    let (revision_after, updated_after, activity_after, software_after) = {
        let store = env.factory.store();
        let asset = store.assets.get(&first.asset_id.to_string()).unwrap();
        (
            asset.revision,
            asset.updated_at,
            store.activity.len(),
            store.software.len(),
        )
    };
    assert_eq!(revision_after, revision_before, "revision must not move");
    assert_eq!(updated_after, updated_before, "updated_at must not move");
    assert_eq!(
        activity_after, activity_before,
        "no-op must not add activity"
    );
    assert_eq!(software_after, software_len);
}

#[test]
fn direct_uow_writes_reject_category_kind_mismatch() {
    let env = test_env();
    let mut media = assetmesh_core::application::media_service::MediaService::new(
        env.factory.clone(),
        env.clock.clone(),
        env.ids.clone(),
    );
    let media_asset = media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            title: "A Movie".into(),
            ..create_media_defaults()
        })
        .unwrap()
        .entry
        .asset
        .id;

    // A direct UnitOfWork write (bypassing the services) must still be
    // rejected by the repository boundary: a CLI software record cannot be
    // attached to a media asset.
    let mut wrong =
        assetmesh_core::domain::software::SoftwareRecord::new(media_asset, SoftwareCategory::Cli);
    wrong.version = Some("1.0".into());
    let err = env
        .factory
        .clone()
        .transact(&mut |uow| uow.software().upsert(&wrong))
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err}");
    assert!(err.to_string().contains("kind"), "{err}");
}
