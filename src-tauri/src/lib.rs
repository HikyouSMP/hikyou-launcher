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
mod startup;

include!("../app_commands.rs");

macro_rules! command_handler {
    ($($module:ident::$command:ident [$($role:ident),*]),* $(,)?) => {
        tauri::generate_handler![$(commands::$module::$command),*]
    };
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> Result<(), String> {
    use tauri::Manager;
    let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool =
        application_commands!(command_handler);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(shortcuts::plugin())
        .setup(app_lifecycle::setup)
        .on_window_event(auth::on_window_event)
        .invoke_handler(move |invoke| {
            let ready = invoke
                .message
                .webview_ref()
                .app_handle()
                .try_state::<std::sync::Arc<startup::Gate>>()
                .ok_or_else(|| "startup is not complete".to_string())
                .and_then(|gate| gate.require_ready());
            if let Err(error) = ready {
                invoke.resolver.reject(error);
                return true;
            }
            handler(invoke)
        })
        .run(tauri::generate_context!())
        .map_err(|error| logger::redact(format!("Application framework startup failed: {error}")))
}
