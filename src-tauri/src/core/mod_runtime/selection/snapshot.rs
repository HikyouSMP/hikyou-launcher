//! Consistent read-only Mod projection; never waits for provider preparation.

use super::*;
use serde::de::DeserializeOwned;
use std::{collections::BTreeSet, io::Read};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModSnapshot {
    pub profile_id: String,
    pub revision: String,
    pub active_revision_id: Option<String>,
    pub target: Option<ModTarget>,
    pub rows: Vec<ModRow>,
    pub requested: requests::RequestedState,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModRow {
    pub key: String,
    pub project_id: Option<String>,
    pub object_id: Option<String>,
    pub name: String,
    pub icon_url: Option<String>,
    pub desired: &'static str,
    pub source: &'static str,
    pub active: Option<ActiveMod>,
    pub reason: &'static str,
    pub can_enable: bool,
    pub can_exclude: bool,
    pub can_inherit: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveMod {
    pub enabled: bool,
    pub version: Option<String>,
    pub filename: String,
}

fn read_json<T: DeserializeOwned>(
    paths: &LauncherPaths,
    path: &Path,
    limit: u64,
) -> Result<Option<T>, String> {
    let root = managed_fs::root(&paths.root())?;
    if managed_fs::inspect(&root, path)?.is_none() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    managed_fs::read_file(&root, path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| {
            "Cannot read Mod snapshot state; retry after checking local access".to_string()
        })?;
    if bytes.len() as u64 > limit {
        return Err("Mod snapshot state exceeds its size limit".into());
    }
    serde_json::from_slice(&bytes).map(Some).map_err(|_| {
        "Invalid Mod snapshot state; repair the damaged record before changing choices".into()
    })
}

struct Inputs {
    head: Option<ModRevision>,
    intent: Option<Intent>,
    recommendations: Option<Vec<AutoMod>>,
    target: Option<ModTarget>,
    protection: Option<MaintenanceState>,
    preparation: Option<mod_intent::Preparation>,
    token: String,
}

fn digest(value: &impl Serialize) -> Result<String, String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(|e| e.to_string())?)
    ))
}

fn capture(paths: &LauncherPaths, id: &str) -> Result<Inputs, String> {
    profile::validate_profile_ref(id)?;
    let root = paths.root();
    let profile_dir = profile::profile_dir_for_ref(&root, id)?;
    let relative = profile_dir
        .strip_prefix(&root)
        .map_err(|_| "Invalid profile scope")?
        .join("profile.json");
    let saved_profile: Option<profile::Profile> = read_json(paths, &relative, 1024 * 1024)?;
    if saved_profile
        .as_ref()
        .is_some_and(|profile| profile.id != id)
    {
        return Err("Profile identity mismatch".into());
    }
    if saved_profile.is_none() && !profile::is_smart_profile_ref(id) {
        return Err("profile not found".into());
    }
    let head = ModRepository::new(&root).head(id)?;
    let source_intent = mod_intent::load(&root, id)?;
    let protection: Option<MaintenanceState> = read_json(
        paths,
        Path::new(&format!("state/mod-maintenance/{}.json", key(id))),
        64 * 1024,
    )?;
    let committed = head
        .as_ref()
        .zip(protection.as_ref())
        .is_some_and(|(head, state)| state.pending_restore.as_deref() == Some(&head.id));
    if source_intent.is_none()
        && head.as_ref().is_some_and(|head| head.intent_id.is_some())
        && !committed
    {
        return Err("Mod wishes are missing for a managed configuration. Restore a recorded configuration before editing its choices.".into());
    }
    let aliases = identities::load(paths, id)?;
    let intent = source_intent
        .as_ref()
        .map(|intent| identities::normalize_intent(intent, &aliases))
        .transpose()?;
    let recommendations =
        if !profile::is_smart_profile_ref(id) || intent.as_ref().is_some_and(|i| i.independent) {
            None
        } else {
            mod_recommendations::load_auto_mods_checked(&paths.auto_mods_file())?
                .map(|entries| identities::recommendations(paths, id, entries))
                .transpose()?
        };
    let preparation = mod_intent::preparation(&root, id)?;
    let target = head.as_ref().map(|head| head.target.clone()).or_else(|| {
        saved_profile.as_ref().and_then(|profile| {
            if profile::is_smart_profile_ref(id) {
                profile.resolved.as_ref().map(|resolved| ModTarget {
                    mc_version: resolved.mc_version.clone(),
                    loader: resolved.loader.clone(),
                    loader_version: resolved.loader_version.clone(),
                })
            } else {
                Some(ModTarget {
                    mc_version: profile.mc_version.clone(),
                    loader: profile.loader.clone(),
                    loader_version: profile.loader_version.clone(),
                })
            }
        })
    });
    let token = digest(&(
        "mod-selection-v1",
        id,
        &head,
        &source_intent,
        &recommendations,
        &saved_profile,
        &protection,
        &preparation,
        &aliases,
    ))?;
    Ok(Inputs {
        head,
        intent,
        recommendations,
        target,
        protection,
        preparation,
        token,
    })
}

pub fn snapshot(paths: &LauncherPaths, id: &str) -> Result<ModSnapshot, String> {
    for _ in 0..3 {
        let inputs = capture(paths, id)?;
        let result = project(paths, id, &inputs)?;
        if inputs.token == capture(paths, id)?.token {
            return Ok(result);
        }
    }
    Err("Mod state changed while reading; refresh and retry".into())
}

fn project(paths: &LauncherPaths, id: &str, inputs: &Inputs) -> Result<ModSnapshot, String> {
    let mut active_intent = inputs
        .head
        .as_ref()
        .map(|head| {
            Ok::<_, String>(
                mod_intent::for_revision(&paths.root(), head)?
                    .map(|r| r.intent)
                    .unwrap_or_else(|| Intent::from_revision(head)),
            )
        })
        .transpose()?;
    let mut intent = inputs
        .intent
        .clone()
        .or_else(|| active_intent.clone())
        .unwrap_or_else(|| Intent::empty(id));
    let frozen = inputs
        .head
        .as_ref()
        .zip(inputs.protection.as_ref())
        .is_some_and(|(head, state)| {
            intent.choice_sequence <= state.choice_sequence
                && (pinned(state, head) || state.pending_restore.as_deref() == Some(&head.id))
        });
    if frozen && let (Some(head), Some(state)) = (&inputs.head, &inputs.protection) {
        let mut restored = decisions::restore_base(paths, head)?
            .or_else(|| active_intent.clone())
            .ok_or("Restored Mod wishes are missing")?;
        restored.choice_sequence = restored
            .choice_sequence
            .max(intent.choice_sequence)
            .max(state.choice_sequence);
        if let Some(active) = &mut active_intent {
            active.choice_sequence = restored.choice_sequence;
        }
        intent = restored;
    } else if let Some(recommendations) = &inputs.recommendations {
        inherit_recommendations(&mut intent, recommendations, inputs.target.as_ref())?;
    }
    let mut selected = policy(&intent)?;
    if profile::is_smart_profile_ref(id)
        && !intent.independent
        && let Some(recommendations) = &inputs.recommendations
    {
        let entries = recommendations
            .iter()
            .map(|entry| {
                Ok(Recommendation {
                    key: ModKey::modrinth(&entry.project_id).map_err(|e| e.to_string())?,
                    enabled: intent.requirements.iter().any(|r| {
                        r.project_id.as_ref() == Some(&entry.project_id)
                            && r.origin == "recommended"
                            && r.enabled
                    }),
                    version: VersionRule::Compatible,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        selected = selected
            .apply(
                &selected.revision().map_err(|e| e.to_string())?,
                Change::ReplaceRecommendations(entries),
            )
            .map_err(|e| e.to_string())?;
    }
    let mut keys: BTreeSet<String> = intent
        .requirements
        .iter()
        .map(Requirement::key)
        .chain(
            intent
                .excluded_projects
                .iter()
                .map(|id| format!("modrinth:{id}")),
        )
        .chain(intent.choice_provenance.keys().cloned())
        .collect();
    if profile::is_smart_profile_ref(id)
        && !intent.independent
        && let Some(recommendations) = &inputs.recommendations
    {
        keys.extend(
            recommendations
                .iter()
                .map(|entry| format!("modrinth:{}", entry.project_id)),
        );
    }
    if let Some(head) = &inputs.head {
        keys.extend(head.mods.iter().map(entry_key));
    }
    if keys.len() > 4096 {
        return Err("Mod snapshot exceeds its row limit".into());
    }
    let mut rows = Vec::new();
    for key in keys {
        let identity = mod_key(&key)?;
        let choice = selected.choice(&identity);
        let request = intent.requirements.iter().find(|r| r.key() == key);
        let active = inputs
            .head
            .as_ref()
            .and_then(|head| head.mods.iter().find(|entry| entry_key(entry) == key));
        let recommendation = inputs.recommendations.as_ref().and_then(|entries| {
            entries
                .iter()
                .find(|entry| identity.project_id() == Some(entry.project_id.as_str()))
        });
        let explicit = intent.choice_provenance.get(&key);
        let legacy = matches!(choice.desired, Desired::Excluded)
            && explicit.is_none()
            && (identity
                .project_id()
                .is_some_and(|id| intent.excluded_projects.iter().any(|project| project == id))
                || request.is_some_and(|r| r.origin == "imported"));
        let source = if legacy {
            "legacyUnknown"
        } else {
            match choice.source {
                ChoiceSource::ExplicitInclude | ChoiceSource::ExplicitExclude => {
                    if request.is_some_and(|r| r.origin == "imported") && explicit.is_none() {
                        "legacyUnknown"
                    } else {
                        "user"
                    }
                }
                ChoiceSource::Recommendation => "recommended",
                ChoiceSource::NotRequested => {
                    if active.is_some() {
                        "dependency"
                    } else {
                        "user"
                    }
                }
            }
        };
        let desired = match choice.desired {
            Desired::Required(_) => "required",
            Desired::Excluded => "excluded",
            Desired::NotRequested => "notRequested",
        };
        let reason = match choice.desired {
            Desired::Excluded => {
                if legacy {
                    "legacyExcluded"
                } else {
                    "excluded"
                }
            }
            Desired::Required(_) => {
                if active.is_some_and(|entry| entry.enabled) {
                    "active"
                } else {
                    "pending"
                }
            }
            Desired::NotRequested => {
                if active.is_some_and(|entry| entry.enabled) {
                    "dependency"
                } else {
                    "defaultOff"
                }
            }
        };
        let name = request
            .and_then(|r| r.display_name.clone())
            .or_else(|| recommendation.map(|r| r.name.clone()))
            .or_else(|| active.and_then(|r| r.display_name.clone()))
            .or_else(|| active.map(|r| r.filename.trim_end_matches(".disabled").to_string()))
            .unwrap_or_else(|| identity.project_id().unwrap_or(&key).into());
        let icon_url = request
            .and_then(|r| r.icon_url.clone())
            .or_else(|| recommendation.and_then(|r| r.icon_url.clone()))
            .or_else(|| active.and_then(|r| r.icon_url.clone()));
        rows.push(ModRow {
            key,
            project_id: identity.project_id().map(str::to_owned),
            object_id: identity.object_id().map(str::to_owned),
            name,
            icon_url,
            desired,
            source,
            active: active.map(|entry| ActiveMod {
                enabled: entry.enabled,
                version: entry
                    .provider_version_number
                    .clone()
                    .or_else(|| entry.version.clone()),
                filename: entry.filename.clone(),
            }),
            reason,
            can_enable: desired != "required",
            can_exclude: desired != "excluded",
            can_inherit: !intent.independent
                && matches!(
                    choice.source,
                    ChoiceSource::ExplicitInclude | ChoiceSource::ExplicitExclude
                ),
        });
    }
    let fingerprint = intent.fingerprint()?;
    let pending = active_intent
        .as_ref()
        .map(Intent::fingerprint)
        .transpose()?
        .as_deref()
        != Some(&fingerprint);
    let preparation = inputs
        .preparation
        .clone()
        .filter(|state| state.intent_id == fingerprint);
    let active_revision_id = inputs.head.as_ref().map(|head| head.id.clone());
    Ok(ModSnapshot {
        profile_id: id.into(),
        revision: digest(&(
            &inputs.token,
            selected.revision().map_err(|e| e.to_string())?,
            &fingerprint,
        ))?,
        active_revision_id: active_revision_id.clone(),
        target: inputs.target.clone(),
        rows,
        requested: requests::RequestedState {
            intent,
            active_revision_id,
            pending,
            preparation,
        },
    })
}

pub(in super::super) fn entry_key(entry: &crate::core::mod_repository::ModEntry) -> String {
    match &entry.project_id {
        Some(project) => format!("modrinth:{project}"),
        None => format!("local:{}", entry.object_id),
    }
}
