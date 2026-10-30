//! Bounded native generations. The receipt is published only after all writes finish.
//! Source metadata/hash reuse is an I/O optimization, NOT authentication against a
//! same-user attacker (who can also rewrite receipts). Download verification remains
//! independent. Output bytes are hashed even on warm launches.

use cap_std::fs::Dir;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use super::managed_fs;

const SCHEMA: u32 = 1;
pub(super) const MAX_INPUTS: usize = 256;
const MAX_ENTRIES: usize = 65_536;
const MAX_OUTPUTS: usize = 2_048;
const MAX_JAR: u64 = 256 * 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_EXPANDED: u64 = 1024 * 1024 * 1024;
const MAX_NATIVE: u64 = 128 * 1024 * 1024;
const MAX_OUTPUT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_RECEIPT: u64 = 4 * 1024 * 1024;
const MAX_GENERATIONS: usize = 32;
const CURRENT: &str = "current.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct NativeInput {
    pub path: String,
    pub sha1: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    len: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
    identity: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct InputReceipt {
    input: NativeInput,
    stamp: Stamp,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
struct OutputReceipt {
    name: String,
    stamp: Stamp,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    schema: u32,
    version: String,
    os: String,
    arch: String,
    key: String,
    generation: String,
    inputs: Vec<InputReceipt>,
    outputs: Vec<OutputReceipt>,
}

fn stamp(file: &File) -> Result<Stamp, String> {
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        format!(
            "{}:{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.ctime(),
            metadata.ctime_nsec()
        )
    };
    #[cfg(windows)]
    let identity = {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
        };
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }
            .map_err(|e| e.to_string())?;
        format!(
            "{}:{}:{}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        )
    };
    Ok(Stamp {
        len: metadata.len(),
        modified: metadata.modified().map_err(|e| e.to_string())?,
        created: metadata.created().ok(),
        identity,
    })
}

fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn key(version: &str, inputs: &[InputReceipt]) -> Result<String, String> {
    Ok(hex_digest(
        &serde_json::to_vec(&(
            SCHEMA,
            std::env::consts::OS,
            std::env::consts::ARCH,
            version,
            inputs,
        ))
        .map_err(|e| e.to_string())?,
    ))
}

fn load_receipt(root: &Dir) -> Option<Receipt> {
    let file = managed_fs::read_file(root, Path::new(CURRENT)).ok()?;
    if file.metadata().ok()?.len() > MAX_RECEIPT {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_RECEIPT + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_RECEIPT {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

// The same opened, link-checked handle is hashed and subsequently parsed as ZIP.
fn hash_source(file: &mut File, input: &NativeInput, before: &Stamp) -> Result<String, String> {
    #[cfg(test)]
    count(0);
    file.rewind().map_err(|e| e.to_string())?;
    let mut sha256 = Sha256::new();
    let mut sha1 = sha1_smol::Sha1::new();
    let mut buffer = [0; 64 * 1024];
    let mut read = 0u64;
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        read += n as u64;
        if read > before.len {
            return Err("native source grew while hashing".to_string());
        }
        sha256.update(&buffer[..n]);
        sha1.update(&buffer[..n]);
    }
    if read != before.len || stamp(file)? != *before {
        return Err("native source changed while hashing".to_string());
    }
    if (input.size != 0 && input.size != read)
        || (!input.sha1.is_empty() && !input.sha1.eq_ignore_ascii_case(&sha1.digest().to_string()))
    {
        return Err(format!(
            "native source fails manifest verification: {}",
            input.path
        ));
    }
    file.rewind().map_err(|e| e.to_string())?;
    Ok(format!("{:x}", sha256.finalize()))
}

fn hash_output(file: &mut File, expected: u64) -> Result<String, String> {
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > expected {
            return Err("native output grew".to_string());
        }
        hash.update(&buffer[..n]);
    }
    if total != expected {
        return Err("native output was truncated".to_string());
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn validate_outputs(root: &Dir, receipt: &Receipt) -> Result<(), String> {
    if receipt.outputs.len() > MAX_OUTPUTS
        || !(0..MAX_GENERATIONS).any(|slot| receipt.generation == format!("g-{slot:02}"))
    {
        return Err("invalid native generation receipt".to_string());
    }
    let generation = managed_fs::directory(root, Path::new(&receipt.generation), false)?;
    let mut names = HashSet::new();
    let mut total = 0u64;
    for output in &receipt.outputs {
        let path = managed_fs::relative_path(&output.name)?;
        if path.components().count() != 1
            || !names.insert(output.name.clone())
            || output.stamp.len > MAX_NATIVE
            || !valid_digest(&output.sha256)
        {
            return Err("invalid native output receipt".to_string());
        }
        total = total
            .checked_add(output.stamp.len)
            .ok_or("native output size overflow")?;
        if total > MAX_OUTPUT_BYTES {
            return Err("native output budget exceeded".to_string());
        }
        let mut file = managed_fs::read_file(&generation, &path)?;
        if stamp(&file)? != output.stamp
            || hash_output(&mut file, output.stamp.len)? != output.sha256
            || stamp(&file)? != output.stamp
        {
            return Err("native output does not match its receipt".to_string());
        }
    }
    // Unexpected files (including links/subdirectories) invalidate the whole generation.
    let mut count = 0usize;
    for entry in generation.entries().map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        count += 1;
        if count > names.len() || !names.contains(&entry.file_name().to_string_lossy().into_owned())
        {
            return Err("unexpected native generation entry".to_string());
        }
    }
    if count != names.len() {
        return Err("missing native generation entry".to_string());
    }
    Ok(())
}

/// Returns a generation path, not a stable directory to overwrite on the next launch.
pub(super) fn prepare(
    libraries_path: &Path,
    cache_path: &Path,
    version: &str,
    inputs: &[NativeInput],
) -> Result<PathBuf, String> {
    if managed_fs::relative_path(version)?.components().count() != 1 || inputs.len() > MAX_INPUTS {
        return Err("invalid native version or too many native inputs".to_string());
    }
    let namespace = hex_digest(
        &serde_json::to_vec(&(
            SCHEMA,
            std::env::consts::OS,
            std::env::consts::ARCH,
            version,
        ))
        .map_err(|e| e.to_string())?,
    );
    let cache = managed_fs::root_or_create(cache_path)?;
    let root = managed_fs::directory(&cache, Path::new(&namespace), true)?;
    let previous = load_receipt(&root).filter(|r| {
        r.schema == SCHEMA
            && r.version == version
            && r.os == std::env::consts::OS
            && r.arch == std::env::consts::ARCH
            && r.inputs.len() <= MAX_INPUTS
            && r.inputs.iter().all(|input| valid_digest(&input.sha256))
            && key(version, &r.inputs).is_ok_and(|key| key == r.key)
    });
    let libraries = if inputs.is_empty() {
        None
    } else {
        Some(managed_fs::root(libraries_path)?)
    };
    let mut files = Vec::new();
    let mut identities = Vec::new();
    let mut source_bytes = 0u64;
    if let Some(libraries) = &libraries {
        for (index, input) in inputs.iter().enumerate() {
            let path = managed_fs::relative_path(&input.path)?;
            if !input.sha1.is_empty()
                && (input.sha1.len() != 40 || !input.sha1.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                return Err(format!("invalid native manifest SHA-1: {}", input.path));
            }
            let mut file = managed_fs::read_file(libraries, &path)
                .map_err(|e| format!("cannot read native input {}: {e}", input.path))?;
            let before = stamp(&file)?;
            if input.size != 0 && input.size != before.len {
                return Err(format!(
                    "native source fails manifest verification: {}",
                    input.path
                ));
            }
            source_bytes = source_bytes
                .checked_add(before.len)
                .ok_or("native source size overflow")?;
            if before.len > MAX_JAR || source_bytes > MAX_SOURCE_BYTES {
                return Err("native source budget exceeded".to_string());
            }
            let reused = previous
                .as_ref()
                .and_then(|r| r.inputs.get(index))
                .filter(|old| {
                    old.input == *input && old.stamp == before && valid_digest(&old.sha256)
                });
            let sha256 = match reused {
                Some(old) => old.sha256.clone(),
                None => hash_source(&mut file, input, &before)?,
            };
            identities.push(InputReceipt {
                input: input.clone(),
                stamp: before,
                sha256,
            });
            files.push(file);
        }
    }
    let input_key = key(version, &identities)?;
    if let Some(old) = &previous
        && old.key == input_key
        && validate_outputs(&root, old).is_ok()
    {
        if let Some(libraries) = &libraries {
            verify_sources(libraries, &identities)?;
        }
        return Ok(cache_path.join(&namespace).join(&old.generation));
    }

    // Fixed exclusive slots bound retained disk use, including crash leftovers and
    // concurrent builds. Never evict a generation: a running JVM may still use it.
    let mut generation_name = None;
    for slot in 0..MAX_GENERATIONS {
        let name = format!("g-{slot:02}");
        match root.create_dir(&name) {
            Ok(()) => {
                generation_name = Some(name);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("cannot reserve native generation: {e}")),
        }
    }
    let generation_name = generation_name.ok_or_else(|| format!(
        "native cache has {MAX_GENERATIONS} retained generations; close games and remove unused cache directory {}",
        cache_path.join(&namespace).display()
    ))?;
    let generation = managed_fs::directory(&root, Path::new(&generation_name), false)?;
    let result = (|| {
        let outputs = extract(&generation, &mut files)?;
        if let Some(libraries) = &libraries {
            verify_sources(libraries, &identities)?;
        }
        let receipt = Receipt {
            schema: SCHEMA,
            version: version.to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            key: input_key,
            generation: generation_name.clone(),
            inputs: identities,
            outputs,
        };
        let bytes = serde_json::to_vec(&receipt).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_RECEIPT {
            return Err("native receipt budget exceeded".to_string());
        }
        // This atomic receipt replacement is the only publication point. A crash
        // beforehand leaves an unreachable slot, never a reusable partial result.
        managed_fs::write(&root, Path::new(CURRENT), &bytes, true)?;
        Ok(cache_path.join(&namespace).join(&generation_name))
    })();
    drop(generation);
    if result.is_err() {
        let _ = root.remove_dir_all(&generation_name);
    }
    result
}

fn verify_sources(libraries: &Dir, inputs: &[InputReceipt]) -> Result<(), String> {
    for input in inputs {
        let file = managed_fs::read_file(libraries, Path::new(&input.input.path))?;
        if stamp(&file)? != input.stamp {
            return Err("native source changed during preparation".to_string());
        }
    }
    Ok(())
}

fn extract(destination: &Dir, files: &mut [File]) -> Result<Vec<OutputReceipt>, String> {
    let mut outputs = Vec::new();
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    let mut output_bytes = 0u64;
    let mut entries = 0usize;
    for file in files {
        #[cfg(test)]
        count(1);
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("invalid native ZIP: {e}"))?;
        entries = entries
            .checked_add(archive.len())
            .ok_or("native ZIP entry count overflow")?;
        if entries > MAX_ENTRIES {
            return Err("too many native ZIP entries".to_string());
        }
        for index in 0..archive.len() {
            let entry = archive.by_index_data(index).map_err(|e| e.to_string())?;
            expanded = expanded
                .checked_add(entry.size())
                .ok_or("native ZIP expanded size overflow")?;
            if expanded > MAX_EXPANDED {
                return Err("native ZIP expansion budget exceeded".to_string());
            }
            let mode = entry.unix_mode().unwrap_or(0) & 0o170000;
            if entry.is_symlink() || !matches!(mode, 0 | 0o100000 | 0o040000) {
                return Err("native ZIP links and special files are unsupported".to_string());
            }
        }
        super::archive::preflight_zip(&mut archive, MAX_ENTRIES, MAX_EXPANDED)?;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
            let name = entry.name().map_err(|e| e.to_string())?.into_owned();
            if entry.is_dir()
                || name.starts_with("META-INF")
                || ![".dll", ".so", ".dylib", ".jnilib"]
                    .iter()
                    .any(|ext| name.ends_with(ext))
            {
                continue;
            }
            // Validate before flattening, so traversal and platform aliases cannot
            // become apparently safe basenames. Link metadata is never followed.
            let path = managed_fs::relative_path(&name)?;
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or("invalid native filename")?
                .to_string();
            if !names.insert(name.to_lowercase()) {
                continue;
            } // Preserve first-library precedence.
            output_bytes = output_bytes
                .checked_add(entry.size())
                .ok_or("native output size overflow")?;
            if entry.size() > MAX_NATIVE
                || output_bytes > MAX_OUTPUT_BYTES
                || outputs.len() >= MAX_OUTPUTS
            {
                return Err("native output budget exceeded".to_string());
            }
            let expected = entry.size();
            let mut hash = Sha256::new();
            managed_fs::write_with(destination, Path::new(&name), false, |out| {
                let mut total = 0u64;
                let mut buffer = [0; 64 * 1024];
                loop {
                    let n = entry
                        .read(&mut buffer)
                        .map_err(|e| format!("corrupt native ZIP: {e}"))?;
                    if n == 0 {
                        break;
                    }
                    total += n as u64;
                    if total > expected {
                        return Err("native ZIP entry exceeded declared size".to_string());
                    }
                    out.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
                    hash.update(&buffer[..n]);
                }
                if total != expected {
                    return Err("truncated native ZIP entry".to_string());
                }
                Ok(())
            })?;
            #[cfg(test)]
            count(2);
            let output = managed_fs::read_file(destination, Path::new(&name))?;
            outputs.push(OutputReceipt {
                name,
                stamp: stamp(&output)?,
                sha256: format!("{:x}", hash.finalize()),
            });
        }
    }
    Ok(outputs)
}

#[cfg(test)]
thread_local! {
    static COUNTS: std::cell::Cell<[u64; 3]> = const { std::cell::Cell::new([0; 3]) };
}

#[cfg(test)]
fn count(index: usize) {
    COUNTS.with(|cell| {
        let mut counts = cell.get();
        counts[index] += 1;
        cell.set(counts);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::test_support;
    use std::io::Cursor;
    use zip::{CompressionMethod, write::SimpleFileOptions};

    #[test]
    fn empty_inputs_do_not_require_a_libraries_directory() -> Result<(), Box<dyn std::error::Error>>
    {
        let temp = tempfile::tempdir()?;
        let libraries = temp.path().join("missing-libraries");
        let cache = temp.path().join("cache");
        let cold = prepare(&libraries, &cache, "empty", &[])?;
        assert_eq!(std::fs::read_dir(&cold)?.count(), 0);
        assert_eq!(prepare(&libraries, &cache, "empty", &[])?, cold);
        assert!(!libraries.exists());
        Ok(())
    }

    #[test]
    fn missing_native_input_returns_its_path_in_the_error() -> Result<(), Box<dyn std::error::Error>>
    {
        let temp = tempfile::tempdir()?;
        let libraries = temp.path().join("libraries");
        std::fs::create_dir(&libraries)?;
        let error = prepare(
            &libraries,
            &temp.path().join("cache"),
            "missing",
            &[NativeInput {
                path: "missing.jar".into(),
                sha1: String::new(),
                size: 0,
            }],
        )
        .err()
        .ok_or("missing native input was accepted")?;
        assert!(error.contains("cannot read native input missing.jar:"));
        Ok(())
    }

    struct Fixture {
        _temp: tempfile::TempDir,
        libraries: PathBuf,
        cache: PathBuf,
        inputs: Vec<NativeInput>,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let libraries = temp.path().join("libraries");
            let cache = temp.path().join("cache");
            std::fs::create_dir(&libraries).unwrap();
            Self {
                _temp: temp,
                libraries,
                cache,
                inputs: Vec::new(),
            }
        }

        fn jar(&mut self, name: &str, entries: &[(&str, &[u8])]) {
            let bytes = test_support::zip(entries, CompressionMethod::Deflated);
            self.bytes(name, &bytes);
        }

        fn bytes(&mut self, name: &str, bytes: &[u8]) {
            std::fs::write(self.libraries.join(name), bytes).unwrap();
            let input = NativeInput {
                path: name.to_string(),
                size: bytes.len() as u64,
                sha1: sha1_smol::Sha1::from(bytes).digest().to_string(),
            };
            if let Some(old) = self.inputs.iter_mut().find(|input| input.path == name) {
                *old = input;
            } else {
                self.inputs.push(input);
            }
        }

        fn prepare(&self) -> Result<PathBuf, String> {
            prepare(&self.libraries, &self.cache, "fixture-1", &self.inputs)
        }
    }

    fn reset_counts() {
        COUNTS.with(|cell| cell.set([0; 3]));
    }
    fn counts() -> [u64; 3] {
        COUNTS.with(|cell| cell.get())
    }

    #[test]
    fn warm_reuses_generation_without_source_reads_zip_opens_or_extraction() {
        let mut fixture = Fixture::new();
        fixture.jar(
            "native.jar",
            &[
                ("nested/a.dll", b"native"),
                ("META-INF/ignored.dll", b"ignore"),
            ],
        );
        reset_counts();
        let cold = fixture.prepare().unwrap();
        assert_eq!(counts(), [1, 1, 1]);
        reset_counts();
        assert_eq!(fixture.prepare().unwrap(), cold);
        assert_eq!(counts(), [0, 0, 0]);
        assert_eq!(std::fs::read(cold.join("a.dll")).unwrap(), b"native");
    }

    #[test]
    fn source_content_order_and_declared_identity_invalidate() {
        let mut fixture = Fixture::new();
        fixture.jar("one.jar", &[("a.dll", b"first")]);
        fixture.jar("two.jar", &[("a.dll", b"second")]);
        let first = fixture.prepare().unwrap();
        fixture.inputs.reverse();
        let reordered = fixture.prepare().unwrap();
        assert_ne!(first, reordered);
        assert_eq!(std::fs::read(reordered.join("a.dll")).unwrap(), b"second");
        fixture.jar("two.jar", &[("a.dll", b"changed")]);
        let changed = fixture.prepare().unwrap();
        assert_ne!(reordered, changed);
        assert_eq!(std::fs::read(changed.join("a.dll")).unwrap(), b"changed");
        assert_eq!(std::fs::read(first.join("a.dll")).unwrap(), b"first");
        fixture.inputs[0].sha1 = fixture.inputs[0].sha1.to_uppercase();
        let metadata_changed = fixture.prepare().unwrap();
        assert_ne!(metadata_changed, changed);
        fixture.inputs[0].sha1 = "0".repeat(40);
        assert!(
            fixture
                .prepare()
                .unwrap_err()
                .contains("manifest verification")
        );
    }

    #[test]
    fn metadata_change_without_manifest_change_rehashes_and_verifies() {
        let mut fixture = Fixture::new();
        fixture.jar("native.jar", &[("a.dll", b"native")]);
        let old = fixture.prepare().unwrap();
        let path = fixture.libraries.join("native.jar");
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(SystemTime::UNIX_EPOCH))
            .unwrap();
        drop(file);
        reset_counts();
        let new = fixture.prepare().unwrap();
        assert_ne!(old, new);
        assert_eq!(counts(), [1, 1, 1]);
        let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(b"bad").unwrap();
        drop(file);
        assert!(fixture.prepare().is_err());
    }

    #[test]
    fn missing_truncated_corrupt_and_extra_outputs_rebuild_without_overwriting_old_generation() {
        for damage in ["missing", "truncated", "same-size", "extra"] {
            let mut fixture = Fixture::new();
            fixture.jar("native.jar", &[("a.dll", b"native")]);
            let old = fixture.prepare().unwrap();
            let output = old.join("a.dll");
            match damage {
                "missing" => std::fs::remove_file(&output).unwrap(),
                "truncated" => std::fs::write(&output, b"n").unwrap(),
                "same-size" => {
                    let modified = std::fs::metadata(&output).unwrap().modified().unwrap();
                    std::fs::write(&output, b"broken").unwrap();
                    File::options()
                        .write(true)
                        .open(&output)
                        .unwrap()
                        .set_times(std::fs::FileTimes::new().set_modified(modified))
                        .unwrap();
                }
                "extra" => std::fs::write(old.join("extra.dll"), b"injected").unwrap(),
                _ => unreachable!(),
            }
            let before = std::fs::read(&output).ok();
            let new = fixture.prepare().unwrap();
            assert_ne!(old, new, "{damage}");
            assert_eq!(std::fs::read(new.join("a.dll")).unwrap(), b"native");
            assert_eq!(std::fs::read(output).ok(), before);
        }
    }

    #[test]
    fn corrupt_or_missing_receipt_rebuilds_and_never_trusts_directory_existence() {
        let mut fixture = Fixture::new();
        fixture.jar("native.jar", &[("a.dll", b"native")]);
        let old = fixture.prepare().unwrap();
        let receipt = old.parent().unwrap().join(CURRENT);
        for bytes in [b"{partial".as_slice(), b"{}", b""] {
            std::fs::write(&receipt, bytes).unwrap();
            let new = fixture.prepare().unwrap();
            assert_ne!(old, new);
        }
        std::fs::remove_file(receipt).unwrap();
        assert_ne!(old, fixture.prepare().unwrap());
    }

    #[test]
    fn inconsistent_input_receipt_cannot_seed_the_source_hash_cache() {
        let mut fixture = Fixture::new();
        fixture.jar("native.jar", &[("a.dll", b"native")]);
        let old = fixture.prepare().unwrap();
        let root = managed_fs::root(old.parent().unwrap()).unwrap();
        let mut receipt = load_receipt(&root).unwrap();
        receipt.inputs[0].sha256 = "0".repeat(64);
        managed_fs::write(
            &root,
            Path::new(CURRENT),
            &serde_json::to_vec(&receipt).unwrap(),
            true,
        )
        .unwrap();
        reset_counts();
        assert_ne!(fixture.prepare().unwrap(), old);
        assert_eq!(counts(), [1, 1, 1]);
        let repaired = load_receipt(&root).unwrap();
        assert_ne!(repaired.inputs[0].sha256, receipt.inputs[0].sha256);
    }

    #[test]
    fn interrupted_and_failed_generations_are_not_published() {
        let mut fixture = Fixture::new();
        fixture.jar("one.jar", &[("a.dll", b"old")]);
        let old = fixture.prepare().unwrap();
        let root = old.parent().unwrap();
        let receipt_before = std::fs::read(root.join(CURRENT)).unwrap();
        // A killed process can leave a partial slot, but cannot publish it.
        std::fs::create_dir(root.join("g-01")).unwrap();
        std::fs::write(root.join("g-01/a.dll"), b"partial").unwrap();
        assert_eq!(fixture.prepare().unwrap(), old);
        fixture.jar("one.jar", &[("a.dll", b"new")]);
        fixture.bytes("late.jar", b"not a ZIP");
        reset_counts();
        assert!(fixture.prepare().is_err());
        assert_eq!(counts()[2], 1); // First JAR extracted before the later failure.
        assert_eq!(std::fs::read(root.join(CURRENT)).unwrap(), receipt_before);
        assert!(!root.join("g-02").exists());
        assert_eq!(std::fs::read(old.join("a.dll")).unwrap(), b"old");
        fixture.inputs.pop();
        let new = fixture.prepare().unwrap();
        assert_eq!(new.file_name().unwrap(), "g-02");
    }

    #[test]
    fn input_and_archive_unsafe_paths_are_rejected_before_flattening() {
        for name in [
            "../a.dll",
            "/a.dll",
            "C:/a.dll",
            "a\\b.dll",
            "a/../b.dll",
            "a//b.dll",
            "a.dll:stream",
        ] {
            let mut fixture = Fixture::new();
            fixture.jar("native.jar", &[(name, b"bad")]);
            assert!(fixture.prepare().is_err(), "accepted {name}");
            fixture.inputs[0].path = name.to_string();
            assert!(fixture.prepare().is_err(), "accepted source {name}");
        }
        let fixture = Fixture::new();
        assert!(prepare(&fixture.libraries, &fixture.cache, "../version", &[]).is_err());
    }

    #[test]
    fn archive_symlinks_are_never_materialized() {
        let mut fixture = Fixture::new();
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .add_symlink("a.dll", "../outside", SimpleFileOptions::default())
            .unwrap();
        fixture.bytes("native.jar", &archive.finish().unwrap().into_inner());
        assert!(fixture.prepare().unwrap_err().contains("links"));
    }

    #[test]
    fn corrupt_payload_and_unsupported_compression_leave_previous_receipt_untouched() {
        let mut fixture = Fixture::new();
        fixture.jar("native.jar", &[("a.dll", b"native")]);
        let old = fixture.prepare().unwrap();
        let receipt = old.parent().unwrap().join(CURRENT);
        let previous = std::fs::read(&receipt).unwrap();
        let mut corrupt = test_support::zip(&[("a.dll", b"native")], CompressionMethod::Stored);
        let mut archive = zip::ZipArchive::new(Cursor::new(&corrupt)).unwrap();
        let offset = archive.by_index_raw(0).unwrap().data_start().unwrap() as usize;
        corrupt[offset] ^= 1;
        for bytes in [
            corrupt,
            test_support::unsupported_zip(&[("a.dll", b"native")]),
        ] {
            fixture.bytes("native.jar", &bytes);
            assert!(fixture.prepare().is_err());
            assert_eq!(std::fs::read(&receipt).unwrap(), previous);
            assert_eq!(std::fs::read(old.join("a.dll")).unwrap(), b"native");
        }
    }

    #[test]
    fn source_and_output_hardlinks_are_not_reused_or_overwritten() {
        let mut fixture = Fixture::new();
        fixture.jar("native.jar", &[("a.dll", b"native")]);
        let old = fixture.prepare().unwrap();
        let peer = fixture._temp.path().join("peer.dll");
        std::fs::hard_link(old.join("a.dll"), &peer).unwrap();
        let new = fixture.prepare().unwrap();
        assert_ne!(old, new);
        assert_eq!(std::fs::read(&peer).unwrap(), b"native");
        let jar_peer = fixture._temp.path().join("peer.jar");
        std::fs::hard_link(fixture.libraries.join("native.jar"), jar_peer).unwrap();
        assert!(fixture.prepare().unwrap_err().contains("hard-linked"));
    }

    #[test]
    fn linked_generations_cache_roots_and_source_ancestors_are_rejected() {
        let mut fixture = Fixture::new();
        fixture.jar("native.jar", &[("a.dll", b"native")]);
        let old = fixture.prepare().unwrap();
        let outside = fixture._temp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("a.dll"), b"outside").unwrap();
        std::fs::remove_file(old.join("a.dll")).unwrap();
        std::fs::remove_dir(&old).unwrap();
        test_support::directory_link(&outside, &old);
        let rebuilt = fixture.prepare().unwrap();
        assert_ne!(old, rebuilt);
        assert_eq!(std::fs::read(outside.join("a.dll")).unwrap(), b"outside");
        test_support::directory_link(&fixture.libraries, &fixture.libraries.join("alias"));
        fixture.inputs[0].path = "alias/native.jar".to_string();
        assert!(fixture.prepare().is_err());
        let linked_cache = fixture._temp.path().join("linked-cache");
        test_support::directory_link(&fixture.cache, &linked_cache);
        assert!(prepare(&fixture.libraries, &linked_cache, "v", &[]).is_err());
    }

    #[test]
    fn declared_expansion_and_entry_budgets_are_enforced() {
        let mut fixture = Fixture::new();
        let mut bytes = test_support::zip(&[("a.dll", b"native")], CompressionMethod::Stored);
        let archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let entry = archive.by_index_data(0).unwrap();
        let central = entry.central_header_start() as usize;
        let local = entry.header_start() as usize;
        bytes[central + 24..central + 28]
            .copy_from_slice(&((MAX_EXPANDED + 1) as u32).to_le_bytes());
        bytes[local + 22..local + 26].copy_from_slice(&((MAX_EXPANDED + 1) as u32).to_le_bytes());
        fixture.bytes("native.jar", &bytes);
        assert!(fixture.prepare().unwrap_err().contains("budget"));
        assert!(
            prepare(
                &fixture.libraries,
                &fixture.cache,
                "v",
                &vec![fixture.inputs[0].clone(); MAX_INPUTS + 1]
            )
            .is_err()
        );
        let file = File::create(fixture.libraries.join("large.jar")).unwrap();
        file.set_len(MAX_JAR + 1).unwrap();
        fixture.inputs = vec![NativeInput {
            path: "large.jar".to_string(),
            sha1: String::new(),
            size: 0,
        }];
        assert!(fixture.prepare().unwrap_err().contains("budget"));
    }

    #[test]
    fn generation_budget_is_hard_bounded_but_does_not_break_valid_warm_launches() {
        let mut fixture = Fixture::new();
        fixture.jar("native.jar", &[("a.dll", b"native")]);
        let old = fixture.prepare().unwrap();
        let root = old.parent().unwrap();
        for slot in 1..MAX_GENERATIONS {
            std::fs::create_dir(root.join(format!("g-{slot:02}"))).unwrap();
        }
        assert_eq!(fixture.prepare().unwrap(), old);
        std::fs::write(old.join("a.dll"), b"bad").unwrap();
        assert!(
            fixture
                .prepare()
                .unwrap_err()
                .contains("retained generations")
        );
        assert_eq!(
            std::fs::read_dir(root).unwrap().count(),
            MAX_GENERATIONS + 1
        );
    }

    #[test]
    fn os_arch_version_and_input_hash_are_bound_into_key() {
        let fixture = Fixture::new();
        let first = fixture.prepare().unwrap();
        let root = managed_fs::root(first.parent().unwrap()).unwrap();
        let mut receipt = load_receipt(&root).unwrap();
        receipt.arch = "other-arch".to_string();
        managed_fs::write(
            &root,
            Path::new(CURRENT),
            &serde_json::to_vec(&receipt).unwrap(),
            true,
        )
        .unwrap();
        assert_ne!(fixture.prepare().unwrap(), first);
        let other_version = prepare(&fixture.libraries, &fixture.cache, "fixture-2", &[]).unwrap();
        assert_ne!(first.parent(), other_version.parent());
    }

    #[test]
    fn concurrent_builds_publish_complete_separate_generations() {
        let mut fixture = Fixture::new();
        fixture.jar("native.jar", &[("a.dll", b"native")]);
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            let launch = || {
                barrier.wait();
                fixture.prepare().unwrap()
            };
            let a = scope.spawn(launch);
            let b = scope.spawn(launch);
            for result in [a.join().unwrap(), b.join().unwrap()] {
                assert_eq!(std::fs::read(result.join("a.dll")).unwrap(), b"native");
            }
        });
        assert_eq!(
            std::fs::read(fixture.prepare().unwrap().join("a.dll")).unwrap(),
            b"native"
        );
    }

    #[test]
    #[ignore = "synthetic extraction microbenchmark, not full game launch timing"]
    fn synthetic_cold_warm_timing() {
        let mut fixture = Fixture::new();
        let payload = (0..1024 * 1024)
            .map(|i| ((i * 17 + i / 257) % 251) as u8)
            .collect::<Vec<_>>();
        let names = (0..16)
            .map(|i| format!("native-{i}.dll"))
            .collect::<Vec<_>>();
        let entries = names
            .iter()
            .map(|name| (name.as_str(), payload.as_slice()))
            .collect::<Vec<_>>();
        fixture.jar("native.jar", &entries);
        reset_counts();
        let start = std::time::Instant::now();
        let cold = fixture.prepare().unwrap();
        let cold_time = start.elapsed();
        let cold_counts = counts();
        reset_counts();
        let start = std::time::Instant::now();
        for _ in 0..10 {
            assert_eq!(fixture.prepare().unwrap(), cold);
        }
        let warm_time = start.elapsed() / 10;
        assert_eq!(counts(), [0, 0, 0]);
        eprintln!(
            "SYNTHETIC ONLY: 1 JAR / 16 natives / 16 MiB output; cold={cold_time:?}, warm_mean_10={warm_time:?}; cold counts={cold_counts:?}, warm counts={:?} (source hashes, ZIP opens, extractions). NOT full game launch timing.",
            counts()
        );
    }
}
