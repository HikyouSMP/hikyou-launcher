use std::{
    collections::{HashMap, HashSet},
    process::Command,
    sync::{Mutex, OnceLock},
};

struct ProcessRegistry {
    running: HashMap<String, u32>,
    launching: HashSet<String>,
    pending_stops: HashSet<String>,
}

static PROCESS_REGISTRY: OnceLock<Mutex<ProcessRegistry>> = OnceLock::new();

fn registry() -> &'static Mutex<ProcessRegistry> {
    PROCESS_REGISTRY.get_or_init(|| {
        Mutex::new(ProcessRegistry {
            running: HashMap::new(),
            launching: HashSet::new(),
            pending_stops: HashSet::new(),
        })
    })
}

pub fn begin_launch(profile_id: &str) {
    if let Ok(mut state) = registry().lock() {
        state.launching.insert(profile_id.to_string());
    }
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

pub fn register(profile_id: &str, pid: u32) {
    let stop_was_requested = registry()
        .lock()
        .map(|mut state| {
            state.running.insert(profile_id.to_string(), pid);
            state.pending_stops.remove(profile_id)
        })
        .unwrap_or(false);

    if stop_was_requested {
        log::info!(
            "[process] Applying queued forced stop to {} (pid {})",
            profile_id,
            pid
        );
        if let Err(error) = stop_pid(pid) {
            log::error!(
                "[process] Queued forced stop failed for {}: {}",
                profile_id,
                error
            );
        } else {
            unregister(profile_id);
        }
    }
}

pub fn unregister(profile_id: &str) {
    if let Some(state) = PROCESS_REGISTRY.get()
        && let Ok(mut state) = state.lock()
    {
        state.running.remove(profile_id);
    }
}

pub fn stop(profile_id: &str) -> Result<(), String> {
    let pid = registry()
        .lock()
        .map_err(|_| "process registry lock was poisoned".to_string())
        .and_then(|mut state| match state.running.get(profile_id).copied() {
            Some(pid) => Ok(Some(pid)),
            None if state.launching.contains(profile_id) => {
                state.pending_stops.insert(profile_id.to_string());
                Ok(None)
            }
            None => Err("profile is not running".to_string()),
        })?;

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
    use super::{begin_launch, finish_launch, stop};

    #[test]
    fn pre_spawn_stop_does_not_leak_into_a_future_launch() {
        let profile_id = format!("test-stop-{}", uuid::Uuid::new_v4());
        begin_launch(&profile_id);
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
