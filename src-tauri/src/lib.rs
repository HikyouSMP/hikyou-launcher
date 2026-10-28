use crate::core::paths::LauncherPaths;

#[cfg(all(test, target_os = "windows", target_env = "msvc"))]
#[link(name = "resource", kind = "static", modifiers = "-bundle")]
unsafe extern "C" {}

mod app_lifecycle;
mod app_window;
mod auth;
mod commands;
mod core;
mod http;
mod logger;
mod shortcuts;

include!("../app_commands.rs");

macro_rules! command_handler {
    ($($module:ident::$command:ident [$($role:ident),*]),* $(,)?) => {
        tauri::generate_handler![$(commands::$module::$command),*]
    };
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(shortcuts::plugin())
        .setup(app_lifecycle::setup)
        .on_window_event(auth::on_window_event)
        .invoke_handler(application_commands!(command_handler))
        .run(tauri::generate_context!())
        .expect("Failed to start the Tauri application");
}
