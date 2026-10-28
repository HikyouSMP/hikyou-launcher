use std::path::Path;

pub(super) fn zip(entries: &[(&str, &[u8])], compression: zip::CompressionMethod) -> Vec<u8> {
    use std::io::Write;
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        archive
            .start_file(
                *name,
                zip::write::SimpleFileOptions::default().compression_method(compression),
            )
            .unwrap();
        archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap().into_inner()
}

pub(super) fn unsupported_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut bytes = zip(entries, zip::CompressionMethod::Stored);
    let archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let entry = archive.by_index_data(entries.len() - 1).unwrap();
    let local = entry.header_start() as usize;
    let central = entry.central_header_start() as usize;
    // Method 12 is Bzip2. Both headers agree; no optional decoder is needed for this fixture.
    bytes[local + 8..local + 10].copy_from_slice(&12u16.to_le_bytes());
    bytes[central + 10..central + 12].copy_from_slice(&12u16.to_le_bytes());
    bytes
}

pub(super) fn directory_link(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command",
                "New-Item -ItemType Junction -Path $env:HIKYOU_TEST_LINK -Target $env:HIKYOU_TEST_TARGET -ErrorAction Stop | Out-Null"])
            .env("HIKYOU_TEST_LINK", link)
            .env("HIKYOU_TEST_TARGET", target)
            .creation_flags(0x08000000)
            .output().unwrap();
        assert!(
            output.status.success(),
            "junction creation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
