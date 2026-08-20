use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::app_window;

/// Shortcut string format: `Modifier+Key`, for example `Alt+E` or `Option+Space`.
pub fn parse_shortcut_str(s: &str) -> Option<Shortcut> {
    let parts: Vec<&str> = s.split('+').collect();
    if parts.len() < 2 {
        return None;
    }
    let key_str = parts.last()?;
    let modifier_strs = &parts[..parts.len() - 1];

    let mut modifiers = Modifiers::empty();
    for m in modifier_strs {
        match m.to_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "alt" | "option" => modifiers |= Modifiers::ALT,
            "shift" => modifiers |= Modifiers::SHIFT,
            "cmd" | "super" | "meta" | "command" => modifiers |= Modifiers::SUPER,
            _ => return None,
        }
    }

    let code = match *key_str {
        "Space" => Code::Space,
        "Enter" => Code::Enter,
        "Tab" => Code::Tab,
        "Backspace" => Code::Backspace,
        "A" => Code::KeyA,
        "B" => Code::KeyB,
        "C" => Code::KeyC,
        "D" => Code::KeyD,
        "E" => Code::KeyE,
        "F" => Code::KeyF,
        "G" => Code::KeyG,
        "H" => Code::KeyH,
        "I" => Code::KeyI,
        "J" => Code::KeyJ,
        "K" => Code::KeyK,
        "L" => Code::KeyL,
        "M" => Code::KeyM,
        "N" => Code::KeyN,
        "O" => Code::KeyO,
        "P" => Code::KeyP,
        "Q" => Code::KeyQ,
        "R" => Code::KeyR,
        "S" => Code::KeyS,
        "T" => Code::KeyT,
        "U" => Code::KeyU,
        "V" => Code::KeyV,
        "W" => Code::KeyW,
        "X" => Code::KeyX,
        "Y" => Code::KeyY,
        "Z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "F1" => Code::F1,
        "F2" => Code::F2,
        "F3" => Code::F3,
        "F4" => Code::F4,
        "F5" => Code::F5,
        "F6" => Code::F6,
        "F7" => Code::F7,
        "F8" => Code::F8,
        "F9" => Code::F9,
        "F10" => Code::F10,
        "F11" => Code::F11,
        "F12" => Code::F12,
        _ => return None,
    };

    Some(Shortcut::new(Some(modifiers), code))
}

pub fn default_shortcut_string() -> String {
    #[cfg(target_os = "macos")]
    return "Option+Space".to_string();
    #[cfg(not(target_os = "macos"))]
    return "Alt+E".to_string();
}

pub fn suspend(app: &AppHandle) -> Result<(), String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())
}

pub fn register(app: &AppHandle, shortcut_str: &str) -> Result<(), String> {
    let shortcut = parse_shortcut_str(shortcut_str)
        .ok_or_else(|| format!("invalid shortcut: {}", shortcut_str))?;
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;
    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| e.to_string())
}

pub fn plugin() -> impl tauri::plugin::Plugin<tauri::Wry> {
    #[cfg(target_os = "macos")]
    let default_shortcut = Shortcut::new(Some(Modifiers::ALT), Code::Space);
    #[cfg(not(target_os = "macos"))]
    let default_shortcut = Shortcut::new(Some(Modifiers::ALT), Code::KeyE);

    let builder = match tauri_plugin_global_shortcut::Builder::new().with_shortcut(default_shortcut)
    {
        Ok(builder) => builder,
        Err(e) => {
            log::warn!("[shortcut] Failed to prepare default shortcut: {}", e);
            tauri_plugin_global_shortcut::Builder::new()
        }
    };

    builder
        .with_handler(|app: &AppHandle, _shortcut: &Shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                app_window::toggle(app);
            }
        })
        .build()
}
