//! auth モジュール
//! Microsoft/Xbox Live/Minecraft の認証フローを管理する。

pub(crate) mod browser_flow;
mod common;
pub(crate) mod crypto;
pub(crate) mod sisu;
mod storage;

// 公開API
pub use storage::{
    PublicAuth, StoredAuth, delete_account_auth, delete_auth, load_account_auth, load_auth,
    save_auth,
};

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

// ─────────────────────────────────────────────────────────────────────────────
// Microsoft Windows Live クライアント ID
//
// 00000000402b5328 は Microsoft が公開している Windows Live API クライアント ID。
// Modrinth, MultiMC など多くの Minecraft ランチャーで使用されている公開値。
// service::user.auth.xboxlive.com::MBI_SSL スコープと組み合わせることで
// Xbox Live ブランドのログイン画面が表示される。
// ─────────────────────────────────────────────────────────────────────────────
pub const CLIENT_ID: &str = "00000000402b5328";
pub const SCOPE: &str = "service::user.auth.xboxlive.com::MBI_SSL";

// Redirect URI: Microsoft の desktop redirect（登録不要の特別なリダイレクト先）
pub const REDIRECT_URI: &str = "https://login.live.com/oauth20_desktop.srf";

/// Microsoft から受け取る OAuth トークンレスポンス
/// OAuth 応答は短命でもアクセス/リフレッシュトークンを含むため、Drop 時にゼロ化する。
#[derive(Debug, Deserialize, Serialize, Clone, Zeroize, ZeroizeOnDrop)]
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
pub async fn ensure_fresh_auth() -> Result<StoredAuth, String> {
    let auth = load_auth().await?;

    if auth.is_valid() {
        return Ok(auth);
    }

    log::info!("Token has expired. Attempting refresh...");

    let refresh_token = auth.refresh_token.as_deref().ok_or(
        "The token has expired and no refresh token is available. Please sign in again."
            .to_string(),
    )?;

    match common::refresh_auth_chain(refresh_token).await {
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
