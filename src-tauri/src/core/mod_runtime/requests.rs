//! Desired-state edits and their relationship to immutable resolved generations.

use super::*;
use crate::core::{
    mod_files,
    mod_intent::{self, DependencyEdge, Intent, Requirement, Resolution},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestedState {
    pub intent: Intent,
    pub active_revision_id: Option<String>,
    pub pending: bool,
    pub preparation: Option<mod_intent::Preparation>,
}

pub fn requested_state(paths: &LauncherPaths, id: &str) -> Result<RequestedState, String> {
    profile::validate_profile_ref(id)?;
    let head = ModRepository::new(&paths.root()).head(id)?;
    let mut intent = mod_intent::load(&paths.root(), id)?.unwrap_or_else(|| {
        head.as_ref()
            .map(Intent::from_revision)
            .unwrap_or_else(|| Intent {
                schema: 1,
                profile_id: id.into(),
                recommendations_initialized: false,
                requirements: Vec::new(),
                excluded_projects: Vec::new(),
            })
    });
    let active = head
        .as_ref()
        .map(|head| mod_intent::for_revision(&paths.root(), head))
        .transpose()?
        .flatten();
    let active_intent = active
        .map(|record| record.intent)
        .or_else(|| head.as_ref().map(Intent::from_revision));
    let state = read_state(paths, id)?;
    if head.as_ref().is_some_and(|head| {
        pinned(&state, head) || state.pending_restore.as_deref() == Some(&head.id)
    }) && let Some(active) = &active_intent
    {
        intent = active.clone();
    }
    let requested_id = intent.fingerprint()?;
    let pending = active_intent
        .as_ref()
        .map(Intent::fingerprint)
        .transpose()?
        .as_deref()
        != Some(requested_id.as_str());
    Ok(RequestedState {
        intent,
        active_revision_id: head.map(|revision| revision.id),
        pending,
        preparation: mod_intent::preparation(&paths.root(), id)?,
    })
}

async fn request_change(
    paths: &Arc<LauncherPaths>,
    id: &str,
    change: impl FnOnce(&mut Intent, Option<&ModRevision>) -> Result<(), String>,
) -> Result<(), String> {
    let guard = edit_guard(paths, id).await?;
    let repo = ModRepository::new(&paths.root());
    if !profile::is_smart_profile_ref(id)
        && !profile::list_profiles(&paths.root())
            .await
            .iter()
            .any(|profile| profile.id == id)
    {
        return Err("profile not found".into());
    }
    let head = repo.head(id)?;
    let state = read_state(paths, id)?;
    if let Some(head) = &head
        && { pinned(&state, head) || state.pending_restore.as_deref() == Some(&head.id) }
    {
        let restored = mod_intent::for_revision(&paths.root(), head)?
            .map(|record| record.intent)
            .unwrap_or_else(|| Intent::from_revision(head));
        mod_intent::save(&paths.root(), &restored)?;
    }
    // Accepting a wish must not depend on network access or an initial resolved head.
    let mut intent = match &head {
        Some(head) => requests::current(paths, id, Some(head), &head.target)?,
        None => mod_intent::load(&paths.root(), id)?.unwrap_or_else(|| Intent {
            schema: 1,
            profile_id: id.into(),
            recommendations_initialized: false,
            requirements: Vec::new(),
            excluded_projects: Vec::new(),
        }),
    };
    change(&mut intent, head.as_ref())?;
    mod_intent::save(&paths.root(), &intent)?;
    let mut state = read_state(paths, id)?;
    state.pinned = false;
    state.pending_restore = None;
    state.pending_edit = false;
    write_state(paths, id, &state)?;
    mod_intent::set_preparation(&paths.root(), id, &intent.fingerprint()?, "queued", None)?;
    drop(guard);
    wake_preparation();
    Ok(())
}

pub async fn request_project(
    paths: &Arc<LauncherPaths>,
    id: &str,
    project_id: &str,
    name: Option<String>,
    icon: Option<String>,
) -> Result<(), String> {
    if !mod_intent::valid_project(project_id) {
        return Err("invalid requested Mod project".into());
    }
    let requirement = Requirement::project(project_id, true, name, icon, "user");
    request_change(paths, id, move |intent, _| intent.upsert(requirement)).await
}

pub async fn remove_request(
    paths: &Arc<LauncherPaths>,
    id: &str,
    requested_key: &str,
) -> Result<(), String> {
    request_change(paths, id, |intent, _| {
        let request = intent
            .requirements
            .iter()
            .find(|request| request.key() == requested_key)
            .cloned()
            .ok_or("requested Mod not found")?;
        intent
            .requirements
            .retain(|other| other.key() != requested_key);
        if let Some(project) = request.project_id {
            intent.excluded_projects.push(project);
            intent.excluded_projects.sort();
            intent.excluded_projects.dedup();
        }
        Ok(())
    })
    .await
}

pub async fn change_installed_request(
    paths: &Arc<LauncherPaths>,
    id: &str,
    filename: &str,
    remove: bool,
) -> Result<(), String> {
    request_change(paths, id, |intent, head| {
        let entry = head
            .ok_or("No active Mod configuration exists")?
            .mods
            .iter()
            .find(|entry| entry.filename == filename)
            .ok_or("Mod not found in the active configuration")?;
        let request = intent
            .requirements
            .iter()
            .find(|request| requests::matches(request, entry))
            .cloned()
            .ok_or("This Mod is a required dependency. Change the Mod that requires it instead.")?;
        let key = request.key();
        if remove {
            intent.requirements.retain(|other| other.key() != key);
            if let Some(project) = request.project_id {
                intent.excluded_projects.push(project);
                intent.excluded_projects.sort();
                intent.excluded_projects.dedup();
            }
        } else {
            let mut request = request;
            request.enabled = !request.enabled;
            if let Some(filename) = &mut request.filename {
                let base = filename
                    .strip_suffix(".disabled")
                    .unwrap_or(filename)
                    .to_string();
                *filename = if request.enabled {
                    base
                } else {
                    format!("{base}.disabled")
                };
            }
            request.origin = "user".into();
            intent.upsert(request)?;
        }
        Ok(())
    })
    .await
}

pub async fn recommendation_changes(
    paths: &Arc<LauncherPaths>,
    desired: Option<&[crate::core::mod_recommendations::AutoMod]>,
) -> Result<Vec<String>, String> {
    let mut update =
        crate::core::mod_recommendations::PolicyUpdate::open(&paths.auto_mods_file(), desired)?;
    let mut affected = Vec::new();
    for (profile_id, changes) in update.pending() {
        let mut changes = changes;
        if let Some(cache) = crate::core::cache::get() {
            for change in &mut changes {
                if let Some(canonical) = cache
                    .get::<String>(
                        "mod_project_identity",
                        &format!("modrinth:{}", change.project_id),
                    )
                    .await
                    && mod_intent::valid_project(&canonical)
                {
                    change.project_id = canonical;
                }
            }
        }
        request_change(paths, &profile_id, |intent, _| {
            // The edit lease orders this check with concurrent restores.
            if desired.is_none() {
                let protection = read_state(paths, &profile_id)?;
                if protection.pinned || protection.pending_restore.is_some() {
                    return Err("Pending recommendation changes cannot override a restored profile automatically. Review and save the recommendation policy again to apply them explicitly.".into());
                }
            }
            for change in &changes {
                if intent.requirements.iter().any(|request| {
                    request.project_id.as_ref() == Some(&change.project_id)
                        && request.origin != "recommended"
                }) || intent.excluded_projects.contains(&change.project_id)
                {
                    continue;
                }
                intent.upsert(Requirement::project(
                    &change.project_id,
                    change.enabled,
                    Some(change.name.clone()),
                    change.icon_url.clone(),
                    "recommended",
                ))?;
            }
            Ok(())
        })
        .await?;
        update.complete_profile(&profile_id)?;
        affected.push(profile_id);
    }
    update.finish()?;
    Ok(affected)
}

pub(super) fn matches(
    request: &Requirement,
    entry: &super::super::mod_repository::ModEntry,
) -> bool {
    match &request.project_id {
        Some(project) => entry.project_id.as_ref() == Some(project),
        None => request.object_id.as_ref() == Some(&entry.object_id),
    }
}

pub(super) fn current(
    paths: &LauncherPaths,
    id: &str,
    head: Option<&ModRevision>,
    target: &ModTarget,
) -> Result<Intent, String> {
    let stored = mod_intent::load(&paths.root(), id)?;
    let mut intent = stored.clone().unwrap_or_else(|| {
        head.map(Intent::from_revision).unwrap_or_else(|| Intent {
            schema: 1,
            profile_id: id.into(),
            recommendations_initialized: false,
            requirements: Vec::new(),
            excluded_projects: Vec::new(),
        })
    });
    if profile::is_smart_profile_ref(id) && !intent.recommendations_initialized {
        // Defaults only fill previously unspecified choices; per-profile user
        // choices, including disabled imports, always retain their meaning.
        if crate::core::mod_recommendations::load_auto_mods_checked(&paths.auto_mods_file())?
            .is_none()
        {
            return Err("Recommendation policy is not initialized; Mod preparation will retry after initialization".into());
        }
        for recommendation in mods::load_auto_mods(&paths.auto_mods_file())? {
            if mods::auto_mod_applies_to_loader_and_version(
                &recommendation,
                &target.mc_version,
                &target.loader,
            ) && !intent
                .excluded_projects
                .contains(&recommendation.project_id)
                && !intent
                    .requirements
                    .iter()
                    .any(|r| r.project_id.as_deref() == Some(&recommendation.project_id))
            {
                intent.upsert(Requirement::project(
                    &recommendation.project_id,
                    true,
                    Some(recommendation.name),
                    recommendation.icon_url,
                    "recommended",
                ))?;
            }
        }
        intent.recommendations_initialized = true;
    }
    if stored.as_ref() != Some(&intent) {
        mod_intent::save(&paths.root(), &intent)?;
    }
    Ok(intent)
}

pub(super) fn record_resolution(
    paths: &LauncherPaths,
    revision: &ModRevision,
    intent: Intent,
    mut edges: Vec<DependencyEdge>,
) -> Result<(), String> {
    mod_intent::verify_selected(&intent, revision)?;
    edges.sort_by(|a, b| {
        (&a.requester, &a.dependency, &a.predicate, &a.exact_version).cmp(&(
            &b.requester,
            &b.dependency,
            &b.predicate,
            &b.exact_version,
        ))
    });
    edges.dedup();
    mod_intent::save_resolution(
        &paths.root(),
        &Resolution {
            revision_id: revision.id.clone(),
            intent,
            edges,
        },
    )
}

pub(super) async fn stage_requests(
    paths: &LauncherPaths,
    game: &Path,
    head: Option<&ModRevision>,
    intent: &Intent,
) -> Result<Vec<DependencyEdge>, String> {
    let previous = head
        .map(|head| mod_intent::for_revision(&paths.root(), head))
        .transpose()?
        .flatten();
    if let (Some(previous), Some(head)) = (&previous, head) {
        for old in &previous.intent.requirements {
            if !intent
                .requirements
                .iter()
                .any(|request| request.key() == old.key())
            {
                for entry in head.mods.iter().filter(|entry| matches(old, entry)) {
                    if game
                        .join("mods")
                        .join(&entry.filename)
                        .try_exists()
                        .map_err(|e| e.to_string())?
                    {
                        mods::remove_mod(game, &entry.filename).await?;
                    }
                }
            }
        }
    }
    let installed = mods::list_mods(game).await;
    let metadata = mod_files::load_meta_checked_in(&mod_files::open_mods(game, false)?)?;
    for request in &intent.requirements {
        if let Some(project) = &request.project_id {
            for file in &installed {
                let base = file
                    .filename
                    .strip_suffix(".disabled")
                    .unwrap_or(&file.filename);
                if metadata
                    .get(&file.filename)
                    .or_else(|| metadata.get(base))
                    .is_some_and(|entry| &entry.project_id == project)
                    && file.filename.to_ascii_lowercase().ends_with(".disabled") == request.enabled
                {
                    mods::toggle_mod(game, &file.filename).await?;
                }
            }
        }
    }
    mod_intent::materialize_local(&paths.root(), game, intent)?;
    Ok(previous.map(|record| record.edges).unwrap_or_default())
}

pub(super) fn failure_reason(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.starts_with("mod_resolution_unsatisfiable:") {
        "dependencies_incompatible"
    } else if lower.starts_with("mod_resolution_limit:") {
        "preparation_failed"
    } else if lower.contains("depend")
        || lower.contains("incompatib")
        || lower.contains("conflict")
        || lower.contains("no compatible")
    {
        "dependencies_incompatible"
    } else if lower.contains("http")
        || lower.contains("request failed")
        || lower.contains("network")
        || lower.contains("fetch")
    {
        "provider_unavailable"
    } else if lower.contains("changed") || lower.contains("missing") || lower.contains("damaged") {
        "files_changed"
    } else {
        "preparation_failed"
    }
}

pub(super) async fn normalize_projects(
    paths: &LauncherPaths,
    intent: &mut Intent,
    head: Option<&ModRevision>,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut projects: std::collections::BTreeSet<_> = intent
        .requirements
        .iter()
        .filter_map(|request| request.project_id.clone())
        .chain(intent.excluded_projects.iter().cloned())
        .collect();
    if let Some(head) = head {
        projects.extend(
            head.mods
                .iter()
                .filter_map(|entry| entry.project_id.clone()),
        );
    }
    if projects.len() > 256 {
        return Err("mod_resolution_limit: project identity lookups".into());
    }
    let mut aliases = std::collections::BTreeMap::new();
    for project in projects {
        let cache_key = format!("modrinth:{project}");
        let cached = if let Some(cache) = crate::core::cache::get() {
            cache
                .get::<String>("mod_project_identity", &cache_key)
                .await
        } else {
            None
        };
        let canonical = match cached {
            Some(canonical) if mod_intent::valid_project(&canonical) => canonical,
            _ => {
                let canonical = mods::canonical_modrinth_project_id(&project).await?;
                if let Some(cache) = crate::core::cache::get() {
                    let _ = cache
                        .set("mod_project_identity", &cache_key, &canonical, 24 * 60 * 60)
                        .await;
                }
                canonical
            }
        };
        aliases.insert(project, canonical);
    }
    let normalized = normalized_intent(intent, &aliases)?;
    if &normalized != intent {
        mod_intent::save(&paths.root(), &normalized)?;
        *intent = normalized;
    }
    Ok(aliases)
}

fn normalized_intent(
    intent: &Intent,
    aliases: &std::collections::BTreeMap<String, String>,
) -> Result<Intent, String> {
    let mut normalized = intent.clone();
    normalized.requirements.clear();
    normalized.excluded_projects = intent
        .excluded_projects
        .iter()
        .map(|project| aliases[project].clone())
        .collect();
    normalized.excluded_projects.sort();
    normalized.excluded_projects.dedup();
    let mut selected = std::collections::BTreeMap::<String, Requirement>::new();
    let priority = |request: &Requirement| match request.origin.as_str() {
        "user" => 2,
        "imported" => 1,
        _ => 0,
    };
    for mut request in intent.requirements.clone() {
        if let Some(project) = &mut request.project_id {
            *project = aliases[project].clone();
        }
        if request
            .project_id
            .as_ref()
            .is_some_and(|project| normalized.excluded_projects.contains(project))
        {
            if request.origin == "recommended" {
                continue;
            }
            return Err("conflicting Mod choices refer to the same project".into());
        }
        if let Some(old) = selected.get(&request.key()) {
            if priority(old) > priority(&request) {
                continue;
            }
            if priority(old) == priority(&request) && old.enabled != request.enabled {
                return Err("conflicting Mod choices refer to the same project".into());
            }
        }
        selected.insert(request.key(), request);
    }
    normalized.requirements = selected.into_values().collect();
    normalized.validate(&intent.profile_id)?;
    Ok(normalized)
}

pub(super) fn normalize_stage(
    game: &Path,
    aliases: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    let dir = mod_files::open_mods(game, true)?;
    let mut metadata = mod_files::load_meta_checked_in(&dir)?;
    let mut changed = false;
    for entry in metadata.values_mut() {
        if let Some(canonical) = aliases.get(&entry.project_id)
            && canonical != &entry.project_id
        {
            entry.project_id = canonical.clone();
            changed = true;
        }
    }
    if changed {
        managed_fs::write(
            &dir,
            Path::new(".mod_meta.json"),
            &serde_json::to_vec(&metadata).map_err(|error| error.to_string())?,
            true,
        )?;
    }
    Ok(())
}

pub(super) fn import_external(
    paths: &LauncherPaths,
    intent: &mut Intent,
    before: Option<&ModRevision>,
    incoming: &ModRevision,
    game: &Path,
) -> Result<(), String> {
    let dir = mod_files::open_mods(game, false)?;
    let mut metadata = mod_files::load_meta_checked_in(&dir)?;
    let mut changed = false;
    for entry in &incoming.mods {
        let previous =
            before.and_then(|head| head.mods.iter().find(|old| old.filename == entry.filename));
        if previous.is_some_and(|old| old.object_id == entry.object_id)
            || intent
                .requirements
                .iter()
                .any(|request| request.object_id.as_ref() == Some(&entry.object_id))
        {
            continue;
        }
        if let Some(previous) = previous {
            intent
                .requirements
                .retain(|request| !matches(request, previous));
        }
        // A manually supplied/modified jar is an exact local object, not proof
        // that an upstream project is allowed to replace those bytes.
        intent.upsert(Requirement {
            project_id: None,
            object_id: Some(entry.object_id.clone()),
            filename: Some(entry.filename.clone()),
            enabled: entry.enabled,
            display_name: entry.display_name.clone(),
            icon_url: entry.icon_url.clone(),
            origin: "imported".into(),
        })?;
        metadata.remove(&entry.filename);
        changed = true;
    }
    if changed {
        managed_fs::write(
            &dir,
            Path::new(".mod_meta.json"),
            &serde_json::to_vec(&metadata).map_err(|e| e.to_string())?,
            true,
        )?;
        mod_intent::save(&paths.root(), intent)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_recommendations_cannot_override_a_disabled_or_excluded_choice() {
        let aliases = std::collections::BTreeMap::from([
            ("sodium".into(), "AANobbMI".into()),
            ("AANobbMI".into(), "AANobbMI".into()),
        ]);
        let mut intent = Intent {
            schema: 1,
            profile_id: "smart:latest-plus".into(),
            recommendations_initialized: true,
            requirements: vec![
                Requirement::project("AANobbMI", false, None, None, "user"),
                Requirement::project("sodium", true, None, None, "recommended"),
            ],
            excluded_projects: Vec::new(),
        };
        let normalized = normalized_intent(&intent, &aliases).unwrap();
        assert_eq!(normalized.requirements.len(), 1);
        assert!(!normalized.requirements[0].enabled);
        assert_eq!(normalized.requirements[0].origin, "user");
        intent.requirements.remove(0);
        intent.excluded_projects.push("AANobbMI".into());
        let normalized = normalized_intent(&intent, &aliases).unwrap();
        assert!(normalized.requirements.is_empty());
        assert_eq!(normalized.excluded_projects, ["AANobbMI"]);
    }
}
