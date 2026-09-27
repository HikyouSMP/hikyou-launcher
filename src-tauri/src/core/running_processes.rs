use std::{
    collections::{HashMap, HashSet},
    process::Command,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::Notify;

pub const LAUNCH_CANCELLED_ERROR: &str = "__launch_cancelled__";

pub struct LaunchCancellation {
    cancelled: AtomicBool,
    notify: Notify,
}

impl LaunchCancellation {
    fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::AcqRel) {
            self.notify.notify_one();
        }
    }

    pub fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            Err(LAUNCH_CANCELLED_ERROR.to_string())
        } else {
            Ok(())
        }
    }

    pub async fn cancelled(&self) {
        let notified = self.notify.notified();
        if self.cancelled.load(Ordering::Acquire) {
            return;
        }
        notified.await;
    }
}

struct ProcessRegistry {
    running: HashMap<String, u32>,
    launching: HashMap<String, Arc<LaunchCancellation>>,
    pending_stops: HashSet<String>,
}

static PROCESS_REGISTRY: OnceLock<Mutex<ProcessRegistry>> = OnceLock::new();

fn registry() -> &'static Mutex<ProcessRegistry> {
    PROCESS_REGISTRY.get_or_init(|| {
        Mutex::new(ProcessRegistry {
            running: HashMap::new(),
            launching: HashMap::new(),
            pending_stops: HashSet::new(),
        })
    })
}

pub fn begin_launch(profile_id: &str) -> Arc<LaunchCancellation> {
    let cancellation = Arc::new(LaunchCancellation::new());
    if let Ok(mut state) = registry().lock() {
        state
            .launching
            .insert(profile_id.to_string(), Arc::clone(&cancellation));
    }
    cancellation
}

pub fn is_active(profile_id: &str) -> bool {
    registry()
        .lock()
        .map(|state| {
            state.launching.contains_key(profile_id) || state.running.contains_key(profile_id)
        })
        .unwrap_or(true)
}

pub fn finish_launch(profile_id: &str) {
    if let Ok(mut state) = registry().lock() {
        state.launching.remove(profile_id);
        // A failed pre-spawn launch must not leave a stop request that could
        // terminate a future, unrelated launch of this profile.
        if !state.running.contains_key(profile_id) {
            state.pending_stops.remove(profile_id);
        }
    }
}

pub fn register(profile_id: &str, pid: u32) -> Result<bool, String> {
    let stop_was_requested = registry()
        .lock()
        .map(|mut state| {
            state.running.insert(profile_id.to_string(), pid);
            state.pending_stops.remove(profile_id)
        })
        .map_err(|_| "process registry lock was poisoned".to_string())?;

    if stop_was_requested {
        log::info!(
            "[process] Applying queued forced stop to {} (pid {})",
            profile_id,
            pid
        );
        stop_pid(pid).map_err(|error| {
            log::error!(
                "[process] Queued forced stop failed for {}: {}",
                profile_id,
                error
            );
            error
        })?;
        unregister(profile_id);
    }
    Ok(stop_was_requested)
}

pub fn unregister(profile_id: &str) {
    if let Some(state) = PROCESS_REGISTRY.get()
        && let Ok(mut state) = state.lock()
    {
        state.running.remove(profile_id);
    }
}

pub fn stop(profile_id: &str) -> Result<(), String> {
    let (pid, cancellation) = registry()
        .lock()
        .map_err(|_| "process registry lock was poisoned".to_string())
        .and_then(|mut state| match state.running.get(profile_id).copied() {
            Some(pid) => Ok((Some(pid), state.launching.get(profile_id).cloned())),
            None if state.launching.contains_key(profile_id) => {
                let cancellation = state.launching.get(profile_id).cloned();
                state.pending_stops.insert(profile_id.to_string());
                Ok((None, cancellation))
            }
            None => Err("profile is not running".to_string()),
        })?;

    if let Some(cancellation) = cancellation {
        cancellation.cancel();
    }

    let Some(pid) = pid else {
        // Launch preparation can be in progress before Java exposes a PID. Keep
        // the user's intent and apply it at registration rather than silently
        // allowing the game to start after Stop was pressed.
        log::info!(
            "[process] Queued forced stop while {} is preparing",
            profile_id
        );
        return Ok(());
    };

    stop_pid(pid)?;
    unregister(profile_id);
    log::info!(
        "[process] Forced stop requested for {} (pid {})",
        profile_id,
        pid
    );
    Ok(())
}

#[cfg(target_os = "windows")]
fn stop_pid(pid: u32) -> Result<(), String> {
    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status()
        .map_err(|e| format!("failed to run taskkill: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("taskkill failed with status: {:?}", status.code()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{begin_launch, finish_launch, stop};

    #[tokio::test]
    async fn pre_spawn_stop_cancels_the_active_launch() {
        let profile_id = format!("test-cancel-{}", uuid::Uuid::new_v4());
        let cancellation = begin_launch(&profile_id);
        let waiting_cancellation = Arc::clone(&cancellation);
        let waiter = tokio::spawn(async move {
            waiting_cancellation.cancelled().await;
        });
        assert!(cancellation.check().is_ok());

        assert!(stop(&profile_id).is_ok());
        tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("cancellation waiter did not wake")
            .expect("cancellation waiter failed");
        assert!(cancellation.check().is_err());
        finish_launch(&profile_id);
    }

    #[test]
    fn pre_spawn_stop_does_not_leak_into_a_future_launch() {
        let profile_id = format!("test-stop-{}", uuid::Uuid::new_v4());
        let _cancellation = begin_launch(&profile_id);
        assert!(stop(&profile_id).is_ok());

        // This simulates a failed launch that never registered a Java PID.
        finish_launch(&profile_id);
        assert!(stop(&profile_id).is_err());
    }
}

#[cfg(not(target_os = "windows"))]
fn stop_pid(pid: u32) -> Result<(), String> {
    let status = Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status()
        .map_err(|e| format!("failed to run kill: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("kill failed with status: {:?}", status.code()))
    }
}
