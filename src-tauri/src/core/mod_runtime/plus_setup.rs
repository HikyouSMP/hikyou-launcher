//! Explicit one-time boundary migration; no live inheritance after acknowledgement.
use super::super::{mod_catalog, mod_intent::Requirement};
use super::selection::snapshot::ModRow;
use super::*;
use futures::{StreamExt, TryStreamExt, stream};
use std::collections::BTreeSet;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Setup {
    profile_id: String,
    revision: String,
    required: bool,
    rows: Vec<Row>,
    warnings: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    #[serde(flatten)]
    row: ModRow,
    supports_fabric: bool,
}

pub async fn setup(paths: &LauncherPaths, id: &str) -> Result<Setup, String> {
    tokio::time::timeout(Duration::from_secs(120), setup_inner(paths, id))
        .await
        .map_err(|_| "plus_setup_catalog_timeout".to_string())?
}
async fn setup_inner(paths: &LauncherPaths, id: &str) -> Result<Setup, String> {
    if !profile::is_smart_profile_ref(id) {
        return Err("not_a_plus_play_environment".into());
    }
    let snapshot = selection::snapshot(paths, id)?;
    let required = !snapshot.requested.intent.independent;
    let mut source = snapshot.rows;
    // A new environment starts from its own catalog, not creation preferences.
    if snapshot.active_revision_id.is_none() && mod_intent::load(&paths.root(), id)?.is_none() {
        source.clear();
        for rec in super::super::mod_recommendations::get_recommended_mods("fabric").await {
            if !rec.default_enabled {
                continue;
            }
            source.push(ModRow {
                key: format!("modrinth:{}", rec.project_id),
                project_id: Some(rec.project_id),
                object_id: None,
                name: rec.name,
                icon_url: rec.icon_url,
                desired: "required",
                source: "recommended",
                active: None,
                reason: "pending",
                can_enable: false,
                can_exclude: true,
                can_inherit: false,
            });
        }
    }
    if source.len() > 128 {
        return Err("plus_setup_identity_limit_or_duplicate".into());
    }
    let rows: Vec<Row> = stream::iter(source)
        .map(|mut row| async move {
            let supported = if required {
                if let Some(id) = &row.project_id {
                    let observed = mod_catalog::project(id).await?;
                    let supported = !mod_catalog::versions(&observed.id, None, "fabric")
                        .await?
                        .is_empty();
                    row.name = observed.title;
                    row.icon_url = observed.icon_url;
                    row.project_id = Some(observed.id.clone());
                    row.key = format!("modrinth:{}", observed.id);
                    supported
                } else {
                    true
                }
            } else {
                true
            };
            Ok::<_, String>(Row {
                row,
                supports_fabric: supported,
            })
        })
        .buffered(4)
        .try_collect()
        .await?;
    let warnings = if rows.iter().any(|r| !r.supports_fabric) {
        vec!["fabric_release_unavailable".into()]
    } else {
        Vec::new()
    };
    if rows.len() > 128
        || rows
            .iter()
            .map(|r| &r.row.key)
            .collect::<BTreeSet<_>>()
            .len()
            != rows.len()
    {
        return Err("plus_setup_identity_limit_or_duplicate".into());
    }
    Ok(Setup {
        profile_id: id.into(),
        revision: snapshot.revision,
        required,
        rows,
        warnings,
    })
}

pub async fn confirm(
    paths: &Arc<LauncherPaths>,
    id: &str,
    expected: &str,
    include_keys: Vec<String>,
) -> Result<ModSnapshot, String> {
    let observed = setup(paths, id).await?;
    if observed.revision != expected || !observed.required {
        return Err("plus_setup_changed".into());
    }
    let guard = edit_guard(paths, id).await?;
    let before = selection::snapshot(paths, id)?;
    if before.revision != expected {
        return Err("plus_setup_changed".into());
    }
    let old = before.requested.intent;
    let head = ModRepository::new(&paths.root()).head(id)?;
    let next = owned_intent(&old, head.as_ref(), observed.rows, &include_keys)?;
    requests::commit_choice(paths, id, &next)?;
    drop(guard);
    wake_preparation();
    selection::snapshot(paths, id)
}
fn owned_intent(
    old: &Intent,
    head: Option<&ModRevision>,
    rows: Vec<Row>,
    include_keys: &[String],
) -> Result<Intent, String> {
    let selected: BTreeSet<_> = include_keys.iter().collect();
    if selected.len() != include_keys.len()
        || include_keys
            .iter()
            .any(|key| !rows.iter().any(|r| r.row.key == *key && r.supports_fabric))
    {
        return Err("plus_setup_invalid_selection".into());
    }
    let imported = head.map(Intent::from_revision);
    let mut next = Intent::empty(&old.profile_id);
    next.independent = true;
    next.choice_sequence = old.choice_sequence;
    for entry in rows {
        let row = entry.row;
        let enabled = selected.contains(&row.key);
        if let Some(project) = row.project_id {
            next.requirements.push(Requirement::project(
                &project,
                enabled,
                Some(row.name),
                row.icon_url,
                "user",
            ));
        } else {
            let mut request = old
                .requirements
                .iter()
                .find(|r| r.key() == row.key)
                .cloned()
                .or_else(|| {
                    imported.as_ref().and_then(|intent| {
                        intent
                            .requirements
                            .iter()
                            .find(|r| r.key() == row.key)
                            .cloned()
                    })
                })
                .ok_or("plus_setup_local_object_missing")?;
            request.enabled = enabled;
            request.origin = "user".into();
            if let Some(name) = &mut request.filename {
                let base = name.trim_end_matches(".disabled").to_string();
                *name = if enabled {
                    base
                } else {
                    format!("{base}.disabled")
                };
            }
            next.requirements.push(request);
        }
    }
    next.advance_choice_sequence()?;
    next.validate(&old.profile_id)?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(project: &str, name: &str, supported: bool) -> Row {
        Row {
            row: ModRow {
                key: format!("modrinth:{project}"),
                project_id: Some(project.into()),
                object_id: None,
                name: name.into(),
                icon_url: None,
                desired: "excluded",
                source: "legacyUnknown",
                active: None,
                reason: "legacyExcluded",
                can_enable: true,
                can_exclude: false,
                can_inherit: false,
            },
            supports_fabric: supported,
        }
    }
    #[test]
    fn explicit_migration_resolves_legacy_exclusion_without_losing_name_or_other_owners()
    -> Result<(), String> {
        let mut old = Intent::empty("smart:latest-plus");
        old.excluded_projects.push("renderer".into());
        let next = owned_intent(
            &old,
            None,
            vec![
                row("renderer", "Renderer", true),
                row("other", "Other", false),
            ],
            &["modrinth:renderer".into()],
        )?;
        assert!(next.independent);
        assert_eq!(next.requirements.len(), 2);
        assert!(next.requirements[0].enabled);
        assert_eq!(
            next.requirements[0].display_name.as_deref(),
            Some("Renderer")
        );
        assert!(!next.requirements[1].enabled);
        assert_eq!(old.excluded_projects, ["renderer"]);
        assert!(!old.independent);
        assert!(
            owned_intent(
                &old,
                None,
                vec![row("other", "Other", false)],
                &["modrinth:other".into()]
            )
            .is_err()
        );
        assert!(
            owned_intent(
                &old,
                None,
                vec![row("renderer", "Renderer", true)],
                &["modrinth:renderer".into(), "modrinth:renderer".into()]
            )
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn local_migration_preserves_exact_object_identity() -> Result<(), String> {
        let mut old = Intent::empty("smart:snapshot-plus");
        let object = "a".repeat(64);
        old.requirements.push(Requirement {
            project_id: None,
            object_id: Some(object.clone()),
            filename: Some("local.jar".into()),
            enabled: true,
            display_name: Some("Local".into()),
            icon_url: None,
            origin: "imported".into(),
        });
        let entry = Row {
            row: ModRow {
                key: format!("local:{object}"),
                project_id: None,
                object_id: Some(object.clone()),
                name: "Local".into(),
                icon_url: None,
                desired: "required",
                source: "user",
                active: None,
                reason: "pending",
                can_enable: false,
                can_exclude: true,
                can_inherit: false,
            },
            supports_fabric: true,
        };
        let next = owned_intent(&old, None, vec![entry], &[])?;
        assert_eq!(
            next.requirements[0].object_id.as_deref(),
            Some(object.as_str())
        );
        assert_eq!(
            next.requirements[0].filename.as_deref(),
            Some("local.jar.disabled")
        );
        assert_eq!(old.requirements[0].filename.as_deref(), Some("local.jar"));
        Ok(())
    }
}
