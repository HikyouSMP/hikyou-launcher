//! Creation-only defaults. No operation here propagates into an existing profile.
use super::{managed_fs, mod_intent, mod_recommendations, paths::LauncherPaths};
use cap_std::fs::OpenOptions;
use futures::{StreamExt, TryStreamExt, stream};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
};

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub project_id: String,
    pub name: String,
    pub icon_url: Option<String>,
    pub loaders: Option<Vec<String>>,
    pub folder_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Folder {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Document {
    sequence: u64,
    entries: Vec<Entry>,
    folders: Vec<Folder>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub revision: String,
    pub entries: Vec<Entry>,
    pub folders: Vec<Folder>,
    pub legacy_entries: Vec<Entry>,
    pub migration_required: bool,
}

fn hash(value: &impl Serialize) -> Result<String, String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(|e| e.to_string())?)
    ))
}
fn directory(paths: &LauncherPaths) -> Result<cap_std::fs::Dir, String> {
    managed_fs::directory(
        &managed_fs::root(&paths.root())?,
        Path::new("state/mod-template"),
        true,
    )
}
fn load(dir: &cap_std::fs::Dir) -> Result<Option<Document>, String> {
    if managed_fs::inspect(dir, Path::new("template.json"))?.is_none() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    managed_fs::read_file(dir, Path::new("template.json"))?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "template_read_failed")?;
    if bytes.len() > 1024 * 1024 {
        return Err("template_too_large".into());
    }
    let value: Document = serde_json::from_slice(&bytes).map_err(|_| "template_invalid")?;
    validate(&value.entries, &value.folders)?;
    Ok(Some(value))
}
fn validate(entries: &[Entry], folders: &[Folder]) -> Result<(), String> {
    if entries.len() > 128 || folders.len() > 128 {
        return Err("template_limit".into());
    }
    let mut projects = BTreeSet::new();
    let mut parents = BTreeMap::new();
    for folder in folders {
        if !mod_intent::valid_project(&folder.id)
            || folder.name.trim().is_empty()
            || folder.name.len() > 256
            || parents
                .insert(&folder.id, folder.parent_id.as_ref())
                .is_some()
        {
            return Err("template_invalid_folder".into());
        }
    }
    for folder in folders {
        let mut current = folder.parent_id.as_ref();
        let mut seen = BTreeSet::from([&folder.id]);
        while let Some(id) = current {
            if !seen.insert(id) || seen.len() > 16 {
                return Err("template_folder_cycle_or_depth".into());
            }
            current = *parents.get(id).ok_or("template_missing_folder")?;
        }
    }
    for entry in entries {
        if !mod_intent::valid_project(&entry.project_id)
            || !projects.insert(&entry.project_id)
            || entry.name.trim().is_empty()
            || entry.name.len() > 4096
            || entry.icon_url.as_ref().is_some_and(|s| s.len() > 2048)
            || entry
                .folder_id
                .as_ref()
                .is_some_and(|id| !parents.contains_key(id))
            || entry.loaders.as_ref().is_some_and(|v| {
                v.is_empty()
                    || v.len() > 4
                    || v.iter()
                        .any(|l| !["fabric", "forge", "quilt", "neoforge"].contains(&l.as_str()))
                    || v.iter().collect::<BTreeSet<_>>().len() != v.len()
            })
        {
            return Err("template_invalid_entry".into());
        }
    }
    Ok(())
}
pub fn snapshot(paths: &LauncherPaths) -> Result<Snapshot, String> {
    let stored = load(&directory(paths)?)?;
    let migration_required = stored.is_none();
    let value = stored.unwrap_or_default();
    let legacy_entries = if migration_required {
        mod_recommendations::load_auto_mods_checked(&paths.auto_mods_file())?
            .unwrap_or_default()
            .into_iter()
            .filter(|m| m.enabled)
            .map(|m| Entry {
                project_id: m.project_id,
                name: m.name,
                icon_url: m.icon_url,
                loaders: if m.loaders.is_empty() {
                    None
                } else {
                    Some(m.loaders)
                },
                folder_id: None,
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(Snapshot {
        revision: hash(&value)?,
        entries: value.entries,
        folders: value.folders,
        legacy_entries,
        migration_required,
    })
}
pub fn save(
    paths: &LauncherPaths,
    expected: &str,
    entries: Vec<Entry>,
    folders: Vec<Folder>,
) -> Result<Snapshot, String> {
    validate(&entries, &folders)?;
    let dir = directory(paths)?;
    managed_fs::inspect(&dir, Path::new(".lock"))?;
    let lock = dir
        .open_with(
            ".lock",
            OpenOptions::new().read(true).write(true).create(true),
        )
        .map_err(|_| "template_lock_failed")?
        .into_std();
    fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| "template_busy")?;
    let old = load(&dir)?.unwrap_or_default();
    if hash(&old)? != expected {
        return Err("template_changed_refresh_required".into());
    }
    let next = Document {
        sequence: old
            .sequence
            .checked_add(1)
            .ok_or("template_sequence_exhausted")?,
        entries,
        folders,
    };
    let bytes = serde_json::to_vec(&next).map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("template_too_large".into());
    }
    managed_fs::write(&dir, Path::new("template.json"), &bytes, true)?;
    snapshot(paths)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewItem {
    pub project_id: String,
    pub name: String,
    pub icon_url: Option<String>,
    pub status: String,
    pub reason: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub revision: String,
    pub token: String,
    pub items: Vec<PreviewItem>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Application {
    pub revision: String,
    pub token: String,
    pub project_ids: Vec<String>,
}

pub async fn preview(paths: &LauncherPaths, mc: &str, loader: &str) -> Result<Preview, String> {
    tokio::time::timeout(
        std::time::Duration::from_secs(120),
        preview_inner(paths, mc, loader),
    )
    .await
    .map_err(|_| "template_catalog_timeout".to_string())?
}
async fn preview_inner(paths: &LauncherPaths, mc: &str, loader: &str) -> Result<Preview, String> {
    if !super::paths::is_safe_component(mc)
        || !["vanilla", "fabric", "forge", "quilt", "neoforge"].contains(&loader)
    {
        return Err("template_invalid_target".into());
    }
    if loader == "vanilla" {
        return Ok(Preview {
            revision: "vanilla".into(),
            token: hash(&(mc, loader))?,
            items: Vec::new(),
        });
    }
    let saved = snapshot(paths)?;
    let items: Vec<PreviewItem> = stream::iter(saved.entries.into_iter().filter(|entry| {
        entry
            .loaders
            .as_ref()
            .is_none_or(|ls| ls.iter().any(|l| l == loader))
    }))
    .map(|entry| async move {
        let project = super::mod_catalog::project(&entry.project_id).await?;
        let available = !super::mod_catalog::versions(&project.id, Some(mc), loader)
            .await?
            .is_empty();
        Ok::<_, String>(PreviewItem {
            project_id: project.id,
            name: project.title,
            icon_url: project.icon_url,
            status: if available {
                "available"
            } else {
                "unavailable"
            }
            .into(),
            reason: if available {
                None
            } else {
                Some("no_target_release".into())
            },
        })
    })
    .buffered(4)
    .try_collect()
    .await?;
    if items
        .iter()
        .map(|i| &i.project_id)
        .collect::<BTreeSet<_>>()
        .len()
        != items.len()
    {
        return Err("template_duplicate_canonical_project".into());
    }
    let token = hash(&(&saved.revision, mc, loader, &items))?;
    Ok(Preview {
        revision: saved.revision,
        token,
        items,
    })
}

pub async fn creation_intent(
    paths: &LauncherPaths,
    id: &str,
    mc: &str,
    loader: &str,
    application: Option<Application>,
) -> Result<mod_intent::Intent, String> {
    let observed = preview(paths, mc, loader).await?;
    let selected = match application {
        Some(value) => {
            if value.revision != observed.revision || value.token != observed.token {
                return Err("template_preview_changed".into());
            }
            let unique: BTreeSet<_> = value.project_ids.iter().collect();
            if unique.len() != value.project_ids.len()
                || value.project_ids.iter().any(|id| {
                    !observed
                        .items
                        .iter()
                        .any(|i| i.project_id == *id && i.status == "available")
                })
            {
                return Err("template_invalid_selection".into());
            }
            value.project_ids
        }
        None if observed.items.is_empty() => Vec::new(),
        None => return Err("template_review_required".into()),
    };
    let mut intent = mod_intent::Intent::empty(id);
    intent.independent = true;
    for item in observed.items {
        if selected.contains(&item.project_id) {
            intent.upsert(mod_intent::Requirement::project(
                &item.project_id,
                true,
                Some(item.name),
                item.icon_url,
                "user",
            ))?;
        }
    }
    Ok(intent)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn vanilla_does_not_read_a_damaged_template() -> Result<(), String> {
        let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let paths = LauncherPaths::new(temp.path().to_path_buf());
        managed_fs::write(&directory(&paths)?, Path::new("template.json"), b"{", true)?;
        assert!(snapshot(&paths).is_err());
        let observed = preview(&paths, "1.21.1", "vanilla").await?;
        assert!(observed.items.is_empty());
        let id = uuid::Uuid::new_v4().to_string();
        let intent = creation_intent(&paths, &id, "1.21.1", "vanilla", None).await?;
        assert!(intent.independent && intent.requirements.is_empty());
        assert!(snapshot(&paths).is_err());
        Ok(())
    }
    #[test]
    fn template_is_independent_and_stale_saves_are_rejected() -> Result<(), String> {
        let root = tempfile::tempdir().map_err(|e| e.to_string())?;
        let paths = LauncherPaths::new(root.path().to_path_buf());
        let before = snapshot(&paths)?;
        assert!(before.entries.is_empty());
        let entry = Entry {
            project_id: "example".into(),
            name: "Example".into(),
            icon_url: None,
            loaders: None,
            folder_id: None,
        };
        let after = save(&paths, &before.revision, vec![entry], Vec::new())?;
        assert_eq!(after.entries.len(), 1);
        assert!(!after.migration_required);
        assert!(save(&paths, &before.revision, Vec::new(), Vec::new()).is_err());
        assert!(!paths.auto_mods_file().exists());
        assert!(mod_intent::load(&paths.root(), "smart:latest-plus")?.is_none());
        Ok(())
    }
    #[test]
    fn hierarchy_rejects_cycles_and_missing_parents() {
        let folders = vec![
            Folder {
                id: "a".into(),
                parent_id: Some("b".into()),
                name: "A".into(),
            },
            Folder {
                id: "b".into(),
                parent_id: Some("a".into()),
                name: "B".into(),
            },
        ];
        assert!(validate(&[], &folders).is_err());
        assert!(validate(&[], &folders[..1]).is_err());
    }
}
