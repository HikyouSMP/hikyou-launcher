use std::{
    collections::HashSet,
    future::Future,
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};

use tauri::AppHandle;

use crate::{LauncherPaths, auth, core};

static ACTIVE_LAUNCHES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

struct LaunchCommandGuard {
    key: String,
    cancellation: Arc<core::running_processes::LaunchCancellation>,
}

impl LaunchCommandGuard {
    fn try_acquire(key: String) -> Result<Self, String> {
        let launches = ACTIVE_LAUNCHES.get_or_init(|| Mutex::new(HashSet::new()));
        let mut launches = launches
            .lock()
            .map_err(|_| "launch guard lock was poisoned".to_string())?;
        if !launches.insert(key.clone()) {
            return Err("launch is already in progress for this profile".to_string());
        }
        let cancellation = core::running_processes::begin_launch(&key);
        Ok(Self { key, cancellation })
    }
}

impl LaunchCommandGuard {
    fn cancellation(&self) -> &Arc<core::running_processes::LaunchCancellation> {
        &self.cancellation
    }
}

async fn until_cancelled<T>(
    cancellation: &core::running_processes::LaunchCancellation,
    future: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => {
            Err(core::running_processes::LAUNCH_CANCELLED_ERROR.to_string())
        }
        result = future => result,
    }
}

impl Drop for LaunchCommandGuard {
    fn drop(&mut self) {
        if let Some(launches) = ACTIVE_LAUNCHES.get()
            && let Ok(mut launches) = launches.lock()
        {
            launches.remove(&self.key);
        }
        crate::core::running_processes::finish_launch(&self.key);
    }
}

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
    max_concurrent_downloads: Option<u32>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
    app: AppHandle,
) -> Result<(), String> {
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
    let launch_guard = LaunchCommandGuard::try_acquire(launch_key)?;
    let cancellation = launch_guard.cancellation();
    let launch_timer = Instant::now();
    let mut stage_timer = Instant::now();
    let mut launch_metrics: Vec<(&'static str, u128)> = Vec::new();

    let manifest = until_cancelled(cancellation, core::manifest::fetch_manifest(&paths)).await?;
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

    let vanilla_json = until_cancelled(
        cancellation,
        core::manifest::fetch_version_json(&entry.id, &entry.url, &paths),
    )
    .await?;
    log::info!(
        "[launch] Version JSON ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("version_json", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

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
                core::neoforge::build_neoforge_version_json(&vanilla_json, lv, &paths).await
            }
            "forge" => {
                let lv = loader_version
                    .as_deref()
                    .ok_or("loader_version is required to launch Forge")?;
                core::forge::build_forge_version_json(&vanilla_json, lv, &paths).await
            }
            _ => Ok(vanilla_json),
        }
    };
    let version_json = until_cancelled(cancellation, version_json_future).await?;
    log::info!(
        "[launch] Loader profile ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("loader_profile", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    let max_dl = max_concurrent_downloads.unwrap_or(16) as usize;
    until_cancelled(
        cancellation,
        core::downloader::download_version_files(&version_json, &paths, &app, max_dl),
    )
    .await?;
    log::info!(
        "[launch] Libraries ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("libraries", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    until_cancelled(
        cancellation,
        core::assets::download_assets(&version_json.asset_index, &paths, &app),
    )
    .await?;
    log::info!(
        "[launch] Assets ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("assets", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    // Authentication data is secret-bearing. Resolve it only after all work
    // that does not need it, immediately before argument construction/spawn.
    let auth = until_cancelled(cancellation, auth::ensure_fresh_auth(&app)).await?;
    log::info!(
        "[launch] Auth ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("auth", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    let pre_launch_elapsed_ms = launch_timer.elapsed().as_millis();

    core::launcher::launch(
        &core::launcher::LaunchRequest {
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
            pre_launch_metrics: launch_metrics,
            pre_launch_elapsed_ms,
            cancellation,
        },
        &app,
    )
    .await?;
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
    }

    Ok(())
}

#[tauri::command]
pub async fn stop_game(profile_id: String) -> Result<(), String> {
    if profile_id.contains(['/', '\\']) {
        return Err("invalid profile id".to_string());
    }
    core::running_processes::stop(&profile_id)?;

    // Let the cancelled launch future drop before the UI clears its state. This
    // prevents an already queued download event from making the profile appear
    // to start again immediately after Stop was pressed.
    for _ in 0..500 {
        if !core::running_processes::is_active(&profile_id) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    Err("timed out while stopping the profile".to_string())
}
