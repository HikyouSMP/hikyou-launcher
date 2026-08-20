//! Xbox Live → XSTS → Minecraft 認証チェーンの共通実装
//! browser_flow から利用される。

use crate::auth::storage::{StoredAuth, save_auth};
use reqwest::{Client, StatusCode, header::RETRY_AFTER};
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
    pub skins: Vec<MinecraftSkin>,
    pub capes: Vec<MinecraftCape>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MinecraftSkin {
    pub id: String,
    pub state: String,
    pub url: String,
    pub variant: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MinecraftCape {
    pub id: String,
    pub state: String,
    pub url: String,
    pub alias: String,
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
}

// ────────────────────────────────────────────────────────────────────────────
// 公開関数
// ────────────────────────────────────────────────────────────────────────────

/// XBL トークンから Minecraft プロファイルまでの認証チェーンを実行し、
/// 認証情報をストレージに保存して `StoredAuth` を返す。
///
/// SISU フローで XBL トークンを取得した後に呼ぶ。
///
/// # フロー
/// XBL Token → XSTS → Minecraft Token → Profile → 保存
pub async fn complete_from_xbl(
    xbl_token: &str,
    uhs: &str,
    ms_refresh_token: Option<String>,
    expires_in: u64,
) -> Result<StoredAuth, String> {
    let xsts = authenticate_with_xsts(xbl_token).await?;
    log::info!("XSTS authentication complete");

    let mc_token = authenticate_with_minecraft(uhs, &xsts.token).await?;
    log::info!("Minecraft authentication complete");

    let profile = get_minecraft_profile(&mc_token).await?;
    log::info!("Minecraft profile acquired: {}", profile.name);

    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_add(expires_in);

    let stored = StoredAuth {
        access_token: mc_token,
        refresh_token: ms_refresh_token,
        expires_at,
        username: Some(profile.name),
        uuid: Some(profile.id),
    };

    save_auth(&stored).await?;
    Ok(stored)
}

// ────────────────────────────────────────────────────────────────────────────
// 内部関数
// ────────────────────────────────────────────────────────────────────────────

async fn authenticate_with_xbox(ms_access_token: &str) -> Result<XblInfo, String> {
    let client = Client::new();
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            // MBI_SSL スコープ（Windows Live / login.live.com 経由）のトークンは t= プレフィックスを使う。
        // Azure AD / login.microsoftonline.com 経由のトークンは d= プレフィックス。
        "RpsTicket": format!("t={}", ms_access_token)
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });

    let res = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Xbox Live request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        return Err(format!("Xbox Live authentication failed: {}", status));
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
        .ok_or("Xbox Live response did not include uhs")?;

    Ok(XblInfo {
        token: std::mem::take(&mut data.token),
        uhs,
    })
}

async fn authenticate_with_xsts(xbl_token: &str) -> Result<XblResponse, String> {
    let client = Client::new();
    let body = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });

    let res = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
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

async fn authenticate_with_minecraft(uhs: &str, xsts_token: &str) -> Result<String, String> {
    let client = Client::new();
    let body = serde_json::json!({
        "identityToken": format!("XBL3.0 x={};{}", uhs, xsts_token)
    });
    let endpoint = "https://api.minecraftservices.com/authentication/login_with_xbox";
    let mut last_error = String::new();
    const MAX_ATTEMPTS: u8 = 2;

    for attempt in 1..=MAX_ATTEMPTS {
        let res = client
            .post(endpoint)
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Minecraft authentication request failed: {}", e))?;

        let status = res.status();
        if status.is_success() {
            let mut data: MinecraftTokenResponse = res
                .json()
                .await
                .map_err(|e| format!("Minecraft authentication response parse failed: {}", e))?;

            if data.access_token.is_empty() {
                return Err("Minecraft access token was not found".to_string());
            }
            return Ok(std::mem::take(&mut data.access_token));
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

async fn get_minecraft_profile(mc_token: &str) -> Result<MinecraftProfile, String> {
    let client = Client::new();
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
pub async fn refresh_auth_chain(refresh_token: &str) -> Result<StoredAuth, String> {
    let client = Client::new();

    // Windows Live クライアント (00000000402b5328) のリフレッシュは oauth20_token.srf を使う
    let res = client
        .post("https://login.live.com/oauth20_token.srf")
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", crate::auth::CLIENT_ID),
            ("refresh_token", refresh_token),
            ("scope", "service::user.auth.xboxlive.com::MBI_SSL"),
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

    // リフレッシュはインタラクティブな SISU フローが不要なため旧 XBL 方式を使用
    let xbl = authenticate_with_xbox(&token.access_token).await?;
    log::info!("Xbox Live re-authentication complete");

    complete_from_xbl(
        &xbl.token,
        &xbl.uhs,
        std::mem::take(&mut token.refresh_token),
        token.expires_in,
    )
    .await
}
