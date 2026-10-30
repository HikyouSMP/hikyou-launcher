use super::*;
use crate::core::test_support::{physical_tempdir, zip};

struct Fixture {
    _directory: tempfile::TempDir,
    paths: LauncherPaths,
    id: String,
}

impl Fixture {
    fn smart() -> Self {
        let directory = physical_tempdir();
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
async fn a_pending_request_does_not_change_or_delay_the_ready_launch()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::smart();
    crate::core::mod_recommendations::save_auto_mods_to_file(&fixture.paths.auto_mods_file(), &[])?;
    fixture.write_mod("fixture.jar", "1.0.0");
    let old = fixture.capture(None);
    fixture.publish(&old);
    let paths = Arc::new(fixture.paths.clone());
    request_project(&paths, &fixture.id, "AANobbMI", Some("Sodium".into()), None).await?;
    let requested = requested_state(&paths, &fixture.id)?;
    assert!(requested.pending);
    assert_eq!(
        requested.preparation.ok_or("missing preparation")?.phase,
        "queued"
    );
    assert!(
        requested
            .intent
            .requirements
            .iter()
            .any(|request| request.project_id.as_deref() == Some("AANobbMI"))
    );
    let _lease = edit_guard(&paths, &fixture.id).await?;
    let launch =
        tokio::time::timeout(Duration::from_secs(1), for_launch(&paths, &fixture.id)).await??;
    assert_eq!(launch.revision_id, old.id);
    Ok(())
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
fn explicitly_removed_recommendations_do_not_reappear() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::smart();
    let recommendation: mods::AutoMod = serde_json::from_value(serde_json::json!({
        "project_id":"AANobbMI", "name":"Sodium", "description":"fixture", "enabled":true, "loaders":["fabric"]
    }))?;
    crate::core::mod_recommendations::save_auto_mods_to_file(
        &fixture.paths.auto_mods_file(),
        &[recommendation],
    )?;
    let wish = Intent {
        schema: 1,
        profile_id: fixture.id.clone(),
        recommendations_initialized: false,
        requirements: Vec::new(),
        excluded_projects: vec!["AANobbMI".into()],
    };
    mod_intent::save(&fixture.paths.root(), &wish)?;
    let target = ModTarget {
        mc_version: "1.21.1".into(),
        loader: "fabric".into(),
        loader_version: Some("0.16.10".into()),
    };
    let seeded = requests::current(&fixture.paths, &fixture.id, None, &target)?;
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
    mod_intent::save(&fixture.paths.root(), &user_choice)?;
    assert_eq!(
        requests::current(&fixture.paths, &fixture.id, None, &target)?,
        user_choice
    );
    Ok(())
}

#[tokio::test]
async fn interrupted_restore_recovers_its_wishes_before_accepting_a_new_request()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::smart();
    crate::core::mod_recommendations::save_auto_mods_to_file(&fixture.paths.auto_mods_file(), &[])?;
    fixture.write_mod("fixture.jar", "1.0.0");
    let restored = fixture.capture(None);
    let restored_intent = Intent::from_revision(&restored);
    requests::record_resolution(
        &fixture.paths,
        &restored,
        restored_intent.clone(),
        Vec::new(),
    )?;
    fixture.publish(&restored);
    let mut abandoned = restored_intent;
    abandoned.upsert(mod_intent::Requirement::project(
        "abandoned",
        true,
        None,
        None,
        "user",
    ))?;
    mod_intent::save(&fixture.paths.root(), &abandoned)?;
    write_state(
        &fixture.paths,
        &fixture.id,
        &MaintenanceState {
            pending_restore: Some(restored.id.clone()),
            ..Default::default()
        },
    )?;
    let paths = Arc::new(fixture.paths.clone());
    request_project(&paths, &fixture.id, "newmod", None, None).await?;
    let intent = mod_intent::load(&paths.root(), &fixture.id)?.ok_or("missing intent")?;
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
    Ok(())
}

#[tokio::test]
async fn a_committed_import_recovers_bookkeeping_without_pinning_updates()
-> Result<(), Box<dyn std::error::Error>> {
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
    .await?;
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
    )?;
    assert!(!pinned(&read_state(&fixture.paths, &fixture.id)?, &next));
    assert!(!requested_state(&fixture.paths, &fixture.id)?.pending);
    let _guard = Arc::new(edit_guard(&fixture.paths, &fixture.id).await?);
    let mut state = read_state(&fixture.paths, &fixture.id)?;
    record::recover(
        &fixture.paths,
        &fixture.id,
        Some(&next),
        &fixture.game(),
        &mut state,
    )?;
    let state = read_state(&fixture.paths, &fixture.id)?;
    assert!(!state.pinned && !state.pending_edit);
    assert!(state.pending_restore.is_none());
    assert_eq!(
        mod_intent::load(&fixture.paths.root(), &fixture.id)?,
        Some(intent)
    );
    Ok(())
}

#[tokio::test]
async fn local_toggle_preserves_identity_and_materializes_the_requested_filename()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::smart();
    crate::core::mod_recommendations::save_auto_mods_to_file(&fixture.paths.auto_mods_file(), &[])?;
    fixture.write_mod("fixture.jar", "1.0.0");
    let old = fixture.capture(None);
    fixture.publish(&old);
    mod_intent::save(&fixture.paths.root(), &Intent::from_revision(&old))?;
    let paths = Arc::new(fixture.paths.clone());
    change_installed_request(&paths, &fixture.id, "fixture.jar", false).await?;
    let intent = mod_intent::load(&paths.root(), &fixture.id)?.ok_or("missing intent")?;
    let request = intent
        .requirements
        .iter()
        .find(|request| request.object_id.is_some())
        .ok_or("missing local request")?;
    assert!(!request.enabled);
    assert_eq!(request.filename.as_deref(), Some("fixture.jar.disabled"));
    let stage = workspace::stage(&paths, &fixture.game())?;
    mod_intent::materialize_local(&paths.root(), stage.path(), &intent)?;
    assert!(stage.path().join("mods/fixture.jar.disabled").is_file());
    assert!(!stage.path().join("mods/fixture.jar").exists());
    Ok(())
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
async fn rollback_is_pinned_and_does_not_restore_world_or_config_files()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::smart();
    fixture.write_mod("fixture.jar", "1.0.0");
    let old = fixture.capture(None);
    fixture.publish(&old);
    fixture.write_mod("fixture.jar", "2.0.0");
    let next = fixture.capture(Some(&old.id));
    fixture.publish(&next);
    std::fs::create_dir_all(fixture.game().join("saves"))?;
    std::fs::write(fixture.game().join("saves/world-marker"), b"world")?;
    std::fs::write(fixture.game().join("options.txt"), b"settings")?;
    restore(&fixture.paths, &fixture.id, &old.id).await?;
    assert_eq!(
        std::fs::read(fixture.game().join("saves/world-marker"))?,
        b"world"
    );
    assert_eq!(
        std::fs::read(fixture.game().join("options.txt"))?,
        b"settings"
    );
    assert!(read_state(&fixture.paths, &fixture.id)?.pinned);
    // A forced maintenance pass must not undo an explicit rollback or contact APIs.
    assert_eq!(
        refresh_smart(&fixture.paths, &fixture.id, true).await?.id,
        old.id
    );
    std::fs::write(fixture.paths.auto_mods_file(), b"[]")?;
    fixture.write_mod("fixture.jar", "3.0.0");
    record(&fixture.paths, &fixture.id).await?;
    assert_eq!(
        refresh_smart(&fixture.paths, &fixture.id, true).await?.id,
        old.id,
        "metadata/import and recipe changes must not undo an explicit pin"
    );
    record_edited(&fixture.paths, &fixture.id).await?;
    assert_ne!(
        ready(&fixture.paths, &fixture.id)?
            .ok_or("ready revision missing after explicit edit")?
            .revision_id,
        old.id
    );
    assert!(!read_state(&fixture.paths, &fixture.id)?.pinned);
    Ok(())
}

#[tokio::test]
async fn corrupt_protection_state_blocks_maintenance_without_replacing_the_head()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let paths = LauncherPaths::new(directory.path().canonicalize()?);
    paths.ensure_dirs()?;
    let id = "smart:latest-plus";
    let game = paths.checked_profile_game_dir(id)?;
    std::fs::create_dir_all(game.join("mods"))?;
    let repo = ModRepository::new(&paths.root());
    let revision = repo.capture(
        id,
        &game,
        ModTarget {
            mc_version: "1.21.1".into(),
            loader: "fabric".into(),
            loader_version: Some("0.19.5".into()),
        },
        None,
    )?;
    repo.materialize(&revision)?;
    repo.publish(id, &revision.id)?;
    write_state(
        &paths,
        id,
        &MaintenanceState {
            pinned: true,
            revision_id: revision.id.clone(),
            ..Default::default()
        },
    )?;
    let dir = state_dir(&paths)?;
    let name = format!("{}.json", key(id));
    for bytes in [b"{".to_vec(), b"[]".to_vec(), vec![b' '; 64 * 1024 + 1]] {
        managed_fs::write(&dir, Path::new(&name), &bytes, true)?;
        assert!(read_state(&paths, id).is_err());
        assert!(refresh_smart(&paths, id, true).await.is_err());
        assert_eq!(repo.head(id)?.ok_or("head disappeared")?.id, revision.id);
        assert_eq!(
            std::fs::read(paths.state_dir().join("mod-maintenance").join(&name))?,
            bytes
        );
    }
    dir.remove_file(&name)?;
    dir.create_dir(&name)?;
    assert!(
        read_state(&paths, id).is_err(),
        "non-file protection state must not become defaults"
    );
    assert_eq!(repo.head(id)?.ok_or("head disappeared")?.id, revision.id);
    Ok(())
}

#[tokio::test]
async fn recommendation_replay_cannot_unpin_a_restored_profile()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let paths = Arc::new(LauncherPaths::new(temp.path().canonicalize()?));
    paths.ensure_dirs()?;
    let policy: Vec<mods::AutoMod> = serde_json::from_value(serde_json::json!([{
        "project_id":"sodium", "name":"Sodium", "description":"fixture",
        "enabled":true, "loaders":["fabric"]
    }]))?;
    drop(crate::core::mod_recommendations::PolicyUpdate::open(
        &paths.auto_mods_file(),
        Some(&policy),
    )?);
    write_state(
        &paths,
        "smart:latest-plus",
        &MaintenanceState {
            pinned: true,
            ..Default::default()
        },
    )?;
    assert!(recommendation_changes(&paths, None).await.is_err());
    assert!(read_state(&paths, "smart:latest-plus")?.pinned);
    assert!(mod_intent::load(&paths.root(), "smart:latest-plus")?.is_none());
    let pending =
        crate::core::mod_recommendations::PolicyUpdate::open(&paths.auto_mods_file(), None)?;
    assert_eq!(pending.pending().len(), 2);
    drop(pending);
    // A fresh explicit save is a new user decision, not automatic replay.
    assert_eq!(
        recommendation_changes(&paths, Some(&policy)).await?.len(),
        2
    );
    assert!(!read_state(&paths, "smart:latest-plus")?.pinned);
    assert!(!paths.root().join("auto_mods.pending.json").exists());
    Ok(())
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
