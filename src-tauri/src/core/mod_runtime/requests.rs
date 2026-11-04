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
    super::selection::snapshot(paths, id).map(|snapshot| snapshot.requested)
}

pub(super) fn commit_choice(
    paths: &LauncherPaths,
    id: &str,
    intent: &Intent,
) -> Result<(), String> {
    // Unknown protection state must fail before the durable choice commit.
    let mut state = read_state(paths, id)?;
    mod_intent::save(&paths.root(), intent)?;
    wake_preparation();
    state.pinned = false;
    state.pending_restore = None;
    state.pending_edit = false;
    state.choice_sequence = intent.choice_sequence;
    write_state(paths, id, &state)?;
    mod_intent::set_preparation(&paths.root(), id, &intent.fingerprint()?, "queued", None)
}

async fn request_change(
    paths: &Arc<LauncherPaths>,
    id: &str,
    change: impl FnOnce(&mut Intent, Option<&ModRevision>) -> Result<(), String>,
) -> Result<(), String> {
    let guard = edit_guard(paths, id).await?;
    let repo = ModRepository::new(&paths.root());
    let before = super::selection::snapshot(paths, id)?;
    let head = repo.head(id)?;
    // Accepting a wish must not depend on network access or an initial resolved head.
    let mut intent = before.requested.intent;
    change(&mut intent, head.as_ref())?;
    super::selection::refresh_inheritance(paths, &mut intent, before.target.as_ref())?;
    intent.advance_choice_sequence()?;
    if super::selection::snapshot(paths, id)?.revision != before.revision {
        return Err("Mod inputs changed; refresh before applying this action".into());
    }
    commit_choice(paths, id, &intent)?;
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
    let requirement = if profile::is_smart_profile_ref(id)
        && mod_intent::load(&paths.root(), id)?.is_some_and(|saved| saved.independent)
    {
        let project = crate::core::mod_catalog::project(project_id).await?;
        if crate::core::mod_catalog::versions(&project.id, None, "fabric")
            .await?
            .is_empty()
        {
            return Err("plus_play_requires_fabric_release".into());
        }
        Requirement::project(
            &project.id,
            true,
            Some(project.title),
            project.icon_url,
            "user",
        )
    } else {
        Requirement::project(project_id, true, name, icon, "user")
    };
    request_change(paths, id, move |intent, head| {
        let key = requirement.key();
        intent.upsert(requirement)?;
        super::selection::choose(intent, head, &key, "include")
    })
    .await
}

pub async fn remove_request(
    paths: &Arc<LauncherPaths>,
    id: &str,
    requested_key: &str,
) -> Result<(), String> {
    request_change(paths, id, |intent, head| {
        if !intent.requirements.iter().any(|r| r.key() == requested_key)
            && !intent.choice_provenance.contains_key(requested_key)
            && !intent
                .excluded_projects
                .iter()
                .any(|id| format!("modrinth:{id}") == requested_key)
            && !head.is_some_and(|head| {
                head.mods
                    .iter()
                    .any(|entry| super::selection::entry_key(entry) == requested_key)
            })
        {
            return Err("requested Mod not found".into());
        }
        super::selection::choose(intent, head, requested_key, "exclude")
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
        // The legacy filename is only a lookup in the captured immutable head.
        // No caller-supplied path or live filename is mutation authority.
        let key = super::selection::entry_key(entry);
        let disabled = intent
            .excluded_projects
            .iter()
            .any(|id| entry.project_id.as_ref() == Some(id))
            || intent
                .requirements
                .iter()
                .any(|r| matches(r, entry) && !r.enabled && r.origin != "recommended");
        let choice = if remove || (!disabled && entry.enabled) {
            "exclude"
        } else {
            "include"
        };
        super::selection::choose(intent, head, &key, choice)
    })
    .await
}

#[cfg(test)]
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
            let target = ModRepository::new(&paths.root()).head(&profile_id)?.map(|head| head.target);
            let mut recommendations = mod_recommendations_for_request(paths)?;
            for change in &changes {
                if let Some(entry) = recommendations.iter_mut().find(|r| r.project_id == change.project_id) {
                    entry.enabled = change.enabled;
                }
            }
            super::selection::inherit_recommendations(intent, &recommendations, target.as_ref())
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
    if stored.is_none() && head.is_some_and(|head| head.intent_id.is_some()) {
        return Err(
            "Mod wishes are missing for a managed configuration; automatic replacement is blocked"
                .into(),
        );
    }
    let mut intent = stored.clone().unwrap_or_else(|| {
        head.map(Intent::from_revision)
            .unwrap_or_else(|| Intent::empty(id))
    });
    intent = super::selection::for_target(paths, &intent, target)?;
    if stored.as_ref() != Some(&intent) {
        mod_intent::save(&paths.root(), &intent)?;
    }
    Ok(intent)
}

fn mod_recommendations_for_request(
    paths: &LauncherPaths,
) -> Result<Vec<crate::core::mod_recommendations::AutoMod>, String> {
    crate::core::mod_recommendations::load_auto_mods_checked(&paths.auto_mods_file())?
        .ok_or_else(|| "Recommendation policy is not initialized; Mod preparation will retry after initialization".into())
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
    let mut desired: std::collections::BTreeMap<_, _> = intent
        .requirements
        .iter()
        .filter_map(|request| {
            request
                .project_id
                .as_ref()
                .map(|id| (id.as_str(), request.enabled))
        })
        .collect();
    for project in &intent.excluded_projects {
        desired.insert(project, false);
    }
    for (project, enabled) in desired {
        for file in &installed {
            let base = file
                .filename
                .strip_suffix(".disabled")
                .unwrap_or(&file.filename);
            if metadata
                .get(&file.filename)
                .or_else(|| metadata.get(base))
                .is_some_and(|entry| entry.project_id == project)
                && file.filename.to_ascii_lowercase().ends_with(".disabled") == enabled
            {
                mods::toggle_mod(game, &file.filename).await?;
            }
        }
    }
    mod_intent::materialize_local(&paths.root(), game, intent)?;
    Ok(previous.map(|record| record.edges).unwrap_or_default())
}

pub(super) fn failure_reason(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.starts_with("mod_decision_required:") {
        "decision_required"
    } else if lower.starts_with("mod_resolution_unsatisfiable:") {
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
        .chain(
            intent
                .choice_provenance
                .keys()
                .filter_map(|key| key.strip_prefix("modrinth:").map(str::to_owned)),
        )
        .collect();
    if let Some(head) = head {
        projects.extend(
            head.mods
                .iter()
                .filter_map(|entry| entry.project_id.clone()),
        );
    }
    // Later target trials can make an initially inapplicable recommendation a
    // root. Resolve its identity in this one bounded pass too.
    if !intent.independent && profile::is_smart_profile_ref(&intent.profile_id) {
        projects.extend(
            mod_recommendations_for_request(paths)?
                .into_iter()
                .map(|entry| entry.project_id),
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
    for canonical in aliases.values().cloned().collect::<Vec<_>>() {
        aliases.entry(canonical.clone()).or_insert(canonical);
    }
    super::identities::save(paths, &intent.profile_id, &aliases)?;
    let normalized = normalized_intent(intent, &aliases)?;
    if &normalized != intent {
        *intent = normalized;
    }
    Ok(aliases)
}

pub(super) fn normalized_intent(
    intent: &Intent,
    aliases: &std::collections::BTreeMap<String, String>,
) -> Result<Intent, String> {
    let mut normalized = intent.clone();
    normalized.requirements.clear();
    normalized.excluded_projects = intent
        .excluded_projects
        .iter()
        .map(|project| {
            aliases
                .get(project)
                .cloned()
                .ok_or_else(|| "Missing canonical Mod identity".to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    normalized.excluded_projects.sort();
    normalized.excluded_projects.dedup();
    normalized.choice_provenance.clear();
    for (key, choice) in &intent.choice_provenance {
        let key = match key.strip_prefix("modrinth:") {
            Some(project) => format!(
                "modrinth:{}",
                aliases
                    .get(project)
                    .ok_or("Missing canonical Mod identity")?
            ),
            None => key.clone(),
        };
        if normalized
            .choice_provenance
            .insert(key, choice.clone())
            .is_some_and(|old| old != *choice)
        {
            return Err("conflicting Mod choices refer to the same project".into());
        }
    }
    let mut selected = std::collections::BTreeMap::<String, Requirement>::new();
    let priority = |request: &Requirement| match request.origin.as_str() {
        "user" => 2,
        "imported" => 1,
        _ => 0,
    };
    for mut request in intent.requirements.clone() {
        if let Some(project) = &mut request.project_id {
            *project = aliases
                .get(project)
                .cloned()
                .ok_or("Missing canonical Mod identity")?;
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
            let replaced: Vec<_> = intent
                .requirements
                .iter()
                .filter(|request| matches(request, previous))
                .map(Requirement::key)
                .collect();
            intent
                .requirements
                .retain(|request| !matches(request, previous));
            for key in replaced {
                intent.choice_provenance.remove(&key);
            }
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
            independent: false,
            requirements: vec![
                Requirement::project("AANobbMI", false, None, None, "user"),
                Requirement::project("sodium", true, None, None, "recommended"),
            ],
            excluded_projects: Vec::new(),
            choice_sequence: 0,
            choice_provenance: std::collections::BTreeMap::new(),
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
