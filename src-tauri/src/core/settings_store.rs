use super::managed_fs;
use std::{io::Read, path::Path};

const LIMIT: u64 = 1024 * 1024;

pub(crate) fn read(root: &Path) -> Result<serde_json::Value, String> {
    let dir = managed_fs::root(root)?;
    let name = Path::new("settings.json");
    if managed_fs::inspect(&dir, name)?.is_none() {
        return Ok(serde_json::json!({}));
    }
    let mut bytes = Vec::new();
    managed_fs::read_file(&dir, name)?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Could not read settings: {e}"))?;
    if bytes.len() as u64 > LIMIT {
        return Err("Settings exceed the size limit; the existing file was retained".into());
    }
    let settings: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Settings are invalid; the existing file was retained: {e}"))?;
    if !settings.is_object() {
        return Err("Settings must be an object; the existing file was retained".into());
    }
    Ok(settings)
}

pub(crate) fn write(root: &Path, settings: &serde_json::Value) -> Result<(), String> {
    if !settings.is_object() {
        return Err("Settings must be an object".into());
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("Settings exceed the size limit".into());
    }
    let dir = managed_fs::root(root)?;
    managed_fs::write(&dir, Path::new("settings.json"), &bytes, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_records_are_not_treated_as_missing_and_invalid_writes_preserve_previous_data()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        assert_eq!(read(temp.path())?, serde_json::json!({}));
        let expected = serde_json::json!({"game":{"memoryMb":4096}});
        write(temp.path(), &expected)?;
        assert!(write(temp.path(), &serde_json::json!([])).is_err());
        assert_eq!(read(temp.path())?, expected);
        for bytes in [b"{".as_slice(), b"null", b"[]"] {
            std::fs::write(temp.path().join("settings.json"), bytes)?;
            assert!(read(temp.path()).is_err());
            assert_eq!(std::fs::read(temp.path().join("settings.json"))?, bytes);
        }
        std::fs::write(
            temp.path().join("settings.json"),
            vec![0; LIMIT as usize + 1],
        )?;
        assert!(read(temp.path()).is_err());
        Ok(())
    }
}
