//! Microsoft OAuth 2.0 Authorization Code Flow (WebView + SISU フロー)
//!
//! Modrinth App と同じ方式:
//! 1. SISU セッション開始 → Xbox Live 署名付きでdevice token + ログイン URL 取得
//! 2. ログイン URL を WebView で開く (SISU が生成した URL には prompt=select_account を含む)
//! 3. oauth20_desktop.srf へのリダイレクトを on_navigation で検出してコードを取り出す
//! 4. コード → アクセストークン交換 (PKCE code_verifier 付き)
//! 5. SISU authorization → XBL トークン取得 (Xbox Live 署名付き)
//! 6. 以降は common.rs の XSTS → Minecraft 認証チェーン

use crate::auth::{CLIENT_ID, REDIRECT_URI, TokenResponse, common, sisu, storage::StoredAuth};
use reqwest::Client;

const SCOPE: &str = "service::user.auth.xboxlive.com::MBI_SSL";
const TOKEN_URL: &str = "https://login.live.com/oauth20_token.srf";

// ─────────────────────────────────────────────────────────────────────────────
// 公開関数
// ─────────────────────────────────────────────────────────────────────────────

/// SISU セッションを開始し、WebView で開くログイン URL を返す。
/// lib.rs の start_webview_login コマンドが最初に呼ぶ。
pub async fn start_sisu_session() -> Result<sisu::SisuSession, String> {
    sisu::start_session().await
}

/// SISU セッションを使って認証を完了する。
///
/// - `code`: WebView の on_navigation で oauth20_desktop.srf から取り出した OAuth コード
/// - `session`: start_sisu_session() の戻り値
pub async fn complete_with_sisu(
    code: &str,
    session: sisu::SisuSession,
) -> Result<StoredAuth, String> {
    let client = Client::new();

    // コード → Microsoft アクセストークン (PKCE code_verifier 付き)
    let mut ms_token = exchange_code(code, session.code_verifier.as_str(), &client).await?;
    log::info!("Microsoft token acquired (SISU Flow)");

    // SISU authorization → XBL トークン + uhs
    let xbl = sisu::authorize(&client, &ms_token.access_token, &session).await?;
    log::info!("Xbox Live (SISU) authentication complete");

    // XBL → XSTS → Minecraft 認証チェーン
    common::complete_from_xbl(
        &xbl.xbl_token,
        &xbl.uhs,
        std::mem::take(&mut ms_token.refresh_token),
        ms_token.expires_in,
    )
    .await
}

// ─────────────────────────────────────────────────────────────────────────────
// 内部: コード交換 (PKCE 付き)
// ─────────────────────────────────────────────────────────────────────────────

async fn exchange_code(
    code: &str,
    code_verifier: &str,
    client: &Client,
) -> Result<TokenResponse, String> {
    let params = [
        ("client_id", CLIENT_ID),
        ("scope", SCOPE),
        ("code", code),
        ("redirect_uri", REDIRECT_URI),
        ("grant_type", "authorization_code"),
        ("code_verifier", code_verifier), // PKCE: SISU で送った code_challenge に対応する verifier
    ];

    let res = client
        .post(TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("token exchange request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        return Err(format!("token exchange failed: {}", status));
    }

    res.json()
        .await
        .map_err(|e| format!("token response parse failed: {}", e))
}
