//! Xbox Live → XSTS → Minecraft 認証チェーンの共通実装
//! browser_flow から利用される。

use crate::auth::storage::StoredAuth;
use reqwest::{
    Client, RequestBuilder, StatusCode,
    header::{ACCEPT, CONTENT_TYPE, RETRY_AFTER},
};
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, sleep};
use zeroize::{Zeroize, ZeroizeOnDrop};

// ────────────────────────────────────────────────────────────────────────────
// 公開型
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MinecraftProfile {
    pub id: String,
    pub name: String,
}

// ────────────────────────────────────────────────────────────────────────────
// 内部型 (Serde デシリアライズ用)
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Zeroize, ZeroizeOnDrop)]
struct XblResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XblDisplayClaims,
}

#[derive(Debug, Deserialize, Zeroize, ZeroizeOnDrop)]
struct XblDisplayClaims {
    xui: Vec<XblXui>,
}

#[derive(Debug, Deserialize, Zeroize, ZeroizeOnDrop)]
struct XblXui {
    uhs: String,
}

#[derive(Zeroize, ZeroizeOnDrop)]
struct XblInfo {
    token: String,
    uhs: String,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
struct MinecraftTokenResponse {
    access_token: String,
    expires_in: u64,
}

pub enum MicrosoftExchangeError {
    Rejected,
    Other(String),
}

impl From<String> for MicrosoftExchangeError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl std::fmt::Display for MicrosoftExchangeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected => formatter
                .write_str("Xbox Live rejected the Microsoft access token. Please sign in again."),
            Self::Other(message) => formatter.write_str(message),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// 公開関数
// ────────────────────────────────────────────────────────────────────────────

/// XBL トークンから Minecraft プロファイルまでの認証チェーンを実行し、
/// `StoredAuth` を返す。保存は呼び出し側の有効な認証操作だけが行う。
///
/// Called after Microsoft-token exchange has produced an Xbox user token.
///
/// # フロー
/// XBL Token → XSTS → Minecraft Token → Profile
pub async fn complete_from_xbl(
    xbl_token: &str,
    uhs: &str,
    ms_refresh_token: Option<String>,
    microsoft_account_key: Option<String>,
) -> Result<StoredAuth, String> {
    let xsts = authenticate_with_xsts(xbl_token).await?;
    log::info!("XSTS authentication complete");

    let mut mc_token = authenticate_with_minecraft(uhs, &xsts.token).await?;
    log::info!("Minecraft authentication complete");

    let profile = get_minecraft_profile(&mc_token.access_token).await?;
    log::info!("Minecraft profile acquired: {}", profile.name);

    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_add(mc_token.expires_in);

    let stored = StoredAuth {
        access_token: std::mem::take(&mut mc_token.access_token),
        refresh_token: ms_refresh_token,
        microsoft_account_key,
        microsoft_account_ref: None,
        microsoft_client_id: Some(crate::auth::CLIENT_ID.to_string()),
        expires_at,
        username: Some(profile.name),
        uuid: Some(profile.id),
    };

    Ok(stored)
}

/// Complete the public-client Xbox/Minecraft chain from a modern Microsoft
/// access token. The token is accepted by Xbox as a `d=` RPS ticket.
pub async fn complete_from_microsoft(
    ms_access_token: &str,
    ms_refresh_token: Option<String>,
    microsoft_account_key: Option<String>,
) -> Result<StoredAuth, MicrosoftExchangeError> {
    let xbl = authenticate_with_xbox(ms_access_token).await?;
    log::info!("Xbox Live authentication complete");
    complete_from_xbl(
        &xbl.token,
        &xbl.uhs,
        ms_refresh_token,
        microsoft_account_key,
    )
    .await
    .map_err(Into::into)
}

// ────────────────────────────────────────────────────────────────────────────
// 内部関数
// ────────────────────────────────────────────────────────────────────────────

async fn authenticate_with_xbox(ms_access_token: &str) -> Result<XblInfo, MicrosoftExchangeError> {
    let client = crate::http::client()?;
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={}", ms_access_token)
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });

    let res = exact_json_post(&client, "https://user.auth.xboxlive.com/user/authenticate")
        .header("x-xbl-contract-version", "1")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Xbox Live request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        return Err(if status == StatusCode::UNAUTHORIZED {
            MicrosoftExchangeError::Rejected
        } else {
            MicrosoftExchangeError::Other(format!("Xbox Live authentication failed: {}", status))
        });
    }

    let mut data: XblResponse = res
        .json()
        .await
        .map_err(|e| format!("Xbox Live response parse failed: {}", e))?;

    let uhs = data
        .display_claims
        .xui
        .first_mut()
        .map(|claim| std::mem::take(&mut claim.uhs))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            MicrosoftExchangeError::Other("Xbox Live response did not include uhs".into())
        })?;

    Ok(XblInfo {
        token: std::mem::take(&mut data.token),
        uhs,
    })
}

async fn authenticate_with_xsts(xbl_token: &str) -> Result<XblResponse, String> {
    let client = crate::http::client()?;
    let body = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });

    let res = exact_json_post(&client, "https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("x-xbl-contract-version", "1")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("XSTS request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let body_text = zeroize::Zeroizing::new(res.text().await.unwrap_or_default());

        if body_text.contains("2148916233") {
            return Err("This Microsoft account does not have an Xbox profile. \
                Create a profile at xbox.com, then try again."
                .to_string());
        }
        if body_text.contains("2148916238") {
            return Err("This account requires family consent. \
                Update the family settings from the parent account, then try again."
                .to_string());
        }

        return Err(format!("XSTS authentication failed: {}", status));
    }

    res.json()
        .await
        .map_err(|e| format!("XSTS response parse failed: {}", e))
}

async fn authenticate_with_minecraft(
    uhs: &str,
    xsts_token: &str,
) -> Result<MinecraftTokenResponse, String> {
    let client = crate::http::client()?;
    let body = serde_json::json!({
        "xtoken": format!("XBL3.0 x={};{}", uhs, xsts_token),
        "platform": "PC_LAUNCHER"
    });
    let endpoint = "https://api.minecraftservices.com/launcher/login";
    let mut last_error = String::new();
    const MAX_ATTEMPTS: u8 = 2;

    for attempt in 1..=MAX_ATTEMPTS {
        let res = exact_json_post(&client, endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Minecraft authentication request failed: {}", e))?;

        let status = res.status();
        if status.is_success() {
            let data: MinecraftTokenResponse = res
                .json()
                .await
                .map_err(|e| format!("Minecraft authentication response parse failed: {}", e))?;

            if data.access_token.is_empty() {
                return Err("Minecraft access token was not found".to_string());
            }
            return Ok(data);
        }

        let retry_after = retry_after_delay(res.headers());
        last_error = status.to_string();

        if !is_retryable_minecraft_auth_status(status) || attempt == MAX_ATTEMPTS {
            break;
        }

        let delay = retry_after.unwrap_or(Duration::from_millis(900));
        if delay > Duration::from_secs(3) {
            log::warn!(
                "Minecraft authentication service returned {} with Retry-After {:?}; not retrying automatically",
                status,
                delay
            );
            break;
        }

        log::warn!(
            "Minecraft authentication service returned {} on attempt {}/{}; retrying once",
            status,
            attempt,
            MAX_ATTEMPTS
        );
        sleep(delay).await;
    }

    Err(format!(
        "Minecraft authentication service is temporarily unavailable. \
         Please try again in a moment. Last response: {}",
        last_error
    ))
}

fn exact_json_post(client: &Client, endpoint: &str) -> RequestBuilder {
    client
        .post(endpoint)
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
}

fn is_retryable_minecraft_auth_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

fn retry_after_delay(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?;
    let seconds = value.parse::<u64>().ok()?;
    Some(Duration::from_secs(seconds))
}

pub(crate) async fn get_minecraft_profile(mc_token: &str) -> Result<MinecraftProfile, String> {
    let client = crate::http::client()?;
    let res = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(mc_token)
        .send()
        .await
        .map_err(|e| format!("profile request failed: {}", e))?;

    let status = res.status();
    if !status.is_success() {
        if status.as_u16() == 404 {
            return Err("Minecraft profile was not found. \
                Make sure this account owns Java Edition."
                .to_string());
        }
        return Err(format!("profile fetch failed: {}", status));
    }

    res.json()
        .await
        .map_err(|e| format!("profile response parse failed: {}", e))
}

// ────────────────────────────────────────────────────────────────────────────
// トークンリフレッシュ
// ────────────────────────────────────────────────────────────────────────────

/// Microsoftのリフレッシュトークンを使って新しいアクセストークンを取得し、
/// 認証チェーン全体を再実行して新しい StoredAuth を返す。
#[cfg(not(target_os = "windows"))]
pub async fn refresh_auth_chain(refresh_token: &str) -> Result<StoredAuth, String> {
    let client = crate::http::client()?;
    let client_id = crate::auth::configured_client_id()?;

    let res = client
        .post(format!("{}/token", crate::auth::AUTHORITY))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("scope", crate::auth::SCOPE),
        ])
        .send()
        .await
        .map_err(|e| format!("refresh request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        return Err(format!(
            "token refresh failed: {}\nPlease sign in again.",
            status
        ));
    }

    let mut token: crate::auth::TokenResponse = res
        .json()
        .await
        .map_err(|e| format!("refresh response parse failed: {}", e))?;

    log::info!("Microsoft token refreshed");

    complete_from_microsoft(
        &token.access_token,
        std::mem::take(&mut token.refresh_token),
        None,
    )
    .await
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod wire_contract_tests {
    use super::{MinecraftProfile, exact_json_post};
    use reqwest::header::CONTENT_TYPE;

    #[test]
    fn minecraft_profile_identity_does_not_require_appearance_metadata() {
        let profile: MinecraftProfile = serde_json::from_value(serde_json::json!({
            "id": "0123456789abcdef0123456789abcdef",
            "name": "Player"
        }))
        .unwrap();
        assert_eq!(profile.id, "0123456789abcdef0123456789abcdef");
        assert_eq!(profile.name, "Player");

        let profile: MinecraftProfile = serde_json::from_value(serde_json::json!({
            "id": "0123456789abcdef0123456789abcdef",
            "name": "Player",
            "skins": [{ "id": "unused" }],
            "capes": [{ "id": "unused" }]
        }))
        .unwrap();
        assert_eq!(profile.name, "Player");
        assert_eq!(
            serde_json::to_value(profile).unwrap(),
            serde_json::json!({
                "id": "0123456789abcdef0123456789abcdef",
                "name": "Player"
            })
        );
        assert!(
            serde_json::from_value::<MinecraftProfile>(serde_json::json!({
                "name": "Player"
            }))
            .is_err()
        );
    }

    #[test]
    fn xbox_json_media_type_has_no_implicit_parameters() {
        let request = exact_json_post(&crate::http::client().unwrap(), "https://example.invalid")
            .json(&serde_json::json!({ "test": true }))
            .build()
            .unwrap();

        assert_eq!(
            request
                .headers()
                .get(CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap(),
            "application/json"
        );
    }
}
