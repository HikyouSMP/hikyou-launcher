use serde::Deserialize;
use tauri::Manager;
use tauri_plugin_shell::ShellExt;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::auth::{common, storage::StoredAuth};

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
struct BrokerToken {
    access_token: String,
    expires_at: i64,
    account_key: String,
}

#[derive(Deserialize)]
struct BrokerError {
    error: String,
    #[serde(default)]
    message: String,
}

fn broker_error_message(error: BrokerError) -> String {
    match error.error.as_str() {
        "cancelled" => "__user_cancelled__".to_string(),
        "interaction_required" => "Microsoft sign-in is required. Please sign in again.".to_string(),
        "runtime_missing" | "runtime_incompatible" =>
            "Windows authentication runtime is missing or incompatible. Please reinstall Hikyou Launcher. [runtime_missing_or_incompatible]".to_string(),
        "runtime_load_failed" =>
            "Windows authentication runtime could not load. Please reinstall Hikyou Launcher. [runtime_load_failed]".to_string(),
        "configuration_error" => "Microsoft authentication is not configured in this build. [configuration_error]".to_string(),
        "msal_error" | "broker_exception" if !error.message.is_empty()
            && error.message.len() <= 80
            && error.message.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') =>
            format!("Windows authentication broker failed. [{}: {}]", error.error, error.message),
        _ => "Windows authentication broker rejected the request.".to_string(),
    }
}

pub async fn login(app: &tauri::AppHandle) -> Result<StoredAuth, String> {
    let token = acquire(app, true, None).await?;
    complete(token).await
}

pub async fn refresh(
    app: &tauri::AppHandle,
    account_key: Option<&str>,
) -> Result<StoredAuth, String> {
    let token = acquire(app, false, account_key).await?;
    complete(token).await
}

async fn complete(token: BrokerToken) -> Result<StoredAuth, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let expires_in = u64::try_from(token.expires_at)
        .unwrap_or_default()
        .saturating_sub(now);
    if expires_in == 0 {
        return Err("Windows authentication broker returned an expired token.".to_string());
    }
    common::complete_from_microsoft(&token.access_token, None, Some(token.account_key.clone()))
        .await
}

async fn acquire(
    app: &tauri::AppHandle,
    interactive: bool,
    account_key: Option<&str>,
) -> Result<BrokerToken, String> {
    let parent = app
        .get_webview_window("main")
        .and_then(|window| window.hwnd().ok())
        .map(|handle| handle.0 as isize)
        .unwrap_or_default();
    let mut args = vec![
        if interactive {
            "--interactive"
        } else {
            "--silent"
        }
        .to_string(),
        "--parent-window".to_string(),
        parent.to_string(),
    ];
    if let Some(account_key) = account_key {
        args.push("--account-key".to_string());
        args.push(account_key.to_string());
    }

    let output = app
        .shell()
        .sidecar("hikyou-auth-broker")
        .map_err(|error| format!("Windows authentication broker is unavailable: {error}"))?
        .args(args)
        .output()
        .await
        .map_err(|error| format!("Windows authentication broker failed to start: {error}"))?;
    let stdout = Zeroizing::new(output.stdout);
    if !output.status.success() {
        if let Ok(error) = serde_json::from_slice::<BrokerError>(&stdout) {
            return Err(broker_error_message(error));
        }
        return Err("Windows authentication broker failed.".to_string());
    }
    serde_json::from_slice(&stdout)
        .map_err(|_| "Windows authentication broker returned an invalid response.".to_string())
}

#[cfg(test)]
mod tests {
    use super::{BrokerError, broker_error_message};

    #[test]
    fn broker_diagnostics_preserve_codes_but_not_exception_text() {
        let error = |code: &str, message: &str| {
            broker_error_message(BrokerError {
                error: code.to_string(),
                message: message.to_string(),
            })
        };
        assert_eq!(error("cancelled", ""), "__user_cancelled__");
        assert!(error("runtime_missing", "").contains("reinstall"));
        assert!(error("msal_error", "wam_runtime_init_failed").contains("wam_runtime_init_failed"));
        assert!(!error("msal_error", "token=private@example.com").contains("private"));
        assert!(!error("unknown", "secret").contains("secret"));
    }
}
