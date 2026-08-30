//! auth モジュール
//! Microsoft/Xbox Live/Minecraft の認証フローを管理する。

#[cfg(not(target_os = "windows"))]
pub(crate) mod browser_flow;
mod common;
pub(crate) mod crypto;
mod storage;
#[cfg(target_os = "windows")]
pub(crate) mod windows_broker;

// 公開API
pub use storage::{
    PublicAuth, StoredAuth, delete_account_auth, delete_auth, load_account_auth, load_auth,
    save_auth,
};

#[cfg(not(target_os = "windows"))]
use serde::{Deserialize, Serialize};
#[cfg(not(target_os = "windows"))]
use zeroize::{Zeroize, ZeroizeOnDrop};

// ─────────────────────────────────────────────────────────────────────────────
// Official builds inject Hikyou's approved public-client registration. The
// identifier is public configuration, but forks must not silently reuse the
// project's operational identity.
// ─────────────────────────────────────────────────────────────────────────────
pub const CLIENT_ID: &str = match option_env!("HIKYOU_MSA_CLIENT_ID") {
    Some(value) => value,
    None => "",
};
#[cfg(not(target_os = "windows"))]
pub const SCOPE: &str = "XboxLive.SignIn XboxLive.offline_access";
#[cfg(not(target_os = "windows"))]
pub const AUTHORITY: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";

#[cfg(not(target_os = "windows"))]
pub const REDIRECT_URI: &str = "http://localhost:8089/callback";

#[cfg(not(target_os = "windows"))]
pub fn configured_client_id() -> Result<&'static str, String> {
    if CLIENT_ID.is_empty() {
        Err(
            "Microsoft authentication is not configured in this build. Set HIKYOU_MSA_CLIENT_ID to an approved public-client registration when building Hikyou Launcher."
                .to_string(),
        )
    } else {
        Ok(CLIENT_ID)
    }
}

/// Microsoft から受け取る OAuth トークンレスポンス
/// OAuth 応答は短命でもアクセス/リフレッシュトークンを含むため、Drop 時にゼロ化する。
#[derive(Debug, Deserialize, Serialize, Clone, Zeroize, ZeroizeOnDrop)]
#[cfg(not(target_os = "windows"))]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
}

/// 保存済み認証を取得し、期限切れならリフレッシュして返す。
/// launch_game など、認証が必要な操作の前に必ず呼ぶこと。
///
/// - トークンが有効 → そのまま返す
/// - 期限切れ + リフレッシュトークンあり → 自動更新して返す
/// - リフレッシュトークンなし or 更新失敗 → Err（再ログインを促す）
pub async fn ensure_fresh_auth(app: &tauri::AppHandle) -> Result<StoredAuth, String> {
    let auth = load_auth().await?;

    if auth.is_valid() {
        return Ok(auth);
    }

    log::info!("Token has expired. Attempting refresh...");

    #[cfg(target_os = "windows")]
    let refreshed = windows_broker::refresh(app, auth.microsoft_account_key.as_deref()).await;

    #[cfg(not(target_os = "windows"))]
    let refreshed = match auth.refresh_token.as_deref() {
        Some(refresh_token) if auth.microsoft_client_id.as_deref() == Some(CLIENT_ID) => {
            common::refresh_auth_chain(refresh_token).await
        }
        _ => Err("This saved login must be upgraded. Please sign in again.".to_string()),
    };

    match refreshed {
        Ok(new_auth) => Ok(new_auth),
        Err(e) => {
            let _ = delete_auth().await;
            Err(format!(
                "Failed to refresh the token. Please sign in again.\nDetails: {}",
                e
            ))
        }
    }
}
