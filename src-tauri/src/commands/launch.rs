use std::{sync::Arc, time::Instant};

use tauri::{AppHandle, Emitter};

use crate::{LauncherPaths, auth, core};

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn launch_game(
    version: String,
    memory_mb: Option<u32>,
    loader_type: Option<String>,
    loader_version: Option<String>,
    profile_id: Option<String>,
    event_profile_id: Option<String>,
    window_width: Option<u32>,
    window_height: Option<u32>,
    jvm_flags_override: Option<String>,
    jvm_tuning_mode: Option<String>,
    jvm_tuning_modules: Option<String>,
    jdk_override: Option<String>,
    java_overrides: Option<core::java_selection::Overrides>,
    max_concurrent_downloads: Option<u32>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
    app: AppHandle,
) -> Result<Option<core::mod_runtime::PreparedProfile>, String> {
    if let Some(pid) = profile_id.as_deref() {
        core::profile::validate_profile_ref(pid)?;
    }
    if let Some(event_pid) = event_profile_id.as_deref()
        && event_pid.contains(['/', '\\'])
    {
        return Err("invalid event profile id".to_string());
    }
    let launch_key = event_profile_id
        .as_deref()
        .or(profile_id.as_deref())
        .unwrap_or(&version)
        .to_string();
    let launch_guard = core::running_processes::LaunchGuard::begin(launch_key)?;
    let cancellation = launch_guard.cancellation();
    let launch_timer = Instant::now();
    let mut stage_timer = Instant::now();
    let mut launch_metrics: Vec<(&'static str, u128)> = Vec::new();
    let play_lease = profile_id
        .as_deref()
        .map(|id| core::mod_runtime::play_lease(&paths, id))
        .transpose()?;
    if let Some(id) = profile_id.as_deref()
        && cancellation
            .run(core::mod_runtime::surviving_game(&paths, id, false))
            .await?
    {
        return Err("This profile already has a running game".to_string());
    }

    let cached = if let Some(id) = profile_id.as_deref() {
        cancellation
            .run(core::mod_runtime::cached_for_launch(&paths, id))
            .await?
    } else {
        None
    };
    let mut edit_guard = if let Some(id) = profile_id
        .as_deref()
        .filter(|id| !core::profile::is_smart_profile_ref(id) && cached.is_none())
    {
        Some(
            cancellation
                .run(core::mod_runtime::edit_guard(&paths, id))
                .await?,
        )
    } else {
        None
    };
    let prepared = if cached.is_some() {
        cached
    } else if let Some(id) = profile_id.as_deref() {
        Some(
            cancellation
                .run(core::mod_runtime::for_launch(&paths, id))
                .await?,
        )
    } else {
        None
    };
    if prepared
        .as_ref()
        .is_some_and(|target| target.loader == "fabric")
    {
        edit_guard = None;
    }
    let version = prepared
        .as_ref()
        .map(|p| p.mc_version.clone())
        .unwrap_or(version);
    let loader_type = prepared.as_ref().map(|p| p.loader.clone()).or(loader_type);
    let loader_version = prepared
        .as_ref()
        .and_then(|p| p.loader_version.clone())
        .or(loader_version);
    launch_metrics.push(("mod_revision", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    let mut observer = if let (Some(prepared), Some(id)) = (&prepared, profile_id.as_deref()) {
        match core::mod_health::begin(&paths.root(), id, &prepared.revision_id).await {
            Ok(observer) => Some(observer),
            Err(error) => {
                log::warn!("[launch] Outcome recording unavailable: {error}");
                None
            }
        }
    } else {
        None
    };
    let result = async {

    let manifest = cancellation
        .run(core::manifest::fetch_manifest(&paths))
        .await?;
    log::info!(
        "[launch] Manifest ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("manifest", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == version || (version == "latest" && v.id == manifest.latest.release))
        .ok_or(format!("Version '{}' was not found", version))?
        .clone();

    let vanilla_json = cancellation
        .run(core::manifest::fetch_version_json(
            &entry.id, &entry.url, &paths,
        ))
        .await?;
    log::info!(
        "[launch] Version JSON ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("version_json", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    let java_overrides = java_overrides.unwrap_or_default();
    let loader = loader_type.as_deref().unwrap_or("vanilla");
    let version_json_future = async {
        match loader {
            "fabric" => {
                let lv = loader_version
                    .as_deref()
                    .ok_or("loader_version is required to launch Fabric")?;
                core::fabric::build_fabric_version_json(&vanilla_json, lv, &paths).await
            }
            "quilt" => {
                let lv = loader_version
                    .as_deref()
                    .ok_or("loader_version is required to launch Quilt")?;
                core::quilt::build_quilt_version_json(&vanilla_json, lv, &paths).await
            }
            "neoforge" => {
                let lv = loader_version
                    .as_deref()
                    .ok_or("loader_version is required to launch NeoForge")?;
                core::neoforge::build_neoforge_version_json(
                    &vanilla_json,
                    lv,
                    &paths,
                    &java_overrides,
                    jdk_override.as_deref(),
                )
                .await
            }
            "forge" => {
                let lv = loader_version
                    .as_deref()
                    .ok_or("loader_version is required to launch Forge")?;
                core::forge::build_forge_version_json(
                    &vanilla_json,
                    lv,
                    &paths,
                    &java_overrides,
                    jdk_override.as_deref(),
                )
                .await
            }
            _ => Ok(vanilla_json),
        }
    };
    let version_json = cancellation.run(version_json_future).await?;
    log::info!(
        "[launch] Loader profile ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("loader_profile", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    let max_dl = max_concurrent_downloads.unwrap_or(16) as usize;
    cancellation
        .run(core::downloader::download_version_files(
            &version_json,
            &paths,
            &app,
            max_dl,
        ))
        .await?;
    log::info!(
        "[launch] Libraries ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("libraries", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    cancellation
        .run(core::assets::download_assets(
            &version_json.asset_index,
            &paths,
            &app,
        ))
        .await?;
    log::info!(
        "[launch] Assets ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("assets", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    // Authentication data is secret-bearing. Resolve it only after all work
    // that does not need it, immediately before argument construction/spawn.
    let auth = cancellation.run(auth::ensure_fresh_auth(&app)).await?;
    log::info!(
        "[launch] Auth ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("auth", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    let pre_launch_elapsed_ms = launch_timer.elapsed().as_millis();

    let request = core::launcher::LaunchRequest {
            version_id: &entry.id,
            version_json: &version_json,
            auth: &auth,
            paths: &paths,
            memory_max_mb: memory_mb.unwrap_or(2048),
            profile_id: profile_id.as_deref(),
            event_profile_id: event_profile_id.as_deref(),
            window_width,
            window_height,
            jvm_flags_override: jvm_flags_override.as_deref(),
            jvm_tuning_mode: jvm_tuning_mode.as_deref(),
            jvm_tuning_modules: jvm_tuning_modules.as_deref(),
            jdk_override: jdk_override.as_deref(),
            java_overrides: &java_overrides,
            pre_launch_metrics: launch_metrics,
            pre_launch_elapsed_ms,
            cancellation,
            mod_revision: prepared.as_ref().map(|p| p.revision_id.as_str()),
            prepared_mods_dir: prepared
                .as_ref()
                .filter(|p| p.loader == "fabric")
                .map(|p| p.mods_dir.as_path()),
            mod_leases: play_lease
                .into_iter()
                .chain(edit_guard.as_ref().map(|guard| guard.lease()))
                .collect(),
        };
    if let Some(observer) = observer.take() {
        core::launcher::launch_with_observer(&request, &app, observer).await?;
    } else {
        core::launcher::launch(&request, &app).await?;
    }
    drop(auth);
    log::info!(
        "[launch] Java process prepared in {} ms; total pre-spawn time {} ms",
        stage_timer.elapsed().as_millis(),
        launch_timer.elapsed().as_millis()
    );

    if let Some(pid) = profile_id.as_deref() {
        let resolved = core::profile::ResolvedTarget {
            mc_version: entry.id.clone(),
            loader: loader.to_string(),
            loader_version: loader_version.clone(),
            resolved_at: chrono::Utc::now(),
        };
        let _ =
            core::profile::touch_launched_with_resolved(&paths.root(), pid, Some(resolved)).await;
        if core::profile::is_smart_profile_ref(pid)
            && let Ok(Some(current)) = core::mod_runtime::ready(&paths, pid)
        {
            let _ = app.emit("mods://prepared", serde_json::json!({ "profileId": pid, "resolved": { "mcVersion": current.mc_version, "loader": current.loader, "loaderVersion": current.loader_version, "resolvedAt": chrono::Utc::now().to_rfc3339() } }));
        }
    }

    cancellation.check()?;
    Ok(prepared)
    }.await;
    if result.is_err()
        && let Some(observer) = &mut observer
    {
        observer
            .preparation_failed(cancellation.check().is_err())
            .await;
    }
    result
}

#[tauri::command]
pub async fn prepare_smart_profile(
    profile_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<core::mod_runtime::PreparedProfile, String> {
    if !core::profile::is_smart_profile_ref(&profile_id) {
        return Err("not a smart profile".to_string());
    }
    core::mod_runtime::for_launch(&paths, &profile_id).await
}

#[tauri::command]
pub async fn stop_game(profile_id: String) -> Result<(), String> {
    if profile_id.contains(['/', '\\']) {
        return Err("invalid profile id".to_string());
    }
    core::running_processes::stop(&profile_id).await
}
