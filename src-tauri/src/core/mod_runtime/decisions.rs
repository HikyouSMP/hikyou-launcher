//! Durable, target-scoped permission. This owner never changes base Mod wishes.

mod store;
use super::*;
use hikyou_mod_policy::{
    ModKey,
    decisions::{Action, Basis, Decision, Observation, Phase},
};
use std::collections::BTreeSet;
use store::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OmittedMod {
    key: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionView {
    id: String,
    profile_id: String,
    target: String,
    phase: Phase,
    omitted: Vec<OmittedMod>,
    baseline_target: Option<String>,
    expected_revision: String,
}

/// Called only under the preparation edit lease, after complete graph validation.
/// Returns true only for an already accepted, exactly matching consequence.
pub(super) fn offer(
    paths: &LauncherPaths,
    base: &Intent,
    baseline: Option<&ModRevision>,
    candidate: &ModRevision,
    omitted: Vec<String>,
) -> Result<bool, String> {
    if !candidate.requires_decision {
        return Err("A Mod alternative must declare its decision requirement".into());
    }
    let source_intent_id = base.fingerprint()?;
    let base = selection::for_target(paths, base, &candidate.target)?;
    base.validate(&candidate.profile_id)?;
    let impact = plan_digest(candidate, &omitted)?;
    let policy = selection::policy(&base)?;
    let keys = mod_keys(&omitted)?;
    if let Some(previous) = current(paths, &base.profile_id)? {
        let decision = model(&previous)?;
        if decision.covers_omissions(&policy, &candidate.target.mc_version, &impact, &keys) {
            bind_revision(paths, &previous, &decision, candidate)?;
            return Ok(true);
        }
        if previous.source_intent_id == source_intent_id
            && previous.base_intent.fingerprint()? == base.fingerprint()?
            && previous.baseline_id.as_deref() == baseline.map(|v| v.id.as_str())
            && previous.impact == impact
            && previous.recipe == recipe(paths)?
            && previous.workspace
                == workspace::fingerprint(&paths.checked_profile_game_dir(&base.profile_id)?)?
            && matches!(decision.phase(), Phase::Pending | Phase::Deferred)
        {
            return Ok(false);
        }
    }
    let recipe = recipe(paths)?;
    let workspace = workspace::fingerprint(&paths.checked_profile_game_dir(&base.profile_id)?)?;
    let facts = hash(&(
        &source_intent_id,
        base.fingerprint()?,
        &recipe,
        &workspace,
        &impact,
    ))?;
    let nonce = hash(&uuid::Uuid::new_v4())?;
    let baseline_id = baseline.map(|revision| revision.id.clone());
    let decision = Decision::new(
        &policy,
        Basis {
            target: &candidate.target.mc_version,
            facts_revision: &facts,
            impact_digest: &impact,
            baseline_activation: baseline_id.as_deref(),
            nonce: &nonce,
        },
        &keys,
    )
    .map_err(|e| e.to_string())?;
    let record = Proposal {
        schema: 1,
        profile_id: base.profile_id.clone(),
        base_intent: base.clone(),
        source_intent_id,
        baseline_id,
        candidate_id: candidate.id.clone(),
        recipe,
        workspace,
        facts,
        impact,
        nonce,
        omitted,
        model: checkpoint(&decision)?,
    };
    let dir =
        directory(paths, &base.profile_id, true)?.ok_or("Mod decision directory unavailable")?;
    write(&dir, &format!("{}.json", decision.id()), &record, false)?;
    write(
        &dir,
        "current.json",
        &Locator {
            proposal_id: decision.id().into(),
        },
        true,
    )?;
    Ok(false)
}

/// Small authoritative reads on launch; no provider work or whole-artifact hashes.
pub(super) fn check_activation(
    paths: &LauncherPaths,
    revision: &ModRevision,
) -> Result<(), String> {
    let Some(receipt) = receipt(paths, revision)? else {
        return if revision.requires_decision {
            Err("Mod decision evidence is missing; this configuration cannot be launched".into())
        } else {
            Ok(())
        };
    };
    if !revision.requires_decision {
        return Err("Unexpected decision receipt on an ordinary Mod revision".into());
    }
    let record = load(paths, &revision.profile_id, &receipt.proposal_id)?;
    let stored =
        mod_intent::load(&paths.root(), &revision.profile_id)?.ok_or("Mod wishes are missing")?;
    let state = read_state(paths, &revision.profile_id)?;
    let intent = if pinned(&state, revision) && stored.choice_sequence <= state.choice_sequence {
        stored
    } else {
        selection::for_target(paths, &stored, &revision.target)?
    };
    validate_permission(&record, &receipt, revision, &intent)
}

fn validate_permission(
    record: &Proposal,
    receipt: &Receipt,
    revision: &ModRevision,
    intent: &Intent,
) -> Result<(), String> {
    if !revision.requires_decision
        || receipt.impact != plan_digest(revision, &receipt.omitted)?
        || record.impact != receipt.impact
        || !model(record)?.covers_omissions(
            &selection::policy(intent)?,
            &revision.target.mc_version,
            &receipt.impact,
            &mod_keys(&receipt.omitted)?,
        )
    {
        return Err("mod_decision_required: This configuration depends on a withdrawn or changed Mod decision. Review the Mod choices or restore a complete configuration.".into());
    }
    Ok(())
}

/// Explicit restore/journal recovery only. Normal launch checks current wishes.
pub(super) fn restore_base(
    paths: &LauncherPaths,
    revision: &ModRevision,
) -> Result<Option<Intent>, String> {
    let Some(receipt) = receipt(paths, revision)? else {
        check_activation(paths, revision)?;
        return Ok(None);
    };
    let record = load(paths, &revision.profile_id, &receipt.proposal_id)?;
    validate_permission(&record, &receipt, revision, &record.base_intent)?;
    Ok(Some(record.base_intent))
}

pub(super) fn prepared_for_intent(
    paths: &LauncherPaths,
    revision: &ModRevision,
    intent: &Intent,
) -> Result<bool, String> {
    let Some(receipt) = receipt(paths, revision)? else {
        return Ok(false);
    };
    check_activation(paths, revision)?;
    let record = load(paths, &revision.profile_id, &receipt.proposal_id)?;
    Ok(
        selection::for_target(paths, intent, &revision.target)?.fingerprint()?
            == record.base_intent.fingerprint()?,
    )
}

fn to_view(paths: &LauncherPaths, record: &Proposal) -> Result<DecisionView, String> {
    let decision = model(record)?;
    let repo = ModRepository::new(&paths.root());
    let candidate = repo.load(&record.candidate_id)?;
    if candidate.profile_id != record.profile_id
        || plan_digest(&candidate, &record.omitted)? != record.impact
    {
        return Err("Mod decision candidate ownership mismatch".into());
    }
    let omitted = record
        .omitted
        .iter()
        .map(|key| {
            let request = record
                .base_intent
                .requirements
                .iter()
                .find(|request| request.key() == *key)
                .ok_or("Mod decision refers to an unknown wish")?;
            Ok(OmittedMod {
                key: key.clone(),
                name: request
                    .display_name
                    .clone()
                    .or_else(|| request.filename.clone())
                    .unwrap_or_else(|| key.clone()),
            })
        })
        .collect::<Result<_, String>>()?;
    let baseline_target = record
        .baseline_id
        .as_ref()
        .map(|id| repo.load(id).map(|v| v.target.mc_version))
        .transpose()?;
    Ok(DecisionView {
        id: decision.id().into(),
        profile_id: record.profile_id.clone(),
        target: candidate.target.mc_version,
        phase: decision.phase(),
        omitted,
        baseline_target,
        expected_revision: hash(&decision.revision().map_err(|e| e.to_string())?)?,
    })
}

pub fn views(paths: &LauncherPaths, id: &str) -> Result<Vec<DecisionView>, String> {
    profile::validate_profile_ref(id)?;
    let head = ModRepository::new(&paths.root()).head(id)?;
    let source_id = mod_intent::load(&paths.root(), id)?
        .as_ref()
        .map(Intent::fingerprint)
        .transpose()?;
    let mut records = Vec::new();
    let mut ids = BTreeSet::new();
    if let Some(record) = current(paths, id)? {
        let decision = model(&record)?;
        let awaiting_commit = head.as_ref().map(|v| &v.id) == record.baseline_id.as_ref()
            && source_id.as_ref() == Some(&record.source_intent_id)
            && recipe(paths)? == record.recipe
            && workspace::fingerprint(&paths.checked_profile_game_dir(id)?)? == record.workspace;
        if awaiting_commit
            && matches!(
                decision.phase(),
                Phase::Pending | Phase::Deferred | Phase::Accepted
            )
        {
            ids.insert(decision.id().to_owned());
            records.push(record);
        }
    }
    if let Some(head) = head
        && let Some(receipt) = receipt(paths, &head)?
        && ids.insert(receipt.proposal_id.clone())
    {
        records.push(load(paths, id, &receipt.proposal_id)?);
    }
    records
        .iter()
        .map(|record| to_view(paths, record))
        .collect()
}

pub async fn answer(
    paths: &LauncherPaths,
    id: &str,
    proposal_id: &str,
    expected: &str,
    action: &str,
) -> Result<(), String> {
    let guard = Arc::new(edit_guard(paths, id).await?);
    let mut record = load(paths, id, proposal_id)?;
    let decision = model(&record)?;
    if hash(&decision.revision().map_err(|e| e.to_string())?)? != expected {
        return Err("Mod decision changed; refresh before answering".into());
    }
    let action = match action {
        "accept" => Action::Accept,
        "defer" => Action::Defer,
        "revoke" => Action::Revoke,
        _ => return Err("Unknown Mod decision action".into()),
    };
    let current_intent = mod_intent::load(&paths.root(), id)?.ok_or("Mod wishes are missing")?;
    let head = ModRepository::new(&paths.root()).head(id)?;
    let candidate = ModRepository::new(&paths.root()).load(&record.candidate_id)?;
    let effective = selection::for_target(paths, &current_intent, &candidate.target)?;
    let policy = selection::policy(&effective)?;
    if action != Action::Revoke {
        let selected = current(paths, id)?.ok_or("Mod decision is no longer current")?;
        if model(&selected)?.id() != proposal_id
            || current_intent.fingerprint()? != record.source_intent_id
            || effective.fingerprint()? != record.base_intent.fingerprint()?
            || head.as_ref().map(|head| &head.id) != record.baseline_id.as_ref()
            || recipe(paths)? != record.recipe
            || workspace::fingerprint(&paths.checked_profile_game_dir(id)?)? != record.workspace
        {
            return Err("Mod decision inputs changed; refresh before answering".into());
        }
    }
    let next = decision
        .answer(
            &decision.revision().map_err(|e| e.to_string())?,
            &policy,
            Observation {
                facts_revision: &record.facts,
                active_activation: head.as_ref().map(|v| v.id.as_str()),
            },
            action,
        )
        .map_err(|e| e.to_string())?;
    record.model = checkpoint(&next)?;
    let dir = directory(paths, id, false)?.ok_or("Mod decision directory missing")?;
    // Durable permission/revocation precedes activation. A crash cannot publish a
    // candidate whose acceptance existed only in frontend memory.
    write(&dir, &format!("{proposal_id}.json"), &record, true)?;
    if action == Action::Accept {
        activate(paths, &record, guard).await?;
    }
    wake_preparation();
    Ok(())
}

async fn activate(
    paths: &LauncherPaths,
    record: &Proposal,
    guard: Arc<EditGuard>,
) -> Result<(), String> {
    if surviving_game(paths, &record.profile_id, true).await? {
        return Err(
            "Stop the game using the editable Mod folder before applying this choice".into(),
        );
    }
    let paths = paths.clone();
    let id = record.profile_id.clone();
    let candidate = record.candidate_id.clone();
    let baseline = record.baseline_id.clone();
    let source_intent_id = record.source_intent_id.clone();
    let expected_recipe = record.recipe.clone();
    let expected_workspace = record.workspace.clone();
    let proposal_id = model(record)?.id().to_owned();
    tokio::task::spawn_blocking(move || {
        let _guard = guard;
        let repo = ModRepository::new(&paths.root());
        let revision = repo.load(&candidate)?;
        let authoritative = load(&paths, &id, &proposal_id)?;
        let decision = model(&authoritative)?;
        if decision.phase() != Phase::Accepted { return Err("The Mod answer is no longer accepted".into()); }
        bind_revision(&paths, &authoritative, &decision, &revision)?;
        check_activation(&paths, &revision)?;
        let head = repo.head(&id)?;
        if head.as_ref().map(|v| &v.id) == Some(&candidate) {
            return Ok(());
        }
        if head.as_ref().map(|v| &v.id) != baseline.as_ref() {
            return Err("Active Mod configuration changed before applying the answer".into());
        }
        if mod_intent::load(&paths.root(), &id)?.ok_or("Mod wishes are missing")?.fingerprint()? != source_intent_id
            || recipe(&paths)? != expected_recipe
            || workspace::fingerprint(&paths.checked_profile_game_dir(&id)?)? != expected_workspace {
            return Err("Mod inputs changed before applying the answer; the previous configuration is retained".into());
        }
        repo.verify(&revision)?;
        let generation = repo.quick_check(&revision)?;
        let game = paths.checked_profile_game_dir(&id)?;
        workspace::checkout(&repo, &revision, &generation, &game)?;
        write_state(
            &paths,
            &id,
            &MaintenanceState {
                revision_id: revision.id,
                workspace_signature: workspace::fingerprint(&game)?,
                recipe: recipe(&paths)?,
                checked_at: chrono::Utc::now().timestamp(),
                pinned: false,
                pending_restore: None,
                pending_edit: false,
                choice_sequence: mod_intent::load(&paths.root(), &id)?
                    .ok_or("Mod wishes are missing")?
                    .choice_sequence,
            },
        )?;
        let intent = mod_intent::load(&paths.root(), &id)?.ok_or("Mod wishes are missing")?;
        mod_intent::set_preparation(&paths.root(), &id, &intent.fingerprint()?, "ready", None)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Resume a durable acceptance interrupted before checkout. Recheck every
/// original precondition; a stale acceptance is retained, never auto-reapplied.
pub(super) async fn recover(
    paths: &LauncherPaths,
    id: &str,
    guard: Arc<EditGuard>,
) -> Result<bool, String> {
    let Some(record) = current(paths, id)? else {
        return Ok(false);
    };
    if model(&record)?.phase() != Phase::Accepted {
        return Ok(false);
    }
    let head = ModRepository::new(&paths.root()).head(id)?;
    if head.as_ref().is_some_and(|v| v.id == record.candidate_id) {
        return Ok(false);
    }
    let source_id = mod_intent::load(&paths.root(), id)?
        .as_ref()
        .map(Intent::fingerprint)
        .transpose()?;
    if head.as_ref().map(|v| &v.id) != record.baseline_id.as_ref()
        || source_id.as_ref() != Some(&record.source_intent_id)
        || recipe(paths)? != record.recipe
        || workspace::fingerprint(&paths.checked_profile_game_dir(id)?)? != record.workspace
    {
        return Ok(false);
    }
    activate(paths, &record, guard).await?;
    Ok(true)
}

#[cfg(test)]
mod tests;
