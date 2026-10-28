use std::path::{Path, PathBuf};

use super::java::JAVA_BIN;

/// ZIPを展開し、トップレベルディレクトリを dest_dir にリネームする
pub(super) fn extract_and_rename_zip(
    bytes: &[u8],
    install_base: &Path,
    dest_dir: &Path,
) -> Result<(), String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("failed to parse ZIP: {}", e))?;
    super::archive::preflight_zip(&mut archive, 250_000, 16 * 1024 * 1024 * 1024)?;
    let mut top_dir_name: Option<String> = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("failed to read ZIP entry: {}", e))?;
        let name = entry
            .name()
            .map_err(|e| format!("failed to read JDK ZIP entry name: {e}"))?;
        let path = name.trim_end_matches('/');
        if path.is_empty()
            || path.contains(['\\', ':', '\0'])
            || path.split('/').any(|part| matches!(part, "" | "." | ".."))
        {
            return Err("JDK ZIP contains an unsafe path".to_string());
        }
        let root = path.split('/').next().unwrap();
        match &top_dir_name {
            Some(expected) if expected != root => {
                return Err("JDK ZIP must contain a single root directory".to_string());
            }
            None => top_dir_name = Some(root.to_string()),
            _ => {}
        }
    }
    let top_dir_name = top_dir_name.ok_or("JDK ZIP is empty")?;

    // Keep archive writes and link targets away from installed runtimes.
    let staging = tempfile::Builder::new()
        .prefix(".jdk-")
        .tempdir_in(install_base)
        .map_err(|e| format!("failed to create JDK staging directory: {e}"))?;
    let unpack = staging.path().join("archive");
    std::fs::create_dir(&unpack)
        .map_err(|e| format!("failed to create JDK extraction directory: {e}"))?;

    archive
        .extract(&unpack)
        .map_err(|e| format!("failed to extract ZIP: {}", e))?;

    commit_runtime(staging, &unpack, &top_dir_name, dest_dir)
}

fn commit_runtime(
    staging: tempfile::TempDir,
    unpack: &Path,
    top_dir_name: &str,
    dest_dir: &Path,
) -> Result<(), String> {
    let extracted = unpack.join(top_dir_name);
    let jre_root = find_jre_root(&extracted);
    let canonical_root = jre_root
        .canonicalize()
        .map_err(|e| format!("failed to resolve extracted JDK: {e}"))?;
    let canonical_unpack = unpack
        .canonicalize()
        .map_err(|e| format!("failed to resolve JDK extraction directory: {e}"))?;
    if !canonical_root.starts_with(&canonical_unpack)
        || !jre_root.join("bin").join(JAVA_BIN).is_file()
    {
        return Err("JDK ZIP does not contain a confined Java runtime".to_string());
    }
    validate_relocatable_jdk(&jre_root, &canonical_root)?;

    let previous = staging.path().join("previous");
    let replacing = dest_dir.exists();
    if replacing {
        std::fs::rename(dest_dir, &previous)
            .map_err(|e| format!("failed to stage previous JDK: {e}"))?;
    }
    if let Err(error) = std::fs::rename(&jre_root, dest_dir) {
        if replacing && let Err(rollback) = std::fs::rename(&previous, dest_dir) {
            // Do not delete the previous installation if recovery itself failed.
            let _ = staging.keep();
            return Err(format!(
                "failed to install JDK: {error}; previous JDK retained in staging after recovery failed: {rollback}"
            ));
        }
        return Err(format!("failed to install JDK: {error}"));
    }

    Ok(())
}

fn relative_link_is_confined(mut depth: usize, target: &Path) -> bool {
    use std::path::Component;
    for component in target.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir if depth > 0 => depth -= 1,
            _ => return false,
        }
    }
    true
}

fn validate_relocatable_jdk(root: &Path, canonical_root: &Path) -> Result<(), String> {
    fn check(directory: &Path, root: &Path, canonical_root: &Path) -> Result<(), String> {
        for entry in std::fs::read_dir(directory)
            .map_err(|e| format!("failed to inspect extracted JDK directory: {e}"))?
        {
            let entry = entry.map_err(|e| format!("failed to inspect extracted JDK entry: {e}"))?;
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|e| format!("failed to inspect extracted JDK entry type: {e}"))?;
            if kind.is_symlink() {
                let target = std::fs::read_link(&path)
                    .map_err(|e| format!("failed to read extracted JDK link: {e}"))?;
                let depth = directory.strip_prefix(root).unwrap().components().count();
                let resolved = path
                    .canonicalize()
                    .map_err(|e| format!("failed to resolve extracted JDK link: {e}"))?;
                if !relative_link_is_confined(depth, &target)
                    || !resolved.starts_with(canonical_root)
                {
                    return Err("JDK link would escape the installed runtime".to_string());
                }
            } else if kind.is_dir() {
                check(&path, root, canonical_root)?;
            }
        }
        Ok(())
    }
    if std::fs::symlink_metadata(root)
        .map_err(|e| format!("failed to inspect extracted JDK root: {e}"))?
        .file_type()
        .is_symlink()
    {
        return Err("JDK installation root must not be a symbolic link".to_string());
    }
    check(root, root, canonical_root)
}

/// tar.gz を展開し、トップレベルディレクトリを dest_dir にリネームする
#[cfg(not(target_os = "windows"))]
pub(super) fn extract_and_rename_targz(
    bytes: &[u8],
    install_base: &Path,
    dest_dir: &Path,
) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let decoder = || GzDecoder::new(std::io::Cursor::new(bytes));
    let mut archive = Archive::new(decoder());
    let mut root = None;
    let mut expanded = 0u64;
    for (index, entry) in archive
        .entries()
        .map_err(|e| format!("invalid JDK tar.gz: {e}"))?
        .enumerate()
    {
        if index >= 250_000 {
            return Err("JDK archive contains too many entries".to_string());
        }
        let entry = entry.map_err(|e| format!("invalid JDK tar entry: {e}"))?;
        let kind = entry.header().entry_type();
        if kind.is_pax_global_extensions() {
            continue;
        }
        if !(kind.is_file() || kind.is_dir() || kind.is_symlink() || kind.is_hard_link()) {
            return Err("JDK archive contains an unsupported special file".to_string());
        }
        let path = entry
            .path()
            .map_err(|e| format!("invalid JDK tar path: {e}"))?;
        if kind.is_dir() && path == Path::new(".") {
            continue;
        }
        let path = tar_member_path(&path)?;
        let top = path
            .components()
            .next()
            .unwrap()
            .as_os_str()
            .to_str()
            .unwrap()
            .to_string();
        if let Some(expected) = &root {
            if expected != &top {
                return Err("JDK archive must contain a single root directory".to_string());
            }
        } else {
            root = Some(top);
        }
        expanded = expanded
            .checked_add(entry.size())
            .ok_or("JDK archive size overflow")?;
        if expanded > 16 * 1024 * 1024 * 1024 {
            return Err("JDK archive expands beyond 16 GiB".to_string());
        }
        if let Some(target) = entry
            .link_name()
            .map_err(|e| format!("invalid JDK tar link: {e}"))?
        {
            if kind.is_hard_link() {
                let target = tar_member_path(&target)?;
                if target.components().next() != path.components().next() {
                    return Err("JDK hard link escapes the archive root".to_string());
                }
            } else if kind.is_symlink()
                && !relative_link_is_confined(path.components().count().saturating_sub(2), &target)
            {
                return Err("JDK symbolic link escapes the archive root".to_string());
            }
        }
    }
    let root = root.ok_or("JDK archive is empty")?;
    let staging = tempfile::Builder::new()
        .prefix(".jdk-")
        .tempdir_in(install_base)
        .map_err(|e| format!("failed to stage JDK: {e}"))?;
    let unpack = staging.path().join("archive");
    std::fs::create_dir(&unpack)
        .map_err(|e| format!("failed to create JDK extraction directory: {e}"))?;
    let mut archive = Archive::new(decoder());
    archive.set_preserve_permissions(false);
    archive.set_preserve_ownerships(false);
    archive.set_mask(0o7000);
    archive
        .unpack(&unpack)
        .map_err(|e| format!("failed to extract JDK tar.gz: {e}"))?;
    // tar stops at its end marker; consume the gzip trailer to validate its checksum too.
    std::io::copy(&mut archive.into_inner(), &mut std::io::sink())
        .map_err(|e| format!("invalid JDK gzip checksum: {e}"))?;
    commit_runtime(staging, &unpack, &root, dest_dir)
}

#[cfg(not(windows))]
fn tar_member_path(path: &Path) -> Result<PathBuf, String> {
    let mut name = path.to_str().ok_or("JDK tar path is not UTF-8")?;
    while let Some(rest) = name.strip_prefix("./") {
        name = rest;
    }
    super::managed_fs::relative_path(name.strip_suffix('/').unwrap_or(name))
}

/// 展開ディレクトリ内で bin/java が存在するルートを探す
/// 通常: dir/bin/java
/// macOS bundles: dir/Contents/Home/bin/java
/// macOS Zulu .jre: dir/zulu-N.jre/Contents/Home/bin/java
fn find_jre_root(dir: &PathBuf) -> PathBuf {
    if dir.join("bin").join(JAVA_BIN).exists() {
        return dir.clone();
    }
    let macos_home = dir.join("Contents").join("Home");
    if macos_home.join("bin").join(JAVA_BIN).exists() {
        return macos_home;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name.ends_with(".jre") || name.ends_with(".jdk") {
                let candidate = path.join("Contents").join("Home");
                if candidate.join("bin").join(JAVA_BIN).exists() {
                    return candidate;
                }
            }
        }
    }
    dir.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::{CompressionMethod, write::SimpleFileOptions};

    #[cfg(not(windows))]
    fn targz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut tar = tar::Builder::new(gz);
        for (name, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, name, *bytes).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }

    #[cfg(not(windows))]
    #[test]
    fn tar_boundary_preserves_previous_runtime_on_corrupt_input() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().to_path_buf();
        let destination = base.join("managed");
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(destination.join("sentinel"), b"previous").unwrap();
        assert!(extract_and_rename_targz(b"corrupt gzip", &base, &destination).is_err());
        assert_eq!(
            std::fs::read(destination.join("sentinel")).unwrap(),
            b"previous"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn tar_boundary_empty_archive_cannot_select_existing_sibling() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().to_path_buf();
        let destination = base.join("managed");
        let sibling = base.join("zulu-existing");
        std::fs::create_dir_all(sibling.join("bin")).unwrap();
        std::fs::write(sibling.join("bin/java"), b"existing runtime").unwrap();
        assert!(extract_and_rename_targz(&targz(&[]), &base, &destination).is_err());
        assert_eq!(
            std::fs::read(sibling.join("bin/java")).unwrap(),
            b"existing runtime"
        );
        assert!(!destination.exists());
    }

    #[cfg(not(windows))]
    #[test]
    fn tar_installs_standard_and_macos_layouts_without_selecting_siblings() {
        use std::os::unix::fs::PermissionsExt;
        for prefix in [
            "bellsoft-vendor",
            "bellsoft-vendor/Contents/Home",
            "bellsoft-vendor/runtime.jre/Contents/Home",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let base = temp.path().to_path_buf();
            let destination = base.join("managed");
            std::fs::create_dir_all(base.join("zulu-existing/bin")).unwrap();
            std::fs::write(base.join("zulu-existing/bin/java"), b"sibling").unwrap();
            std::fs::create_dir(&destination).unwrap();
            std::fs::write(destination.join("old"), b"previous").unwrap();
            let name = format!("./{prefix}/bin/java");
            extract_and_rename_targz(&targz(&[(&name, b"runtime")]), &base, &destination).unwrap();
            assert_eq!(
                std::fs::read(destination.join("bin/java")).unwrap(),
                b"runtime"
            );
            assert_eq!(
                std::fs::read(base.join("zulu-existing/bin/java")).unwrap(),
                b"sibling"
            );
            assert!(!destination.join("old").exists());
            assert_ne!(
                std::fs::metadata(destination.join("bin/java"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o111,
                0
            );
            assert_eq!(std::fs::read_dir(&base).unwrap().count(), 2);
        }
    }

    #[cfg(not(windows))]
    fn linked_targz(symbolic: bool, target: &str) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut archive = tar::Builder::new(gz);
        let mut file = tar::Header::new_gnu();
        file.set_size(7);
        file.set_mode(0o6755);
        file.set_cksum();
        archive
            .append_data(&mut file, "vendor/bin/java-real", &b"runtime"[..])
            .unwrap();
        let mut link = tar::Header::new_gnu();
        link.set_size(0);
        link.set_entry_type(if symbolic {
            tar::EntryType::Symlink
        } else {
            tar::EntryType::Link
        });
        link.set_mode(0o777);
        archive
            .append_link(&mut link, "vendor/bin/java", target)
            .unwrap();
        archive.into_inner().unwrap().finish().unwrap()
    }

    #[cfg(not(windows))]
    #[test]
    fn tar_preserves_internal_symlinks_and_hardlinks_without_special_permission_bits() {
        use std::os::unix::fs::PermissionsExt;
        for (symbolic, target) in [(true, "java-real"), (false, "vendor/bin/java-real")] {
            let temp = tempfile::tempdir().unwrap();
            let base = temp.path().to_path_buf();
            let destination = base.join("managed");
            extract_and_rename_targz(&linked_targz(symbolic, target), &base, &destination).unwrap();
            assert_eq!(
                std::fs::read(destination.join("bin/java")).unwrap(),
                b"runtime"
            );
            assert_eq!(
                std::fs::metadata(destination.join("bin/java"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o7000,
                0
            );
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn tar_rejects_multiple_roots_external_links_and_bad_gzip_checksum() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().to_path_buf();
        let destination = base.join("managed");
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(destination.join("sentinel"), b"previous").unwrap();
        let mut bad_crc = targz(&[("vendor/bin/java", b"runtime")]);
        let footer = bad_crc.len() - 8;
        bad_crc[footer] ^= 1;
        for bytes in [
            targz(&[
                ("vendor/bin/java", b"runtime"),
                ("sibling/sentinel", b"overwrite"),
            ]),
            linked_targz(true, "../../outside/java"),
            linked_targz(false, "other-root/bin/java"),
            bad_crc,
        ] {
            assert!(extract_and_rename_targz(&bytes, &base, &destination).is_err());
            assert_eq!(
                std::fs::read(destination.join("sentinel")).unwrap(),
                b"previous"
            );
            assert_eq!(std::fs::read_dir(&base).unwrap().count(), 1);
        }
    }

    fn zip(entries: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
        let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, content) in entries {
            archive
                .start_file(
                    *name,
                    SimpleFileOptions::default().compression_method(method),
                )
                .unwrap();
            archive.write_all(content).unwrap();
        }
        archive.finish().unwrap().into_inner()
    }

    #[test]
    fn jdk_zip_installs_stored_and_deflated_standard_and_macos_layouts() {
        for method in [CompressionMethod::Stored, CompressionMethod::Deflated] {
            for prefix in [
                "vendor",
                "vendor/Contents/Home",
                "vendor/runtime.jre/Contents/Home",
            ] {
                let directory = tempfile::tempdir().unwrap();
                let base = directory.path().to_path_buf();
                let destination = base.join("managed");
                std::fs::create_dir(&destination).unwrap();
                std::fs::write(destination.join("old"), b"previous").unwrap();
                let java = format!("{prefix}/bin/{JAVA_BIN}");
                let bytes = zip(&[(&java, b"runtime")], method);
                extract_and_rename_zip(&bytes, &base, &destination).unwrap();
                assert_eq!(
                    std::fs::read(destination.join("bin").join(JAVA_BIN)).unwrap(),
                    b"runtime"
                );
                assert!(!destination.join("old").exists());
                assert_eq!(std::fs::read_dir(&base).unwrap().count(), 1);
            }
        }
    }

    #[test]
    fn jdk_zip_rejects_unsafe_names_without_touching_previous_installation() {
        for name in [
            "../escape/bin/java",
            "/escape/bin/java",
            "C:/escape/bin/java",
            "C:\\escape/bin/java",
            "//server/share/bin/java",
            "vendor/../escape/bin/java",
            "vendor/./bin/java",
            "vendor//bin/java",
            "vendor/bin/java:stream",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let base = directory.path().to_path_buf();
            let destination = base.join("managed");
            std::fs::create_dir(&destination).unwrap();
            std::fs::write(destination.join("sentinel"), b"previous").unwrap();
            let bytes = zip(&[(name, b"invalid")], CompressionMethod::Stored);
            assert!(
                extract_and_rename_zip(&bytes, &base, &destination).is_err(),
                "accepted {name}"
            );
            assert_eq!(
                std::fs::read(destination.join("sentinel")).unwrap(),
                b"previous"
            );
            assert_eq!(std::fs::read_dir(&base).unwrap().count(), 1);
        }
    }

    #[test]
    fn jdk_zip_rejects_multiple_roots_without_overwriting_sibling_runtime() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().to_path_buf();
        let destination = base.join("managed");
        let sibling = base.join("sibling");
        std::fs::create_dir(&sibling).unwrap();
        std::fs::write(sibling.join("sentinel"), b"original").unwrap();
        let java = format!("vendor/bin/{JAVA_BIN}");
        let bytes = zip(
            &[(&java, b"runtime"), ("sibling/sentinel", b"overwrite")],
            CompressionMethod::Stored,
        );
        assert!(extract_and_rename_zip(&bytes, &base, &destination).is_err());
        assert_eq!(
            std::fs::read(sibling.join("sentinel")).unwrap(),
            b"original"
        );
        assert!(!destination.exists());
    }

    #[test]
    fn jdk_zip_preserves_previous_installation_on_invalid_input() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().to_path_buf();
        let destination = base.join("managed");
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(destination.join("sentinel"), b"previous").unwrap();
        for bytes in [
            b"not a ZIP".to_vec(),
            zip(&[], CompressionMethod::Stored),
            zip(
                &[("vendor/readme", b"no runtime")],
                CompressionMethod::Stored,
            ),
        ] {
            assert!(extract_and_rename_zip(&bytes, &base, &destination).is_err());
            assert_eq!(
                std::fs::read(destination.join("sentinel")).unwrap(),
                b"previous"
            );
            assert_eq!(std::fs::read_dir(&base).unwrap().count(), 1);
        }
    }

    #[test]
    fn relocated_jdk_links_cannot_rely_on_the_previous_parent_directory() {
        assert!(relative_link_is_confined(1, Path::new("java-real")));
        assert!(relative_link_is_confined(2, Path::new("../libjava.so")));
        assert!(!relative_link_is_confined(
            1,
            Path::new("../../sibling/bin/java")
        ));
        assert!(!relative_link_is_confined(
            0,
            Path::new("../vendor/bin/java")
        ));
        assert!(!relative_link_is_confined(
            1,
            Path::new("/absolute/bin/java")
        ));
        #[cfg(windows)]
        {
            assert!(!relative_link_is_confined(1, Path::new("C:java.exe")));
            assert!(!relative_link_is_confined(1, Path::new("C:\\java.exe")));
            assert!(!relative_link_is_confined(
                1,
                Path::new("\\\\server\\java.exe")
            ));
        }
    }

    #[cfg(unix)]
    #[test]
    fn jdk_zip_rejects_root_and_descendant_links_changed_by_relocation() {
        for root_link in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let base = directory.path().to_path_buf();
            let destination = base.join("zulu-16");
            let sibling = base.join("zulu-8");
            for dir in [&destination, &sibling] {
                std::fs::create_dir(dir).unwrap();
                std::fs::write(dir.join("sentinel"), b"original").unwrap();
            }
            let options = SimpleFileOptions::default();
            let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            archive
                .start_file("vendor/Contents/zulu-8/bin/java", options)
                .unwrap();
            archive.write_all(b"runtime").unwrap();
            if root_link {
                archive
                    .add_symlink("vendor/Contents/Home", "zulu-8", options)
                    .unwrap();
            } else {
                archive
                    .add_symlink(
                        "vendor/Contents/Home/bin/java",
                        "../../zulu-8/bin/java",
                        options,
                    )
                    .unwrap();
            }
            let bytes = archive.finish().unwrap().into_inner();
            assert!(extract_and_rename_zip(&bytes, &base, &destination).is_err());
            for dir in [&destination, &sibling] {
                assert_eq!(std::fs::read(dir.join("sentinel")).unwrap(), b"original");
            }
            assert_eq!(std::fs::read_dir(&base).unwrap().count(), 2);
        }
    }

    #[cfg(unix)]
    #[test]
    fn jdk_zip_rejects_external_links_and_keeps_internal_runtime_links() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().to_path_buf();
        let destination = base.join("managed");
        let options = SimpleFileOptions::default();
        for target in ["../../outside", "/tmp/outside"] {
            let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            archive
                .add_symlink("vendor/bin/java", target, options)
                .unwrap();
            let bytes = archive.finish().unwrap().into_inner();
            assert!(extract_and_rename_zip(&bytes, &base, &destination).is_err());
            assert!(!destination.exists());
        }
        let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        archive
            .start_file("vendor/bin/java-real", options.unix_permissions(0o755))
            .unwrap();
        archive.write_all(b"runtime").unwrap();
        archive
            .add_symlink("vendor/bin/java", "java-real", options)
            .unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        extract_and_rename_zip(&bytes, &base, &destination).unwrap();
        assert_eq!(
            std::fs::read(destination.join("bin/java")).unwrap(),
            b"runtime"
        );
    }
}
