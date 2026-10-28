//! Owns Java process observation and termination; launch planning stays in launcher.rs.

use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Child,
    task::JoinSet,
};

use super::running_processes::{self, LaunchCancellation};

pub(super) async fn supervise(
    mut child: Child,
    app: &AppHandle,
    version_id: &str,
    profile_id: &str,
    cancellation: LaunchCancellation,
    leases: Vec<std::sync::Arc<std::fs::File>>,
    mut observer: Option<super::mod_health::Observer>,
) -> Result<(), String> {
    let Some(pid) = child.id() else {
        if let Some(observer) = &mut observer {
            observer.spawned().await;
            observer
                .supervision_failed(None, cancellation.check().is_err())
                .await;
        }
        return Err("Java process has no process identifier".into());
    };
    if let Err(error) = running_processes::register(profile_id, pid) {
        let _ = child.kill().await;
        let code = child.wait().await.ok().and_then(|status| status.code());
        if let Some(observer) = &mut observer {
            observer.spawned().await;
            observer
                .supervision_failed(code, cancellation.check().is_err())
                .await;
        }
        return Err(error);
    }
    let mut readers = JoinSet::new();
    forward_logs(&mut readers, child.stdout.take(), app, profile_id);
    forward_logs(&mut readers, child.stderr.take(), app, profile_id);
    let app = app.clone();
    let version_id = version_id.to_string();
    let profile_id = profile_id.to_string();
    tokio::spawn(async move {
        // No await between successful registration and handing the child to this task.
        if let Some(observer) = &mut observer {
            observer.spawned().await;
        }
        let observation = wait_for_exit(&mut child, &cancellation).await;
        if let Some(observer) = &mut observer {
            observer
                .exited(
                    observation
                        .status
                        .as_ref()
                        .ok()
                        .and_then(|status| status.code()),
                    observation
                        .status
                        .as_ref()
                        .is_ok_and(|status| status.success()),
                    observation.stopped,
                    observation.status.is_err(),
                )
                .await;
        }
        drop(leases);
        // Drain final log lines before exit. Descendants must not keep this
        // task alive indefinitely by retaining an inherited output pipe.
        if tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while readers.join_next().await.is_some() {}
        })
        .await
        .is_err()
        {
            readers.shutdown().await;
        }
        let exit_code = match observation.status {
            Ok(status) => {
                log::info!("Minecraft exited: {:?}", status.code());
                status.code()
            }
            Err(error) => {
                log::error!("Process wait failed: {error}");
                None
            }
        };
        let _ = app.emit(
            "game://exit",
            serde_json::json!({
                "version_id": version_id,
                "profile_id": profile_id,
                "exit_code": exit_code,
                "stopped": observation.stopped,
            }),
        );
        running_processes::unregister(&profile_id, pid);
    });
    Ok(())
}

struct ExitObservation {
    status: std::io::Result<std::process::ExitStatus>,
    stopped: bool,
}

async fn wait_for_exit(child: &mut Child, cancellation: &LaunchCancellation) -> ExitObservation {
    tokio::select! {
        biased;
        result = child.wait() => ExitObservation { status: result, stopped: false },
        _ = cancellation.cancelled() => {
            // Prefer an already observed exit over a simultaneous stop request.
            match child.try_wait() {
                Ok(Some(status)) => return ExitObservation { status: Ok(status), stopped: false },
                Err(error) => return ExitObservation { status: Err(error), stopped: false },
                Ok(None) => {}
            }
            #[cfg(target_os = "windows")]
            if let Some(pid) = child.id()
                && let Err(error) = stop_windows_tree(pid).await
            {
                log::warn!("Process tree stop failed; terminating Java directly: {error}");
            }
            let _ = child.kill().await;
            ExitObservation { status: child.wait().await, stopped: true }
        }
    }
}

fn forward_logs<R: AsyncRead + Unpin + Send + 'static>(
    readers: &mut JoinSet<()>,
    pipe: Option<R>,
    app: &AppHandle,
    profile_id: &str,
) {
    if let Some(pipe) = pipe {
        let app = app.clone();
        let profile_id = profile_id.to_string();
        readers.spawn(async move {
            let mut lines = BufReader::new(pipe).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app.emit(
                    "game://log",
                    serde_json::json!({
                        "profile_id": profile_id, "line": line,
                    }),
                );
            }
        });
    }
}

#[cfg(target_os = "windows")]
async fn stop_windows_tree(pid: u32) -> Result<(), String> {
    let system = std::env::var_os("SystemRoot").ok_or("Windows system directory is unavailable")?;
    let output = tokio::process::Command::new(
        std::path::PathBuf::from(system).join("System32/taskkill.exe"),
    )
    .args(["/PID", &pid.to_string(), "/T", "/F"])
    .creation_flags(0x0800_0000)
    .kill_on_drop(true)
    .output()
    .await
    .map_err(|error| format!("failed to stop process tree: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "process tree stop returned {:?}",
            output.status.code()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{process::Stdio, time::Duration};

    #[test]
    fn child_fixture() {
        if let Ok(code) = std::env::var("HIKYOU_OUTCOME_TEST_EXIT") {
            std::process::exit(code.parse().unwrap());
        }
        if std::env::var_os("HIKYOU_STOP_TEST_CHILD").is_some() {
            std::thread::sleep(Duration::from_secs(30));
        }
    }

    #[tokio::test]
    async fn cancellation_terminates_and_reaps_owned_child() {
        let key = format!("stop-test-{}", uuid::Uuid::new_v4());
        let guard = running_processes::LaunchGuard::begin(key.clone()).unwrap();
        let cancellation = guard.cancellation();
        let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "core::game_process::tests::child_fixture"])
            .env("HIKYOU_STOP_TEST_CHILD", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let pid = child.id().unwrap();
        running_processes::register(&key, pid).unwrap();
        running_processes::request_stop(&key).unwrap();
        let observation = tokio::time::timeout(
            Duration::from_secs(5),
            wait_for_exit(&mut child, cancellation),
        )
        .await
        .unwrap();
        assert!(observation.stopped);
        observation.status.unwrap();
        assert!(child.id().is_none());
        running_processes::unregister(&key, pid);
    }

    #[tokio::test]
    async fn synthetic_exit_codes_are_preserved_and_late_stop_does_not_change_exit() {
        for code in [0, 23] {
            let key = format!("outcome-test-{}", uuid::Uuid::new_v4());
            let guard = running_processes::LaunchGuard::begin(key.clone()).unwrap();
            let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "core::game_process::tests::child_fixture"])
                .env("HIKYOU_OUTCOME_TEST_EXIT", code.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            // Reap first to deterministically model a stop request arriving after natural exit.
            child.wait().await.unwrap();
            running_processes::request_stop(&key).unwrap();
            let observation = wait_for_exit(&mut child, guard.cancellation()).await;
            assert!(!observation.stopped);
            let status = observation.status.unwrap();
            assert_eq!(status.code(), Some(code));
            assert_eq!(status.success(), code == 0);
        }
    }
}
