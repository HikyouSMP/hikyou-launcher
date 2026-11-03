//! Read-only projection and semantic choice edits. Intent is the only durable policy.

use super::*;
use crate::core::{
    mod_intent::Requirement,
    mod_recommendations::{self, AutoMod},
};
use hikyou_mod_policy::{
    Change, ChoiceSource, Desired, Inclusion, ModKey, Policy, Recommendation, VersionRule,
};

mod snapshot;
#[cfg(test)]
use snapshot::ModRow;
pub(super) use snapshot::entry_key;
pub use snapshot::{ModSnapshot, snapshot};

fn mod_key(key: &str) -> Result<ModKey, String> {
    if let Some(id) = key.strip_prefix("modrinth:") {
        ModKey::modrinth(id)
    } else if let Some(id) = key.strip_prefix("local:") {
        ModKey::local(id)
    } else {
        return Err("invalid semantic Mod key".into());
    }
    .map_err(|error| error.to_string())
}

fn apply(policy: &Policy, key: ModKey, inclusion: Inclusion) -> Result<Policy, String> {
    policy
        .apply(
            &policy.revision().map_err(|e| e.to_string())?,
            Change::SetInclusion { key, inclusion },
        )
        .map_err(|e| e.to_string())
}

/// Reconstructed from the authoritative Intent, never from a second policy store.
pub(super) fn policy(intent: &Intent) -> Result<Policy, String> {
    intent.validate(&intent.profile_id)?;
    let recommendations = intent
        .requirements
        .iter()
        .filter(|r| r.origin == "recommended")
        .map(|r| {
            Ok(Recommendation {
                key: mod_key(&r.key())?,
                enabled: r.enabled,
                version: VersionRule::Compatible,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut result = Policy::new(&intent.profile_id, recommendations).map_err(|e| e.to_string())?;
    for request in &intent.requirements {
        if request.origin == "recommended"
            || intent
                .choice_provenance
                .get(&request.key())
                .is_some_and(|choice| choice == "inherit")
        {
            continue;
        }
        result = apply(
            &result,
            mod_key(&request.key())?,
            if request.enabled {
                Inclusion::Include
            } else {
                Inclusion::Exclude
            },
        )?;
    }
    for project in &intent.excluded_projects {
        result = apply(
            &result,
            ModKey::modrinth(project).map_err(|e| e.to_string())?,
            Inclusion::Exclude,
        )?;
    }
    Ok(result)
}

/// Recommended roots are inherited, not explicit prohibitions when switched off.
pub(super) fn inherit_recommendations(
    intent: &mut Intent,
    recommendations: &[AutoMod],
    target: Option<&ModTarget>,
) -> Result<(), String> {
    if !profile::is_smart_profile_ref(&intent.profile_id) {
        return Ok(());
    }
    // Smart profiles prefer Fabric. A temporary Vanilla candidate is an
    // alternative without Mods, not a reason to silently erase those wishes.
    let preferred = target
        .filter(|target| target.loader == "vanilla")
        .map(|target| ModTarget {
            loader: "fabric".into(),
            ..target.clone()
        });
    let target = preferred.as_ref().or(target);
    intent
        .requirements
        .retain(|request| request.origin != "recommended");
    for recommendation in recommendations {
        let key = format!("modrinth:{}", recommendation.project_id);
        if intent
            .excluded_projects
            .contains(&recommendation.project_id)
            || intent.requirements.iter().any(|r| r.key() == key)
        {
            continue;
        }
        let enabled = target.is_some_and(|target| {
            mods::auto_mod_applies_to_loader_and_version(
                recommendation,
                &target.mc_version,
                &target.loader,
            )
        });
        // Keep defaults visible, but do not feed disabled inherited roots to the solver.
        if enabled {
            intent.requirements.push(Requirement::project(
                &recommendation.project_id,
                true,
                Some(recommendation.name.clone()),
                recommendation.icon_url.clone(),
                "recommended",
            ));
        }
    }
    intent.requirements.sort_by_key(Requirement::key);
    intent.recommendations_initialized = true;
    intent.validate(&intent.profile_id)
}

pub(super) fn refresh_inheritance(
    paths: &LauncherPaths,
    intent: &mut Intent,
    target: Option<&ModTarget>,
) -> Result<(), String> {
    if let Some(recommendations) =
        mod_recommendations::load_auto_mods_checked(&paths.auto_mods_file())?
    {
        let recommendations =
            identities::recommendations(paths, &intent.profile_id, recommendations)?;
        inherit_recommendations(intent, &recommendations, target)?;
    }
    Ok(())
}

/// Read-only target projection. Call before reducing a candidate for an omission
/// decision; calling it afterward would reintroduce omitted recommended roots.
pub(super) fn for_target(
    paths: &LauncherPaths,
    base: &Intent,
    target: &ModTarget,
) -> Result<Intent, String> {
    base.validate(&base.profile_id)?;
    let mut effective = base.clone();
    if profile::is_smart_profile_ref(&base.profile_id) {
        let recommendations = mod_recommendations::load_auto_mods_checked(&paths.auto_mods_file())?
            .ok_or("Recommendation policy is not initialized; target selection is blocked")?;
        let recommendations =
            identities::recommendations(paths, &base.profile_id, recommendations)?;
        inherit_recommendations(&mut effective, &recommendations, Some(target))?;
    }
    Ok(effective)
}

pub(super) fn choose(
    intent: &mut Intent,
    head: Option<&ModRevision>,
    key: &str,
    choice: &str,
) -> Result<(), String> {
    let identity = mod_key(key)?;
    let inclusion = match choice {
        "include" => Inclusion::Include,
        "exclude" => Inclusion::Exclude,
        "inherit" => Inclusion::Inherit,
        _ => return Err("Mod choice must be include, exclude or inherit".into()),
    };
    let next = apply(&policy(intent)?, identity.clone(), inclusion)?;
    let previous = intent.requirements.iter().find(|r| r.key() == key).cloned();
    let entry = head.and_then(|head| head.mods.iter().find(|entry| entry_key(entry) == key));
    let desired = next.choice(&identity).desired;
    intent.requirements.retain(|r| r.key() != key);
    if let Some(project) = identity.project_id() {
        intent.excluded_projects.retain(|id| id != project);
        if choice == "exclude" {
            intent.excluded_projects.push(project.into());
        } else if choice == "include" {
            intent.requirements.push(Requirement::project(
                project,
                true,
                previous
                    .as_ref()
                    .and_then(|r| r.display_name.clone())
                    .or_else(|| entry.and_then(|e| e.display_name.clone())),
                previous
                    .as_ref()
                    .and_then(|r| r.icon_url.clone())
                    .or_else(|| entry.and_then(|e| e.icon_url.clone())),
                "user",
            ));
        }
    } else {
        let mut request = previous
            .or_else(|| {
                entry.map(|entry| Requirement {
                    project_id: None,
                    object_id: Some(entry.object_id.clone()),
                    filename: Some(entry.filename.clone()),
                    enabled: entry.enabled,
                    display_name: entry.display_name.clone(),
                    icon_url: entry.icon_url.clone(),
                    origin: "user".into(),
                })
            })
            .ok_or("Exact local Mod object is unavailable; import it again")?;
        request.enabled = matches!(desired, Desired::Required(_));
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
        intent.requirements.push(request);
    }
    intent.choice_provenance.insert(key.into(), choice.into());
    intent.requirements.sort_by_key(Requirement::key);
    intent.excluded_projects.sort();
    intent.validate(&intent.profile_id)
}

pub async fn set_choice(
    paths: &Arc<LauncherPaths>,
    id: &str,
    key: &str,
    choice: &str,
    expected_revision: &str,
) -> Result<ModSnapshot, String> {
    let guard = edit_guard(paths, id).await?;
    let before = snapshot(paths, id)?;
    if before.revision != expected_revision {
        return Err("Mod choices changed; refresh before applying this action".into());
    }
    if !before.rows.iter().any(|row| row.key == key) {
        return Err("Mod key is not present in this snapshot".into());
    }
    let head = ModRepository::new(&paths.root()).head(id)?;
    let mut intent = before.requested.intent;
    choose(&mut intent, head.as_ref(), key, choice)?;
    refresh_inheritance(paths, &mut intent, before.target.as_ref())?;
    intent.advance_choice_sequence()?;
    // Global recommendations do not take the profile lease: recheck all tokens before commit.
    if snapshot(paths, id)?.revision != expected_revision {
        return Err("Mod inputs changed; refresh before applying this action".into());
    }
    requests::commit_choice(paths, id, &intent)?;
    drop(guard);
    wake_preparation();
    snapshot(paths, id)
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
