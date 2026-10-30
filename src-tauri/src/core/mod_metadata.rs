use std::io::Cursor;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FabricDependency {
    pub(super) mod_id: String,
    pub(super) predicate: Option<String>,
}

pub(super) struct InstalledModManifest {
    pub(super) id: String,
    pub(super) version: Option<String>,
    pub(super) minecraft_predicates: Option<Vec<String>>,
    pub(super) required_dependencies: Vec<FabricDependency>,
    pub(super) incompatible_dependencies: Vec<FabricDependency>,
}

pub(super) fn mod_jar_is_loadable_for_loader(bytes: &[u8], loader: &str) -> bool {
    if loader != "fabric" && loader != "quilt" {
        return true;
    }
    match fabric_mod_json_has_invalid_wildcard_predicate(bytes) {
        Ok(false) => true,
        Ok(true) => false,
        Err(error) => {
            log::warn!("[mods] Failed to inspect fabric.mod.json: {}", error);
            false
        }
    }
}

pub(super) fn fabric_mod_json_has_invalid_wildcard_predicate(bytes: &[u8]) -> Result<bool, String> {
    let json = read_fabric_mod_json(bytes)?;

    for key in ["depends", "recommends", "suggests", "conflicts", "breaks"] {
        if let Some(value) = json.get(key)
            && json_value_has_invalid_wildcard_predicate(value)
        {
            return Ok(true);
        }
    }

    Ok(false)
}

pub(super) fn fabric_mod_json_required_dependencies(
    bytes: &[u8],
) -> Result<Vec<FabricDependency>, String> {
    let json = read_fabric_mod_json(bytes)?;
    let mut dependencies = Vec::new();
    if let Some(serde_json::Value::Object(depends)) = json.get("depends") {
        for (mod_id, value) in depends {
            if is_external_fabric_dependency(mod_id) {
                dependencies.push(FabricDependency {
                    mod_id: mod_id.to_string(),
                    predicate: fabric_dependency_predicate(value),
                });
            }
        }
    }
    sort_dependencies(&mut dependencies);
    Ok(dependencies)
}

pub(super) fn read_installed_mod_manifest(
    _filename: &str,
    bytes: &[u8],
) -> Result<Option<InstalledModManifest>, String> {
    let json = read_fabric_mod_json(bytes)?;
    if json.is_null() {
        return Ok(None);
    }
    let Some(id) = json.get("id").and_then(|value| value.as_str()) else {
        return Ok(None);
    };
    Ok(Some(InstalledModManifest {
        id: id.to_string(),
        version: json
            .get("version")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        minecraft_predicates: json
            .get("depends")
            .and_then(|depends| depends.get("minecraft"))
            .map(fabric_dependency_predicates),
        required_dependencies: fabric_mod_json_required_dependencies(bytes)?,
        incompatible_dependencies: fabric_mod_json_incompatible_dependencies(bytes)?,
    }))
}

pub(super) fn fabric_mod_json_incompatible_dependencies(
    bytes: &[u8],
) -> Result<Vec<FabricDependency>, String> {
    let json = read_fabric_mod_json(bytes)?;
    let mut dependencies = Vec::new();
    // Fabric's `conflicts` is advisory; only `breaks` forbids loading.
    if let Some(serde_json::Value::Object(entries)) = json.get("breaks") {
        for (mod_id, value) in entries {
            if is_external_fabric_dependency(mod_id) {
                dependencies.push(FabricDependency {
                    mod_id: mod_id.to_string(),
                    predicate: fabric_dependency_predicate(value),
                });
            }
        }
    }
    sort_dependencies(&mut dependencies);
    Ok(dependencies)
}

fn sort_dependencies(dependencies: &mut Vec<FabricDependency>) {
    dependencies.sort_by(|a, b| {
        a.mod_id
            .cmp(&b.mod_id)
            .then_with(|| a.predicate.cmp(&b.predicate))
    });
    dependencies.dedup();
}

fn fabric_dependency_predicate(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.trim().to_string()).filter(|s| !s.is_empty()),
        serde_json::Value::Array(values) => values.iter().find_map(fabric_dependency_predicate),
        serde_json::Value::Object(map) => map.values().find_map(fabric_dependency_predicate),
        _ => None,
    }
}

fn fabric_dependency_predicates(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                Vec::new()
            } else {
                vec![text.to_string()]
            }
        }
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(|value| match value {
                serde_json::Value::String(text) => {
                    let text = text.trim();
                    (!text.is_empty()).then(|| text.to_string())
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn read_fabric_mod_json(bytes: &[u8]) -> Result<serde_json::Value, String> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("failed to open mod JAR: {}", e))?;
    let Ok(entry) = archive.by_name("fabric.mod.json") else {
        return Ok(serde_json::Value::Null);
    };

    let bytes = super::mod_files::read_bounded(entry, 8 * 1024 * 1024)?;
    parse_fabric_metadata(&bytes)
}

pub(super) fn parse_fabric_metadata(bytes: &[u8]) -> Result<serde_json::Value, String> {
    const LIMIT: usize = 8 * 1024 * 1024;
    if bytes.len() > LIMIT {
        return Err("Fabric metadata exceeds size limit".into());
    }
    let parse = |bytes: &[u8]| -> Result<serde_json::Value, String> {
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| format!("invalid Fabric metadata: {e}"))?;
        if !value.is_object() {
            return Err("Fabric metadata root must be an object".into());
        }
        Ok(value)
    };
    let original_error = match parse(bytes) {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };
    // Fabric's JsonReader accepts literal control characters in quoted strings.
    // Escape only those bytes; retain serde_json's syntax/depth/UTF-8 validation.
    let mut normalized = Vec::with_capacity(bytes.len());
    let (mut quoted, mut escaped, mut changed) = (false, false, false);
    const HEX: &[u8] = b"0123456789abcdef";
    for &byte in bytes {
        if quoted && !escaped && byte < 0x20 {
            if normalized.len() > LIMIT - 6 {
                return Err("normalized Fabric metadata exceeds size limit".into());
            }
            normalized.extend_from_slice(b"\\u00");
            normalized.extend_from_slice(&[HEX[(byte >> 4) as usize], HEX[(byte & 15) as usize]]);
            changed = true;
            continue;
        }
        if normalized.len() == LIMIT {
            return Err("normalized Fabric metadata exceeds size limit".into());
        }
        normalized.push(byte);
        if escaped {
            escaped = false;
        } else if quoted && byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            quoted = !quoted;
        }
    }
    if changed {
        parse(&normalized)
    } else {
        Err(original_error)
    }
}

fn is_external_fabric_dependency(mod_id: &str) -> bool {
    !matches!(
        mod_id,
        "java"
            | "minecraft"
            | "fabricloader"
            | "fabric-loader"
            | "quilt_loader"
            | "quilt-loader"
            | "quilted_fabric_api"
    )
}

fn json_value_has_invalid_wildcard_predicate(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(text) => version_predicate_has_invalid_wildcard(text),
        serde_json::Value::Array(values) => {
            values.iter().any(json_value_has_invalid_wildcard_predicate)
        }
        serde_json::Value::Object(map) => {
            map.values().any(json_value_has_invalid_wildcard_predicate)
        }
        _ => false,
    }
}

fn version_predicate_has_invalid_wildcard(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let has_wildcard = lower.contains(".x") || lower.contains(".*");
    has_wildcard
        && [">=", "<=", ">", "<"]
            .iter()
            .any(|operator| lower.contains(operator))
}

#[cfg(test)]
mod metadata_tests {
    use super::parse_fabric_metadata;

    #[test]
    fn accepts_fabric_quoted_line_breaks_without_changing_the_value() -> Result<(), String> {
        let value = parse_fabric_metadata(
            b"{\"id\":\"example\",\"description\":\"one\n two\r\nthree\tend\"}",
        )?;
        assert_eq!(value["description"], "one\n two\r\nthree\tend");
        assert_eq!(value["id"], "example");
        Ok(())
    }

    #[test]
    fn escaping_keeps_string_boundaries_and_version_bytes() -> Result<(), String> {
        let input = b"{\"description\":\"quote \\\" and newline\nend\",\"version\":\"1+build\"}";
        let value = parse_fabric_metadata(input)?;
        assert_eq!(value["description"], "quote \" and newline\nend");
        assert_eq!(value["version"], "1+build");
        Ok(())
    }

    #[test]
    fn no_global_lenient_json_or_trailing_payload_is_accepted() {
        for input in [
            b"{unquoted:1}".as_slice(),
            b"{\"id\":\"x\"} {}",
            b"{\"id\":\"x\",}",
            b"{\"id\":\"x\\q\"}",
            b"{\0\"id\":\"x\"}",
            b"{\"id\":\"\xff\"}",
            b"[]",
        ] {
            assert!(parse_fabric_metadata(input).is_err());
        }
        assert!(parse_fabric_metadata(&vec![b' '; 8 * 1024 * 1024 + 1]).is_err());
    }
}
