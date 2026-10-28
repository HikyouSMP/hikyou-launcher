use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{INVOKE_KEY, get_ipc_response, mock_builder},
    webview::InvokeRequest,
};

include!("../app_commands.rs");

macro_rules! command_policy {
    ($($module:ident::$command:ident [$($role:ident),*]),* $(,)?) => {
        &[$((stringify!($command), &[$(stringify!($role)),*] as &[&str])),*]
    };
}

const COMMAND_POLICY: &[(&str, &[&str])] = application_commands!(command_policy);

#[test]
fn java_file_selection_is_main_window_only() {
    for name in [
        "pick_java_runtime",
        "validate_java_runtime",
        "cancel_auth_login",
    ] {
        let (_, roles) = COMMAND_POLICY
            .iter()
            .find(|(command, _)| *command == name)
            .unwrap();
        assert!(
            roles.is_empty(),
            "Java file access must not be exposed to tool windows"
        );
    }
}

#[test]
fn removed_management_commands_and_window_role_are_not_registered() {
    for removed in [
        "list_screenshots",
        "get_screenshot_thumbnail",
        "trash_screenshot",
        "get_account_appearance",
        "upload_account_skin",
        "set_account_skin_variant",
        "set_account_cape",
    ] {
        assert!(
            COMMAND_POLICY
                .iter()
                .all(|(command, _)| *command != removed),
            "removed command remains registered: {removed}"
        );
    }
    assert!(
        COMMAND_POLICY
            .iter()
            .all(|(_, roles)| !roles.contains(&"screenshots")),
        "removed window role retains command access"
    );
}

#[tauri::command]
fn start_webview_login() -> &'static str {
    "privileged"
}

#[tauri::command]
fn get_settings() -> &'static str {
    "settings"
}

#[test]
fn tool_windows_cannot_acquire_credentials_and_remote_origins_have_no_app_access() {
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![start_webview_login, get_settings])
        .build(tauri::generate_context!())
        .unwrap();
    let local = if cfg!(windows) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    };
    for label in ["main", "game-log", "screenshots", "ms-login"] {
        let window = tauri::WebviewWindowBuilder::new(&app, label, Default::default())
            .build()
            .unwrap();
        let invoke = |command: &str, origin: &str| {
            get_ipc_response(
                &window,
                InvokeRequest {
                    cmd: command.to_string(),
                    callback: CallbackFn(0),
                    error: CallbackFn(1),
                    url: origin.parse().unwrap(),
                    body: InvokeBody::default(),
                    headers: Default::default(),
                    invoke_key: INVOKE_KEY.to_string(),
                },
            )
        };
        assert_eq!(
            invoke("start_webview_login", local).is_ok(),
            label == "main",
            "{label}"
        );
        assert_eq!(
            invoke("get_settings", local).is_ok(),
            matches!(label, "main" | "game-log"),
            "{label}"
        );
        assert!(
            invoke("get_settings", "https://untrusted.invalid").is_err(),
            "{label}"
        );
    }
}
