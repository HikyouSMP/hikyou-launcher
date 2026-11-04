//! Verified provider aliases survive restart; display names never establish identity.

use super::*;
use std::{collections::BTreeMap, io::Read};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Catalog {
    profile_id: String,
    aliases: BTreeMap<String, String>,
}

pub(super) fn load(paths: &LauncherPaths, id: &str) -> Result<BTreeMap<String, String>, String> {
    profile::validate_profile_ref(id)?;
    let root = managed_fs::root(&paths.root())?;
    let path = PathBuf::from(format!("state/mod-identities/{}.json", key(id)));
    if managed_fs::inspect(&root, &path)?.is_none() {
        return Ok(BTreeMap::new());
    }
    let mut bytes = Vec::new();
    managed_fs::read_file(&root, &path)?
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 256 * 1024 {
        return Err("Mod identity catalog exceeds its size limit".into());
    }
    let catalog: Catalog = serde_json::from_slice(&bytes)
        .map_err(|_| "Invalid Mod identity catalog; selection is blocked")?;
    if catalog.profile_id != id
        || catalog.aliases.len() > 512
        || catalog.aliases.iter().any(|(source, target)| {
            !mod_intent::valid_project(source)
                || !mod_intent::valid_project(target)
                || catalog
                    .aliases
                    .get(target)
                    .is_some_and(|next| next != target)
        })
    {
        return Err("Invalid or ambiguous Mod identity catalog".into());
    }
    Ok(catalog.aliases)
}

pub(super) fn save(
    paths: &LauncherPaths,
    id: &str,
    aliases: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut previous = load(paths, id)?;
    for (source, target) in aliases {
        if previous.get(source).is_some_and(|old| old != target) {
            return Err(
                "A Mod project alias changed identity; review it before changing saved choices"
                    .into(),
            );
        }
        previous.insert(source.clone(), target.clone());
    }
    if previous.len() > 512
        || previous.iter().any(|(source, target)| {
            !mod_intent::valid_project(source)
                || !mod_intent::valid_project(target)
                || previous.get(target).is_some_and(|next| next != target)
        })
    {
        return Err("Mod identity catalog exceeds its validation limits".into());
    }
    let dir = managed_fs::directory(
        &managed_fs::root(&paths.root())?,
        Path::new("state/mod-identities"),
        true,
    )?;
    managed_fs::write(
        &dir,
        Path::new(&format!("{}.json", key(id))),
        &serde_json::to_vec(&Catalog {
            profile_id: id.into(),
            aliases: previous,
        })
        .map_err(|e| e.to_string())?,
        true,
    )
}

pub(super) fn normalize_intent(
    intent: &Intent,
    known: &BTreeMap<String, String>,
) -> Result<Intent, String> {
    let mut aliases = known.clone();
    for id in intent
        .requirements
        .iter()
        .filter_map(|r| r.project_id.as_deref())
        .chain(intent.excluded_projects.iter().map(String::as_str))
        .chain(
            intent
                .choice_provenance
                .keys()
                .filter_map(|key| key.strip_prefix("modrinth:")),
        )
    {
        aliases
            .entry(id.to_string())
            .or_insert_with(|| id.to_string());
    }
    super::requests::normalized_intent(intent, &aliases)
}

pub(super) fn recommendations(
    paths: &LauncherPaths,
    id: &str,
    mut entries: Vec<super::super::mod_recommendations::AutoMod>,
) -> Result<Vec<super::super::mod_recommendations::AutoMod>, String> {
    let aliases = load(paths, id)?;
    let mut ids = std::collections::BTreeSet::new();
    for entry in &mut entries {
        if let Some(canonical) = aliases.get(&entry.project_id) {
            entry.project_id = canonical.clone();
        }
        if !ids.insert(entry.project_id.clone()) {
            return Err(
                "Multiple recommendations refer to the same Mod; review the recommendation list"
                    .into(),
            );
        }
    }
    Ok(entries)
}
