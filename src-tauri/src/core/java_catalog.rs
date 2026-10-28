//! Azul's official metadata and downloads, using the shared OS certificate policy.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct Package {
    pub package_uuid: uuid::Uuid,
    pub java_version: Vec<u32>,
    pub distro_version: Vec<u32>,
    pub openjdk_build_number: u32,
    pub download_url: String,
    pub sha256_hash: String,
}

#[derive(Deserialize)]
struct CatalogPackage {
    #[serde(flatten)]
    package: Package,
    arch: String,
    hw_bitness: u32,
    os: String,
    lib_c_type: Option<String>,
}

impl CatalogPackage {
    fn validate(&self, major: u32, os: &str, arch: &str) -> Result<(), String> {
        self.package.validate(major)?;
        let expected_arch = match arch {
            "x64" => "x86",
            "aarch64" => "arm",
            _ => return Err("unsupported Azul architecture".into()),
        };
        let expected_os = os.strip_suffix("-glibc").unwrap_or(os);
        if self.arch != expected_arch
            || self.hw_bitness != 64
            || self.os != expected_os
            || (os == "linux-glibc" && self.lib_c_type.as_deref() != Some("glibc"))
        {
            return Err(
                "Azul returned a runtime for the wrong architecture or operating system".into(),
            );
        }
        Ok(())
    }
}

pub(super) fn client() -> Result<reqwest::Client, String> {
    crate::http::builder()?
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !trusted_url(attempt.url()) {
                attempt.error("unexpected Java download redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| format!("failed to initialize Java HTTPS client: {e}"))
}

fn trusted_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && matches!(
            url.host_str(),
            Some("api.azul.com" | "cdn.azul.com" | "static.azul.com")
        )
}

pub(super) async fn latest(client: &reqwest::Client, major: u32) -> Result<Package, String> {
    let (os, arch, archive) = platform(std::env::consts::OS, std::env::consts::ARCH)?;
    let response = request(client, major, os, arch, archive)
        .send()
        .await
        .map_err(|e| format!("Azul metadata request failed: {e}"))?;
    let bytes = bounded_body(response, 1024 * 1024).await?;
    let packages: Vec<CatalogPackage> =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid Azul metadata: {e}"))?;
    let package = packages
        .into_iter()
        .next()
        .ok_or_else(|| format!("no Azul Zulu Java {major} runtime is published for {os}/{arch}"))?;
    package.validate(major, os, arch)?;
    Ok(package.package)
}

fn request(
    client: &reqwest::Client,
    major: u32,
    os: &str,
    arch: &str,
    archive: &str,
) -> reqwest::RequestBuilder {
    client
        .get("https://api.azul.com/metadata/v1/zulu/packages/")
        .query(&[
            ("java_version", major.to_string()),
            ("os", os.to_string()),
            ("arch", arch.to_string()),
            ("archive_type", archive.to_string()),
            ("java_package_type", "jre".to_string()),
            ("javafx_bundled", "false".to_string()),
            ("crac_supported", "false".to_string()),
            ("release_status", "ga".to_string()),
            ("release_type", "PSU".to_string()),
            ("availability_types", "CA".to_string()),
            ("latest", "true".to_string()),
            (
                "include_fields",
                "sha256_hash,arch,hw_bitness,os,lib_c_type".to_string(),
            ),
            ("page_size", "1".to_string()),
        ])
        .timeout(Duration::from_secs(20))
}

pub(super) fn platform(
    os: &str,
    arch: &str,
) -> Result<(&'static str, &'static str, &'static str), String> {
    let (os, archive) = match os {
        "windows" => ("windows", "zip"),
        "macos" => ("macos", "tar.gz"),
        "linux" => ("linux-glibc", "tar.gz"),
        _ => return Err("unsupported Java platform".to_string()),
    };
    let arch = match arch {
        // Metadata API combines architecture and bitness; hw_bitness is not a filter.
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        _ => return Err("unsupported Java architecture".to_string()),
    };
    Ok((os, arch, archive))
}

impl Package {
    fn validate(&self, major: u32) -> Result<(), String> {
        if self.java_version.first() != Some(&major)
            || self.distro_version.is_empty()
            || self.sha256_hash.len() != 64
            || !self.sha256_hash.bytes().all(|b| b.is_ascii_hexdigit())
            || !reqwest::Url::parse(&self.download_url).is_ok_and(|url| trusted_url(&url))
        {
            return Err(
                "Azul metadata has an unexpected version, checksum or download origin".to_string(),
            );
        }
        Ok(())
    }

    pub async fn download(&self, client: &reqwest::Client) -> Result<Vec<u8>, String> {
        let response = client
            .get(&self.download_url)
            .timeout(Duration::from_secs(300))
            .send()
            .await
            .map_err(|e| format!("Zulu download failed: {e}"))?;
        let bytes = bounded_body(response, 1024 * 1024 * 1024).await?;
        self.verify(&bytes)?;
        Ok(bytes)
    }

    pub fn verify(&self, bytes: &[u8]) -> Result<(), String> {
        let actual = format!("{:x}", Sha256::digest(bytes));
        if !actual.eq_ignore_ascii_case(&self.sha256_hash) {
            return Err(
                "Zulu download SHA-256 mismatch; the existing runtime was not changed".to_string(),
            );
        }
        Ok(())
    }
}

async fn bounded_body(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    if !response.status().is_success() {
        return Err(format!("Azul server returned HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err("Azul response exceeds its size limit".to_string());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Azul response failed: {e}"))?
    {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err("Azul response exceeds its size limit".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_official_https_origins_and_expected_major_are_accepted() {
        let mut package = Package {
            package_uuid: uuid::Uuid::nil(),
            java_version: vec![21, 0, 1],
            distro_version: vec![21, 1, 1],
            openjdk_build_number: 1,
            download_url: "https://cdn.azul.com/zulu/runtime.zip".to_string(),
            sha256_hash: format!("{:x}", Sha256::digest(b"runtime")),
        };
        package.validate(21).unwrap();
        package.verify(b"runtime").unwrap();
        assert!(package.verify(b"tampered").is_err());
        assert!(package.validate(17).is_err());
        for url in [
            "http://cdn.azul.com/a",
            "https://cdn.azul.com.evil.test/a",
            "https://evil.test/a",
            "https://user@cdn.azul.com/a",
            "https://cdn.azul.com:444/a",
        ] {
            package.download_url = url.to_string();
            assert!(package.validate(21).is_err());
        }
    }
    #[test]
    fn platform_matrix_uses_correct_api_names_and_formats() {
        assert_eq!(
            platform("windows", "x86_64").unwrap(),
            ("windows", "x64", "zip")
        );
        assert_eq!(
            platform("macos", "aarch64").unwrap(),
            ("macos", "aarch64", "tar.gz")
        );
        assert_eq!(
            platform("linux", "x86_64").unwrap(),
            ("linux-glibc", "x64", "tar.gz")
        );
        assert!(platform("linux", "x86").is_err());
    }

    #[test]
    fn metadata_request_and_response_require_the_exact_64_bit_platform() {
        let client = client().unwrap();
        for os in ["windows", "macos", "linux"] {
            for arch in ["x86_64", "aarch64"] {
                let (os, arch, archive) = platform(os, arch).unwrap();
                for major in [8, 17, 21, 25] {
                    let url = request(&client, major, os, arch, archive)
                        .build()
                        .unwrap()
                        .url()
                        .clone();
                    let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
                    assert_eq!(query.get("arch").unwrap(), arch);
                    assert!(!query.contains_key("hw_bitness"));
                    assert_eq!(query.get("os").unwrap(), os);
                    let mut response: CatalogPackage = serde_json::from_value(serde_json::json!({
                        "package_uuid": uuid::Uuid::nil(), "java_version": [major],
                        "distro_version": [1], "openjdk_build_number": 1,
                        "download_url": "https://cdn.azul.com/runtime.zip", "sha256_hash": "a".repeat(64),
                        "arch": if arch == "x64" { "x86" } else { "arm" }, "hw_bitness": 64,
                        "os": os.strip_suffix("-glibc").unwrap_or(os),
                        "lib_c_type": if os == "linux-glibc" { Some("glibc") } else { None }
                    })).unwrap();
                    response.validate(major, os, arch).unwrap();
                    response.hw_bitness = 32;
                    assert!(response.validate(major, os, arch).is_err());
                    response.hw_bitness = 64;
                    response.arch = "wrong-architecture".into();
                    assert!(response.validate(major, os, arch).is_err());
                }
            }
        }
    }
}
