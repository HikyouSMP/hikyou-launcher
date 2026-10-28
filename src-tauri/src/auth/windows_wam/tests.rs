use super::*;

#[test]
fn token_requests_do_not_bypass_the_account_picker_or_force_credentials() {
    // Local WinRT objects only: no account discovery, network or sign-in UI.
    let icon = windows::Foundation::Uri::CreateUri(&HSTRING::from("https://example.invalid/icon"))
        .unwrap();
    let provider = WebAccountProvider::CreateWebAccountProvider(
        &HSTRING::from("test-provider"),
        &HSTRING::from("Test"),
        &icon,
    )
    .unwrap();
    let request = create_request(&provider, "test-client").unwrap();
    assert_eq!(
        request.PromptType().unwrap(),
        WebTokenRequestPromptType::Default
    );
    assert_eq!(request.Scope().unwrap(), HSTRING::from(SCOPE));
    let properties = request.Properties().unwrap();
    assert!(!properties.HasKey(&HSTRING::from("Client_uiflow")).unwrap());
    assert_eq!(
        properties.Lookup(&HSTRING::from("api-version")).unwrap(),
        HSTRING::from("2.0")
    );
    assert_eq!(
        properties.Lookup(&HSTRING::from("oauth2_batch")).unwrap(),
        HSTRING::from("1")
    );
}

#[test]
fn tokens_are_bounded_and_ambiguous_responses_are_rejected() {
    let parse = |raw: &str| parse_token(Zeroizing::new(raw.to_string()));
    assert_eq!(&**parse("opaque-token").unwrap(), "opaque-token");
    assert_eq!(
        &**parse("access_token=encoded%2Btoken&expires_in=3600").unwrap(),
        "encoded+token"
    );
    assert_eq!(
        &**parse("token_type=Bearer&access_token=encoded%2Btoken").unwrap(),
        "encoded+token"
    );
    for invalid in [
        "",
        "access_token=",
        "access_token=a&access_token=b",
        "{\"token\":\"x\"}",
        "a&b",
        "a\nb",
        "access_token=a%00b",
    ] {
        assert!(parse(invalid).is_err());
    }
    assert!(parse(&"a".repeat(MAX_TOKEN_BYTES + 1)).is_err());
}

#[test]
fn all_provider_outcomes_fail_closed_except_success() {
    assert!(check_status(WebTokenRequestStatus::Success).is_ok());
    for status in [
        WebTokenRequestStatus::UserCancel,
        WebTokenRequestStatus::AccountSwitch,
        WebTokenRequestStatus::UserInteractionRequired,
        WebTokenRequestStatus::AccountProviderNotAvailable,
        WebTokenRequestStatus::ProviderError,
        WebTokenRequestStatus(999),
    ] {
        assert!(check_status(status).is_err());
    }
    assert_eq!(
        check_status(WebTokenRequestStatus::UserCancel).unwrap_err(),
        session::CANCELLED
    );
}

#[test]
fn silent_auth_cannot_substitute_another_account() {
    assert!(verify_account("account-a", "account-a").is_ok());
    assert!(verify_account("account-a", "account-b").is_err());
    assert!(verify_account("", "").is_err());
    assert!(!valid_reference("a\0b"));
    assert!(!valid_reference(&"a".repeat(MAX_ACCOUNT_BYTES + 1)));
    assert_ne!(account_key("account-a"), account_key("account-b"));
}

#[test]
fn legacy_or_mismatched_records_require_login_before_any_os_request() {
    let mut saved = StoredAuth {
        access_token: "minecraft-secret".into(),
        refresh_token: None,
        microsoft_account_key: Some(account_key("os-reference")),
        microsoft_account_ref: Some("os-reference".into()),
        microsoft_client_id: Some(super::super::CLIENT_ID.into()),
        expires_at: 0,
        username: Some("Player".into()),
        uuid: Some("0123456789abcdef0123456789abcdef".into()),
    };
    assert!(saved_reference(&saved, super::super::CLIENT_ID).is_ok());
    assert!(saved_reference(&saved, "another-client").is_err());
    saved.microsoft_account_key = Some("old-msal-account-hash".into());
    assert!(saved_reference(&saved, super::super::CLIENT_ID).is_err());
    saved.microsoft_account_ref = None;
    assert!(saved_reference(&saved, super::super::CLIENT_ID).is_err());
}
