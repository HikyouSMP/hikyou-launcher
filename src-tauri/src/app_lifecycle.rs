//! Application setup and tray lifecycle wiring.

use std::sync::Arc;

use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};

use crate::core::paths::LauncherPaths;
use crate::{app_window, core, logger, shortcuts};

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let data_root = dirs::data_dir()
        .expect("Could not resolve the data directory")
        .join("hikyou-launcher");

    let paths = LauncherPaths::new(data_root);
    paths
        .ensure_dirs()
        .expect("Failed to create launcher directories");

    logger::init(&paths);
    core::cache::init(&paths.cache_db()).expect("Failed to initialize API cache DB");
    core::launcher_state::init(&paths.launcher_state_db())
        .expect("Failed to initialize launcher state DB");

    tauri::async_runtime::spawn(async {
        if let Some(cache) = core::cache::get() {
            let _ = cache.prune_expired().await;
        }
    });

    register_saved_shortcut(app, &paths);
    app.manage(Arc::new(paths));

    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);

    setup_tray(app)?;
    show_main_window(app);
    configure_log_window(app);

    Ok(())
}

fn register_saved_shortcut(app: &tauri::App, paths: &LauncherPaths) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let settings_path = paths.root().join("settings.json");
    if let Ok(json) = std::fs::read_to_string(&settings_path)
        && let Ok(val) = serde_json::from_str::<serde_json::Value>(&json)
        && let Some(shortcut_str) = val.get("shortcut").and_then(|v| v.as_str())
        && let Some(shortcut) = shortcuts::parse_shortcut_str(shortcut_str)
    {
        if let Err(e) = app.global_shortcut().unregister_all() {
            log::warn!("[shortcut] Failed to unregister existing shortcuts: {}", e);
        }
        if let Err(e) = app.global_shortcut().register(shortcut) {
            log::warn!("[shortcut] Failed to register custom shortcut: {}", e);
        }
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
