//! Shared HTTPS trust policy: OS verification on Windows/macOS, system CA bundle on Linux.

use std::sync::{Arc, OnceLock};

use reqwest::{Client, ClientBuilder};
use rustls::ClientConfig;
use rustls_platform_verifier::BuilderVerifierExt;

fn tls_config() -> Result<ClientConfig, String> {
    static CONFIG: OnceLock<ClientConfig> = OnceLock::new();
    if let Some(config) = CONFIG.get() {
        return Ok(config.clone());
    }
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|error| format!("TLS configuration failed: {error}"))?
            .with_platform_verifier()
            .map_err(|error| format!("System certificate verifier initialization failed: {error}"))?
            .with_no_client_auth();
    Ok(CONFIG.get_or_init(|| config).clone())
}

pub(crate) fn builder() -> Result<ClientBuilder, String> {
    Ok(Client::builder()
        .use_preconfigured_tls(tls_config()?)
        .https_only(true))
}

pub(crate) fn client() -> Result<Client, String> {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client.clone());
    }
    let client = builder()?
        .build()
        .map_err(|error| format!("HTTPS client initialization failed: {error}"))?;
    Ok(CLIENT.get_or_init(|| client).clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use rustls::client::danger::ServerCertVerifier;

    #[test]
    fn clients_initialize_with_the_system_verifier() {
        builder().unwrap().build().unwrap();
        client().unwrap();
    }

    #[tokio::test]
    async fn plaintext_requests_are_rejected_without_connecting() {
        for client in [client().unwrap(), builder().unwrap().build().unwrap()] {
            let error = client.get("http://127.0.0.1:1/").send().await.unwrap_err();
            assert!(error.is_builder(), "unexpected request error: {error}");
        }
    }

    #[test]
    fn untrusted_self_signed_certificate_is_rejected() {
        // Public test certificate only; its private key was discarded, never installed.
        let der = base64::engine::general_purpose::STANDARD.decode(concat!(
            "MIIC6TCCAdGgAwIBAgIJAKM2cMZk+kYeMA0GCSqGSIb3DQEBCwUAMCAxHjAcBgNVBAMTFWhpa3lvdS11bnRydXN0ZWQudGVzdDAgFw0yNTAxMDEwMDAwMDBaGA8yMTI1MDEwMTAwMDAwMFowIDEeMBwGA1UEAxMVaGlreW91LXVudHJ1c3RlZC50ZXN0",
            "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAlWU9ha4JdRCn9DA2p+/tigrR8wXqAj2w5Ezg3d2GrrcNnsUsRu1+zZWVwPyKK105hznHYCS9axUB3D68aYBTT4Dc1eGT4YzMRG3+KqjloHReyD4unWnEPMeM6HUkbRY4R6momBEw68874ADsFB7T3FgnnLkW2prJEESWtIV6siYpB5VdsryVNCkpoaT7RqGb5aQYsC7qzCflpMos9ZRPylX4iaPM88V47xOVDamkb/rq+7+qhLBG8AnWbmSlWocf6/XfUlX/nXYZFF0OOqeetHzxpSNvaSH1lF+dRDbrxtYruGm5dCbxMSNX1th4iTrBGBZ3h8tz40kroX9kt2bvsQIDAQAB",
            "oyQwIjAgBgNVHREEGTAXghVoaWt5b3UtdW50cnVzdGVkLnRlc3QwDQYJKoZIhvcNAQELBQADggEBAAp2HS0hw99y/5lqe3k78sGNTY7fTZ+gtvrE1gVKL0YSQBIPWAuNTRJKW6o4WN//h1jB9dTExro1iztpcsfKlV84ji4sIWGdVFCPci8pl0rGKmC09Mbn3ui6K9FJta6rLbtHCAAcSuk39IpZA0gTc5PIppj3UBeNLxm/ww/GQPIFLko0jqYI8MoZGKNcUDh15fvc1N72khEtWK4IgoKrAbQB3vQSaV+Im0DNSVWjsDMiWM4wLxEd2mAD6yHgta607x3gzGiJVmG/IwUsWR5m8yGQgLqRQLsR1qATZh7B9RhtwUqSNuAwQetMMViIDEIywbriCt1jY1u5ptCb099Tg5U="
        )).unwrap();
        let verifier = rustls_platform_verifier::Verifier::new(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .unwrap();
        let result = verifier.verify_server_cert(
            &rustls::pki_types::CertificateDer::from(der),
            &[],
            &rustls::pki_types::ServerName::try_from("hikyou-untrusted.test").unwrap(),
            &[],
            rustls::pki_types::UnixTime::now(),
        );
        assert!(result.is_err(), "untrusted certificate was accepted");
    }

    #[test]
    fn application_clients_cannot_bypass_the_shared_policy() {
        fn check(directory: &std::path::Path) {
            for entry in std::fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    check(&path);
                } else if path.extension().is_some_and(|extension| extension == "rs")
                    && path.file_name().unwrap() != "http.rs"
                {
                    let source = std::fs::read_to_string(&path).unwrap();
                    for bypass in [
                        "Client::new(",
                        "Client::builder(",
                        "ClientBuilder::new(",
                        "reqwest::get(",
                    ] {
                        assert!(
                            !source.contains(bypass),
                            "TLS policy bypass {bypass} in {}",
                            path.display()
                        );
                    }
                }
            }
        }
        check(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    }

    #[tokio::test]
    #[ignore = "requires public network access; no credentials or account data are sent"]
    async fn public_https_and_certificate_rejection() {
        let client = builder()
            .unwrap()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap();
        for url in [
            "https://api.minecraftservices.com/",
            "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json",
            "https://api.modrinth.com/v2/tag/game_version",
        ] {
            client.head(url).send().await.unwrap();
        }
        for url in [
            "https://self-signed.badssl.com/",
            "https://expired.badssl.com/",
            "https://wrong.host.badssl.com/",
        ] {
            let error = client.head(url).send().await.unwrap_err();
            assert!(
                error.is_connect() && !error.is_timeout(),
                "unexpected TLS error for {url}: {error}"
            );
            fn rejected_certificate(error: &(dyn std::error::Error + 'static)) -> bool {
                matches!(
                    error.downcast_ref::<rustls::Error>(),
                    Some(rustls::Error::InvalidCertificate(_))
                ) || error
                    .downcast_ref::<std::io::Error>()
                    .and_then(std::io::Error::get_ref)
                    .is_some_and(|inner| rejected_certificate(inner))
                    || error.source().is_some_and(rejected_certificate)
            }
            assert!(
                rejected_certificate(&error),
                "connection failed without proof of certificate rejection: {error:?}"
            );
        }
    }
}
