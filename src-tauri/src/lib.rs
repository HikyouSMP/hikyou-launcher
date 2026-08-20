use crate::core::paths::LauncherPaths;

mod app_lifecycle;
mod app_window;
mod auth;
mod commands;
mod core;
mod logger;
mod shortcuts;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(shortcuts::plugin())
        .setup(app_lifecycle::setup)
        .invoke_handler(tauri::generate_handler![
            commands::auth::start_webview_login,
            commands::auth::get_saved_auth,
            commands::auth::get_auth_token_debug_status,
            commands::auth::logout,
            commands::auth::apply_log_window_backdrop,
            commands::profiles::switch_account,
            commands::profiles::delete_account_auth_cmd,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::system::get_default_shortcut,
            commands::system::suspend_shortcut,
            commands::system::register_shortcut,
            commands::system::open_crash_report_issue,
            commands::versions::get_version_manifest,
            commands::versions::refresh_version_manifest,
            commands::versions::get_fabric_versions,
            commands::versions::get_quilt_versions,
            commands::versions::get_neoforge_versions,
            commands::versions::get_forge_versions,
            commands::profiles::list_profiles,
            commands::profiles::create_profile,
            commands::profiles::update_profile,
            commands::profiles::delete_profile,
            commands::profiles::copy_profile_options,
            commands::mods::get_profile_mods,
            commands::mods::remove_profile_mod,
            commands::mods::search_modrinth,
            commands::mods::install_modrinth_mod,
            commands::mods::mark_profile_auto_mod_sync_current,
            commands::mods::toggle_profile_mod,
            commands::mods::backfill_mod_metadata,
            commands::mods::get_recommended_mods,
            commands::mods::get_all_recommended_mods,
            commands::mods::get_auto_mods,
            commands::mods::save_auto_mods,
            commands::mods::init_auto_mods,
            commands::mods::search_modrinth_modpacks,
            commands::mods::get_modpack_versions,
            commands::mods::install_modrinth_modpack,
            commands::mods::install_modpack_as_profile,
            commands::launch::launch_game,
            commands::launch::stop_game,
            commands::crash::ensure_log_inspector_enabled_cmd,
            commands::crash::get_latest_crash_analysis,
            commands::crash::list_profile_log_sources,
            commands::crash::read_profile_log_source,
            commands::crash::parse_crash_log,
            commands::system::open_folder,
            commands::system::hide_main_window,
            commands::cache::fetch_cached_icon,
            commands::cache::clear_api_cache,
            commands::cache::get_cache_stats,
            commands::system::get_launcher_paths,
            commands::system::get_java_debug_info,
            commands::system::get_smart_profile_statuses,
            commands::system::record_launch_metrics,
            commands::system::get_launch_metric_history,
            commands::system::get_secure_storage_backend,
            commands::system::detect_gpu_vendor,
        ])
        .run(tauri::generate_context!())
        .expect("Failed to start the Tauri application");
}
