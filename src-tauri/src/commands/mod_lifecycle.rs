use crate::{
    LauncherPaths,
    core::{mod_runtime, profile},
};
use std::sync::Arc;
use tauri::Emitter;

#[tauri::command]
pub async fn get_plus_play_setup(
    profile_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<mod_runtime::PlusPlaySetup, String> {
    mod_runtime::plus_play_setup(&paths, &profile_id).await
}
#[tauri::command]
pub async fn confirm_plus_play_setup(
    profile_id: String,
    expected_revision: String,
    include_keys: Vec<String>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
    app: tauri::AppHandle,
) -> Result<mod_runtime::ModSnapshot, String> {
    let result =
        mod_runtime::confirm_plus_play_setup(&paths, &profile_id, &expected_revision, include_keys)
            .await;
    let _ = app.emit(
        "mods://requests-changed",
        serde_json::json!({"profileId":profile_id}),
    );
    result
}

#[tauri::command]
pub async fn get_profile_mod_snapshot(
    profile_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<mod_runtime::ModSnapshot, String> {
    let paths = paths.inner().clone();
    tokio::task::spawn_blocking(move || mod_runtime::snapshot(&paths, &profile_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_profile_mod_choice(
    profile_id: String,
    key: String,
    choice: String,
    expected_revision: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
    app: tauri::AppHandle,
) -> Result<mod_runtime::ModSnapshot, String> {
    let result =
        mod_runtime::set_choice(&paths, &profile_id, &key, &choice, &expected_revision).await;
    // An event is only invalidation: consumers re-read backend-owned state.
    let _ = app.emit(
        "mods://requests-changed",
        serde_json::json!({ "profileId": profile_id }),
    );
    result
}

#[tauri::command]
pub async fn get_mod_decisions(
    profile_id: Option<String>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<mod_runtime::DecisionView>, String> {
    let ids = match profile_id {
        Some(id) => {
            profile::validate_profile_ref(&id)?;
            vec![id]
        }
        None => profile::list_profiles(&paths.root())
            .await
            .into_iter()
            .map(|p| p.id)
            .collect(),
    };
    let paths = paths.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut result = Vec::new();
        for id in ids {
            result.extend(mod_runtime::decision_views(&paths, &id)?);
        }
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn answer_mod_decision(
    profile_id: String,
    proposal_id: String,
    expected_revision: String,
    action: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let result = mod_runtime::answer_decision(
        &paths,
        &profile_id,
        &proposal_id,
        &expected_revision,
        &action,
    )
    .await;
    let _ = app.emit(
        "mods://decisions-changed",
        serde_json::json!({ "profileId": profile_id }),
    );
    let _ = app.emit(
        "mods://requests-changed",
        serde_json::json!({ "profileId": profile_id }),
    );
    result
}
