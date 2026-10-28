use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

pub const LAUNCH_CANCELLED_ERROR: &str = "__launch_cancelled__";

#[derive(Clone, Default)]
pub struct LaunchCancellation(CancellationToken);

impl LaunchCancellation {
    pub async fn cancelled(&self) {
        self.0.cancelled().await;
    }

    pub fn check(&self) -> Result<(), String> {
        if self.0.is_cancelled() {
            Err(LAUNCH_CANCELLED_ERROR.to_string())
        } else {
            Ok(())
        }
    }

    pub async fn run<T>(
        &self,
        future: impl Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        tokio::select! {
            biased;
            _ = self.0.cancelled() => Err(LAUNCH_CANCELLED_ERROR.to_string()),
            result = future => result,
        }
    }
}

#[derive(Default)]
struct Run {
    cancellation: LaunchCancellation,
    finished: CancellationToken,
}

struct Entry {
    run: Arc<Run>,
    preparing: bool,
    pid: Option<u32>,
}

static RUNS: LazyLock<Mutex<HashMap<String, Entry>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Owns one launch attempt. A live Java process retains its entry after preparation ends.
pub struct LaunchGuard {
    profile_id: String,
    run: Arc<Run>,
}

impl LaunchGuard {
    pub fn begin(profile_id: String) -> Result<Self, String> {
        let mut runs = RUNS
            .lock()
            .map_err(|_| "process registry lock was poisoned")?;
        if runs.contains_key(&profile_id) {
            return Err("profile is already running or preparing to launch".to_string());
        }
        let run = Arc::new(Run::default());
        runs.insert(
            profile_id.clone(),
            Entry {
                run: Arc::clone(&run),
                preparing: true,
                pid: None,
            },
        );
        Ok(Self { profile_id, run })
    }

    pub fn cancellation(&self) -> &LaunchCancellation {
        &self.run.cancellation
    }
}

impl Drop for LaunchGuard {
    fn drop(&mut self) {
        if let Ok(mut runs) = RUNS.lock()
            && let Some(entry) = runs.get_mut(&self.profile_id)
            && Arc::ptr_eq(&entry.run, &self.run)
        {
            entry.preparing = false;
            finish_if_idle(&mut runs, &self.profile_id);
        }
    }
}

fn finish_if_idle(runs: &mut HashMap<String, Entry>, profile_id: &str) {
    if runs
        .get(profile_id)
        .is_some_and(|entry| !entry.preparing && entry.pid.is_none())
        && let Some(entry) = runs.remove(profile_id)
    {
        entry.run.finished.cancel();
    }
}

pub fn is_active(profile_id: &str) -> bool {
    RUNS.lock()
        .map(|runs| runs.contains_key(profile_id))
        .unwrap_or(true)
}

pub fn register(profile_id: &str, pid: u32) -> Result<(), String> {
    let mut runs = RUNS
        .lock()
        .map_err(|_| "process registry lock was poisoned")?;
    let entry = runs.get_mut(profile_id).ok_or("launch is not registered")?;
    if entry.pid.is_some() {
        return Err("profile already has a Java process".to_string());
    }
    entry.pid = Some(pid);
    Ok(())
}

/// A stale wait task must never remove a newer process belonging to this profile.
pub fn unregister(profile_id: &str, pid: u32) -> bool {
    let Ok(mut runs) = RUNS.lock() else {
        return false;
    };
    let Some(entry) = runs.get_mut(profile_id) else {
        return false;
    };
    if entry.pid != Some(pid) {
        return false;
    }
    let stopped = entry.run.cancellation.0.is_cancelled();
    entry.pid = None;
    finish_if_idle(&mut runs, profile_id);
    stopped
}

pub fn request_stop(profile_id: &str) -> Result<Option<CancellationToken>, String> {
    let snapshot = {
        let runs = RUNS
            .lock()
            .map_err(|_| "process registry lock was poisoned")?;
        runs.get(profile_id).map(|entry| Arc::clone(&entry.run))
    };
    let Some(run) = snapshot else { return Ok(None) };
    run.cancellation.0.cancel();
    Ok(Some(run.finished.clone()))
}

pub async fn stop(profile_id: &str) -> Result<(), String> {
    if let Some(finished) = request_stop(profile_id)? {
        tokio::time::timeout(Duration::from_secs(5), finished.cancelled())
            .await
            .map_err(|_| "timed out while stopping the profile".to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_id() -> String {
        format!("test-run-{}", uuid::Uuid::new_v4())
    }

    #[tokio::test]
    async fn stop_wakes_all_waiters_and_finishes_only_after_preparation_exits() {
        let id = profile_id();
        let guard = LaunchGuard::begin(id.clone()).unwrap();
        let cancellation = guard.cancellation().clone();
        let first = cancellation.run(std::future::pending::<Result<(), String>>());
        let second = cancellation.run(std::future::pending::<Result<(), String>>());
        let finished = request_stop(&id).unwrap().unwrap();
        assert!(!finished.is_cancelled());
        let (first, second) = tokio::join!(first, second);
        assert!(first.is_err() && second.is_err());
        drop(guard);
        assert!(finished.is_cancelled());
        assert!(!is_active(&id));
        assert!(
            LaunchGuard::begin(id)
                .unwrap()
                .cancellation()
                .check()
                .is_ok()
        );
    }

    #[test]
    fn running_process_blocks_relaunch_and_stale_exit_cannot_clear_it() {
        let id = profile_id();
        let guard = LaunchGuard::begin(id.clone()).unwrap();
        register(&id, 123).unwrap();
        drop(guard);
        assert!(LaunchGuard::begin(id.clone()).is_err());
        assert!(!unregister(&id, 122));
        assert!(is_active(&id));
        unregister(&id, 123);
        assert!(!is_active(&id));
    }
}
