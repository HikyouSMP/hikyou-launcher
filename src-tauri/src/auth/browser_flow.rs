//! Modern Microsoft public-client OAuth with authorization code + PKCE.
//!
//! This is the non-Windows fallback. Windows uses MSAL/WAM, while both paths
//! share Hikyou's client identity and the Xbox/Minecraft exchange in common.rs.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use reqwest::Client;
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::auth::{
    AUTHORITY, CLIENT_ID, REDIRECT_URI, SCOPE, TokenResponse, common, storage::StoredAuth,
};

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct OAuthSession {
    pub login_url: String,
    pub code_verifier: String,
    pub oauth_state: String,
}

pub fn start_session() -> Result<OAuthSession, String> {
    let client_id = crate::auth::configured_client_id()?;
    let code_verifier = random_base64url(64);
    let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
    let oauth_state = random_base64url(32);
    let mut login_url = url::Url::parse(&format!("{AUTHORITY}/authorize"))
        .map_err(|error| format!("Microsoft authorization URL is invalid: {error}"))?;
    login_url
        .query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("scope", SCOPE)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &oauth_state)
        .append_pair("prompt", "select_account");

    Ok(OAuthSession {
        login_url: login_url.into(),
        code_verifier,
        oauth_state,
    })
}

pub async fn complete(code: &str, session: OAuthSession) -> Result<StoredAuth, String> {
    let client = Client::new();
    let mut token = exchange_code(code, &session.code_verifier, &client).await?;
    log::info!("Microsoft token acquired (OAuth PKCE)");
    common::complete_from_microsoft(
        &token.access_token,
        std::mem::take(&mut token.refresh_token),
        None,
    )
    .await
}

async fn exchange_code(
    code: &str,
    code_verifier: &str,
    client: &Client,
) -> Result<TokenResponse, String> {
    let client_id = crate::auth::configured_client_id()?;
    let response = client
        .post(format!("{AUTHORITY}/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
            ("client_id", client_id),
            ("code_verifier", code_verifier),
        ])
        .send()
        .await
        .map_err(|error| format!("token exchange request failed: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("token exchange failed: {}", response.status()));
    }
    response
        .json()
        .await
        .map_err(|error| format!("token response parse failed: {error}"))
}

fn random_base64url(length: usize) -> String {
    let mut bytes = zeroize::Zeroizing::new(vec![0_u8; length]);
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes.as_slice())
}

#[cfg(test)]
mod tests {
    use super::start_session;

    #[test]
    fn authorization_request_uses_hikyou_public_client_pkce_contract() {
        if crate::auth::CLIENT_ID.is_empty() {
            assert!(start_session().is_err());
            return;
        }
        let session = start_session().unwrap();
        let url = url::Url::parse(&session.login_url).unwrap();
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();

        assert_eq!(url.host_str(), Some("login.microsoftonline.com"));
        assert_eq!(
            params.get("client_id").map(String::as_str),
            Some(crate::auth::CLIENT_ID)
        );
        assert_eq!(
            params.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert_eq!(
            params.get("scope").map(String::as_str),
            Some(crate::auth::SCOPE)
        );
        assert!(!session.code_verifier.is_empty());
        assert!(!session.oauth_state.is_empty());
    }
}
