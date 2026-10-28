//! Factual process outcomes, not evidence of title-screen arrival or Mod compatibility.

use super::{managed_fs, profile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    io::Read,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::Instant,
};

const DIRECTORY: &str = "state/mod-launch-outcomes";
const MAX_BYTES: u64 = 16 * 1024;
static ACTIVE: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static IO: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Preparing,
    Spawned,
    NormalExit,
    Failed,
    Stopped,
    UnknownInterrupted,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    PreparationStarted,
    ProcessSpawned,
    ProcessExit,
    PreparationFailed,
    SupervisionFailed,
    WaitFailed,
    UserStop,
    ObservationLost,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub attempt_id: String,
    pub profile_id: String,
    pub mod_revision: String,
    pub outcome: Outcome,
    pub evidence: Evidence,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub elapsed_ms: u64,
    pub process_elapsed_ms: Option<u64>,
    pub exit_code: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Latest {
    pub latest_attempt: Attempt,
    /// Only an observed successful process exit, never title-screen success.
    pub last_normal_exit: Option<Attempt>,
}

/// An owned observer; dropping it before a terminal update leaves an interrupted attempt.
pub struct Observer {
    root: PathBuf,
    attempt: Attempt,
    started: Instant,
    spawned: Option<Instant>,
}

impl Drop for Observer {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE.lock() {
            active.remove(&self.attempt.attempt_id);
        }
    }
}

fn validate_revision(id: &str) -> Result<(), String> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid Mod revision ID".into());
    }
    Ok(())
}

fn filename(profile_id: &str) -> PathBuf {
    PathBuf::from(format!("{:x}.json", Sha256::digest(profile_id.as_bytes())))
}

fn read(dir: &cap_std::fs::Dir, profile_id: &str) -> Result<Option<Latest>, String> {
    let path = filename(profile_id);
    if managed_fs::inspect(dir, &path)?.is_none() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    managed_fs::read_file(dir, &path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read Mod launch outcome")?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Mod launch outcome exceeds size limit".into());
    }
    let state: Latest =
        serde_json::from_slice(&bytes).map_err(|_| "invalid Mod launch outcome record")?;
    for attempt in std::iter::once(&state.latest_attempt).chain(state.last_normal_exit.iter()) {
        profile::validate_profile_ref(&attempt.profile_id)?;
        validate_revision(&attempt.mod_revision)?;
        uuid::Uuid::parse_str(&attempt.attempt_id).map_err(|_| "invalid launch attempt ID")?;
        if attempt.profile_id != profile_id {
            return Err("Mod launch outcome profile mismatch".into());
        }
        chrono::DateTime::parse_from_rfc3339(&attempt.started_at)
            .map_err(|_| "invalid launch start time")?;
        if let Some(at) = &attempt.finished_at {
            chrono::DateTime::parse_from_rfc3339(at).map_err(|_| "invalid launch finish time")?;
        }
        if attempt.outcome == Outcome::NormalExit
            && (attempt.exit_code != Some(0)
                || attempt.evidence != Evidence::ProcessExit
                || attempt.finished_at.is_none())
        {
            return Err("invalid normal process exit record".into());
        }
    }
    if state
        .last_normal_exit
        .as_ref()
        .is_some_and(|a| a.outcome != Outcome::NormalExit)
    {
        return Err("invalid last normal process exit".into());
    }
    Ok(Some(state))
}

/// Synchronous, bounded read. Async callers should use spawn_blocking.
/// `root` is LauncherPaths::root(), not a game directory or Mod generation.
pub fn latest(root: &Path, profile_id: &str) -> Result<Option<Latest>, String> {
    profile::validate_profile_ref(profile_id)?;
    let _io = IO.lock().map_err(|_| "Mod outcome lock poisoned")?;
    let root = managed_fs::root(root)?;
    if managed_fs::inspect(&root, Path::new(DIRECTORY))?.is_none() {
        return Ok(None);
    }
    let dir = managed_fs::directory(&root, Path::new(DIRECTORY), false)?;
    let mut state = read(&dir, profile_id)?;
    if let Some(state) = &mut state
        && matches!(
            state.latest_attempt.outcome,
            Outcome::Preparing | Outcome::Spawned
        )
        && !ACTIVE
            .lock()
            .map_err(|_| "Mod observer lock poisoned")?
            .contains(&state.latest_attempt.attempt_id)
    {
        // Do not invent an exit code, end time, or extra elapsed time after observation ceased.
        state.latest_attempt.outcome = Outcome::UnknownInterrupted;
        state.latest_attempt.evidence = Evidence::ObservationLost;
    }
    Ok(state)
}

fn write(root: &Path, attempt: &Attempt, begin: bool) -> Result<(), String> {
    let _io = IO.lock().map_err(|_| "Mod outcome lock poisoned")?;
    let root = managed_fs::root(root)?;
    let dir = managed_fs::directory(&root, Path::new(DIRECTORY), true)?;
    // Serialize read/compare/write across launcher instances without replacing the lock inode.
    if managed_fs::inspect(&dir, Path::new(".lock"))?.is_none() {
        match managed_fs::write(&dir, Path::new(".lock"), b"", false) {
            Ok(()) => {}
            Err(_) if managed_fs::inspect(&dir, Path::new(".lock"))?.is_some() => {}
            Err(error) => return Err(error),
        }
    }
    let lock = managed_fs::read_file(&dir, Path::new(".lock"))?;
    fs2::FileExt::lock_exclusive(&lock).map_err(|_| "cannot lock Mod outcomes")?;
    let previous = read(&dir, &attempt.profile_id)?;
    if !begin
        && !previous.as_ref().is_some_and(|state| {
            state.latest_attempt.attempt_id == attempt.attempt_id
                && matches!(
                    state.latest_attempt.outcome,
                    Outcome::Preparing | Outcome::Spawned
                )
        })
    {
        return Ok(()); // Stale observers and terminal attempts cannot overwrite newer facts.
    }
    let last_normal_exit = if attempt.outcome == Outcome::NormalExit {
        Some(attempt.clone())
    } else {
        previous.and_then(|state| state.last_normal_exit)
    };
    let state = Latest {
        latest_attempt: attempt.clone(),
        last_normal_exit,
    };
    let bytes = serde_json::to_vec(&state).map_err(|_| "cannot encode Mod outcome")?;
    managed_fs::write(&dir, &filename(&attempt.profile_id), &bytes, true)
}

pub async fn begin(root: &Path, profile_id: &str, mod_revision: &str) -> Result<Observer, String> {
    profile::validate_profile_ref(profile_id)?;
    validate_revision(mod_revision)?;
    let root = root.to_path_buf();
    let profile_id = profile_id.to_string();
    let mod_revision = mod_revision.to_string();
    tokio::task::spawn_blocking(move || {
        let observer = Observer {
            root,
            attempt: Attempt {
                attempt_id: uuid::Uuid::new_v4().to_string(),
                profile_id,
                mod_revision,
                outcome: Outcome::Preparing,
                evidence: Evidence::PreparationStarted,
                started_at: chrono::Utc::now().to_rfc3339(),
                finished_at: None,
                elapsed_ms: 0,
                process_elapsed_ms: None,
                exit_code: None,
            },
            started: Instant::now(),
            spawned: None,
        };
        ACTIVE
            .lock()
            .map_err(|_| "Mod observer lock poisoned")?
            .insert(observer.attempt.attempt_id.clone());
        write(&observer.root, &observer.attempt, true)?;
        Ok(observer)
    })
    .await
    .map_err(|_| "Mod outcome task interrupted")?
}

impl Observer {
    pub(super) fn matches(
        &self,
        root: &Path,
        profile: Option<&str>,
        revision: Option<&str>,
    ) -> bool {
        self.root == root
            && profile == Some(self.attempt.profile_id.as_str())
            && revision == Some(self.attempt.mod_revision.as_str())
            && self.attempt.outcome == Outcome::Preparing
    }

    async fn update(&mut self, outcome: Outcome, evidence: Evidence, code: Option<i32>) {
        if !matches!(self.attempt.outcome, Outcome::Preparing | Outcome::Spawned) {
            return;
        }
        self.attempt.outcome = outcome;
        self.attempt.evidence = evidence;
        self.attempt.exit_code = code;
        self.attempt.elapsed_ms = millis(self.started.elapsed());
        self.attempt.process_elapsed_ms = self.spawned.map(|at| millis(at.elapsed()));
        if !matches!(outcome, Outcome::Preparing | Outcome::Spawned) {
            self.attempt.finished_at = Some(chrono::Utc::now().to_rfc3339());
        }
        let root = self.root.clone();
        let attempt = self.attempt.clone();
        let result = tokio::task::spawn_blocking(move || write(&root, &attempt, false)).await;
        if !matches!(result, Ok(Ok(()))) {
            // No raw errors: OS errors can contain absolute paths or sensitive inputs.
            log::warn!("Mod launch outcome could not be persisted");
        }
    }

    pub(super) fn mark_spawned(&mut self) {
        self.spawned.get_or_insert_with(Instant::now);
    }

    pub(super) async fn spawned(&mut self) {
        self.mark_spawned();
        self.update(Outcome::Spawned, Evidence::ProcessSpawned, None)
            .await;
    }

    pub async fn preparation_failed(&mut self, cancelled: bool) {
        if self.attempt.outcome != Outcome::Preparing {
            return;
        }
        let (outcome, evidence) = if cancelled {
            (Outcome::Stopped, Evidence::UserStop)
        } else {
            (Outcome::Failed, Evidence::PreparationFailed)
        };
        self.update(outcome, evidence, None).await;
    }

    pub(super) async fn supervision_failed(&mut self, code: Option<i32>, stopped: bool) {
        self.update(
            if stopped {
                Outcome::Stopped
            } else {
                Outcome::Failed
            },
            if stopped {
                Evidence::UserStop
            } else {
                Evidence::SupervisionFailed
            },
            code,
        )
        .await;
    }

    pub(super) async fn exited(
        &mut self,
        code: Option<i32>,
        success: bool,
        stopped: bool,
        wait_failed: bool,
    ) {
        let (outcome, evidence) = if stopped {
            (Outcome::Stopped, Evidence::UserStop)
        } else if wait_failed {
            (Outcome::UnknownInterrupted, Evidence::WaitFailed)
        } else if success && code == Some(0) && self.spawned.is_some() {
            (Outcome::NormalExit, Evidence::ProcessExit)
        } else {
            (Outcome::Failed, Evidence::ProcessExit)
        };
        self.update(outcome, evidence, code).await;
    }
}

fn millis(duration: std::time::Duration) -> u64 {
    duration.as_millis().min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    const PROFILE: &str = "550e8400-e29b-41d4-a716-446655440000";
    const REVISION: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[tokio::test]
    async fn failed_preparation_preserves_last_normal_and_revision() {
        let temp = tempfile::tempdir().unwrap();
        let mut first = begin(temp.path(), PROFILE, REVISION).await.unwrap();
        first.spawned().await;
        first.exited(Some(0), true, false, false).await;
        let normal = latest(temp.path(), PROFILE)
            .unwrap()
            .unwrap()
            .latest_attempt;
        let revision = "b".repeat(64);
        let mut second = begin(temp.path(), PROFILE, &revision).await.unwrap();
        second.preparation_failed(false).await;
        let state = latest(temp.path(), PROFILE).unwrap().unwrap();
        assert_eq!(state.latest_attempt.outcome, Outcome::Failed);
        assert_eq!(state.latest_attempt.mod_revision, revision);
        assert_eq!(state.last_normal_exit, Some(normal));
    }

    #[tokio::test]
    async fn synthetic_process_outcomes_are_factual() {
        let temp = tempfile::tempdir().unwrap();
        for (code, success, stopped, wait_failed, expected) in [
            (Some(0), true, false, false, Outcome::NormalExit),
            (Some(7), false, false, false, Outcome::Failed),
            (None, false, false, false, Outcome::Failed),
            (Some(0), true, true, false, Outcome::Stopped),
            (Some(9), false, true, false, Outcome::Stopped),
            (None, false, true, true, Outcome::Stopped),
            (None, false, false, true, Outcome::UnknownInterrupted),
            (Some(8), true, false, false, Outcome::Failed),
        ] {
            let mut observer = begin(temp.path(), PROFILE, REVISION).await.unwrap();
            observer.spawned().await;
            observer.exited(code, success, stopped, wait_failed).await;
            let state = latest(temp.path(), PROFILE).unwrap().unwrap();
            assert_eq!(state.latest_attempt.outcome, expected);
            assert_eq!(state.latest_attempt.exit_code, code);
            assert!(state.latest_attempt.process_elapsed_ms.is_some());
            if expected != Outcome::NormalExit {
                assert_ne!(
                    state.last_normal_exit.as_ref().unwrap().attempt_id,
                    observer.attempt.attempt_id
                );
            }
        }
    }

    #[tokio::test]
    async fn cancelled_preparation_is_not_failure_or_normal_exit() {
        let temp = tempfile::tempdir().unwrap();
        let mut observer = begin(temp.path(), PROFILE, REVISION).await.unwrap();
        observer.preparation_failed(true).await;
        let state = latest(temp.path(), PROFILE).unwrap().unwrap();
        assert_eq!(state.latest_attempt.outcome, Outcome::Stopped);
        assert_eq!(state.latest_attempt.exit_code, None);
        assert_eq!(state.last_normal_exit, None);
    }

    #[tokio::test]
    async fn stale_attempt_and_terminal_rewrites_are_ignored() {
        let temp = tempfile::tempdir().unwrap();
        let mut old = begin(temp.path(), PROFILE, REVISION).await.unwrap();
        old.spawned().await;
        let mut new = begin(temp.path(), PROFILE, REVISION).await.unwrap();
        old.exited(Some(0), true, false, false).await;
        let state = latest(temp.path(), PROFILE).unwrap().unwrap();
        assert_eq!(state.latest_attempt.attempt_id, new.attempt.attempt_id);
        assert_eq!(state.last_normal_exit, None);
        new.preparation_failed(true).await;
        new.exited(Some(0), true, false, false).await;
        assert_eq!(
            latest(temp.path(), PROFILE)
                .unwrap()
                .unwrap()
                .latest_attempt
                .outcome,
            Outcome::Stopped
        );
    }

    #[tokio::test]
    async fn abnormal_exit_never_creates_a_normal_baseline() {
        for (code, success) in [(Some(23), false), (None, false), (Some(0), false)] {
            let temp = tempfile::tempdir().unwrap();
            let mut observer = begin(temp.path(), PROFILE, REVISION).await.unwrap();
            observer.spawned().await;
            observer.exited(code, success, false, false).await;
            let state = latest(temp.path(), PROFILE).unwrap().unwrap();
            assert_eq!(state.latest_attempt.outcome, Outcome::Failed);
            assert_eq!(state.latest_attempt.exit_code, code);
            assert_eq!(state.last_normal_exit, None);
        }
    }

    #[tokio::test]
    async fn coordinator_handoff_requires_same_profile_revision_and_root() {
        let temp = tempfile::tempdir().unwrap();
        let mut observer = begin(temp.path(), PROFILE, REVISION).await.unwrap();
        assert!(observer.matches(temp.path(), Some(PROFILE), Some(REVISION)));
        assert!(!observer.matches(temp.path(), None, Some(REVISION)));
        assert!(!observer.matches(temp.path(), Some(PROFILE), Some(&"b".repeat(64))));
        assert!(!observer.matches(&temp.path().join("other"), Some(PROFILE), Some(REVISION)));
        observer.preparation_failed(true).await;
        assert!(!observer.matches(temp.path(), Some(PROFILE), Some(REVISION)));
    }

    #[tokio::test]
    async fn interrupted_preparation_and_spawn_remain_distinguishable() {
        let temp = tempfile::tempdir().unwrap();
        for spawned in [false, true] {
            let mut observer = begin(temp.path(), PROFILE, REVISION).await.unwrap();
            if spawned {
                observer.spawned().await;
            }
            let active = latest(temp.path(), PROFILE).unwrap().unwrap();
            assert_eq!(
                active.latest_attempt.outcome,
                if spawned {
                    Outcome::Spawned
                } else {
                    Outcome::Preparing
                }
            );
            drop(observer);
            let state = latest(temp.path(), PROFILE).unwrap().unwrap();
            assert_eq!(state.latest_attempt.outcome, Outcome::UnknownInterrupted);
            assert_eq!(state.latest_attempt.evidence, Evidence::ObservationLost);
            assert_eq!(state.latest_attempt.process_elapsed_ms.is_some(), spawned);
            assert_eq!(state.latest_attempt.finished_at, None);
            assert_eq!(state.latest_attempt.exit_code, None);
        }
    }

    #[tokio::test]
    async fn validates_ids_and_persists_only_bounded_structured_facts() {
        let temp = tempfile::tempdir().unwrap();
        for profile in ["../escape", "smart:../escape", "", "C:\\private"] {
            assert!(begin(temp.path(), profile, REVISION).await.is_err());
            assert!(latest(temp.path(), profile).is_err());
        }
        for revision in ["../escape", "short", &"A".repeat(64)] {
            assert!(begin(temp.path(), PROFILE, revision).await.is_err());
        }
        assert!(latest(temp.path(), PROFILE).unwrap().is_none());
        for _ in 0..8 {
            let mut observer = begin(temp.path(), PROFILE, REVISION).await.unwrap();
            observer.preparation_failed(false).await;
        }
        let dir = temp.path().join(DIRECTORY);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2); // one record and one lock
        let bytes = std::fs::read(dir.join(filename(PROFILE))).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        for forbidden in [
            "token",
            "args",
            "username",
            "healthy",
            "compatible",
            temp.path().to_str().unwrap(),
        ] {
            assert!(!text.contains(forbidden));
        }
    }
}
