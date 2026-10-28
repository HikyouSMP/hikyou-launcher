//! Windows owns Microsoft credentials. This adapter only acquires a short-lived
//! access token and an opaque account reference; neither is sent to the webview.

#[cfg(test)]
mod live_tests;
mod operation;
mod picker;
#[cfg(test)]
mod tests;

use sha2::{Digest, Sha256};
use std::time::Duration;
use tauri::Manager;
use tokio_util::sync::CancellationToken;
use windows::{
    Security::{
        Authentication::Web::Core::{
            WebAuthenticationCoreManager as Wam, WebTokenRequest, WebTokenRequestPromptType,
            WebTokenRequestResult, WebTokenRequestStatus,
        },
        Credentials::WebAccountProvider,
    },
    Win32::{Foundation::HWND, System::WinRT::IWebAuthenticationCoreManagerInterop},
    core::{HSTRING, factory},
};
use zeroize::Zeroizing;

use super::{common, session, storage::StoredAuth};
use operation::{Pending, on_ui, win};

const SCOPE: &str = "XboxLive.SignIn XboxLive.offline_access";
const MAX_TOKEN_BYTES: usize = 64 * 1024;
const MAX_ACCOUNT_BYTES: usize = 4096;

struct Grant {
    access_token: Zeroizing<String>,
    account_ref: Zeroizing<String>,
    account_key: String,
    cached_result: WebTokenRequestResult,
}

pub async fn login(
    app: &tauri::AppHandle,
    cancel: &CancellationToken,
) -> Result<StoredAuth, String> {
    complete(app, acquire(app, None, cancel).await?, cancel).await
}

pub async fn refresh(
    app: &tauri::AppHandle,
    saved: &StoredAuth,
    cancel: &CancellationToken,
) -> Result<StoredAuth, String> {
    let reference = saved_reference(saved, super::configured_client_id()?)?;
    complete(app, acquire(app, Some(reference), cancel).await?, cancel).await
}

fn saved_reference<'a>(saved: &'a StoredAuth, client: &str) -> Result<&'a str, String> {
    if saved.microsoft_client_id.as_deref() != Some(client) {
        return Err("This saved login must be upgraded. Please sign in again.".into());
    }
    let reference = saved
        .microsoft_account_ref
        .as_deref()
        .filter(|id| valid_reference(id))
        .ok_or("This saved login must be upgraded. Please sign in again.")?;
    if saved.microsoft_account_key.as_deref() != Some(account_key(reference).as_str()) {
        return Err("The saved Windows account reference is invalid. Please sign in again.".into());
    }
    Ok(reference)
}

async fn complete(
    app: &tauri::AppHandle,
    mut grant: Grant,
    cancel: &CancellationToken,
) -> Result<StoredAuth, String> {
    for attempt in 0..2 {
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(session::CANCELLED.into()),
            auth = common::complete_from_microsoft(&grant.access_token, None, Some(grant.account_key.clone())) => auth,
        };
        match result {
            Ok(mut auth) => {
                auth.microsoft_account_ref = Some(grant.account_ref.to_string());
                return Ok(auth);
            }
            Err(common::MicrosoftExchangeError::Rejected) if attempt == 0 => {
                invalidate(app, grant.cached_result.clone(), cancel).await?;
                grant = acquire(app, Some(&grant.account_ref), cancel).await?;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("Microsoft authentication could not be renewed.".into())
}

async fn invalidate(
    app: &tauri::AppHandle,
    result: WebTokenRequestResult,
    cancel: &CancellationToken,
) -> Result<(), String> {
    on_ui(app, cancel, move || {
        Ok(operation::PendingAction::new(win(
            "invalidate_cache",
            result.InvalidateCacheAsync(),
        )?))
    })
    .await?
    .wait(cancel, Duration::from_secs(20))
    .await
}

async fn acquire(
    app: &tauri::AppHandle,
    reference: Option<&str>,
    cancel: &CancellationToken,
) -> Result<Grant, String> {
    let client = super::configured_client_id()?;
    let pending = on_ui(app, cancel, || {
        Ok(Pending::new(win(
            "find_provider",
            Wam::FindAccountProviderWithAuthorityAsync(
                &HSTRING::from("https://login.microsoft.com"),
                &HSTRING::from("consumers"),
            ),
        )?))
    })
    .await?;
    let provider = pending.wait(cancel, Duration::from_secs(20)).await?;
    let provider = if reference.is_none() {
        picker::select_provider(app, provider, cancel).await?
    } else {
        provider
    };
    let expected = reference.map(|id| Zeroizing::new(id.to_string()));
    let request_provider = provider.clone();
    let request = on_ui(app, cancel, move || {
        create_request(&request_provider, client)
    })
    .await?;
    let pending = if let Some(reference) = &expected {
        let id = HSTRING::from(reference.as_str());
        let pending = on_ui(app, cancel, move || {
            Ok(Pending::new(win(
                "find_account",
                Wam::FindAccountAsync(&provider, &id),
            )?))
        })
        .await?;
        let account = pending.wait(cancel, Duration::from_secs(20)).await?;
        let id = on_ui(app, cancel, {
            let account = account.clone();
            move || Ok(Zeroizing::new(win("account_id", account.Id())?.to_string()))
        })
        .await?;
        verify_account(reference, &id)?;
        on_ui(app, cancel, move || {
            Ok(Pending::new(win(
                "silent_start",
                Wam::GetTokenSilentlyWithWebAccountAsync(&request, &account),
            )?))
        })
        .await?
    } else {
        let owner = app.clone();
        on_ui(app, cancel, move || {
            let window = owner
                .get_webview_window("main")
                .ok_or("The main window is unavailable.")?;
            let hwnd = window
                .hwnd()
                .map_err(|_| "The main window handle is unavailable.")?;
            let interop: IWebAuthenticationCoreManagerInterop = win(
                "desktop_interop",
                factory::<Wam, IWebAuthenticationCoreManagerInterop>(),
            )?;
            Ok(Pending::new(win("interactive_start", unsafe {
                interop.RequestTokenForWindowAsync(HWND(hwnd.0), &request)
            })?))
        })
        .await?
    };
    let result = pending
        .wait(
            cancel,
            Duration::from_secs(if expected.is_some() { 60 } else { 600 }),
        )
        .await?;
    let grant = on_ui(app, cancel, move || inspect_result(&result)).await?;
    if let Some(expected) = expected {
        verify_account(&expected, &grant.account_ref)?;
    }
    Ok(grant)
}

fn create_request(provider: &WebAccountProvider, client: &str) -> Result<WebTokenRequest, String> {
    let request = win(
        "create_request",
        WebTokenRequest::CreateWithPromptType(
            provider,
            &HSTRING::from(SCOPE),
            &HSTRING::from(client),
            WebTokenRequestPromptType::Default,
        ),
    )?;
    // These consumer-provider options match Microsoft's MSA v2 WAM request.
    // Without them the fixed Xbox scopes are rejected by the MSA provider.
    let properties = win("request_properties", request.Properties())?;
    for (name, value) in [("api-version", "2.0"), ("oauth2_batch", "1")] {
        win(
            "request_property",
            properties.Insert(&HSTRING::from(name), &HSTRING::from(value)),
        )?;
    }
    Ok(request)
}

fn inspect_result(result: &WebTokenRequestResult) -> Result<Grant, String> {
    let status = win("response_status", result.ResponseStatus())?;
    if status == WebTokenRequestStatus::ProviderError {
        let code = result
            .ResponseError()
            .and_then(|error| error.ErrorCode())
            .unwrap_or_default();
        return Err(format!(
            "Windows account provider rejected authentication. [provider_error:0x{code:08X}]"
        ));
    }
    check_status(status)?;
    let responses = win("responses", result.ResponseData())?;
    if win("response_count", responses.Size())? != 1 {
        return Err("Windows authentication returned an unexpected response count.".into());
    }
    let response = win("response", responses.GetAt(0))?;
    let raw = win("access_token", response.Token())?;
    if raw.len() > MAX_TOKEN_BYTES {
        return Err("Windows authentication returned an oversized token.".into());
    }
    let access_token = parse_token(Zeroizing::new(raw.to_string()))?;
    let account = win("response_account", response.WebAccount())?;
    let id = win("account_id", account.Id())?;
    if id.len() > MAX_ACCOUNT_BYTES {
        return Err("Windows authentication returned an oversized account reference.".into());
    }
    let account_ref = Zeroizing::new(id.to_string());
    if !valid_reference(&account_ref) {
        return Err("Windows authentication returned an invalid account reference.".into());
    }
    Ok(Grant {
        cached_result: result.clone(),
        account_key: account_key(&account_ref),
        access_token,
        account_ref,
    })
}

fn check_status(status: WebTokenRequestStatus) -> Result<(), String> {
    match status {
        WebTokenRequestStatus::Success => Ok(()),
        WebTokenRequestStatus::UserCancel => Err(session::CANCELLED.into()),
        WebTokenRequestStatus::UserInteractionRequired => Err(
            "Microsoft sign-in is required. Please sign in again. [interaction_required]".into(),
        ),
        WebTokenRequestStatus::AccountSwitch => {
            Err("The Microsoft account changed. Please sign in again. [account_switch]".into())
        }
        WebTokenRequestStatus::AccountProviderNotAvailable => {
            Err("Windows account provider is unavailable. [provider_unavailable]".into())
        }
        WebTokenRequestStatus::ProviderError => {
            Err("Windows account provider rejected authentication. [provider_error]".into())
        }
        _ => Err("Windows authentication returned an unknown status. [unknown_status]".into()),
    }
}

fn parse_token(raw: Zeroizing<String>) -> Result<Zeroizing<String>, String> {
    if raw.is_empty() || raw.len() > MAX_TOKEN_BYTES || raw.chars().any(char::is_control) {
        return Err("Windows authentication returned an invalid token.".into());
    }
    if !raw
        .split('&')
        .any(|field| field.starts_with("access_token="))
    {
        if raw.contains('&') || raw.starts_with('{') || raw.chars().any(char::is_whitespace) {
            return Err("Windows authentication returned an unsupported token format.".into());
        }
        return Ok(raw);
    }
    let mut access = None;
    for (name, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        let value = Zeroizing::new(value.into_owned());
        if name == "access_token" {
            if access.is_some()
                || value.is_empty()
                || value
                    .chars()
                    .any(|character| character.is_whitespace() || character.is_control())
            {
                return Err("Windows authentication returned an invalid token response.".into());
            }
            access = Some(value);
        }
    }
    access.ok_or_else(|| "Windows authentication did not return an access token.".into())
}

fn valid_reference(reference: &str) -> bool {
    !reference.is_empty()
        && reference.len() <= MAX_ACCOUNT_BYTES
        && !reference.chars().any(char::is_control)
}

fn account_key(reference: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"hikyou-os-wam-v1\0consumers\0");
    hash.update(super::CLIENT_ID.as_bytes());
    hash.update(b"\0");
    hash.update(reference.as_bytes());
    format!("{:x}", hash.finalize())
}

fn verify_account(expected: &str, actual: &str) -> Result<(), String> {
    if valid_reference(expected) && expected == actual {
        Ok(())
    } else {
        Err("Windows authentication returned a different account. Please sign in again. [account_mismatch]".into())
    }
}
