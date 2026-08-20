//! SISU (Sign In and Sign Up) フロー
//!
//! Modrinth App と同じ実装方式。
//! 各 Xbox Live リクエストに ECDSA P-256 署名を付与することで
//! リクエストの正当性を証明する (Proof-of-Possession)。
//!
//! フロー概要:
//!   1. EC 鍵ペアを生成 (セッションごと)
//!   2. device.auth.xboxlive.com でdevice token取得 (署名付き)
//!   3. sisu.xboxlive.com/authenticate でセッション + ログイン URL 取得 (署名付き)
//!   4. WebView でユーザーがログイン → OAuth コード取得
//!   5. oauth20_token.srf でコード → アクセストークン交換 (PKCE あり)
//!   6. sisu.xboxlive.com/authorize で XBL トークン取得 (署名付き)
//!   7. 以降は XSTS → Minecraft 認証チェーン (common.rs)

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use rand::rngs::OsRng;
use reqwest::Client;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::auth::{CLIENT_ID, REDIRECT_URI, SCOPE};

// ─────────────────────────────────────────────────────────────────────────────
// 公開型
// ─────────────────────────────────────────────────────────────────────────────

/// WebView ログイン中に保持する SISU セッション状態
pub struct SisuSession {
    /// WebView で開く Microsoft ログイン URL (SISU から払い出される)
    pub login_url: String,
    /// sisu/authorize で使うセッション ID (レスポンスヘッダー X-SessionId)
    pub session_id: Zeroizing<String>,
    /// sisu/authorize で使うdevice token
    pub device_token: Zeroizing<String>,
    /// トークン交換 (oauth20_token.srf) で使う PKCE code_verifier
    pub code_verifier: Zeroizing<String>,
    /// OAuth callback がこの SISU セッションに属することを検証する CSRF state。
    pub oauth_state: Zeroizing<String>,
    /// sisu/authorize リクエストの署名に使う EC 秘密鍵
    pub signing_key: SigningKey,
}

/// SISU authorizationの結果: XBL トークンと UserHash
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SisuXblResult {
    pub xbl_token: String,
    pub uhs: String,
}

#[derive(serde::Deserialize, Zeroize, ZeroizeOnDrop)]
struct DeviceAuthResponse {
    #[serde(rename = "Token")]
    token: String,
}

#[derive(serde::Deserialize, Zeroize, ZeroizeOnDrop)]
struct SisuAuthenticationResponse {
    #[serde(rename = "MsaOauthRedirect")]
    login_url: String,
}

#[derive(serde::Deserialize, Zeroize, ZeroizeOnDrop)]
struct SisuAuthorizeResponse {
    #[serde(rename = "UserToken")]
    user_token: SisuUserToken,
}

#[derive(serde::Deserialize, Zeroize, ZeroizeOnDrop)]
struct SisuUserToken {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: SisuDisplayClaims,
}

#[derive(serde::Deserialize, Zeroize, ZeroizeOnDrop)]
struct SisuDisplayClaims {
    xui: Vec<SisuXui>,
}

#[derive(serde::Deserialize, Zeroize, ZeroizeOnDrop)]
struct SisuXui {
    uhs: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// 公開関数
// ─────────────────────────────────────────────────────────────────────────────

/// SISU セッションを開始する。
/// 戻り値の `login_url` を WebView で開き、ユーザーにログインさせる。
pub async fn start_session() -> Result<SisuSession, String> {
    let client = Client::new();

    // EC 鍵ペアを生成 (セッションごとに新規作成)
    let signing_key = SigningKey::random(&mut OsRng);
    let device_id = Uuid::new_v4();

    // ① device token取得
    let device_token = device_authenticate(&client, &signing_key, device_id).await?;
    log::info!("SISU: device token acquired");

    // ② PKCE challenge/verifier 生成
    let (code_verifier, code_challenge) = generate_pkce();

    // ③ SISU セッション取得 → ログイン URL を得る
    let (session_id, login_url, oauth_state) =
        sisu_authenticate(&client, &signing_key, &device_token, &code_challenge).await?;
    log::info!("SISU: session acquired");

    Ok(SisuSession {
        login_url,
        session_id: Zeroizing::new(session_id),
        device_token: Zeroizing::new(device_token),
        code_verifier: Zeroizing::new(code_verifier),
        oauth_state: Zeroizing::new(oauth_state),
        signing_key,
    })
}

/// SISU authorizationを実行し XBL トークンを取得する。
/// `access_token`: oauth20_token.srf で取得した Microsoft アクセストークン
pub async fn authorize(
    client: &Client,
    access_token: &str,
    session: &SisuSession,
) -> Result<SisuXblResult, String> {
    sisu_authorize(client, access_token, session).await
}

// ─────────────────────────────────────────────────────────────────────────────
// 内部: ① デバイス認証
// ─────────────────────────────────────────────────────────────────────────────

async fn device_authenticate(
    client: &Client,
    signing_key: &SigningKey,
    device_id: Uuid,
) -> Result<String, String> {
    let (x, y) = public_key_coords(signing_key)?;

    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "ProofOfPossession",
            // Xbox Live は UUID を大文字 + 中括弧で受け取る
            "Id": format!("{{{}}}", device_id.to_string().to_uppercase()),
            "DeviceType": "Win32",
            "Version": "10.16.0",
            "ProofKey": {
                "kty": "EC",
                "x": x,
                "y": y,
                "crv": "P-256",
                "alg": "ES256",
                "use": "sig"
            }
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });

    let body_bytes = serde_json::to_vec(&body)
        .map_err(|e| format!("device auth request build failed: {}", e))?;
    let ts = windows_filetime();
    let sig = compute_signature(&body_bytes, "/device/authenticate", ts, None, signing_key);

    let res = client
        .post("https://device.auth.xboxlive.com/device/authenticate")
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Accept", "application/json")
        .header("x-xbl-contract-version", "1")
        .header("Signature", sig)
        .body(body_bytes)
        .send()
        .await
        .map_err(|e| format!("device auth request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        return Err(format!("device auth failed: {}", status));
    }

    let mut data: DeviceAuthResponse = res
        .json()
        .await
        .map_err(|e| format!("device auth response parse failed: {}", e))?;

    if data.token.is_empty() {
        return Err("device token was not found".to_string());
    }
    Ok(std::mem::take(&mut data.token))
}

// ─────────────────────────────────────────────────────────────────────────────
// 内部: ③ SISU 認証 (ログイン URL 取得)
// ─────────────────────────────────────────────────────────────────────────────

async fn sisu_authenticate(
    client: &Client,
    signing_key: &SigningKey,
    device_token: &str,
    code_challenge: &str,
) -> Result<(String, String, String), String> {
    let state = generate_random_state();

    let body = serde_json::json!({
        "AppId": CLIENT_ID,
        "DeviceToken": device_token,
        "Offers": [SCOPE],
        "Query": {
            "code_challenge": code_challenge,
            "code_challenge_method": "S256",
            "state": state,
            // アカウント選択画面を常に表示 (アカウント追加時にも別アカウントを選べる)
            "prompt": "select_account"
        },
        "RedirectUri": REDIRECT_URI,
        "Sandbox": "RETAIL",
        "TokenType": "code",
        "TitleId": "1794566092"  // Minecraft Java Edition の Xbox タイトル ID
    });

    let body_bytes =
        serde_json::to_vec(&body).map_err(|e| format!("SISU auth request build failed: {}", e))?;
    let ts = windows_filetime();
    let sig = compute_signature(&body_bytes, "/authenticate", ts, None, signing_key);

    let res = client
        .post("https://sisu.xboxlive.com/authenticate")
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Accept", "application/json")
        .header("x-xbl-contract-version", "1")
        .header("Signature", sig)
        .body(body_bytes)
        .send()
        .await
        .map_err(|e| format!("SISU auth request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        return Err(format!("SISU auth failed: {}", status));
    }

    // セッション ID はレスポンスヘッダーから取得
    let session_id = res
        .headers()
        .get("X-SessionId")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| "X-SessionId headerwas not found".to_string())?;

    let mut data: SisuAuthenticationResponse = res
        .json()
        .await
        .map_err(|e| format!("SISU auth response parse failed: {}", e))?;

    if data.login_url.is_empty() {
        return Err("MsaOauthRedirect was not found".to_string());
    }
    let login_url = std::mem::take(&mut data.login_url);

    Ok((session_id, login_url, state))
}

// ─────────────────────────────────────────────────────────────────────────────
// 内部: ⑥ SISU authorization (XBL トークン取得)
// ─────────────────────────────────────────────────────────────────────────────

async fn sisu_authorize(
    client: &Client,
    access_token: &str,
    session: &SisuSession,
) -> Result<SisuXblResult, String> {
    let (x, y) = public_key_coords(&session.signing_key)?;

    let body = serde_json::json!({
        "AccessToken": format!("t={}", access_token),
        "AppId": CLIENT_ID,
        "DeviceToken": session.device_token.as_str(),
        "ProofKey": {
            "kty": "EC",
            "x": x,
            "y": y,
            "crv": "P-256",
            "alg": "ES256",
            "use": "sig"
        },
        "Sandbox": "RETAIL",
        "SessionId": session.session_id.as_str(),
        "SiteName": "user.auth.xboxlive.com",
        "RelyingParty": "http://xboxlive.com",
        "UseModernGamertag": true
    });

    let body_bytes = serde_json::to_vec(&body)
        .map_err(|e| format!("SISU authorize request build failed: {}", e))?;
    let ts = windows_filetime();
    // sisu/authorize は x-xbl-contract-version ヘッダーを送らない
    let sig = compute_signature(&body_bytes, "/authorize", ts, None, &session.signing_key);

    let res = client
        .post("https://sisu.xboxlive.com/authorize")
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Accept", "application/json")
        .header("Signature", sig)
        .body(body_bytes)
        .send()
        .await
        .map_err(|e| format!("SISU authorize request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        return Err(format!("SISU authorize failed: {}", status));
    }

    let mut data: SisuAuthorizeResponse = res
        .json()
        .await
        .map_err(|e| format!("SISU authorize response parse failed: {}", e))?;

    // sisu/authorize のレスポンス構造 (Modrinth App と同じ):
    // {
    //   "UserToken":  { "Token": "...", "DisplayClaims": { "xui": [{ "uhs": "..." }] } },
    //   "TitleToken": { "Token": "...", ... }
    // }
    // UserToken が XSTS に渡す XBL トークン。
    if data.user_token.token.is_empty() {
        return Err("SISU authorization did not include UserToken.Token".to_string());
    }
    let xbl_token = std::mem::take(&mut data.user_token.token);
    let uhs = data
        .user_token
        .display_claims
        .xui
        .first_mut()
        .map(|claim| std::mem::take(&mut claim.uhs))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "SISU authorization did not include UserToken.DisplayClaims.xui[0].uhs".to_string()
        })?;

    Ok(SisuXblResult { xbl_token, uhs })
}

// ─────────────────────────────────────────────────────────────────────────────
// ユーティリティ
// ─────────────────────────────────────────────────────────────────────────────

/// EC P-256 公開鍵の x, y 座標を Base64URL (no-pad) でエンコードして返す。
fn public_key_coords(signing_key: &SigningKey) -> Result<(String, String), String> {
    let point = signing_key.verifying_key().to_encoded_point(false); // uncompressed
    let x = URL_SAFE_NO_PAD.encode(point.x().ok_or("failed to get P-256 x coordinate")?);
    let y = URL_SAFE_NO_PAD.encode(point.y().ok_or("failed to get P-256 y coordinate")?);
    Ok((x, y))
}

/// Unix タイムスタンプを Windows FILETIME に変換する。
/// Windows FILETIME: 1601-01-01 からの 100 ナノ秒単位の整数
fn windows_filetime() -> u64 {
    let unix_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // 1601-01-01 〜 1970-01-01 の秒数 = 11644473600
    (unix_secs + 11_644_473_600) * 10_000_000
}

/// Xbox Live 署名リクエストの `Signature` ヘッダー値を生成する。
///
/// # 署名対象バッファの構造 (Modrinth App と同じ)
/// ```text
/// version(4B) | 0x00 | timestamp(8B) | 0x00 |
/// "POST" | 0x00 | url_path | 0x00 | authorization | 0x00 | body | 0x00
/// ```
///
/// # ヘッダー値の構造
/// ```text
/// version(4B) | timestamp(8B) | r(32B) | s(32B)  →  Base64 Standard エンコード
/// ```
fn compute_signature(
    body: &[u8],
    url_path: &str,
    timestamp: u64,
    authorization: Option<&str>,
    signing_key: &SigningKey,
) -> String {
    let mut buf: Vec<u8> = Vec::with_capacity(body.len() + 128);
    buf.extend_from_slice(&1_u32.to_be_bytes()); // policy version = 1
    buf.push(0u8);
    buf.extend_from_slice(&timestamp.to_be_bytes());
    buf.push(0u8);
    buf.extend_from_slice(b"POST");
    buf.push(0u8);
    buf.extend_from_slice(url_path.as_bytes());
    buf.push(0u8);
    buf.extend_from_slice(authorization.unwrap_or("").as_bytes());
    buf.push(0u8);
    buf.extend_from_slice(body);
    buf.push(0u8);

    // Signer::sign() は内部で SHA-256 ハッシュを計算してから署名する
    let sig: Signature = signing_key.sign(&buf);
    let sig_bytes = sig.to_bytes(); // 64 bytes: r (32) || s (32)

    let mut out: Vec<u8> = Vec::with_capacity(44);
    out.extend_from_slice(&1_i32.to_be_bytes()); // policy version
    out.extend_from_slice(&timestamp.to_be_bytes());
    out.extend_from_slice(&sig_bytes);

    STANDARD.encode(&out)
}

/// PKCE (code_verifier, code_challenge) ペアを生成する。
/// code_challenge = BASE64URL(SHA-256(code_verifier))
pub fn generate_pkce() -> (String, String) {
    let verifier_bytes: [u8; 32] = rand::random();
    let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn generate_random_state() -> String {
    let bytes: [u8; 16] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}
