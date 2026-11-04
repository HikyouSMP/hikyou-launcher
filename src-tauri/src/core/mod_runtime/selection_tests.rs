use super::*;
use std::io::Write;

const ID: &str = "smart:latest-plus";
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn excluded_project_is_disabled_before_another_mod_is_enabled() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.recommendations(false)?;
    fixture.head(true)?;
    let game = fixture.paths.checked_profile_game_dir(ID)?;
    let bytes = std::fs::read(game.join("mods/local.jar"))?;
    std::fs::write(game.join("mods/renderer.jar.disabled"), &bytes)?;
    std::fs::write(
        game.join("mods/.mod_meta.json"),
        serde_json::to_vec(&serde_json::json!({
            "local.jar":{"project_id":"excluded","display_name":"Excluded utility","icon_url":null},
            "renderer.jar.disabled":{"project_id":"renderer","display_name":"Renderer","icon_url":null}
        }))?,
    )?;
    let mut intent = Intent::empty(ID);
    intent.excluded_projects.push("excluded".into());
    intent
        .requirements
        .push(Requirement::project("renderer", true, None, None, "user"));
    let stage = workspace::stage(&fixture.paths, &game)?;
    requests::stage_requests(&fixture.paths, stage.path(), None, &intent).await?;
    assert!(!stage.path().join("mods/local.jar").exists());
    assert_eq!(
        std::fs::read(stage.path().join("mods/local.jar.disabled"))?,
        bytes
    );
    assert_eq!(
        std::fs::read(stage.path().join("mods/renderer.jar"))?,
        bytes
    );
    assert!(!stage.path().join("mods/renderer.jar.disabled").exists());
    assert_eq!(std::fs::read(game.join("mods/local.jar"))?, bytes);
    fixture.assert_assets()?;
    Ok(())
}

#[test]
fn independent_selection_never_reads_creation_or_legacy_preferences() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.head(false)?;
    let mut intent = Intent::empty(ID);
    intent.independent = true;
    intent.requirements.push(Requirement::project(
        "renderer",
        false,
        Some("Renderer".into()),
        None,
        "user",
    ));
    mod_intent::save(&fixture.paths.root(), &intent)?;
    std::fs::write(fixture.paths.auto_mods_file(), b"{")?;
    let observed = snapshot(&fixture.paths, ID)?;
    assert_eq!(observed.rows.len(), 1);
    assert_eq!(observed.rows[0].name, "Renderer");
    assert_eq!(observed.rows[0].desired, "excluded");
    assert!(!observed.rows[0].can_inherit);
    assert_eq!(
        selection::for_target(&fixture.paths, &intent, &target())?,
        intent
    );
    fixture.assert_assets()?;
    Ok(())
}

#[test]
fn missing_managed_wishes_are_not_reconstructed_as_an_empty_selection() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.recommendations(false)?;
    let legacy = fixture.head(false)?;
    let repo = ModRepository::new(&fixture.paths.root());
    let desired = Intent::empty(ID);
    let revision = repo.capture_with_intent(
        ID,
        &fixture.paths.checked_profile_game_dir(ID)?,
        target(),
        Some(&legacy.id),
        Some(&desired.fingerprint()?),
    )?;
    requests::record_resolution(&fixture.paths, &revision, desired, Vec::new())?;
    repo.materialize(&revision)?;
    repo.publish(ID, &revision.id)?;
    assert!(mod_intent::load(&fixture.paths.root(), ID)?.is_none());
    assert!(snapshot(&fixture.paths, ID).is_err());
    assert!(requests::current(&fixture.paths, ID, Some(&revision), &target()).is_err());
    assert!(mod_intent::load(&fixture.paths.root(), ID)?.is_none());
    assert_eq!(repo.head(ID)?.ok_or("missing head")?.id, revision.id);
    fixture.assert_assets()?;
    Ok(())
}

struct Fixture {
    directory: tempfile::TempDir,
    paths: Arc<LauncherPaths>,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let paths = Arc::new(LauncherPaths::new(directory.path().canonicalize()?));
        Ok(Self { directory, paths })
    }

    fn recommendations(&self, enabled: bool) -> TestResult {
        let entry: AutoMod = serde_json::from_value(serde_json::json!({
            "project_id": "example", "name": "Example", "description": "fixture",
            "enabled": enabled, "loaders": ["fabric"]
        }))?;
        mod_recommendations::save_auto_mods_to_file(&self.paths.auto_mods_file(), &[entry])?;
        Ok(())
    }

    fn head(&self, local: bool) -> Result<ModRevision, Box<dyn std::error::Error>> {
        let game = self.paths.checked_profile_game_dir(ID)?;
        std::fs::create_dir_all(game.join("mods"))?;
        std::fs::create_dir_all(game.join("saves"))?;
        std::fs::write(game.join("saves/sentinel"), b"world")?;
        std::fs::write(game.join("options.txt"), b"config")?;
        if local {
            let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            archive.start_file(
                "fabric.mod.json",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )?;
            archive.write_all(br#"{"schemaVersion":1,"id":"fixture","version":"1.0.0"}"#)?;
            std::fs::write(game.join("mods/local.jar"), archive.finish()?.into_inner())?;
        }
        let repo = ModRepository::new(&self.paths.root());
        let revision = repo.capture(ID, &game, target(), None)?;
        repo.materialize(&revision)?;
        repo.publish(ID, &revision.id)?;
        Ok(revision)
    }

    fn assert_assets(&self) -> TestResult {
        let game = self.paths.checked_profile_game_dir(ID)?;
        assert_eq!(std::fs::read(game.join("saves/sentinel"))?, b"world");
        assert_eq!(std::fs::read(game.join("options.txt"))?, b"config");
        Ok(())
    }
}

fn target() -> ModTarget {
    ModTarget {
        mc_version: "1.21.1".into(),
        loader: "fabric".into(),
        loader_version: Some("0.16.10".into()),
    }
}

fn row<'a>(snapshot: &'a ModSnapshot, key: &str) -> Result<&'a ModRow, String> {
    snapshot
        .rows
        .iter()
        .find(|row| row.key == key)
        .ok_or_else(|| format!("missing row: {key}"))
}

#[tokio::test]
async fn legacy_exclusion_without_request_or_artifact_can_be_reenabled_after_restart() -> TestResult
{
    let fixture = Fixture::new()?;
    fixture.recommendations(true)?;
    let head = fixture.head(false)?;
    let mut intent = Intent::empty(ID);
    intent.excluded_projects.push("example".into());
    mod_intent::save(&fixture.paths.root(), &intent)?;
    let before = snapshot(&fixture.paths, ID)?;
    let excluded = row(&before, "modrinth:example")?;
    assert_eq!(
        (excluded.desired, excluded.source, excluded.reason),
        ("excluded", "legacyUnknown", "legacyExcluded")
    );
    assert!(excluded.active.is_none() && excluded.can_enable && excluded.can_inherit);
    let included = set_choice(
        &fixture.paths,
        ID,
        "modrinth:example",
        "include",
        &before.revision,
    )
    .await?;
    assert_eq!(row(&included, "modrinth:example")?.desired, "required");
    assert!(included.requested.pending);
    assert!(row(&included, "modrinth:example")?.active.is_none());
    let restarted = Arc::new(LauncherPaths::new(fixture.paths.root()));
    assert_eq!(snapshot(&restarted, ID)?.revision, included.revision);
    let excluded = set_choice(
        &restarted,
        ID,
        "modrinth:example",
        "exclude",
        &included.revision,
    )
    .await?;
    assert_eq!(row(&excluded, "modrinth:example")?.source, "user");
    assert!(excluded.requested.intent.requirements.is_empty());
    let inherited = set_choice(
        &restarted,
        ID,
        "modrinth:example",
        "inherit",
        &excluded.revision,
    )
    .await?;
    assert_eq!(
        (
            row(&inherited, "modrinth:example")?.desired,
            row(&inherited, "modrinth:example")?.source
        ),
        ("required", "recommended")
    );
    assert_eq!(
        ModRepository::new(&restarted.root())
            .head(ID)?
            .ok_or("missing head")?
            .id,
        head.id
    );
    fixture.assert_assets()?;
    Ok(())
}

#[tokio::test]
async fn stale_unknown_and_filename_actions_do_not_mutate_and_aba_is_detected() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.recommendations(false)?;
    let initial = snapshot(&fixture.paths, ID)?;
    assert!(
        set_choice(
            &fixture.paths,
            ID,
            "example.jar",
            "include",
            &initial.revision
        )
        .await
        .is_err()
    );
    assert!(
        set_choice(
            &fixture.paths,
            ID,
            "modrinth:unknown",
            "include",
            &initial.revision
        )
        .await
        .is_err()
    );
    assert!(
        set_choice(
            &fixture.paths,
            ID,
            "modrinth:example",
            "invalid",
            &initial.revision
        )
        .await
        .is_err()
    );
    assert!(mod_intent::load(&fixture.paths.root(), ID)?.is_none());
    let b = set_choice(
        &fixture.paths,
        ID,
        "modrinth:example",
        "include",
        &initial.revision,
    )
    .await?;
    assert!(
        set_choice(
            &fixture.paths,
            ID,
            "modrinth:example",
            "exclude",
            &initial.revision
        )
        .await
        .is_err()
    );
    let a = set_choice(
        &fixture.paths,
        ID,
        "modrinth:example",
        "inherit",
        &b.revision,
    )
    .await?;
    assert_eq!(
        row(&a, "modrinth:example")?.desired,
        row(&initial, "modrinth:example")?.desired
    );
    assert_ne!(a.revision, initial.revision);
    let b2 = set_choice(
        &fixture.paths,
        ID,
        "modrinth:example",
        "include",
        &a.revision,
    )
    .await?;
    let a2 = set_choice(
        &fixture.paths,
        ID,
        "modrinth:example",
        "inherit",
        &b2.revision,
    )
    .await?;
    assert_eq!(
        a.requested.intent.choice_provenance,
        a2.requested.intent.choice_provenance
    );
    assert_eq!(
        a2.requested.intent.choice_sequence,
        a.requested.intent.choice_sequence + 2
    );
    assert_ne!(a.revision, a2.revision);
    assert!(
        set_choice(
            &fixture.paths,
            ID,
            "modrinth:example",
            "exclude",
            &a.revision
        )
        .await
        .is_err()
    );
    Ok(())
}

#[test]
fn default_off_and_target_applicability_do_not_create_prohibitions_or_erase_explicit_choices()
-> TestResult {
    let fixture = Fixture::new()?;
    fixture.recommendations(false)?;
    let recommendations =
        mod_recommendations::load_auto_mods_checked(&fixture.paths.auto_mods_file())?
            .ok_or("missing policy")?;
    let mut intent = Intent::empty(ID);
    inherit_recommendations(&mut intent, &recommendations, Some(&target()))?;
    assert!(intent.requirements.is_empty() && intent.excluded_projects.is_empty());
    assert_eq!(
        policy(&intent)?
            .choice(&ModKey::modrinth("example")?)
            .desired,
        Desired::NotRequested
    );
    let mut dependency_revision = fixture.head(true)?;
    let dependency = dependency_revision
        .mods
        .first_mut()
        .ok_or("missing dependency fixture")?;
    dependency.project_id = Some("example".into());
    mod_intent::verify_selected(&intent, &dependency_revision)?;
    choose(&mut intent, None, "modrinth:example", "include")?;
    let vanilla = ModTarget {
        loader: "vanilla".into(),
        ..target()
    };
    inherit_recommendations(&mut intent, &recommendations, Some(&vanilla))?;
    assert!(matches!(
        policy(&intent)?
            .choice(&ModKey::modrinth("example")?)
            .desired,
        Desired::Required(_)
    ));
    choose(&mut intent, None, "modrinth:example", "exclude")?;
    let mut enabled = recommendations.clone();
    enabled[0].enabled = true;
    inherit_recommendations(&mut intent, &enabled, Some(&target()))?;
    assert_eq!(intent.excluded_projects, ["example"]);
    choose(&mut intent, None, "modrinth:example", "inherit")?;
    inherit_recommendations(&mut intent, &enabled, Some(&vanilla))?;
    assert!(intent.requirements.iter().any(|request| request.enabled));
    // Vanilla fallback must ask to omit the desired Fabric Mods, not make them
    // disappear by declaring that its chosen loader makes them inapplicable.
    let other_loader = ModTarget {
        loader: "neoforge".into(),
        ..target()
    };
    inherit_recommendations(&mut intent, &enabled, Some(&other_loader))?;
    assert!(intent.requirements.is_empty());
    inherit_recommendations(&mut intent, &enabled, Some(&target()))?;
    assert!(
        intent
            .requirements
            .iter()
            .any(|r| r.enabled && r.origin == "recommended")
    );
    Ok(())
}

#[tokio::test]
async fn local_exclude_inherit_and_reenable_preserve_exact_bytes_and_live_files() -> TestResult {
    let fixture = Fixture::new()?;
    let head = fixture.head(true)?;
    let entry = head.mods.first().ok_or("missing local entry")?;
    let key = entry_key(entry);
    let game = fixture.paths.checked_profile_game_dir(ID)?;
    let original = std::fs::read(game.join("mods/local.jar"))?;
    let before = snapshot(&fixture.paths, ID)?;
    let excluded = set_choice(&fixture.paths, ID, &key, "exclude", &before.revision).await?;
    assert_eq!(row(&excluded, &key)?.desired, "excluded");
    let disabled = excluded
        .requested
        .intent
        .requirements
        .first()
        .ok_or("missing retained local object")?;
    assert_eq!(disabled.object_id.as_ref(), Some(&entry.object_id));
    assert_eq!(disabled.filename.as_deref(), Some("local.jar.disabled"));
    let inherited = set_choice(&fixture.paths, ID, &key, "inherit", &excluded.revision).await?;
    assert_eq!(row(&inherited, &key)?.desired, "notRequested");
    assert_eq!(
        policy(&inherited.requested.intent)?
            .choice(&ModKey::local(&entry.object_id)?)
            .desired,
        Desired::NotRequested
    );
    let restarted = Arc::new(LauncherPaths::new(fixture.paths.root()));
    let included = set_choice(&restarted, ID, &key, "include", &inherited.revision).await?;
    assert_eq!(row(&included, &key)?.desired, "required");
    assert_eq!(std::fs::read(game.join("mods/local.jar"))?, original);
    assert_eq!(
        std::fs::read(
            fixture
                .paths
                .root()
                .join("meta/mod-objects")
                .join(&entry.object_id)
        )?,
        original
    );
    assert_eq!(
        ModRepository::new(&fixture.paths.root())
            .head(ID)?
            .ok_or("missing active head")?
            .id,
        head.id
    );
    fixture.assert_assets()?;
    Ok(())
}

#[tokio::test]
async fn snapshot_is_read_only_and_does_not_wait_for_the_edit_lease() -> TestResult {
    let fixture = Fixture::new()?;
    let empty = snapshot(&fixture.paths, ID)?;
    assert!(empty.target.is_none() && empty.rows.is_empty());
    assert_eq!(std::fs::read_dir(fixture.directory.path())?.count(), 0);
    let _guard = edit_guard(&fixture.paths, ID).await?;
    let paths = fixture.paths.clone();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::task::spawn_blocking(move || snapshot(&paths, ID)),
    )
    .await???;
    assert!(result.rows.is_empty());
    Ok(())
}

#[tokio::test]
async fn corrupt_intent_policy_and_protection_are_not_missing_state() -> TestResult {
    let fixture = Fixture::new()?;
    let first = snapshot(&fixture.paths, ID)?;
    std::fs::write(fixture.paths.auto_mods_file(), b"{")?;
    assert!(snapshot(&fixture.paths, ID).is_err());
    assert!(
        set_choice(
            &fixture.paths,
            ID,
            "modrinth:example",
            "include",
            &first.revision
        )
        .await
        .is_err()
    );
    fixture.recommendations(false)?;
    mod_intent::save(&fixture.paths.root(), &Intent::empty(ID))?;
    let intent_path = fixture
        .paths
        .root()
        .join(format!("state/mod-intents/{}.json", key(ID)));
    std::fs::write(&intent_path, b"{")?;
    assert!(snapshot(&fixture.paths, ID).is_err());
    mod_intent::save(&fixture.paths.root(), &Intent::empty(ID))?;
    let before = snapshot(&fixture.paths, ID)?;
    let state = fixture.paths.root().join("state/mod-maintenance");
    std::fs::create_dir_all(&state)?;
    std::fs::write(state.join(format!("{}.json", key(ID))), b"{")?;
    assert!(snapshot(&fixture.paths, ID).is_err());
    assert!(
        set_choice(
            &fixture.paths,
            ID,
            "modrinth:example",
            "include",
            &before.revision
        )
        .await
        .is_err()
    );
    assert_eq!(
        mod_intent::load(&fixture.paths.root(), ID)?,
        Some(Intent::empty(ID))
    );
    Ok(())
}

#[test]
fn legacy_serde_and_fingerprint_are_backward_readable_and_sequence_sensitive() -> TestResult {
    let old = serde_json::json!({ "schema":1, "profileId":ID, "requirements":[], "excludedProjects":["example"] });
    let mut intent: Intent = serde_json::from_value(old.clone())?;
    intent.validate(ID)?;
    assert_eq!(intent.choice_sequence, 0);
    assert!(intent.choice_provenance.is_empty());
    assert_eq!(serde_json::to_value(&intent)?, old);
    let old_id = intent.fingerprint()?;
    intent.advance_choice_sequence()?;
    assert_ne!(intent.fingerprint()?, old_id);
    intent.choice_sequence = u64::MAX;
    assert!(intent.advance_choice_sequence().is_err());
    Ok(())
}

#[test]
fn decision_required_failure_keeps_its_actionable_category() {
    assert_eq!(
        requests::failure_reason("mod_decision_required: review the proposed omission"),
        "decision_required"
    );
}

#[test]
fn target_projection_changes_only_inherited_roots_and_never_commits_base() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.recommendations(true)?;
    let mut base = Intent::empty(ID);
    base.upsert(Requirement::project("explicit", true, None, None, "user"))?;
    base.excluded_projects.push("excluded".into());
    base.choice_sequence = 17;
    mod_intent::save(&fixture.paths.root(), &base)?;
    let other_loader = ModTarget {
        loader: "neoforge".into(),
        ..target()
    };
    let first = for_target(&fixture.paths, &base, &other_loader)?;
    let second = for_target(&fixture.paths, &base, &target())?;
    assert!(
        !first
            .requirements
            .iter()
            .any(|r| r.key() == "modrinth:example")
    );
    assert!(
        second
            .requirements
            .iter()
            .any(|r| r.key() == "modrinth:example" && r.enabled)
    );
    for projected in [&first, &second] {
        assert!(
            projected
                .requirements
                .iter()
                .any(|r| r.key() == "modrinth:explicit" && r.enabled)
        );
        assert_eq!(projected.excluded_projects, base.excluded_projects);
        assert_eq!(projected.choice_sequence, 17);
    }
    assert_eq!(mod_intent::load(&fixture.paths.root(), ID)?, Some(base));
    Ok(())
}

#[test]
fn a_durable_choice_survives_termination_before_restore_pin_bookkeeping() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.recommendations(true)?;
    let head = fixture.head(false)?;
    let mut old = Intent::empty(ID);
    old.excluded_projects.push("example".into());
    mod_intent::save(&fixture.paths.root(), &old)?;
    write_state(
        &fixture.paths,
        ID,
        &MaintenanceState {
            revision_id: head.id.clone(),
            pinned: true,
            ..MaintenanceState::default()
        },
    )?;
    let mut next = old.clone();
    choose(&mut next, Some(&head), "modrinth:example", "include")?;
    next.advance_choice_sequence()?;
    mod_intent::save(&fixture.paths.root(), &next)?;
    // Termination after the atomic Intent commit, before clearing the old pin.
    let visible = snapshot(&fixture.paths, ID)?;
    assert_eq!(row(&visible, "modrinth:example")?.desired, "required");
    let mut state = read_state(&fixture.paths, ID)?;
    recover_choice_state(&fixture.paths, ID, &mut state)?;
    assert!(!state.pinned && state.choice_sequence == next.choice_sequence);
    assert_eq!(mod_intent::load(&fixture.paths.root(), ID)?, Some(next));
    fixture.assert_assets()?;
    Ok(())
}

#[tokio::test]
async fn verified_alias_cannot_resurrect_an_excluded_recommendation_or_split_its_action()
-> TestResult {
    let fixture = Fixture::new()?;
    fixture.head(false)?;
    let entry: AutoMod = serde_json::from_value(serde_json::json!({
        "project_id":"alias","name":"Renderer","description":"fixture","enabled":true,"loaders":["fabric"]
    }))?;
    mod_recommendations::save_auto_mods_to_file(&fixture.paths.auto_mods_file(), &[entry])?;
    let mut old = Intent::empty(ID);
    old.excluded_projects.push("canonical".into());
    mod_intent::save(&fixture.paths.root(), &old)?;
    let known = std::collections::BTreeMap::from([
        ("alias".into(), "canonical".into()),
        ("canonical".into(), "canonical".into()),
    ]);
    identities::save(&fixture.paths, ID, &known)?;
    let before = snapshot(&fixture.paths, ID)?;
    assert_eq!(before.rows.len(), 1);
    assert_eq!(row(&before, "modrinth:canonical")?.desired, "excluded");
    let inherited = set_choice(
        &fixture.paths,
        ID,
        "modrinth:canonical",
        "inherit",
        &before.revision,
    )
    .await?;
    assert_eq!(row(&inherited, "modrinth:canonical")?.desired, "required");
    assert_eq!(inherited.rows.len(), 1);
    assert_eq!(
        mod_intent::load(&fixture.paths.root(), ID)?
            .ok_or("missing intent")?
            .requirements[0]
            .project_id
            .as_deref(),
        Some("canonical")
    );
    let conflicting = std::collections::BTreeMap::from([("alias".into(), "another".into())]);
    assert!(identities::save(&fixture.paths, ID, &conflicting).is_err());
    assert_eq!(identities::load(&fixture.paths, ID)?, known);
    fixture.assert_assets()?;
    Ok(())
}

#[tokio::test]
async fn preparation_does_not_block_choices_and_its_stale_result_cannot_commit() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.recommendations(true)?;
    let head = fixture.head(false)?;
    let mut desired = Intent::empty(ID);
    desired.excluded_projects.push("example".into());
    mod_intent::save(&fixture.paths.root(), &desired)?;
    let files = workspace::fingerprint(&fixture.paths.checked_profile_game_dir(ID)?)?;
    let policy = recipe(&fixture.paths)?;
    let before = snapshot(&fixture.paths, ID)?;
    let _preparer = operation_guard(&fixture.paths, ID, true).await?;
    let changed = tokio::time::timeout(
        Duration::from_secs(2),
        set_choice(
            &fixture.paths,
            ID,
            "modrinth:example",
            "include",
            &before.revision,
        ),
    )
    .await??;
    assert_eq!(row(&changed, "modrinth:example")?.desired, "required");
    let _commit = edit_guard(&fixture.paths, ID).await?;
    assert!(
        validate_prepared_inputs(
            &fixture.paths,
            ID,
            Some(&head.id),
            &desired.fingerprint()?,
            &files,
            &policy
        )
        .is_err()
    );
    assert_eq!(
        ModRepository::new(&fixture.paths.root())
            .head(ID)?
            .ok_or("missing head")?
            .id,
        head.id
    );
    fixture.assert_assets()?;
    Ok(())
}
