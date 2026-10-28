use std::io::{self, Read, Seek};
use zip::{CompressionMethod, ZipArchive};

/// Validate every member before caller-owned destinations are modified.
pub(super) fn preflight_zip<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    max_files: usize,
    max_expanded: u64,
) -> Result<(), String> {
    if archive.len() > max_files {
        return Err("ZIP contains too many entries".to_string());
    }
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index_data(index)
            .map_err(|e| format!("invalid ZIP metadata: {e}"))?;
        let name = entry
            .name()
            .map_err(|e| format!("invalid ZIP entry name: {e}"))?
            .into_owned();
        super::managed_fs::relative_path(name.strip_suffix('/').unwrap_or(&name))?;
        if entry.encrypted() {
            return Err(format!(
                "Encrypted ZIP entry {name:?} is unsupported. Use a ZIP without a password."
            ));
        }
        if !matches!(
            entry.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err(format!(
                "Unsupported ZIP compression {:?} in {name:?}. Only Stored and Deflate are supported; re-create the ZIP using Deflate.",
                entry.compression()
            ));
        }
        let size = entry.size();
        expanded = expanded
            .checked_add(size)
            .ok_or("ZIP expanded size overflow")?;
        if expanded > max_expanded {
            return Err(format!(
                "ZIP expands beyond its {max_expanded}-byte safety limit"
            ));
        }
        let file = archive
            .by_index(index)
            .map_err(|e| format!("cannot open ZIP entry {name:?}: {e}"))?;
        let actual = io::copy(&mut file.take(size.saturating_add(1)), &mut io::sink())
            .map_err(|e| format!("corrupt ZIP entry {name:?}: {e}"))?;
        if actual != size {
            return Err(format!(
                "ZIP entry size does not match its header: {name:?}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::test_support;

    #[test]
    fn supports_stored_and_deflated_and_checks_budgets() {
        for method in [CompressionMethod::Stored, CompressionMethod::Deflated] {
            let bytes = test_support::zip(&[("file.txt", b"contents")], method);
            let mut archive = ZipArchive::new(io::Cursor::new(bytes)).unwrap();
            preflight_zip(&mut archive, 1, 8).unwrap();
            assert!(preflight_zip(&mut archive, 0, 8).is_err());
            assert!(preflight_zip(&mut archive, 1, 7).is_err());
        }
    }

    #[test]
    fn unsupported_late_entry_names_the_method_and_remedy() {
        let bytes =
            test_support::unsupported_zip(&[("valid.txt", b"valid"), ("late.txt", b"late")]);
        let mut archive = ZipArchive::new(io::Cursor::new(bytes)).unwrap();
        let error = preflight_zip(&mut archive, 10, 100).unwrap_err();
        assert!(
            error.contains("late.txt") && error.contains("12") && error.contains("Deflate"),
            "{error}"
        );
    }

    #[test]
    fn corrupt_payload_is_rejected_not_just_its_header() {
        let mut bytes = test_support::zip(&[("file.txt", b"contents")], CompressionMethod::Stored);
        let mut archive = ZipArchive::new(io::Cursor::new(&bytes)).unwrap();
        let offset = archive.by_index_raw(0).unwrap().data_start().unwrap() as usize;
        bytes[offset] ^= 1;
        let mut archive = ZipArchive::new(io::Cursor::new(bytes)).unwrap();
        assert!(
            preflight_zip(&mut archive, 10, 100)
                .unwrap_err()
                .contains("corrupt ZIP entry")
        );
    }
}
