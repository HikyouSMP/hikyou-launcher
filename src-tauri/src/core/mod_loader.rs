//! Discover loader-owned nested mods from the exact official Loader artifact.

use super::{LocalModNode, check_mod_jar_structure, collect_local_mods};
use std::{collections::BTreeMap, sync::OnceLock};

pub(super) const OWNER: &str = "@fabric-loader";
const MAX_LOADER_BYTES: u64 = 16 * 1024 * 1024;

pub(super) async fn load(loader: &str, version: Option<&str>) -> Result<Vec<LocalModNode>, String> {
    if loader != "fabric" {
        return Ok(Vec::new());
    }
    let Some(version) = version else {
        return Ok(Vec::new());
    };
    if version.len() > 128
        || version.is_empty()
        || !version
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".+-".contains(&c))
    {
        return Err("invalid Fabric Loader version".into());
    }
    static CACHE: OnceLock<tokio::sync::Mutex<BTreeMap<String, Vec<LocalModNode>>>> =
        OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().await;
    if let Some(nodes) = cache.get(version) {
        return Ok(nodes.clone());
    }
    let url = format!(
        "https://maven.fabricmc.net/net/fabricmc/fabric-loader/{version}/fabric-loader-{version}.jar"
    );
    let client = crate::http::builder()?
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let digest = super::download_mod_bytes(
        client
            .get(format!("{url}.sha512"))
            .send()
            .await
            .map_err(|e| e.to_string())?,
        Some(256),
    )
    .await?;
    let digest = std::str::from_utf8(&digest)
        .map_err(|_| "invalid Loader digest")?
        .trim();
    if digest.len() != 128 || !digest.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("invalid Fabric Loader SHA-512".into());
    }
    let bytes = super::download_mod_bytes(
        client.get(url).send().await.map_err(|e| e.to_string())?,
        Some(MAX_LOADER_BYTES),
    )
    .await?;
    super::verify_mod_bytes(&bytes, &[("sha512".into(), digest.into())].into(), None)?;
    let nodes = inspect(version, &bytes)?;
    if cache.len() == 16 {
        cache.pop_first();
    }
    cache.insert(version.into(), nodes.clone());
    Ok(nodes)
}

pub(super) fn inspect(version: &str, bytes: &[u8]) -> Result<Vec<LocalModNode>, String> {
    check_mod_jar_structure("fabric-loader.jar", bytes)?;
    let mut nodes = Vec::new();
    collect_local_mods("fabric-loader.jar", bytes, "fabric", 0, &mut nodes)?;
    let root = nodes.first_mut().ok_or("Fabric Loader metadata missing")?;
    if root.id != "fabricloader" || root.version.as_deref() != Some(version) {
        return Err("Fabric Loader artifact identity mismatch".into());
    }
    // RuntimeCheck owns this verified root's identity/version. The root only
    // enables optional bundled candidates; it never becomes a user Mod.
    root.ids.clear();
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_loader_owns_nested_candidates_without_shadowing_runtime() -> Result<(), String> {
        let child = crate::core::test_support::zip(
            &[("fabric.mod.json", br#"{"id":"mixinextras","version":"3"}"#)],
            zip::CompressionMethod::Stored,
        );
        let bytes = crate::core::test_support::zip(
            &[
                (
                    "fabric.mod.json",
                    br#"{"id":"fabricloader","version":"0.19.5","jars":[{"file":"child.jar"}]}"#,
                ),
                ("child.jar", &child),
            ],
            zip::CompressionMethod::Stored,
        );
        let nodes = inspect("0.19.5", &bytes)?;
        assert!(nodes[0].ids.is_empty());
        assert_eq!(nodes[1].version.as_deref(), Some("3"));
        assert_eq!(
            nodes[1].parent_digest.as_deref(),
            Some(nodes[0].digest.as_str())
        );
        assert!(inspect("0.19.4", &bytes).is_err());
        Ok(())
    }

    #[test]
    fn loader_bundled_version_is_a_candidate_not_a_fixed_runtime_version() -> Result<(), String> {
        let zip = crate::core::test_support::zip;
        let child = |version: &str| {
            zip(
                &[(
                    "fabric.mod.json",
                    format!(r#"{{"id":"mixinextras","version":"{version}"}}"#).as_bytes(),
                )],
                zip::CompressionMethod::Stored,
            )
        };
        let old = child("3");
        let loader = zip(
            &[
                (
                    "fabric.mod.json",
                    br#"{"id":"fabricloader","version":"1","jars":[{"file":"lib.jar"}]}"#,
                ),
                ("lib.jar", &old),
            ],
            zip::CompressionMethod::Stored,
        );
        let newer = child("5");
        let root = zip(&[
            ("fabric.mod.json", br#"{"id":"root","version":"1","depends":{"mixinextras":">=5"},"jars":[{"file":"lib.jar"}]}"#),
            ("lib.jar", &newer),
        ], zip::CompressionMethod::Stored);
        let mut nodes = inspect("1", &loader)?;
        collect_local_mods("root.jar", &root, "fabric", 0, &mut nodes)?;
        let selected = super::super::local_graph::select(
            &nodes.iter().collect::<Vec<_>>(),
            false,
            &Default::default(),
            &|_, _| Ok(super::super::local_graph::RuntimeCheck::NotRuntime),
        )?;
        let super::super::local_graph::Resolution::Selected(selected) = selected else {
            return Err("a Mod may supply a newer compatible bundled library".into());
        };
        let libraries: Vec<_> = selected.iter().filter(|n| n.id == "mixinextras").collect();
        assert_eq!(libraries.len(), 1);
        assert_eq!(libraries[0].version.as_deref(), Some("5"));
        Ok(())
    }
}
