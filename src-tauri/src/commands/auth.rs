use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use zeroize::Zeroizing;

use crate::{app_window, auth};

#[tauri::command]
pub async fn start_webview_login(
    app: tauri::AppHandle,
    window_title: Option<String>,
) -> Result<auth::PublicAuth, String> {
    let session = auth::browser_flow::start_sisu_session().await?;
    let login_url: url::Url = session
        .login_url
        .parse()
        .map_err(|e: url::ParseError| format!("login URL parse error: {}", e))?;

    if let Some(existing) = app.get_webview_window("ms-login") {
        if let Err(e) = existing.close() {
            log::warn!("[login] Failed to close existing login window: {}", e);
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }

    // The authorization code is a short-lived credential. Keep the receiver's
    // owned copy zeroizing while the async login flow completes.
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<Zeroizing<String>, String>>();
    let tx = Arc::new(Mutex::new(Some(tx)));
    let tx_nav = Arc::clone(&tx);
    let tx_close = Arc::clone(&tx);

    let redirect_prefix = auth::REDIRECT_URI;
    let expected_oauth_state = session.oauth_state.clone();

    let win =
        tauri::WebviewWindowBuilder::new(&app, "ms-login", tauri::WebviewUrl::External(login_url))
            .title(
                window_title
                    .as_deref()
                    .unwrap_or("Sign into Hikyou Launcher"),
            )
            .inner_size(480.0, 700.0)
            .min_inner_size(400.0, 540.0)
            .resizable(true)
            .always_on_top(true)
            .center()
            .on_navigation(move |url| {
                if is_oauth_redirect(url, redirect_prefix) {
                    let code = extract_callback_code(url, expected_oauth_state.as_str());
                    if let Ok(mut guard) = tx_nav.lock()
                        && let Some(sender) = guard.take()
                    {
                        let _ = sender.send(code);
                    }
                    return false;
                }
                true
            })
            .build()
            .map_err(|e| format!("failed to create login window: {}", e))?;

    win.on_window_event(move |event| {
        if matches!(
            event,
            tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
        ) && let Ok(mut guard) = tx_close.lock()
            && let Some(sender) = guard.take()
        {
            let _ = sender.send(Err("__user_cancelled__".to_string()));
        }
    });

    let code_result = tokio::time::timeout(std::time::Duration::from_secs(600), rx)
        .await
        .map_err(|_| {
            if let Err(e) = win.close() {
                log::warn!("[login] Failed to close login window after timeout: {}", e);
            }
            "authentication timed out after 10 minutes".to_string()
        })?
        .map_err(|_| "__user_cancelled__".to_string())?;

    if let Err(e) = win.close() {
        log::warn!(
            "[login] Failed to close login window after authentication: {}",
            e
        );
    }

    let code = code_result?;
    auth::browser_flow::complete_with_sisu(&code, session)
        .await
        .map(|auth| auth.to_public())
}

fn is_oauth_redirect(url: &url::Url, redirect_uri: &str) -> bool {
    let Ok(expected) = url::Url::parse(redirect_uri) else {
        return false;
    };
    url.scheme() == expected.scheme()
        && url.host_str() == expected.host_str()
        && url.port_or_known_default() == expected.port_or_known_default()
        && url.path() == expected.path()
}

fn extract_callback_code(
    url: &url::Url,
    expected_state: &str,
) -> Result<Zeroizing<String>, String> {
    let mut code = None;
    let mut returned_state = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => returned_state = Some(value.into_owned()),
            _ => {}
        }
    }

    match (code, returned_state) {
        (Some(code), Some(returned_state)) if returned_state == expected_state => {
            Ok(Zeroizing::new(code))
        }
        (Some(_), _) => Err("OAuth response state did not match the active login session".to_string()),
        (None, _) => Err("authorization code was not found".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_callback_code, is_oauth_redirect};

    const REDIRECT: &str = "https://login.live.com/oauth20_desktop.srf";

    #[test]
    fn accepts_only_the_exact_desktop_redirect_with_matching_state() {
        let url = url::Url::parse(
            "https://login.live.com/oauth20_desktop.srf?code=short-lived-code&state=expected",
        )
        .unwrap();

        assert!(is_oauth_redirect(&url, REDIRECT));
        assert_eq!(
            extract_callback_code(&url, "expected").unwrap().as_str(),
            "short-lived-code"
        );
    }

    #[test]
    fn rejects_a_callback_with_a_mismatched_state_or_path() {
        let mismatched_state = url::Url::parse(
            "https://login.live.com/oauth20_desktop.srf?code=short-lived-code&state=other",
        )
        .unwrap();
        assert!(extract_callback_code(&mismatched_state, "expected").is_err());

        let lookalike = url::Url::parse(
            "https://login.live.com/oauth20_desktop.srf.evil?code=code&state=expected",
        )
        .unwrap();
        assert!(!is_oauth_redirect(&lookalike, REDIRECT));
    }
}

#[tauri::command]
pub async fn get_saved_auth() -> Result<auth::PublicAuth, String> {
    auth::load_auth().await.map(|auth| auth.to_public())
}

/// Safe authentication diagnostics. Token values are never returned to the
/// frontend because the debug surface is routinely copied and screenshotted.
#[derive(Serialize)]
pub struct AuthTokenDebugStatus {
    pub minecraft_access: TokenDebugState,
    pub microsoft_refresh: TokenDebugState,
    pub microsoft_access: TokenDebugState,
    pub xbox_user: TokenDebugState,
    pub xsts: TokenDebugState,
}

#[derive(Serialize)]
pub struct TokenDebugState {
    pub persisted: bool,
    pub available: bool,
    pub expires_at: Option<u64>,
}

#[tauri::command]
pub async fn get_auth_token_debug_status() -> AuthTokenDebugStatus {
    let saved = auth::load_auth().await.ok();
    let minecraft_access = TokenDebugState {
        persisted: true,
        available: saved
            .as_ref()
            .is_some_and(|auth| !auth.access_token.is_empty()),
        expires_at: saved.as_ref().map(|auth| auth.expires_at),
    };
    let microsoft_refresh = TokenDebugState {
        persisted: true,
        available: saved.as_ref().is_some_and(|auth| {
            auth.refresh_token
                .as_deref()
                .is_some_and(|token| !token.is_empty())
        }),
        // The specific legacy Microsoft Account refresh-token expiry is not
        // returned by this flow, so do not invent a date in diagnostics.
        expires_at: None,
    };

    AuthTokenDebugStatus {
        minecraft_access,
        microsoft_refresh,
        // These are deliberately limited to the active OAuth/Xbox exchange.
        microsoft_access: TokenDebugState {
            persisted: false,
            available: false,
            expires_at: None,
        },
        xbox_user: TokenDebugState {
            persisted: false,
            available: false,
            expires_at: None,
        },
        xsts: TokenDebugState {
            persisted: false,
            available: false,
            expires_at: None,
        },
    }
}

#[tauri::command]
pub async fn logout() -> Result<(), String> {
    auth::delete_auth().await
}

#[tauri::command]
pub fn apply_log_window_backdrop(app: AppHandle) -> Result<(), String> {
    app_window::configure_log_backdrop(&app)
}
