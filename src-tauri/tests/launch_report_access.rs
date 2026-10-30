include!("../app_commands.rs");

macro_rules! command_policy {
    ($($module:ident::$command:ident [$($role:ident),*]),* $(,)?) => {
        &[$((stringify!($command), &[$(stringify!($role)),*] as &[&str])),*]
    };
}

const POLICY: &[(&str, &[&str])] = application_commands!(command_policy);

#[test]
fn launch_reporting_is_registered_for_main_only() -> Result<(), String> {
    for name in ["prepare_launch_error_report", "open_launch_error_report"] {
        let (_, roles) = POLICY
            .iter()
            .find(|(command, _)| *command == name)
            .ok_or_else(|| format!("missing command: {name}"))?;
        assert!(roles.is_empty());
    }
    Ok(())
}

#[tauri::command]
fn prepare_launch_error_report() -> &'static str {
    "local preview"
}

#[tauri::command]
fn open_launch_error_report() -> &'static str {
    "explicit open"
}

#[test]
fn tool_windows_and_remote_origins_cannot_use_reporting() -> Result<(), Box<dyn std::error::Error>>
{
    use tauri::{
        ipc::{CallbackFn, InvokeBody},
        test::{INVOKE_KEY, get_ipc_response, mock_builder},
        webview::InvokeRequest,
    };
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            prepare_launch_error_report,
            open_launch_error_report
        ])
        .build(tauri::generate_context!())?;
    let local = if cfg!(windows) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    };
    for label in ["main", "game-log", "ms-login"] {
        let window = tauri::WebviewWindowBuilder::new(&app, label, Default::default()).build()?;
        for command in ["prepare_launch_error_report", "open_launch_error_report"] {
            for origin in [local, "https://untrusted.invalid"] {
                let response = get_ipc_response(
                    &window,
                    InvokeRequest {
                        cmd: command.into(),
                        callback: CallbackFn(0),
                        error: CallbackFn(1),
                        url: origin.parse()?,
                        body: InvokeBody::default(),
                        headers: Default::default(),
                        invoke_key: INVOKE_KEY.into(),
                    },
                );
                assert_eq!(
                    response.is_ok(),
                    label == "main" && origin == local,
                    "{label}: {command}"
                );
            }
        }
    }
    Ok(())
}
