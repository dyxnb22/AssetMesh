//! Resource limits must fail safely at real SQLite/filesystem boundaries.

#[test]
fn import_probes_only_incoming_identities_and_leaves_unrelated_damaged_history_untouched() {
    let source = Env::new();
    source.add("incoming", InfoType::Text);
    let bundle = source.export().export("test").unwrap();
    let destination = Env::new();
    destination.add("existing", InfoType::Text);
    destination.factory.0.with_raw_connection(|conn| {
        conn.execute("INSERT INTO activity_events VALUES (?1,?2,'custom.unrelated',NULL,'user','bad-json')", rusqlite::params![uuid::Uuid::now_v7().to_string(),SystemClock.now().to_rfc3339()]).unwrap();
    }).unwrap();
    let before = destination.count_history();
    let mut import = PortableImportService::new(destination.factory.clone());
    let preview = import.import_bundle(&bundle, true).unwrap();
    let receipt = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(
        serde_json::to_value(&preview).unwrap(),
        serde_json::to_value(&receipt).unwrap()
    );
    assert_eq!(receipt.assets_created, 1);
    assert_eq!(
        destination.count_history(),
        before + receipt.activity_created
    );
}

#[test]
fn relation_edge_budget_handles_a_broad_archived_frontier() {
    let env = Env::new();
    let root = env.add("root", InfoType::Text).to_string();
    env.factory.0.with_raw_connection_mut(|conn| {
        let tx = conn.transaction().unwrap();
        for index in 0..5100 {
            let id = uuid::Uuid::now_v7().to_string();
            tx.execute("INSERT INTO assets(id,kind,name,lifecycle_state,revision,created_at,updated_at,archived_at) VALUES (?1,'info.item',?2,'archived',1,?3,?3,?3)", rusqlite::params![id,format!("archived-{index}"),SystemClock.now().to_rfc3339()]).unwrap();
            let (source,target) = if root < id { (&root,&id) } else { (&id,&root) };
            tx.execute("INSERT INTO relations VALUES (?1,?2,?3,'related_to',NULL,'manual',?4)", rusqlite::params![uuid::Uuid::now_v7().to_string(),source,target,SystemClock.now().to_rfc3339()]).unwrap();
        }
        tx.commit().unwrap();
    }).unwrap();
    let root = AssetId::from_uuid(uuid::Uuid::parse_str(&root).unwrap());
    let result = RelationQueryService::new(env.factory.clone())
        .traverse(root, &TraversalOptions::default())
        .unwrap();
    assert!(result.nodes.is_empty());
    assert!(
        result.truncated,
        "the resource budget does not imply a complete scan"
    );
}

use assetmesh_core::ports::backup::BackupStore;
use assetmesh_core::{
    application::{
        info_service::{CreateInfo, InfoService},
        portable::{
            read_bundle_from_directory, write_bundle_to_directory, ExportFile,
            PortableExportService, PortableImportService, PortableInfoRecordV1,
            MAX_BUNDLE_FILE_BYTES, MAX_BUNDLE_RECORDS, MAX_MANIFEST_BYTES,
        },
        relation_query_service::{RelationQueryService, TraversalOptions, MAX_TRAVERSAL_NODES},
    },
    domain::{ids::AssetId, info::InfoType},
    ports::{Clock, SystemClock, UuidV7Generator},
    AppError,
};
use assetmesh_storage_sqlite::{backup::SqliteBackupStore, SharedSqlite};
use chrono::Duration;
use std::{fs, path::PathBuf, sync::Arc};

struct Env {
    root: PathBuf,
    database: PathBuf,
    factory: SharedSqlite,
}
impl Env {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("assetmesh-health-{}", uuid::Uuid::now_v7()));
        fs::create_dir(&root).unwrap();
        let database = root.join("library.db");
        let factory = SharedSqlite(Arc::new(
            assetmesh_storage_sqlite::open(database.to_str().unwrap()).unwrap(),
        ));
        Self {
            root,
            database,
            factory,
        }
    }
    fn add(&self, name: &str, kind: InfoType) -> AssetId {
        InfoService::new(
            self.factory.clone(),
            Arc::new(SystemClock),
            Arc::new(UuidV7Generator),
        )
        .create(CreateInfo {
            name: name.into(),
            info_type: kind,
            value: format!("value-{name}"),
            notes: None,
            tags: vec!["personal".into()],
        })
        .unwrap()
        .id
    }
    fn export(&self) -> PortableExportService<SharedSqlite> {
        PortableExportService::new(self.factory.clone(), Arc::new(SystemClock))
    }
    fn count_history(&self) -> usize {
        self.factory
            .0
            .with_raw_connection(|conn| {
                conn.query_row("SELECT count(*) FROM activity_events", [], |row| row.get(0))
                    .unwrap()
            })
            .unwrap()
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn streamed_export_preserves_v1_and_excludes_keys_with_their_redirects_and_related_rows() {
    let env = Env::new();
    let key = env.add("private-key", InfoType::ApiKey);
    let public = env.add("public-note", InfoType::Text);
    let redirect = env.add("old-private-key", InfoType::Text);
    env.factory.0.with_raw_connection(|conn| {
        conn.execute("UPDATE assets SET lifecycle_state='merged', merged_into_asset_id=?1 WHERE id=?2", [key.to_string(), redirect.to_string()]).unwrap();
        let (source, target) = if key < public { (key,public) } else { (public,key) };
        conn.execute("INSERT INTO relations(id,source_asset_id,target_asset_id,relation_type,note,provenance,created_at) VALUES (?1,?2,?3,'related_to',NULL,'manual',?4)", rusqlite::params![uuid::Uuid::now_v7().to_string(),source.to_string(),target.to_string(),SystemClock.now().to_rfc3339()]).unwrap();
        conn.execute("INSERT INTO external_refs(id,asset_id,namespace,external_id,source_url,metadata,created_at,updated_at) VALUES (?1,?2,'test_key','private-ref',NULL,'{}',?3,?3)", rusqlite::params![uuid::Uuid::now_v7().to_string(),key.to_string(),SystemClock.now().to_rfc3339()]).unwrap();
        conn.execute("INSERT INTO activity_events(id,occurred_at,event_type,asset_id,actor,payload) VALUES (?1,?2,'asset.merged',?3,'user',?4)", rusqlite::params![uuid::Uuid::now_v7().to_string(),SystemClock.now().to_rfc3339(),public.to_string(),r#"{"snapshot":{"info_type":"api_key","value":"nested-key-value"}}"#]).unwrap();
    }).unwrap();
    let full = env.export().export("test").unwrap();
    let full_path = env.root.join("full");
    env.export()
        .export_to_directory("test", &full_path, true)
        .unwrap();
    let streamed = read_bundle_from_directory(&full_path).unwrap();
    assert_eq!(full.manifest.record_counts, streamed.manifest.record_counts);
    for file in full.files {
        assert_eq!(
            file.content,
            streamed
                .files
                .iter()
                .find(|row| row.path == file.path)
                .unwrap()
                .content,
            "{}",
            file.path
        );
    }
    let destination = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open_in_memory().unwrap(),
    ));
    PortableImportService::new(destination)
        .import_bundle(&streamed, false)
        .unwrap();
    let legacy: PortableInfoRecordV1 = serde_json::from_str(&format!(
        r#"{{"asset_id":"{key}","info_type":"api_key","value":"legacy-key","notes":null}}"#
    ))
    .unwrap();
    assert_eq!(
        serde_json::to_value(legacy).unwrap()["info_type"],
        "api_key"
    );

    let safe_path = env.root.join("safe");
    env.export()
        .export_to_directory("test", &safe_path, false)
        .unwrap();
    let safe = read_bundle_from_directory(&safe_path).unwrap();
    assert_eq!(safe.manifest.record_counts["assets"], 1);
    assert_eq!(safe.manifest.record_counts["info"], 1);
    assert_eq!(safe.manifest.record_counts["relations"], 0);
    assert_eq!(safe.manifest.record_counts["external_refs"], 0);
    for file in &safe.files {
        assert!(!file.content.contains("value-private-key"));
        assert!(!file.content.contains("nested-key-value"));
    }
    let destination = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open_in_memory().unwrap(),
    ));
    PortableImportService::new(destination)
        .import_bundle(&safe, false)
        .unwrap();
    let store = SqliteBackupStore::new(env.factory.clone(), &env.database);
    assert!(
        store
            .snapshot(SystemClock.now(), "manual")
            .unwrap()
            .contains_api_keys
    );
}

#[test]
fn input_limits_reject_oversized_files_forged_counts_and_unsafe_paths() {
    let env = Env::new();
    let target = env.root.join("bundle");
    env.export()
        .export_to_directory("test", &target, true)
        .unwrap();
    let original = read_bundle_from_directory(&target).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(target.join("assets.jsonl"))
        .unwrap()
        .set_len(MAX_BUNDLE_FILE_BYTES as u64 + 1)
        .unwrap();
    assert!(read_bundle_from_directory(&target).is_err());
    write_bundle_to_directory(&original, &target).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(target.join("manifest.json"))
        .unwrap()
        .set_len(MAX_MANIFEST_BYTES as u64 + 1)
        .unwrap();
    assert!(read_bundle_from_directory(&target).is_err());
    fs::remove_dir_all(&target).unwrap();
    let mut too_many = original.clone();
    too_many
        .manifest
        .record_counts
        .insert("assets".into(), MAX_BUNDLE_RECORDS + 1);
    fs::create_dir(&target).unwrap();
    fs::write(
        target.join("manifest.json"),
        too_many.manifest_json().unwrap(),
    )
    .unwrap();
    assert!(
        read_bundle_from_directory(&target).is_err(),
        "reject before attempting section reads"
    );

    let outside = env.root.join("outside.txt");
    fs::write(&outside, "keep").unwrap();
    let mut traversal = original.clone();
    traversal.files.push(ExportFile {
        path: "../outside.txt".into(),
        content: "replace".into(),
    });
    assert!(write_bundle_to_directory(&traversal, &env.root.join("unsafe")).is_err());
    assert_eq!(fs::read_to_string(&outside).unwrap(), "keep");
    let mut tags = original;
    tags.files
        .iter_mut()
        .find(|file| file.path == "tags.json")
        .unwrap()
        .content = "[{}]".into();
    let destination = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open_in_memory().unwrap(),
    ));
    assert!(PortableImportService::new(destination)
        .import_bundle(&tags, false)
        .is_err());
}

#[cfg(unix)]
#[test]
fn nonregular_and_outside_symlink_inputs_are_rejected_without_blocking() {
    let env = Env::new();
    let target = env.root.join("input");
    fs::create_dir(&target).unwrap();
    let manifest = target.join("manifest.json");
    assert!(std::process::Command::new("mkfifo")
        .arg(&manifest)
        .status()
        .unwrap()
        .success());
    let began = std::time::Instant::now();
    assert!(read_bundle_from_directory(&target).is_err());
    assert!(began.elapsed() < std::time::Duration::from_secs(1));
    fs::remove_file(&manifest).unwrap();
    let outside = env.root.join("outside.json");
    fs::write(
        &outside,
        env.export()
            .export("test")
            .unwrap()
            .manifest_json()
            .unwrap(),
    )
    .unwrap();
    std::os::unix::fs::symlink(&outside, &manifest).unwrap();
    assert!(read_bundle_from_directory(&target).is_err());
}

#[test]
fn failed_streaming_export_keeps_the_previous_complete_bundle() {
    let env = Env::new();
    env.add("safe", InfoType::Text);
    let target = env.root.join("bundle");
    env.export()
        .export_to_directory("test", &target, true)
        .unwrap();
    let before = read_bundle_from_directory(&target)
        .unwrap()
        .fingerprint()
        .unwrap();
    env.factory.0.with_raw_connection(|conn| {
        conn.execute("INSERT INTO activity_events(id,occurred_at,event_type,asset_id,actor,payload) VALUES (?1,?2,'custom.large',NULL,'user',?3)", rusqlite::params![uuid::Uuid::now_v7().to_string(),SystemClock.now().to_rfc3339(),format!(r#"{{"padding":"{}"}}"#, "x".repeat(1024*1024))]).unwrap();
    }).unwrap();
    assert!(env
        .export()
        .export_to_directory("test", &target, true)
        .is_err());
    assert_eq!(
        read_bundle_from_directory(&target)
            .unwrap()
            .fingerprint()
            .unwrap(),
        before
    );
}

#[cfg(unix)]
#[test]
fn databases_sidecars_and_portable_output_have_private_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let env = Env::new();
    fs::set_permissions(&env.database, fs::Permissions::from_mode(0o644)).unwrap();
    let reopened = assetmesh_storage_sqlite::open(env.database.to_str().unwrap()).unwrap();
    env.add("note", InfoType::Text);
    for path in [
        &env.database,
        &PathBuf::from(format!("{}-wal", env.database.display())),
        &PathBuf::from(format!("{}-shm", env.database.display())),
    ] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600,
            "{}",
            path.display()
        );
    }
    let target = env.root.join("private");
    env.export()
        .export_to_directory("test", &target, true)
        .unwrap();
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o700
    );
    for file in read_bundle_from_directory(&target).unwrap().files {
        assert_eq!(
            fs::metadata(target.join(file.path))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    drop(reopened);
}

#[test]
fn history_cleanup_is_backed_up_and_preserves_imported_unknown_and_significant_events() {
    let env = Env::new();
    let now = SystemClock.now();
    env.factory.0.with_raw_connection_mut(|conn| {
        let tx = conn.transaction().unwrap();
        for n in 0..10_005 {
            tx.execute("INSERT INTO activity_events VALUES (?1,?2,'media.progress_changed',NULL,'user','{}')", rusqlite::params![uuid::Uuid::now_v7().to_string(),(now-Duration::days(1)+Duration::seconds(n)).to_rfc3339()]).unwrap();
        }
        for (event,actor) in [("media.progress_changed","user"),("media.progress_changed","import"),("asset.merged","user"),("import.completed","system"),("subscription.renewed","user"),("custom.unknown","user")] {
            tx.execute("INSERT INTO activity_events VALUES (?1,?2,?3,NULL,?4,'{}')", rusqlite::params![uuid::Uuid::now_v7().to_string(),(now-Duration::days(200)).to_rfc3339(),event,actor]).unwrap();
        }
        tx.commit().unwrap();
    }).unwrap();
    let count = env.count_history();
    assert!(env
        .factory
        .0
        .maintain_history(now, || Err(AppError::storage("cannot back up")))
        .is_err());
    assert_eq!(env.count_history(), count);
    let store = SqliteBackupStore::new(env.factory.clone(), &env.database);
    let mut recovery = None;
    let pruned = env
        .factory
        .0
        .maintain_history(now, || {
            recovery = Some(store.snapshot(now, "before_history_cleanup")?);
            Ok(())
        })
        .unwrap();
    assert_eq!(pruned, 6);
    assert_eq!(env.count_history(), 10_005);
    let entry = recovery.unwrap();
    store.preview(&entry.source_dir).unwrap();
    let snapshot = rusqlite::Connection::open_with_flags(
        PathBuf::from(&entry.source_dir).join("library.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM activity_events", [], |row| row
                .get::<_, usize>(0))
            .unwrap(),
        count
    );
    assert_eq!(
        env.factory
            .0
            .maintain_history(now, || panic!(
                "unchanged history needs no new recovery point"
            ))
            .unwrap(),
        0
    );
}

#[test]
fn wide_graphs_are_bounded_and_do_not_decode_unrelated_module_data() {
    let env = Env::new();
    let root = env.add("root", InfoType::Text);
    let unrelated = env.add("unrelated", InfoType::Text);
    env.factory.0.with_raw_connection_mut(|conn| {
        let tx = conn.transaction().unwrap();
        tx.execute_batch("PRAGMA ignore_check_constraints=ON").unwrap();
        tx.execute("UPDATE info_records SET info_type='broken' WHERE asset_id=?1", [unrelated.to_string()]).unwrap();
        tx.execute_batch("PRAGMA ignore_check_constraints=OFF").unwrap();
        for n in 0..(MAX_TRAVERSAL_NODES + 1) {
            let id = uuid::Uuid::now_v7().to_string();
            tx.execute("INSERT INTO assets(id,kind,name,lifecycle_state,revision,created_at,updated_at) VALUES (?1,'info.item',?2,'active',1,?3,?3)", rusqlite::params![id,format!("neighbor-{n}"),SystemClock.now().to_rfc3339()]).unwrap();
            let root_id = root.to_string();
            let (source,target) = if root_id < id { (&root_id,&id) } else { (&id,&root_id) };
            tx.execute("INSERT INTO relations VALUES (?1,?2,?3,'related_to',NULL,'manual',?4)", rusqlite::params![uuid::Uuid::now_v7().to_string(),source,target,SystemClock.now().to_rfc3339()]).unwrap();
        }
        tx.commit().unwrap();
    }).unwrap();
    let graph = RelationQueryService::new(env.factory.clone())
        .traverse(root, &TraversalOptions::default())
        .unwrap();
    assert_eq!(graph.nodes.len(), MAX_TRAVERSAL_NODES);
    assert!(graph.truncated);
}
