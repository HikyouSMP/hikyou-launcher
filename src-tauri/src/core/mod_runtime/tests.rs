use super::*;
use crate::core::test_support::zip;

struct Fixture {
    _directory: tempfile::TempDir,
    paths: LauncherPaths,
    id: String,
}

impl Fixture {
    fn smart() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let paths = LauncherPaths::new(directory.path().to_path_buf());
        paths.ensure_dirs().unwrap();
        let fixture = Self {
            _directory: directory,
            paths,
            id: "smart:latest-plus".into(),
        };
        std::fs::create_dir_all(fixture.game().join("mods")).unwrap();
        fixture
    }

    fn game(&self) -> PathBuf {
        self.paths.checked_profile_game_dir(&self.id).unwrap()
    }

    fn write_mod(&self, name: &str, version: &str) {
        let json = serde_json::to_vec(
            &serde_json::json!({ "schemaVersion": 1, "id": "fixture", "version": version }),
        )
        .unwrap();
        let bytes = zip(
            &[("fabric.mod.json", &json)],
            zip::CompressionMethod::Deflated,
        );
        std::fs::write(self.game().join("mods").join(name), bytes).unwrap();
    }

    fn capture(&self, parent: Option<&str>) -> ModRevision {
        let repo = ModRepository::new(&self.paths.root());
        let revision = repo
            .capture(
                &self.id,
                &self.game(),
                ModTarget {
                    mc_version: "1.21.1".into(),
                    loader: "fabric".into(),
                    loader_version: Some("0.16.10".into()),
                },
                parent,
            )
            .unwrap();
        repo.materialize(&revision).unwrap();
        revision
    }

    fn publish(&self, revision: &ModRevision) {
        ModRepository::new(&self.paths.root())
            .publish(&self.id, &revision.id)
            .unwrap();
    }
}

#[tokio::test]
async fn ready_launch_ignores_expiry_and_does_not_wait_for_update_mutex() {
    let fixture = Fixture::smart();
    fixture.write_mod("fixture.jar", "1.0.0");
    let revision = fixture.capture(None);
    fixture.publish(&revision);
    write_state(
        &fixture.paths,
        &fixture.id,
        &MaintenanceState {
            checked_at: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let _background_update = edit_guard(&fixture.paths, &fixture.id).await.unwrap();
    let prepared = tokio::time::timeout(
        Duration::from_secs(1),
        for_launch(&fixture.paths, &fixture.id),
    )
    .await
    .expect("a ready launch waited for background work")
    .unwrap();
    assert_eq!(prepared.revision_id, revision.id);
    assert_eq!(prepared.mc_version, "1.21.1");
    assert!(prepared.mods_dir.join("fixture.jar").is_file());
}

#[tokio::test]
async fn a_pending_request_does_not_change_or_delay_the_ready_launch() {
    let fixture = Fixture::smart();
    fixture.write_mod("fixture.jar", "1.0.0");
    let old = fixture.capture(None);
    fixture.publish(&old);
    let paths = Arc::new(fixture.paths.clone());
    request_project(&paths, &fixture.id, "AANobbMI", Some("Sodium".into()), None)
        .await
        .unwrap();
    let requested = requested_state(&paths, &fixture.id).unwrap();
    assert!(requested.pending);
    assert_eq!(requested.preparation.unwrap().phase, "queued");
    assert!(
        requested
            .intent
            .requirements
            .iter()
            .any(|request| request.project_id.as_deref() == Some("AANobbMI"))
    );
    let _lease = edit_guard(&paths, &fixture.id).await.unwrap();
    let launch = tokio::time::timeout(Duration::from_secs(1), for_launch(&paths, &fixture.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(launch.revision_id, old.id);
}

#[tokio::test]
async fn first_request_is_durable_without_a_head_or_network() {
    let fixture = Fixture::smart();
    let paths = Arc::new(fixture.paths.clone());
    request_project(&paths, &fixture.id, "AANobbMI", None, None)
        .await
        .unwrap();
    assert!(
        ModRepository::new(&paths.root())
            .head(&fixture.id)
            .unwrap()
            .is_none()
    );
    let requested = requested_state(&paths, &fixture.id).unwrap();
    assert!(requested.pending);
    assert_eq!(requested.intent.requirements.len(), 1);
    remove_request(&paths, &fixture.id, "modrinth:AANobbMI")
        .await
        .unwrap();
    let requested = requested_state(&paths, &fixture.id).unwrap();
    assert!(requested.intent.requirements.is_empty());
    assert_eq!(requested.intent.excluded_projects, ["AANobbMI"]);
}

#[tokio::test]
async fn a_failed_import_never_replaces_pending_wishes() {
    let fixture = Fixture::smart();
    fixture.write_mod("fixture.jar", "1.0.0");
    let old = fixture.capture(None);
    fixture.publish(&old);
    let mut pending = Intent::from_revision(&old);
    pending
        .upsert(mod_intent::Requirement::project(
            "pendingmod",
            true,
            None,
            None,
            "user",
        ))
        .unwrap();
    mod_intent::save(&fixture.paths.root(), &pending).unwrap();
    std::fs::write(
        fixture.paths.state_dir().join("mod-intents/resolutions"),
        b"block publication",
    )
    .unwrap();
    fixture.write_mod("fixture.jar", "2.0.0");
    assert!(record_edited(&fixture.paths, &fixture.id).await.is_err());
    assert_eq!(
        mod_intent::load(&fixture.paths.root(), &fixture.id).unwrap(),
        Some(pending)
    );
    assert_eq!(
        ready(&fixture.paths, &fixture.id)
            .unwrap()
            .unwrap()
            .revision_id,
        old.id
    );
}

#[test]
fn explicitly_removed_recommendations_do_not_reappear() {
    let fixture = Fixture::smart();
    let recommendation: mods::AutoMod = serde_json::from_value(serde_json::json!({
        "project_id":"AANobbMI", "name":"Sodium", "description":"fixture", "enabled":true, "loaders":["fabric"]
    })).unwrap();
    mods::save_auto_mods_to_file(&fixture.paths.auto_mods_file(), &[recommendation]).unwrap();
    let wish = Intent {
        schema: 1,
        profile_id: fixture.id.clone(),
        recommendations_initialized: false,
        requirements: Vec::new(),
        excluded_projects: vec!["AANobbMI".into()],
    };
    mod_intent::save(&fixture.paths.root(), &wish).unwrap();
    let target = ModTarget {
        mc_version: "1.21.1".into(),
        loader: "fabric".into(),
        loader_version: Some("0.16.10".into()),
    };
    let seeded = requests::current(&fixture.paths, &fixture.id, None, &target).unwrap();
    assert!(
        !seeded
            .requirements
            .iter()
            .any(|request| request.project_id.as_deref() == Some("AANobbMI"))
    );
    assert_eq!(seeded.excluded_projects, wish.excluded_projects);
    assert!(seeded.recommendations_initialized);
    let mut user_choice = seeded.clone();
    user_choice.requirements.clear();
    mod_intent::save(&fixture.paths.root(), &user_choice).unwrap();
    assert_eq!(
        requests::current(&fixture.paths, &fixture.id, None, &target).unwrap(),
        user_choice
    );
}

#[tokio::test]
async fn interrupted_restore_recovers_its_wishes_before_accepting_a_new_request() {
    let fixture = Fixture::smart();
    fixture.write_mod("fixture.jar", "1.0.0");
    let restored = fixture.capture(None);
    let restored_intent = Intent::from_revision(&restored);
    requests::record_resolution(
        &fixture.paths,
        &restored,
        restored_intent.clone(),
        Vec::new(),
    )
    .unwrap();
    fixture.publish(&restored);
    let mut abandoned = restored_intent;
    abandoned
        .upsert(mod_intent::Requirement::project(
            "abandoned",
            true,
            None,
            None,
            "user",
        ))
        .unwrap();
    mod_intent::save(&fixture.paths.root(), &abandoned).unwrap();
    write_state(
        &fixture.paths,
        &fixture.id,
        &MaintenanceState {
            pending_restore: Some(restored.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let paths = Arc::new(fixture.paths.clone());
    request_project(&paths, &fixture.id, "newmod", None, None)
        .await
        .unwrap();
    let intent = mod_intent::load(&paths.root(), &fixture.id)
        .unwrap()
        .unwrap();
    assert!(
        !intent
            .requirements
            .iter()
            .any(|request| request.project_id.as_deref() == Some("abandoned"))
    );
    assert!(
        intent
            .requirements
            .iter()
            .any(|request| request.project_id.as_deref() == Some("newmod"))
    );
    assert!(
        intent
            .requirements
            .iter()
            .any(|request| request.object_id.is_some())
    );
}

#[tokio::test]
async fn a_committed_import_recovers_bookkeeping_without_pinning_updates() {
    let fixture = Fixture::smart();
    fixture.write_mod("fixture.jar", "1.0.0");
    let old = fixture.capture(None);
    fixture.publish(&old);
    let mut intent = Intent::from_revision(&old);
    intent.recommendations_initialized = true;
    let next = seal_requested(
        &fixture.paths,
        &fixture.id,
        fixture.game(),
        old.target.clone(),
        Some(old.id.clone()),
        intent.clone(),
        Vec::new(),
    )
    .await
    .unwrap();
    fixture.publish(&next);
    write_state(
        &fixture.paths,
        &fixture.id,
        &MaintenanceState {
            revision_id: old.id,
            pending_restore: Some(next.id.clone()),
            pending_edit: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!pinned(&read_state(&fixture.paths, &fixture.id), &next));
    assert!(
        !requested_state(&fixture.paths, &fixture.id)
            .unwrap()
            .pending
    );
    let _guard = Arc::new(edit_guard(&fixture.paths, &fixture.id).await.unwrap());
    let mut state = read_state(&fixture.paths, &fixture.id);
    record::recover(
        &fixture.paths,
        &fixture.id,
        Some(&next),
        &fixture.game(),
        &mut state,
    )
    .unwrap();
    let state = read_state(&fixture.paths, &fixture.id);
    assert!(!state.pinned && !state.pending_edit);
    assert!(state.pending_restore.is_none());
    assert_eq!(
        mod_intent::load(&fixture.paths.root(), &fixture.id).unwrap(),
        Some(intent)
    );
}

#[tokio::test]
async fn local_toggle_preserves_identity_and_materializes_the_requested_filename() {
    let fixture = Fixture::smart();
    fixture.write_mod("fixture.jar", "1.0.0");
    let old = fixture.capture(None);
    fixture.publish(&old);
    mod_intent::save(&fixture.paths.root(), &Intent::from_revision(&old)).unwrap();
    let paths = Arc::new(fixture.paths.clone());
    change_installed_request(&paths, &fixture.id, "fixture.jar", false)
        .await
        .unwrap();
    let intent = mod_intent::load(&paths.root(), &fixture.id)
        .unwrap()
        .unwrap();
    let request = intent
        .requirements
        .iter()
        .find(|request| request.object_id.is_some())
        .unwrap();
    assert!(!request.enabled);
    assert_eq!(request.filename.as_deref(), Some("fixture.jar.disabled"));
    let stage = workspace::stage(&paths, &fixture.game()).unwrap();
    mod_intent::materialize_local(&paths.root(), stage.path(), &intent).unwrap();
    assert!(stage.path().join("mods/fixture.jar.disabled").is_file());
    assert!(!stage.path().join("mods/fixture.jar").exists());
}

#[test]
fn unpublished_successor_does_not_change_pinned_generation() {
    let fixture = Fixture::smart();
    fixture.write_mod("fixture.jar", "1.0.0");
    let old = fixture.capture(None);
    fixture.publish(&old);
    let pinned = ready(&fixture.paths, &fixture.id).unwrap().unwrap();
    let original = std::fs::read(pinned.mods_dir.join("fixture.jar")).unwrap();
    fixture.write_mod("fixture.jar", "2.0.0");
    let next = fixture.capture(Some(&old.id));
    assert_eq!(
        ready(&fixture.paths, &fixture.id)
            .unwrap()
            .unwrap()
            .revision_id,
        old.id
    );
    assert_eq!(
        std::fs::read(pinned.mods_dir.join("fixture.jar")).unwrap(),
        original
    );
    fixture.publish(&next);
    assert_eq!(
        ready(&fixture.paths, &fixture.id)
            .unwrap()
            .unwrap()
            .revision_id,
        next.id
    );
    assert_eq!(
        std::fs::read(pinned.mods_dir.join("fixture.jar")).unwrap(),
        original
    );
}

#[tokio::test]
async fn rollback_is_pinned_and_does_not_restore_world_or_config_files() {
    let fixture = Fixture::smart();
    fixture.write_mod("fixture.jar", "1.0.0");
    let old = fixture.capture(None);
    fixture.publish(&old);
    fixture.write_mod("fixture.jar", "2.0.0");
    let next = fixture.capture(Some(&old.id));
    fixture.publish(&next);
    std::fs::create_dir_all(fixture.game().join("saves")).unwrap();
    std::fs::write(fixture.game().join("saves/world-marker"), b"world").unwrap();
    std::fs::write(fixture.game().join("options.txt"), b"settings").unwrap();
    restore(&fixture.paths, &fixture.id, &old.id).await.unwrap();
    assert_eq!(
        std::fs::read(fixture.game().join("saves/world-marker")).unwrap(),
        b"world"
    );
    assert_eq!(
        std::fs::read(fixture.game().join("options.txt")).unwrap(),
        b"settings"
    );
    assert!(read_state(&fixture.paths, &fixture.id).pinned);
    // A forced maintenance pass must not undo an explicit rollback or contact APIs.
    assert_eq!(
        refresh_smart(&fixture.paths, &fixture.id, true)
            .await
            .unwrap()
            .id,
        old.id
    );
    std::fs::write(fixture.paths.auto_mods_file(), b"[]").unwrap();
    fixture.write_mod("fixture.jar", "3.0.0");
    record(&fixture.paths, &fixture.id).await.unwrap();
    assert_eq!(
        refresh_smart(&fixture.paths, &fixture.id, true)
            .await
            .unwrap()
            .id,
        old.id,
        "metadata/import and recipe changes must not undo an explicit pin"
    );
    record_edited(&fixture.paths, &fixture.id).await.unwrap();
    assert_ne!(
        ready(&fixture.paths, &fixture.id)
            .unwrap()
            .unwrap()
            .revision_id,
        old.id
    );
    assert!(!read_state(&fixture.paths, &fixture.id).pinned);
}

#[test]
fn interrupted_checkout_recovers_previous_working_tree_and_head() {
    let fixture = Fixture::smart();
    fixture.write_mod("fixture.jar", "1.0.0");
    let old = fixture.capture(None);
    fixture.publish(&old);
    let original = std::fs::read(fixture.game().join("mods/fixture.jar")).unwrap();
    fixture.write_mod("fixture.jar", "2.0.0");
    let next = fixture.capture(Some(&old.id));
    std::fs::write(fixture.game().join("mods/fixture.jar"), &original).unwrap();
    let backup = ".hikyou-mod-backup-test";
    let stage = ".hikyou-mod-stage-test";
    std::fs::rename(fixture.game().join("mods"), fixture.game().join(backup)).unwrap();
    std::fs::create_dir(fixture.game().join("mods")).unwrap();
    std::fs::write(fixture.game().join("mods/partial.jar"), b"incomplete").unwrap();
    std::fs::create_dir(fixture.game().join(stage)).unwrap();
    std::fs::write(fixture.game().join(".hikyou-mod-checkout.json"), serde_json::to_vec(&serde_json::json!({ "revision": next.id, "previous_head": old.id, "backup": backup, "stage": stage, "committed": false })).unwrap()).unwrap();
    workspace::recover(
        &ModRepository::new(&fixture.paths.root()),
        &fixture.id,
        &fixture.game(),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(fixture.game().join("mods/fixture.jar")).unwrap(),
        original
    );
    assert!(!fixture.game().join("mods/partial.jar").exists());
    assert!(!fixture.game().join(".hikyou-mod-checkout.json").exists());
    assert_eq!(
        ready(&fixture.paths, &fixture.id)
            .unwrap()
            .unwrap()
            .revision_id,
        old.id
    );
}

#[tokio::test]
async fn use_lease_outlives_the_local_edit_guard() {
    let fixture = Fixture::smart();
    let guard = edit_guard(&fixture.paths, &fixture.id).await.unwrap();
    let lease = guard.lease();
    drop(guard);
    assert!(edit_guard(&fixture.paths, &fixture.id).await.is_err());
    drop(lease);
    assert!(edit_guard(&fixture.paths, &fixture.id).await.is_ok());
}

#[tokio::test]
async fn ordinary_first_import_deduplicates_and_repeated_launch_is_read_only() {
    let mut fixture = Fixture::smart();
    let profile = profile::Profile::new(
        "Fixture".into(),
        "1.21.1".into(),
        "fabric".into(),
        Some("0.16.10".into()),
    );
    fixture.id = profile.id.clone();
    profile::save_profile(&fixture.paths.root(), &profile)
        .await
        .unwrap();
    std::fs::create_dir_all(fixture.game().join("mods")).unwrap();
    fixture.write_mod("fixture.jar", "1.0.0");
    record(&fixture.paths, &fixture.id).await.unwrap();
    let first = ready(&fixture.paths, &fixture.id).unwrap().unwrap();
    record(&fixture.paths, &fixture.id).await.unwrap();
    assert_eq!(
        ModRepository::new(&fixture.paths.root())
            .history(&fixture.id)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        ready(&fixture.paths, &fixture.id)
            .unwrap()
            .unwrap()
            .revision_id,
        first.revision_id
    );
    let _maintenance = edit_guard(&fixture.paths, &fixture.id).await.unwrap();
    let cached = tokio::time::timeout(
        Duration::from_secs(1),
        cached_for_launch(&fixture.paths, &fixture.id),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    assert_eq!(cached.revision_id, first.revision_id);
}

#[tokio::test]
async fn missing_working_folder_cannot_publish_an_empty_successor() {
    let fixture = Fixture::smart();
    fixture.write_mod("fixture.jar", "1.0.0");
    let revision = fixture.capture(None);
    fixture.publish(&revision);
    std::fs::remove_dir_all(fixture.game().join("mods")).unwrap();
    assert!(
        refresh_smart(&fixture.paths, &fixture.id, true)
            .await
            .is_err()
    );
    assert!(record(&fixture.paths, &fixture.id).await.is_err());
    assert!(backup_working_tree(&fixture.paths, &fixture.id).is_err());
    assert!(!fixture.game().join("mods").exists());
    assert_eq!(
        ready(&fixture.paths, &fixture.id)
            .unwrap()
            .unwrap()
            .revision_id,
        revision.id
    );
}

#[test]
fn restore_intent_pins_only_after_its_head_is_published() {
    let fixture = Fixture::smart();
    fixture.write_mod("fixture.jar", "1.0.0");
    let before = fixture.capture(None);
    fixture.publish(&before);
    fixture.write_mod("fixture.jar", "2.0.0");
    let after = fixture.capture(Some(&before.id));
    let state = MaintenanceState {
        revision_id: before.id.clone(),
        pending_restore: Some(after.id.clone()),
        ..Default::default()
    };
    assert!(!pinned(&state, &before));
    assert!(pinned(&state, &after));
    write_state(&fixture.paths, &fixture.id, &state).unwrap();
    fixture.publish(&after);
    assert!(
        status(&fixture.paths, &fixture.id).unwrap().unwrap()["pinned"]
            .as_bool()
            .unwrap()
    );
}

#[test]
fn quick_check_measurement_excludes_content_hashing() {
    let fixture = Fixture::smart();
    let payload = vec![7; 8 * 1024 * 1024];
    let bytes = zip(
        &[
            ("fabric.mod.json", br#"{"id":"fixture","version":"1.0.0"}"#),
            ("payload.bin", &payload),
        ],
        zip::CompressionMethod::Stored,
    );
    std::fs::write(fixture.game().join("mods/fixture.jar"), bytes).unwrap();
    let revision = fixture.capture(None);
    fixture.publish(&revision);
    let repo = ModRepository::new(&fixture.paths.root());
    let full = std::time::Instant::now();
    repo.verify(&revision).unwrap();
    let full = full.elapsed();
    let quick = std::time::Instant::now();
    ready(&fixture.paths, &fixture.id).unwrap().unwrap();
    let quick = quick.elapsed();
    eprintln!(
        "[mod measurement] 8 MiB artifact: deep verification={full:?}, ready snapshot lookup={quick:?}; this is object I/O, not total Minecraft launch time"
    );
}

#[test]
fn rejected_checkout_cleans_staging_and_preserves_unknown_folders() {
    let fixture = Fixture::smart();
    fixture.write_mod("fixture.jar", "1.0.0");
    let revision = fixture.capture(None);
    fixture.publish(&revision);
    std::fs::create_dir(fixture.game().join("mods/user-folder")).unwrap();
    std::fs::write(fixture.game().join("mods/user-folder/keep"), b"untouched").unwrap();
    let repo = ModRepository::new(&fixture.paths.root());
    let generation = repo.quick_check(&revision).unwrap();
    assert!(workspace::checkout(&repo, &revision, &generation, &fixture.game()).is_err());
    assert_eq!(
        std::fs::read(fixture.game().join("mods/user-folder/keep")).unwrap(),
        b"untouched"
    );
    assert!(!std::fs::read_dir(fixture.game()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".hikyou-mod-stage-")
    }));
}

#[tokio::test]
#[ignore = "read-only validation of an explicitly configured local Mod folder"]
async fn live_local_mod_validation() {
    let game =
        std::env::var_os("HIKYOU_TEST_MODS_GAME_DIR").expect("set HIKYOU_TEST_MODS_GAME_DIR");
    let version = std::env::var("HIKYOU_TEST_MC_VERSION").expect("set HIKYOU_TEST_MC_VERSION");
    let loader =
        std::env::var("HIKYOU_TEST_LOADER_VERSION").expect("set HIKYOU_TEST_LOADER_VERSION");
    let java = std::env::var("HIKYOU_TEST_JAVA_MAJOR")
        .expect("set HIKYOU_TEST_JAVA_MAJOR")
        .parse()
        .expect("Java major must be numeric");
    mods::validate_mod_set_with_runtime(
        Path::new(&game),
        &version,
        "fabric",
        Some(&loader),
        Some(java),
    )
    .await
    .unwrap();
}
