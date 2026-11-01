//! Bounded, token-free validation of launcher-owned flags. No Mod code is loaded.

use super::jvm_policy::Collector;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::{Duration, SystemTime},
};
use tokio::io::AsyncReadExt;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    path: PathBuf,
    length: u64,
    modified: SystemTime,
    release: [u8; 32],
    flags: Vec<String>,
}
static VERIFIED: LazyLock<Mutex<BTreeSet<Key>>> = LazyLock::new(Default::default);
const MAX_OUTPUT: u64 = 512 * 1024;

fn identity(path: &Path, flags: &[String]) -> Option<Key> {
    let path = path.canonicalize().ok()?;
    let metadata = std::fs::metadata(&path).ok()?;
    let release = path.parent()?.parent()?.join("release");
    let mut bytes = Vec::new();
    std::fs::File::open(release)
        .ok()?
        .take(65537)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 65536 {
        return None;
    }
    Some(Key {
        path,
        length: metadata.len(),
        modified: metadata.modified().ok()?,
        release: Sha256::digest(bytes).into(),
        flags: flags.to_vec(),
    })
}

async fn bounded(reader: impl tokio::io::AsyncRead + Unpin) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_OUTPUT + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| "Java preflight output could not be read")?;
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err("Java preflight output exceeds its limit".into());
    }
    Ok(bytes)
}

fn effective(output: &[u8], flags: &[String], collector: Collector) -> bool {
    let name = match collector {
        Collector::G1 => "UseG1GC",
        Collector::Shenandoah => "UseShenandoahGC",
        Collector::Zgc => "UseZGC",
        Collector::JvmDefault => return true,
        Collector::External => return false,
    };
    let mut required = vec![name];
    for flag in flags {
        if let Some(boolean) = flag.strip_prefix("-XX:+") {
            required.push(boolean);
        }
    }
    let text = String::from_utf8_lossy(output);
    required.iter().all(|name| {
        let values: Vec<_> = text
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                if parts.next()? != "bool" || parts.next()? != *name {
                    return None;
                }
                if !matches!(parts.next()?, "=" | ":=") {
                    return None;
                }
                parts.next()
            })
            .collect();
        values.as_slice() == ["true"]
    })
}

/// Some(cache_hit) means verified, None means the optional plan was rejected.
pub(super) async fn verify(
    path: &Path,
    flags: Vec<String>,
    collector: Collector,
) -> Result<Option<bool>, String> {
    let key = identity(path, &flags);
    if key
        .as_ref()
        .is_some_and(|key| VERIFIED.lock().is_ok_and(|cache| cache.contains(key)))
    {
        return Ok(Some(true));
    }
    let mut command = tokio::process::Command::new(path);
    command.args(["-Xms32M", "-Xmx128M"]).args(&flags);
    if collector != Collector::JvmDefault {
        command.arg("-XX:+PrintFlagsFinal");
    }
    command
        .arg("-version")
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let (success, output) = tokio::time::timeout(Duration::from_secs(10), async {
        let mut child = command.spawn().map_err(|error| {
            format!(
                "Cannot start Java preflight (OS error {:?})",
                error.raw_os_error()
            )
        })?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Java preflight stdout unavailable")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("Java preflight stderr unavailable")?;
        let (mut out, err) = tokio::try_join!(bounded(stdout), bounded(stderr))?;
        let status = child
            .wait()
            .await
            .map_err(|_| "Cannot observe Java preflight exit")?;
        out.extend(err);
        Ok::<_, String>((status.success(), out))
    })
    .await
    .map_err(|_| "Java preflight timed out; no game process was started")??;
    if !success || !effective(&output, &flags, collector) {
        return Ok(None);
    }
    let after = identity(path, &flags);
    if key != after {
        return Err("Java runtime changed during flag validation; retry the launch".into());
    }
    if let Some(key) = key
        && let Ok(mut cache) = VERIFIED.lock()
    {
        if cache.len() >= 32 {
            cache.pop_first();
        }
        cache.insert(key);
    }
    Ok(Some(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_identity_tracks_runtime_build_and_exact_options()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin)?;
        let java = bin.join("java");
        let release = root.path().join("release");
        std::fs::write(&java, b"fixture executable identity")?;
        std::fs::write(&release, b"JAVA_VERSION=25.0.1")?;
        let flags = vec!["-XX:+UseG1GC".into()];
        let before = identity(&java, &flags).ok_or("fixture identity missing")?;
        assert!(identity(&java, &flags).is_some_and(|key| key == before));
        assert!(identity(&java, &["-XX:+UseZGC".into()]).is_some_and(|key| key != before));
        std::fs::write(&release, b"JAVA_VERSION=25.0.2")?;
        assert!(identity(&java, &flags).is_some_and(|key| key != before));
        std::fs::write(&release, vec![b'x'; 65537])?;
        assert!(identity(&java, &flags).is_none());
        Ok(())
    }

    #[tokio::test]
    async fn probe_output_is_bounded() -> Result<(), String> {
        assert_eq!(bounded(&b"version"[..]).await?, b"version");
        let oversized = vec![0; MAX_OUTPUT as usize + 1];
        assert!(bounded(oversized.as_slice()).await.is_err());
        Ok(())
    }

    #[test]
    fn success_requires_effective_unique_flags_not_just_exit_zero() {
        let flags = vec!["-XX:+UseG1GC".into(), "-XX:+UseCompactObjectHeaders".into()];
        assert!(effective(
            b"bool UseG1GC = true {product}\nbool UseCompactObjectHeaders = true {product}",
            &flags,
            Collector::G1
        ));
        assert!(!effective(
            b"bool UseG1GC = true\nbool UseCompactObjectHeaders = false",
            &flags,
            Collector::G1
        ));
        assert!(!effective(
            b"bool UseG1GC = true\nbool UseG1GC = true\nbool UseCompactObjectHeaders = true",
            &flags,
            Collector::G1
        ));
        assert!(!effective(b"version 25", &flags, Collector::G1));
    }
    #[tokio::test]
    #[ignore = "executes explicitly selected trusted Java runtimes; no game or account"]
    async fn live_owned_flags() -> Result<(), String> {
        let path = PathBuf::from(
            std::env::var_os("HIKYOU_TEST_JAVA").ok_or("HIKYOU_TEST_JAVA is required")?,
        );
        let major: u32 = std::env::var("HIKYOU_TEST_JAVA_MAJOR")
            .map_err(|_| "HIKYOU_TEST_JAVA_MAJOR is required")?
            .parse()
            .map_err(|_| "invalid test Java major")?;
        let java = super::super::java::probe_java(&path, major).await?;
        for hardware in [
            super::super::jvm_policy::Hardware {
                cores: Some(8),
                total_mb: 32768,
                available_mb: 24576,
            },
            super::super::jvm_policy::Hardware {
                cores: Some(2),
                total_mb: 4096,
                available_mb: 1024,
            },
        ] {
            let plan = super::super::jvm_policy::prepare(&java, 6144, hardware, None, None).await?;
            assert!(!matches!(plan.collector, Collector::External));
            eprintln!(
                "Java {major}: {:?}, fallback={}, cached={}",
                plan.collector, plan.fallback, plan.cache_hit
            );
        }
        Ok(())
    }
}
