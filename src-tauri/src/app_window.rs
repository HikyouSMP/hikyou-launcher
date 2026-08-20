use tauri::{AppHandle, Manager};

pub fn toggle(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let visible = window.is_visible().unwrap_or(false);
        if visible {
            if let Err(e) = window.hide() {
                log::warn!("[window] Failed to hide main window: {}", e);
            } else {
                log::info!("[window] Main window hidden via toggle");
            }
        } else {
            position_on_cursor_monitor(&window);
            #[cfg(target_os = "macos")]
            activate_app_macos();
            if let Err(e) = window.show() {
                log::warn!("[window] Failed to show main window: {}", e);
            }
            if let Err(e) = window.set_focus() {
                log::warn!("[window] Failed to focus main window: {}", e);
            } else {
                log::info!("[window] Main window shown via toggle");
            }
        }
    } else {
        log::warn!("[window] Toggle requested but main window was not found");
    }
}

pub fn configure_log_backdrop(app: &AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("game-log") else {
        return Err("Log Inspector window was not found".to_string());
    };
    #[cfg(target_os = "macos")]
    {
        use tauri::utils::config::WindowEffectsConfig;
        use tauri::window::{Effect, EffectState};
        window
            .set_effects(WindowEffectsConfig {
                effects: vec![Effect::HudWindow],
                state: Some(EffectState::Active),
                radius: Some(12.0),
                color: None,
            })
            .map_err(|e| format!("failed to set Log Inspector macOS effect: {}", e))?;
        configure_macos_window(&window);
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    configure_windows_log_window(&window)
}

#[cfg(target_os = "macos")]
pub fn activate_app_macos() {
    use std::os::raw::{c_char, c_void};

    #[link(name = "objc", kind = "dylib")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *const c_void;
        fn objc_msgSend(obj: *mut c_void, sel: *const c_void) -> *mut c_void;
    }

    unsafe {
        let cls = objc_getClass(b"NSApplication\0".as_ptr() as *const c_char);
        let sel_shared = sel_registerName(b"sharedApplication\0".as_ptr() as *const c_char);
        let ns_app = objc_msgSend(cls, sel_shared);
        let sel_activate =
            sel_registerName(b"activateIgnoringOtherApps:\0".as_ptr() as *const c_char);
        type MsgSendBool = unsafe extern "C" fn(*mut c_void, *const c_void, u8) -> *mut c_void;
        let send: MsgSendBool = std::mem::transmute(
            objc_msgSend as unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void,
        );
        send(ns_app, sel_activate, 1u8);
    }
}

#[cfg(target_os = "macos")]
pub fn configure_macos_window(window: &tauri::WebviewWindow) {
    use std::os::raw::{c_char, c_void};

    let ns_window = match window.ns_window() {
        Ok(ptr) => ptr as *mut c_void,
        Err(e) => {
            log::warn!("Failed to get NSWindow: {}", e);
            return;
        }
    };
    let wk_view = match window.ns_view() {
        Ok(ptr) => ptr as *mut c_void,
        Err(e) => {
            log::warn!("Failed to get NSView: {}", e);
            return;
        }
    };

    #[link(name = "objc", kind = "dylib")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *const c_void;
        fn objc_msgSend(obj: *mut c_void, sel: *const c_void) -> *mut c_void;
    }

    unsafe {
        type Msg = unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void;
        type MsgBool = unsafe extern "C" fn(*mut c_void, *const c_void, u8) -> *mut c_void;
        type MsgF64 = unsafe extern "C" fn(*mut c_void, *const c_void, f64) -> *mut c_void;
        type MsgPtr = unsafe extern "C" fn(*mut c_void, *const c_void, *mut c_void) -> *mut c_void;
        type MsgUsize = unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> *mut c_void;
        type MsgCount = unsafe extern "C" fn(*mut c_void, *const c_void) -> usize;

        let base = objc_msgSend as unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void;
        let msg: Msg = std::mem::transmute(base);
        let msg_bool: MsgBool = std::mem::transmute(base);
        let msg_f64: MsgF64 = std::mem::transmute(base);
        let msg_ptr: MsgPtr = std::mem::transmute(base);
        let msg_usize: MsgUsize = std::mem::transmute(base);
        let msg_count: MsgCount = std::mem::transmute(base);

        macro_rules! sel {
            ($s:expr) => {
                sel_registerName($s.as_ptr() as *const c_char)
            };
        }
        macro_rules! cls {
            ($s:expr) => {
                objc_getClass($s.as_ptr() as *const c_char)
            };
        }

        msg_bool(ns_window, sel!(b"setOpaque:\0"), 0u8);
        let ns_color = cls!(b"NSColor\0");
        let clear = msg(ns_color, sel!(b"clearColor\0"));
        msg_ptr(ns_window, sel!(b"setBackgroundColor:\0"), clear);

        {
            type MsgRespSel = unsafe extern "C" fn(*mut c_void, *const c_void, *const c_void) -> u8;
            let responds_to: MsgRespSel = std::mem::transmute(base);
            let set_db_sel = sel!(b"setDrawsBackground:\0");

            unsafe fn find_webview(
                view: *mut c_void,
                set_db_sel: *const c_void,
                responds_to: unsafe extern "C" fn(*mut c_void, *const c_void, *const c_void) -> u8,
                msg: unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void,
                msg_usize: unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> *mut c_void,
                msg_count: unsafe extern "C" fn(*mut c_void, *const c_void) -> usize,
                responds_sel: *const c_void,
                subviews_sel: *const c_void,
                count_sel: *const c_void,
                object_at_index_sel: *const c_void,
                depth: usize,
            ) -> *mut c_void {
                if view.is_null() || depth > 5 {
                    return std::ptr::null_mut();
                }
                if unsafe { responds_to(view, responds_sel, set_db_sel) } != 0 {
                    return view;
                }
                let subviews = unsafe { msg(view, subviews_sel) };
                if subviews.is_null() {
                    return std::ptr::null_mut();
                }
                let count = unsafe { msg_count(subviews, count_sel) };
                for i in 0..count {
                    let subview = unsafe { msg_usize(subviews, object_at_index_sel, i) };
                    let found = unsafe {
                        find_webview(
                            subview,
                            set_db_sel,
                            responds_to,
                            msg,
                            msg_usize,
                            msg_count,
                            responds_sel,
                            subviews_sel,
                            count_sel,
                            object_at_index_sel,
                            depth + 1,
                        )
                    };
                    if !found.is_null() {
                        return found;
                    }
                }
                std::ptr::null_mut()
            }

            let actual_wkview: *mut c_void =
                if responds_to(wk_view, sel!(b"respondsToSelector:\0"), set_db_sel) != 0 {
                    wk_view
                } else {
                    find_webview(
                        wk_view,
                        set_db_sel,
                        responds_to,
                        msg,
                        msg_usize,
                        msg_count,
                        sel!(b"respondsToSelector:\0"),
                        sel!(b"subviews\0"),
                        sel!(b"count\0"),
                        sel!(b"objectAtIndex:\0"),
                        0,
                    )
                };

            if !actual_wkview.is_null() {
                msg_bool(actual_wkview, set_db_sel, 0u8);
                log::debug!("WKWebView.drawsBackground = NO set");
            } else {
                log::debug!("WKWebView not found; drawsBackground was not changed");
            }
        }

        msg_bool(wk_view, sel!(b"setWantsLayer:\0"), 1u8);
        let wk_layer = msg(wk_view, sel!(b"layer\0"));
        if !wk_layer.is_null() {
            msg_f64(wk_layer, sel!(b"setCornerRadius:\0"), 12.0f64);
            msg_bool(wk_layer, sel!(b"setMasksToBounds:\0"), 1u8);
        }

        let content_view = msg(ns_window, sel!(b"contentView\0"));
        if !content_view.is_null() {
            msg_bool(content_view, sel!(b"setWantsLayer:\0"), 1u8);
            let cv_layer = msg(content_view, sel!(b"layer\0"));
            if !cv_layer.is_null() {
                msg_f64(cv_layer, sel!(b"setCornerRadius:\0"), 12.0f64);
                msg_bool(cv_layer, sel!(b"setMasksToBounds:\0"), 1u8);
            }
        }

        let subviews = msg(wk_view, sel!(b"subviews\0"));
        let count = msg_count(subviews, sel!(b"count\0"));
        for i in 0..count {
            let subview = msg_usize(subviews, sel!(b"objectAtIndex:\0"), i);
            let responds: u8 = {
                type MsgBoolSel =
                    unsafe extern "C" fn(*mut c_void, *const c_void, *const c_void) -> u8;
                let f: MsgBoolSel = std::mem::transmute(base);
                f(
                    subview,
                    sel!(b"respondsToSelector:\0"),
                    sel!(b"setState:\0"),
                )
            };
            if responds != 0 {
                msg_usize(subview, sel!(b"setState:\0"), 1usize);
                log::debug!("NSVisualEffectView.state = Active (wk_view subview {})", i);
            }
        }

        msg_bool(ns_window, sel!(b"setHasShadow:\0"), 1u8);
    }

    log::debug!("macOS window configured (transparent, drawsBackground=NO, cornerRadius=12)");
}

#[cfg(target_os = "windows")]
pub fn configure_windows_log_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    use std::ffi::c_void;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DWMSBT_MAINWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
        DwmSetWindowAttribute,
    };

    let tauri_hwnd = window
        .hwnd()
        .map_err(|e| format!("failed to get Log Inspector HWND: {}", e))?;
    let hwnd = HWND(tauri_hwnd.0);

    let dark_mode: u32 = 1;
    let backdrop = DWMSBT_MAINWINDOW;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_mode as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        )
        .map_err(|e| format!("failed to set Log Inspector dark Mica mode: {}", e))?;

        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as *const c_void,
            std::mem::size_of_val(&backdrop) as u32,
        )
        .map_err(|e| format!("failed to set Log Inspector Mica backdrop: {}", e))?;
    }

    log::info!("Log Inspector Windows Mica backdrop configured");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn configure_windows_log_window(_: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

pub fn position_on_cursor_monitor(window: &tauri::WebviewWindow) {
    let cursor = match window.cursor_position() {
        Ok(p) => p,
        Err(_) => return,
    };

    let monitors = match window.available_monitors() {
        Ok(ms) => ms,
        Err(_) => return,
    };

    let Some(target) = monitors.iter().find(|m| {
        let pos = m.position();
        let size = m.size();
        cursor.x >= pos.x as f64
            && cursor.x < (pos.x as f64 + size.width as f64)
            && cursor.y >= pos.y as f64
            && cursor.y < (pos.y as f64 + size.height as f64)
    }) else {
        return;
    };

    let mon_pos = target.position();
    let mon_size = target.size();
    let scale = target.scale_factor();
    let win_w = (750.0 * scale).round() as i32;
    let win_h = (470.0 * scale).round() as i32;
    let x = mon_pos.x + (mon_size.width as i32 - win_w) / 2;
    let raw_y = mon_pos.y + (mon_size.height as i32 * 38 / 100) - (win_h / 2);
    let y = raw_y
        .max(mon_pos.y + 24)
        .min(mon_pos.y + mon_size.height as i32 - win_h - 24);

    if let Err(e) = window.set_position(tauri::PhysicalPosition::new(x, y)) {
        log::warn!("[window] Failed to position main window: {}", e);
    }
}
