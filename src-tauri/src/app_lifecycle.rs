//! Application setup and tray lifecycle wiring.

use std::sync::Arc;

use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};

use crate::core::paths::LauncherPaths;
use crate::{app_window, core, logger, shortcuts};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let gate = Arc::new(crate::startup::Gate::default());
    if !app.manage(gate.clone()) {
        return Err("startup gate already exists".into());
    }
    let result = configure(app).map_err(|error| logger::redact(format!("Startup failed: {error}")));
    gate.complete(result.clone())?;
    if let Err(message) = result {
        eprintln!("{message}");
        log::error!("{message}");
        let handle = app.handle().clone();
        app.dialog()
            .message(format!("{message}\n\nHikyou did not complete startup. Do not delete your profiles to repair this error.\nReport this diagnostic at https://github.com/HikyouSMP/hikyou-launcher/issues"))
            .title("Hikyou Launcher")
            .kind(MessageDialogKind::Error)
            .show(move |_| handle.exit(1));
    }
    Ok(())
}

fn configure(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let data_root = dirs::data_dir()
        .ok_or("startup.data_directory: could not resolve the data directory")?
        .join("hikyou-launcher");

    let paths = LauncherPaths::new(data_root);
    paths
        .ensure_dirs()
        .map_err(|e| format!("startup.directories: {e}"))?;

    logger::init(&paths);
    core::cache::init(&paths.cache_db()).map_err(|e| format!("startup.api_cache: {e}"))?;
    core::launcher_state::init(&paths.launcher_state_db())
        .map_err(|e| format!("startup.observed_state: {e}"))?;

    // No background mutation is started until all required startup resources exist.
    setup_tray(app)?;
    tauri::async_runtime::spawn_blocking(|| {
        let _ = crate::auth::crypto::diagnostics();
    });

    tauri::async_runtime::spawn(async {
        if let Some(cache) = core::cache::get()
            && let Err(error) = cache.prune_expired().await
        {
            log::warn!("[cache] Expiry cleanup failed: {error}");
        }
    });

    register_saved_shortcut(app, &paths);
    let paths = Arc::new(paths);
    tauri::async_runtime::spawn(core::java::maintain(paths.clone()));
    tauri::async_runtime::spawn(core::mod_runtime::maintain(
        paths.clone(),
        app.handle().clone(),
    ));
    app.manage(paths);

    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);

    show_main_window(app);
    configure_log_window(app);

    Ok(())
}

fn register_saved_shortcut(app: &tauri::App, paths: &LauncherPaths) {
    let settings_path = paths.root().join("settings.json");
    let configured = std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|value| {
            value
                .get("shortcut")
                .and_then(|entry| entry.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(shortcuts::default_shortcut_string);
    if let Err(e) = shortcuts::register(app.handle(), &configured) {
        log::warn!(
            "[shortcut] Could not register {}. The launcher remains available from the tray: {}",
            configured,
            e
        );
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let mut tray_builder = TrayIconBuilder::new();
    if let Some(icon) = app.default_window_icon() {
        tray_builder = tray_builder.icon(icon.clone());
    } else {
        log::warn!(
            "[tray] Default window icon is unavailable; tray icon will use platform fallback"
        );
    }

    tray_builder
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                app_window::toggle(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => app_window::toggle(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn show_main_window(app: &tauri::App) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    app_window::position_on_cursor_monitor(&window);
    #[cfg(target_os = "macos")]
    app_window::activate_app_macos();
    if let Err(e) = window.show() {
        log::warn!("[setup] Failed to show window: {}", e);
    }
    if let Err(e) = window.set_focus() {
        log::warn!("[setup] Failed to focus window: {}", e);
    }

    configure_main_window_effects(&window);
}

#[cfg(target_os = "windows")]
fn configure_main_window_effects(window: &tauri::WebviewWindow) {
    use std::ffi::c_void;
    use windows::Win32::Foundation::{BOOL, HWND};
    use windows::Win32::Graphics::Dwm::{DWMWA_TRANSITIONS_FORCEDISABLED, DwmSetWindowAttribute};

    if let Ok(tauri_hwnd) = window.hwnd() {
        let hwnd = HWND(tauri_hwnd.0);
        let disable = BOOL(1);
        unsafe {
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_TRANSITIONS_FORCEDISABLED,
                &disable as *const BOOL as *const c_void,
                std::mem::size_of::<BOOL>() as u32,
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn configure_main_window_effects(window: &tauri::WebviewWindow) {
    use tauri::utils::config::WindowEffectsConfig;
    use tauri::window::{Effect, EffectState};

    if let Err(e) = window.set_effects(WindowEffectsConfig {
        effects: vec![Effect::HudWindow],
        state: Some(EffectState::Active),
        radius: Some(12.0),
        color: None,
        ..Default::default()
    }) {
        log::warn!("[setup] Failed to apply HudWindow effect: {}", e);
    }
    app_window::configure_macos_window(window);
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn configure_main_window_effects(_: &tauri::WebviewWindow) {}

fn configure_log_window(app: &tauri::App) {
    if let Some(window) = app.get_webview_window("game-log")
        && let Err(e) = app_window::configure_windows_log_window(&window)
    {
        log::warn!("[setup] Failed to apply Log Inspector backdrop: {}", e);
    }
}
