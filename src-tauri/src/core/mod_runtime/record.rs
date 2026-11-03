use super::*;

pub(super) fn recover(
    paths: &LauncherPaths,
    id: &str,
    head: Option<&ModRevision>,
    game: &Path,
    state: &mut MaintenanceState,
) -> Result<(), String> {
    if let Some(head) = head
        && state.pending_edit
        && state.pending_restore.as_deref() == Some(&head.id)
    {
        let mut committed = mod_intent::for_revision(&paths.root(), head)?
            .ok_or("Committed edit is missing its intent")?;
        let saved = mod_intent::load(&paths.root(), id)?;
        if saved
            .as_ref()
            .is_none_or(|intent| intent.choice_sequence <= state.choice_sequence)
        {
            committed.intent.choice_sequence = state.choice_sequence;
            mod_intent::save(&paths.root(), &committed.intent)?;
        } else if let Some(saved) = saved {
            state.choice_sequence = saved.choice_sequence;
        }
        state.revision_id = head.id.clone();
        state.workspace_signature = workspace::fingerprint(game)?;
        state.recipe = recipe(paths)?;
        state.checked_at = 0;
        state.pinned = false;
        state.pending_restore = None;
        state.pending_edit = false;
        write_state(paths, id, state)?;
    }
    Ok(())
}

/// Explicit edits become ready only after local validation, independently of update discovery.
pub async fn record(paths: &LauncherPaths, id: &str) -> Result<(), String> {
    record_inner(paths, id, false).await
}

#[cfg(test)]
pub async fn record_edited(paths: &LauncherPaths, id: &str) -> Result<(), String> {
    record_inner(paths, id, true).await
}

async fn record_inner(paths: &LauncherPaths, id: &str, explicit_edit: bool) -> Result<(), String> {
    let repo = ModRepository::new(&paths.root());
    let head = repo.head(id)?;
    let mut state = read_state(paths, id)?;
    recover(
        paths,
        id,
        head.as_ref(),
        &paths.checked_profile_game_dir(id)?,
        &mut state,
    )?;
    if head.as_ref().is_some_and(|head| head.intent_id.is_some())
        && mod_intent::load(&paths.root(), id)?.is_none()
    {
        return Err(
            "Mod wishes are missing for a managed configuration; restore a recorded configuration"
                .into(),
        );
    }
    if !explicit_edit
        && let Some(ref head) = head
        && pinned(&state, head)
        && mod_intent::load(&paths.root(), id)?
            .is_none_or(|intent| intent.choice_sequence <= state.choice_sequence)
    {
        return Ok(());
    }
    require_existing_source(paths, id)?;
    let target = if profile::is_smart_profile_ref(id) {
        let Some(ref head) = head else {
            return Ok(());
        };
        head.target.clone()
    } else {
        normal_target(paths, id).await?
    };
    let game_dir = paths.checked_profile_game_dir(id)?;
    managed_fs::directory(
        &managed_fs::root_or_create(&game_dir)?,
        Path::new("mods"),
        true,
    )?;
    let signature = workspace::fingerprint(&game_dir)?;
    if let Some(ref head) = head
        && !explicit_edit
        && head.target == target
        && state.revision_id == head.id
        && state.workspace_signature == signature
        && repo.quick_check(head).is_ok()
    {
        return Ok(());
    }
    validate_inputs(paths, &game_dir, &target, false).await?;
    let root = paths.root();
    let source = game_dir.clone();
    let profile_id = id.to_string();
    let capture_target = target.clone();
    let parent = head.as_ref().map(|head| head.id.clone());
    let baseline = tokio::task::spawn_blocking(move || {
        ModRepository::new(&root).capture(&profile_id, &source, capture_target, parent.as_deref())
    })
    .await
    .map_err(|e| e.to_string())??;
    let intent = if explicit_edit {
        let mut next = Intent::from_revision(&baseline);
        next.choice_sequence = mod_intent::load(&paths.root(), id)?
            .map(|v| v.choice_sequence)
            .unwrap_or(0)
            .max(state.choice_sequence);
        next.advance_choice_sequence()?;
        next
    } else {
        mod_intent::load(&paths.root(), id)?.unwrap_or_else(|| Intent::from_revision(&baseline))
    };
    mod_intent::verify_selected(&intent, &baseline)?;
    let edges = if explicit_edit {
        Vec::new()
    } else {
        head.as_ref()
            .map(|head| mod_intent::for_revision(&paths.root(), head))
            .transpose()?
            .flatten()
            .map(|record| record.edges)
            .unwrap_or_default()
    };
    let revision = seal_requested(
        paths,
        id,
        game_dir,
        target,
        head.map(|r| r.id),
        intent.clone(),
        edges,
    )
    .await?;
    let next_state = MaintenanceState {
        revision_id: revision.id.clone(),
        workspace_signature: signature,
        recipe: recipe(paths)?,
        checked_at: state.checked_at,
        pinned: false,
        pending_restore: None,
        pending_edit: false,
        choice_sequence: intent.choice_sequence,
    };
    let mut journal = state.clone();
    journal.pending_restore = Some(revision.id.clone());
    journal.pending_edit = true;
    journal.choice_sequence = intent.choice_sequence;
    write_state(paths, id, &journal)?;
    if let Err(error) = repo.publish(id, &revision.id) {
        write_state(paths, id, &state)?;
        return Err(error);
    }
    // Publication is the commit point. The revision's immutable intent plus the
    // journal recover any interrupted post-commit bookkeeping before the next edit.
    let finish = || -> Result<(), String> {
        mod_intent::save(&paths.root(), &intent)?;
        mod_intent::set_preparation(&paths.root(), id, &intent.fingerprint()?, "ready", None)?;
        write_state(paths, id, &next_state)
    };
    if let Err(error) = finish() {
        log::warn!("[mods] Committed revision bookkeeping will be recovered: {error}");
    }
    Ok(())
}
