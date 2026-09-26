//! Xbox Live → XSTS → Minecraft 認証チェーンの共通実装
//! browser_flow から利用される。

use crate::auth::storage::{StoredAuth, save_auth};
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
    expires_in: u64,
}

// ────────────────────────────────────────────────────────────────────────────
// 公開関数
// ────────────────────────────────────────────────────────────────────────────

/// XBL トークンから Minecraft プロファイルまでの認証チェーンを実行し、
/// 認証情報をストレージに保存して `StoredAuth` を返す。
///
/// Called after Microsoft-token exchange has produced an Xbox user token.
///
/// # フロー
/// XBL Token → XSTS → Minecraft Token → Profile → 保存
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
        microsoft_client_id: Some(crate::auth::CLIENT_ID.to_string()),
        expires_at,
        username: Some(profile.name),
        uuid: Some(profile.id),
    };

    save_auth(&stored).await?;
    Ok(stored)
}

/// Complete the public-client Xbox/Minecraft chain from a modern Microsoft
/// access token. The token is accepted by Xbox as a `d=` RPS ticket.
pub async fn complete_from_microsoft(
    ms_access_token: &str,
    ms_refresh_token: Option<String>,
    microsoft_account_key: Option<String>,
) -> Result<StoredAuth, String> {
    let xbl = authenticate_with_xbox(ms_access_token).await?;
    log::info!("Xbox Live authentication complete");
    complete_from_xbl(
        &xbl.token,
        &xbl.uhs,
        ms_refresh_token,
        microsoft_account_key,
    )
    .await
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
    let client = Client::new();
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

pub(crate) async fn upload_minecraft_skin(
    mc_token: &str,
    variant: &str,
    filename: &str,
    bytes: Vec<u8>,
) -> Result<MinecraftProfile, String> {
    if !matches!(variant, "classic" | "slim") {
        return Err("skin variant must be classic or slim".to_string());
    }
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(filename.to_string())
        .mime_str("image/png")
        .map_err(|e| format!("failed to prepare skin upload: {e}"))?;
    let form = reqwest::multipart::Form::new()
        .text("variant", variant.to_string())
        .part("file", part);
    let response = Client::new()
        .post("https://api.minecraftservices.com/minecraft/profile/skins")
        .bearer_auth(mc_token)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("skin upload request failed: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Minecraft rejected the skin upload: {status}"));
    }
    get_minecraft_profile(mc_token).await
}

pub(crate) async fn set_minecraft_skin_variant(
    mc_token: &str,
    variant: &str,
) -> Result<MinecraftProfile, String> {
    if !matches!(variant, "classic" | "slim") {
        return Err("skin variant must be classic or slim".to_string());
    }
    let profile = get_minecraft_profile(mc_token).await?;
    let active_skin = profile
        .skins
        .iter()
        .find(|skin| skin.state.eq_ignore_ascii_case("active"))
        .or_else(|| profile.skins.first())
        .ok_or_else(|| "Minecraft profile does not have a skin".to_string())?;
    let mut url = reqwest::Url::parse(&active_skin.url)
        .map_err(|_| "Minecraft returned an invalid skin texture URL".to_string())?;
    if url.host_str() != Some("textures.minecraft.net") {
        return Err("Minecraft returned an untrusted skin texture host".to_string());
    }
    url.set_scheme("https")
        .map_err(|_| "Minecraft skin texture URL could not be secured".to_string())?;

    let mut response = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("failed to prepare skin download: {e}"))?
        .get(url)
        .send()
        .await
        .map_err(|e| format!("skin texture download failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Minecraft skin texture download failed: {}",
            response.status()
        ));
    }

    const MAX_SKIN_BYTES: usize = 4 * 1024 * 1024;
    let initial_capacity = response
        .content_length()
        .unwrap_or(0)
        .min(MAX_SKIN_BYTES as u64) as usize;
    let mut bytes = Vec::with_capacity(initial_capacity);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("skin texture download failed: {e}"))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_SKIN_BYTES {
            return Err("skin texture exceeds 4 MiB".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .map_err(|_| "Minecraft skin texture was not a valid PNG image".to_string())?;
    if !matches!((image.width(), image.height()), (64, 64) | (64, 32)) {
        return Err("Minecraft skin texture had unsupported dimensions".to_string());
    }
    upload_minecraft_skin(mc_token, variant, "current-skin.png", bytes).await
}

pub(crate) async fn set_minecraft_cape(
    mc_token: &str,
    cape_id: Option<&str>,
) -> Result<MinecraftProfile, String> {
    let client = Client::new();
    let request = match cape_id {
        Some(id) => client
            .put("https://api.minecraftservices.com/minecraft/profile/capes/active")
            .bearer_auth(mc_token)
            .json(&serde_json::json!({ "capeId": id })),
        None => client
            .delete("https://api.minecraftservices.com/minecraft/profile/capes/active")
            .bearer_auth(mc_token),
    };
    let response = request
        .send()
        .await
        .map_err(|e| format!("cape selection request failed: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Minecraft rejected the cape selection: {status}"));
    }
    get_minecraft_profile(mc_token).await
}

// ────────────────────────────────────────────────────────────────────────────
// トークンリフレッシュ
// ────────────────────────────────────────────────────────────────────────────

/// Microsoftのリフレッシュトークンを使って新しいアクセストークンを取得し、
/// 認証チェーン全体を再実行して新しい StoredAuth を返す。
#[cfg(not(target_os = "windows"))]
pub async fn refresh_auth_chain(refresh_token: &str) -> Result<StoredAuth, String> {
    let client = Client::new();
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
}

#[cfg(test)]
mod wire_contract_tests {
    use super::exact_json_post;
    use reqwest::header::CONTENT_TYPE;

    #[test]
    fn xbox_json_media_type_has_no_implicit_parameters() {
        let request = exact_json_post(&reqwest::Client::new(), "https://example.invalid")
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
