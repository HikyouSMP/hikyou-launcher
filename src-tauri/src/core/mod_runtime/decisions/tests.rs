use super::*;
use crate::core::{mod_repository::CapturePolicy, test_support::zip};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Fixture {
    _temp: tempfile::TempDir,
    paths: LauncherPaths,
    base: Intent,
    head: ModRevision,
    candidate: ModRevision,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let paths = LauncherPaths::new(temp.path().canonicalize()?);
        paths.ensure_dirs()?;
        std::fs::write(paths.auto_mods_file(), b"[]")?;
        let id = "smart:latest-plus";
        let game = paths.checked_profile_game_dir(id)?;
        std::fs::create_dir_all(game.join("mods"))?;
        std::fs::create_dir_all(game.join("saves"))?;
        std::fs::write(game.join("saves/sentinel"), b"world-owned-by-user")?;
        std::fs::write(game.join("options.txt"), b"settings-owned-by-user")?;
        let bytes = zip(
            &[("fabric.mod.json", br#"{"id":"renderer","version":"1"}"#)],
            zip::CompressionMethod::Stored,
        );
        std::fs::write(game.join("mods/renderer.jar"), bytes)?;
        std::fs::write(
            game.join("mods/.mod_meta.json"),
            br#"{"renderer.jar":{"project_id":"render","display_name":"Renderer"}}"#,
        )?;
        let base: Intent = serde_json::from_value(serde_json::json!({
            "schema":1,"profileId":id,"recommendationsInitialized":true,
            "requirements":[{"projectId":"render","objectId":null,"filename":null,
                "enabled":true,"displayName":"Renderer","iconUrl":null,"origin":"user"}],"excludedProjects":[]
        }))?;
        mod_intent::save(&paths.root(), &base)?;
        let repo = ModRepository::new(&paths.root());
        let target = ModTarget {
            mc_version: "1.21.1".into(),
            loader: "fabric".into(),
            loader_version: Some("0.16.10".into()),
        };
        let head =
            repo.capture_with_intent(id, &game, target.clone(), None, Some(&base.fingerprint()?))?;
        requests::record_resolution(&paths, &head, base.clone(), Vec::new())?;
        repo.materialize(&head)?;
        repo.publish(id, &head.id)?;
        let stage = workspace::stage(&paths, &game)?;
        std::fs::rename(
            stage.path().join("mods/renderer.jar"),
            stage.path().join("mods/renderer.jar.disabled"),
        )?;
        let effective = preparation::without(&base, &["modrinth:render".into()])?;
        let candidate = repo.capture_config(
            id,
            stage.path(),
            target,
            Some(&head.id),
            CapturePolicy {
                intent_id: Some(&effective.fingerprint()?),
                requires_decision: true,
            },
        )?;
        requests::record_resolution(&paths, &candidate, effective, Vec::new())?;
        repo.materialize(&candidate)?;
        Ok(Self {
            _temp: temp,
            paths,
            base,
            head,
            candidate,
        })
    }

    fn offer(&self) -> Result<DecisionView, String> {
        assert!(!offer(
            &self.paths,
            &self.base,
            Some(&self.head),
            &self.candidate,
            vec!["modrinth:render".into()]
        )?);
        views(&self.paths, &self.base.profile_id)?
            .into_iter()
            .next()
            .ok_or("proposal not visible".into())
    }

    fn assets_unchanged(&self) -> TestResult {
        let game = self.paths.checked_profile_game_dir(&self.base.profile_id)?;
        assert_eq!(
            std::fs::read(game.join("saves/sentinel"))?,
            b"world-owned-by-user"
        );
        assert_eq!(
            std::fs::read(game.join("options.txt"))?,
            b"settings-owned-by-user"
        );
        assert_eq!(
            mod_intent::load(&self.paths.root(), &self.base.profile_id)?,
            Some(self.base.clone())
        );
        Ok(())
    }
}

#[tokio::test]
async fn wait_accept_restart_revoke_and_late_capability_obey_authoritative_answer() -> TestResult {
    let fixture = Fixture::new()?;
    let first = fixture.offer()?;
    assert!(check_activation(&fixture.paths, &fixture.candidate).is_err());
    answer(
        &fixture.paths,
        &first.profile_id,
        &first.id,
        &first.expected_revision,
        "defer",
    )
    .await?;
    let deferred = views(&fixture.paths, &first.profile_id)?.remove(0);
    assert_eq!(deferred.phase, Phase::Deferred);
    let retried = fixture.offer()?;
    assert_eq!(retried.id, deferred.id);
    assert_eq!(retried.phase, Phase::Deferred);
    assert_eq!(
        ModRepository::new(&fixture.paths.root())
            .head(&first.profile_id)?
            .ok_or("missing head")?
            .id,
        fixture.head.id
    );
    assert!(
        answer(
            &fixture.paths,
            &first.profile_id,
            &first.id,
            &first.expected_revision,
            "accept"
        )
        .await
        .is_err()
    );
    answer(
        &fixture.paths,
        &first.profile_id,
        &deferred.id,
        &deferred.expected_revision,
        "accept",
    )
    .await?;
    let selected =
        ready(&fixture.paths, &first.profile_id)?.ok_or("accepted configuration not ready")?;
    assert_eq!(selected.revision_id(), fixture.candidate.id);
    let restarted_paths = LauncherPaths::new(fixture.paths.root());
    selected.recheck(&restarted_paths)?;
    let accepted = views(&restarted_paths, &first.profile_id)?.remove(0);
    assert_eq!(accepted.phase, Phase::Accepted);
    answer(
        &restarted_paths,
        &first.profile_id,
        &accepted.id,
        &accepted.expected_revision,
        "revoke",
    )
    .await?;
    assert!(ready(&restarted_paths, &first.profile_id).is_err());
    assert!(selected.recheck(&restarted_paths).is_err());
    assert!(
        answer(
            &restarted_paths,
            &first.profile_id,
            &deferred.id,
            &deferred.expected_revision,
            "accept"
        )
        .await
        .is_err()
    );
    fixture.assets_unchanged()?;
    Ok(())
}

#[tokio::test]
async fn changed_wishes_or_files_cannot_accept_a_stale_proposal() -> TestResult {
    let fixture = Fixture::new()?;
    let first = fixture.offer()?;
    let mut changed = fixture.base.clone();
    changed.upsert(mod_intent::Requirement::project(
        "other", true, None, None, "user",
    ))?;
    mod_intent::save(&fixture.paths.root(), &changed)?;
    assert!(
        answer(
            &fixture.paths,
            &first.profile_id,
            &first.id,
            &first.expected_revision,
            "accept"
        )
        .await
        .is_err()
    );
    mod_intent::save(&fixture.paths.root(), &fixture.base)?;
    let game = fixture.paths.checked_profile_game_dir(&first.profile_id)?;
    std::fs::write(game.join("mods/new.jar"), b"uninspected external change")?;
    assert!(
        answer(
            &fixture.paths,
            &first.profile_id,
            &first.id,
            &first.expected_revision,
            "accept"
        )
        .await
        .is_err()
    );
    assert_eq!(
        ModRepository::new(&fixture.paths.root())
            .head(&first.profile_id)?
            .ok_or("missing head")?
            .id,
        fixture.head.id
    );
    fixture.assets_unchanged()?;
    Ok(())
}

#[tokio::test]
async fn durable_acceptance_recovers_before_and_after_publication_without_rewriting_wishes()
-> TestResult {
    let fixture = Fixture::new()?;
    let first = fixture.offer()?;
    let mut record = load(&fixture.paths, &first.profile_id, &first.id)?;
    let original = model(&record)?;
    let accepted = original.answer(
        &original.revision()?,
        &selection::policy(&fixture.base)?,
        Observation {
            facts_revision: &record.facts,
            active_activation: Some(&fixture.head.id),
        },
        Action::Accept,
    )?;
    record.model = checkpoint(&accepted)?;
    let dir = directory(&fixture.paths, &first.profile_id, false)?.ok_or("missing directory")?;
    write(&dir, &format!("{}.json", first.id), &record, true)?;
    drop(dir); // Termination after durable answer, before the activation commit.
    let guard = Arc::new(edit_guard(&fixture.paths, &first.profile_id).await?);
    assert!(recover(&fixture.paths, &first.profile_id, guard.clone()).await?);
    assert!(!recover(&fixture.paths, &first.profile_id, guard).await?);
    assert_eq!(
        ready(&fixture.paths, &first.profile_id)?
            .ok_or("not ready")?
            .revision_id(),
        fixture.candidate.id
    );
    fixture.assets_unchanged()?;
    Ok(())
}

#[test]
fn missing_or_corrupt_permission_evidence_is_not_an_ordinary_unrestricted_revision() -> TestResult {
    let fixture = Fixture::new()?;
    assert!(check_activation(&fixture.paths, &fixture.candidate).is_err());
    let first = fixture.offer()?;
    let dir = directory(&fixture.paths, &first.profile_id, false)?.ok_or("missing directory")?;
    write(
        &dir,
        &format!("{}.json", first.id),
        &serde_json::json!({"schema":1}),
        true,
    )?;
    assert!(check_activation(&fixture.paths, &fixture.candidate).is_err());
    assert!(views(&fixture.paths, &first.profile_id).is_err());
    fixture.assets_unchanged()?;
    Ok(())
}

#[tokio::test]
async fn another_proposal_cannot_reactivate_withdrawn_authority_without_a_fresh_answer()
-> TestResult {
    let fixture = Fixture::new()?;
    let first = fixture.offer()?;
    answer(
        &fixture.paths,
        &first.profile_id,
        &first.id,
        &first.expected_revision,
        "accept",
    )
    .await?;
    let accepted = views(&fixture.paths, &first.profile_id)?.remove(0);
    answer(
        &fixture.paths,
        &first.profile_id,
        &accepted.id,
        &accepted.expected_revision,
        "revoke",
    )
    .await?;
    assert!(check_activation(&fixture.paths, &fixture.candidate).is_err());
    assert!(!offer(
        &fixture.paths,
        &fixture.base,
        Some(&fixture.candidate),
        &fixture.candidate,
        vec!["modrinth:render".into()]
    )?);
    let next = views(&fixture.paths, &first.profile_id)?
        .into_iter()
        .find(|v| v.phase == Phase::Pending)
        .ok_or("new pending decision missing")?;
    assert_ne!(next.id, first.id);
    assert!(check_activation(&fixture.paths, &fixture.candidate).is_err());
    assert!(
        answer(
            &fixture.paths,
            "smart:snapshot-plus",
            &next.id,
            &next.expected_revision,
            "accept"
        )
        .await
        .is_err()
    );
    answer(
        &fixture.paths,
        &first.profile_id,
        &next.id,
        &next.expected_revision,
        "accept",
    )
    .await?;
    check_activation(&fixture.paths, &fixture.candidate)?;
    let dir = directory(&fixture.paths, &first.profile_id, false)?.ok_or("missing directory")?;
    dir.remove_file(format!("revision-{}.json", fixture.candidate.id))?;
    assert!(check_activation(&fixture.paths, &fixture.candidate).is_err());
    fixture.assets_unchanged()?;
    Ok(())
}

#[tokio::test]
async fn explicit_restore_recovers_accepted_base_wishes_but_never_a_withdrawn_answer() -> TestResult
{
    let fixture = Fixture::new()?;
    let first = fixture.offer()?;
    answer(
        &fixture.paths,
        &first.profile_id,
        &first.id,
        &first.expected_revision,
        "accept",
    )
    .await?;
    let mut changed = fixture.base.clone();
    selection::choose(
        &mut changed,
        Some(&fixture.candidate),
        "modrinth:render",
        "exclude",
    )?;
    changed.advance_choice_sequence()?;
    mod_intent::save(&fixture.paths.root(), &changed)?;
    assert!(check_activation(&fixture.paths, &fixture.candidate).is_err());
    assert_eq!(
        restore_base(&fixture.paths, &fixture.candidate)?,
        Some(fixture.base.clone())
    );
    // A restore has published its accepted generation but not saved its wishes.
    let next_sequence = changed
        .choice_sequence
        .checked_add(1)
        .ok_or("sequence exhausted")?;
    write_state(
        &fixture.paths,
        &first.profile_id,
        &MaintenanceState {
            revision_id: fixture.candidate.id.clone(),
            pending_restore: Some(fixture.candidate.id.clone()),
            choice_sequence: next_sequence,
            ..MaintenanceState::default()
        },
    )?;
    let intents = managed_fs::directory(
        &managed_fs::root(&fixture.paths.root())?,
        Path::new("state/mod-intents"),
        false,
    )?;
    intents.remove_file(format!("{}.json", key(&first.profile_id)))?;
    assert!(selection::snapshot(&fixture.paths, &first.profile_id).is_ok());
    assert_eq!(
        refresh_smart(&fixture.paths, &first.profile_id, true)
            .await?
            .id,
        fixture.candidate.id
    );
    let saved = mod_intent::load(&fixture.paths.root(), &first.profile_id)?
        .ok_or("restored wishes missing")?;
    assert_eq!(saved.choice_sequence, next_sequence);
    assert!(
        saved
            .requirements
            .iter()
            .any(|r| r.enabled && r.project_id.as_deref() == Some("render"))
    );
    check_activation(&fixture.paths, &fixture.candidate)?;
    let accepted = views(&fixture.paths, &first.profile_id)?
        .into_iter()
        .find(|v| v.phase == Phase::Accepted)
        .ok_or("accepted decision missing")?;
    answer(
        &fixture.paths,
        &first.profile_id,
        &accepted.id,
        &accepted.expected_revision,
        "revoke",
    )
    .await?;
    assert!(restore_base(&fixture.paths, &fixture.candidate).is_err());
    let game = fixture.paths.checked_profile_game_dir(&first.profile_id)?;
    assert_eq!(
        std::fs::read(game.join("saves/sentinel"))?,
        b"world-owned-by-user"
    );
    Ok(())
}
