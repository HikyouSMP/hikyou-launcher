use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::{LauncherPaths, core, shortcuts};

#[tauri::command]
pub fn open_folder(path: String, paths: tauri::State<Arc<LauncherPaths>>) -> Result<(), String> {
    let path = core::paths::validate_open_dir(&paths.root(), &path)?;

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path.as_os_str())
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path.as_os_str())
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path.as_os_str())
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn hide_main_window(app: AppHandle, reason: String) -> Result<(), String> {
    let Some(window) = app.get_webview_window("main") else {
        log::warn!(
            "[window] Hide requested but main window was not found ({})",
            reason
        );
        return Ok(());
    };
    log::info!("[window] Hiding main window ({})", reason);
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_launcher_paths(paths: tauri::State<Arc<LauncherPaths>>) -> serde_json::Value {
    serde_json::json!({
        "root":          paths.root(),
        "profiles":      paths.profiles(),
        "smart_profiles": paths.smart_profiles(),
        "meta":          paths.meta(),
        "versions":      paths.versions(),
        "libraries":     paths.libraries(),
        "assets":        paths.assets(),
        "java_versions": paths.java_versions(),
        "caches":        paths.caches_dir(),
        "state":         paths.state_dir(),
    })
}

#[tauri::command]
pub fn get_secure_storage_backend() -> String {
    crate::auth::crypto::backend().backend_name().to_string()
}

#[tauri::command]
pub async fn get_smart_profile_statuses(
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<serde_json::Value, String> {
    let mut items = Vec::new();
    for (id, name) in [
        ("smart:latest-plus", "Latest+"),
        ("smart:snapshot-plus", "Snapshot+"),
    ] {
        let game_dir = match paths.profile_game_dir_for_ref(id) {
            Ok(dir) => dir,
            Err(_) => continue,
        };
        let sync = core::mods::read_auto_mod_sync_state(&game_dir).await;
        items.push(serde_json::json!({
            "id": id,
            "name": name,
            "game_dir": game_dir,
            "sync": sync,
        }));
    }
    Ok(serde_json::json!(items))
}

#[tauri::command]
pub async fn record_launch_metrics(
    metrics: core::launcher_state::LaunchMetricsRecord,
) -> Result<(), String> {
    let store = core::launcher_state::get()
        .ok_or_else(|| "launcher state store is not initialized".to_string())?;
    store.record_launch_metrics(metrics).await
}

#[tauri::command]
pub async fn get_launch_metric_history(
    limit: Option<usize>,
) -> Result<Vec<core::launcher_state::LaunchMetricsRecord>, String> {
    let store = core::launcher_state::get()
        .ok_or_else(|| "launcher state store is not initialized".to_string())?;
    store.launch_metric_history(limit.unwrap_or(10)).await
}

#[tauri::command]
pub fn detect_gpu_vendor() -> String {
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("wmic")
            .args([
                "path",
                "win32_VideoController",
                "get",
                "Caption",
                "/format:list",
            ])
            .output();
        if let Ok(output) = output {
            let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
            if text.contains("nvidia") {
                return "nvidia".to_string();
            }
            if text.contains("amd") || text.contains("radeon") {
                return "amd".to_string();
            }
        }
        return "other".to_string();
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("system_profiler")
            .args(["SPDisplaysDataType", "-json"])
            .output();
        if let Ok(output) = output {
            let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
            if text.contains("apple") {
                return "apple".to_string();
            }
            if text.contains("amd") || text.contains("radeon") {
                return "amd".to_string();
            }
        }
        return "apple".to_string();
    }
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/dev/nvidia0").exists() {
            return "nvidia".to_string();
        }
        let output = std::process::Command::new("lspci").output();
        if let Ok(output) = output {
            let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
            if text.contains("nvidia") {
                return "nvidia".to_string();
            }
            if text.contains("amd") || text.contains("radeon") {
                return "amd".to_string();
            }
        }
        return "other".to_string();
    }
    #[allow(unreachable_code)]
    "unknown".to_string()
}

#[tauri::command]
pub fn get_java_debug_info(
    memory_mb: u32,
    paths: tauri::State<Arc<LauncherPaths>>,
) -> serde_json::Value {
    let check_versions: &[(u32, &str)] = &[
        (21, "liberica-nik-21"),
        (17, "liberica-nik-17"),
        (8, "zulu-8"),
        (16, "zulu-16"),
    ];

    for (major, dir_name) in check_versions {
        let bin = paths
            .java_version_dir(dir_name)
            .join("bin")
            .join(core::java::JAVA_BIN);

        if bin.exists() {
            let is_liberica = matches!(major, 17 | 21);
            return serde_json::json!({
                "found": true,
                "java_version": major,
                "java_dist": if is_liberica { "Liberica NIK" } else { "Azul Zulu" },
                "java_path": bin.to_string_lossy(),
                "is_liberica_nik": is_liberica,
                "use_zgc": false,
                "memory_mb": memory_mb,
            });
        }
    }

    serde_json::json!({ "found": false })
}

#[tauri::command]
pub fn get_default_shortcut() -> String {
    shortcuts::default_shortcut_string()
}

#[tauri::command]
pub async fn suspend_shortcut(app: AppHandle) -> Result<(), String> {
    shortcuts::suspend(&app)
}

#[tauri::command]
pub async fn register_shortcut(app: AppHandle, shortcut_str: String) -> Result<(), String> {
    shortcuts::register(&app, &shortcut_str)
}
#[tauri::command]
pub fn open_crash_report_issue() -> Result<(), String> {
    tauri_plugin_opener::open_url(
        "https://github.com/Hikyou-SMP/hikyou-launcher/issues/new",
        None::<&str>,
    )
    .map_err(|e| format!("Failed to open GitHub issue page: {}", e))
}
