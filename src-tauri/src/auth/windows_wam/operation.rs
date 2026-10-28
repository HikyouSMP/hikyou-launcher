use std::time::Duration;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use windows::{
    Foundation::{
        AsyncActionCompletedHandler, AsyncOperationCompletedHandler, IAsyncAction, IAsyncOperation,
    },
    core::RuntimeType,
};

use crate::auth::session::CANCELLED;

pub(super) fn win<T>(stage: &str, result: windows::core::Result<T>) -> Result<T, String> {
    result.map_err(|error| {
        // Error text can contain account information. Preserve only a fixed
        // local stage and the numerical OS error, never the provider message.
        if error.code().0 == 0 {
            format!("Windows authentication result is unavailable. [{stage}:not_found]")
        } else {
            format!(
                "Windows authentication failed. [{stage}:0x{:08X}]",
                error.code().0 as u32
            )
        }
    })
}

pub(super) async fn on_ui<T: Send + 'static>(
    app: &tauri::AppHandle,
    cancel: &CancellationToken,
    start: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = oneshot::channel();
    let token = cancel.clone();
    app.run_on_main_thread(move || {
        if !token.is_cancelled() && !sender.is_closed() {
            let _ = sender.send(start());
        }
    })
    .map_err(|_| "Windows authentication UI dispatch failed.")?;
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(CANCELLED.into()),
        result = tokio::time::timeout(Duration::from_secs(20), receiver) => {
            result.map_err(|_| "Windows authentication UI dispatch timed out.")?
                .map_err(|_| CANCELLED.to_string())?
        }
    }
}

pub(super) struct Pending<T: RuntimeType + 'static>(IAsyncOperation<T>);

impl<T: RuntimeType + 'static> Pending<T> {
    pub(super) fn new(operation: IAsyncOperation<T>) -> Self {
        Self(operation)
    }
}

impl<T: RuntimeType + Send + 'static> Pending<T> {
    pub(super) async fn wait(
        self,
        cancel: &CancellationToken,
        timeout: Duration,
    ) -> Result<T, String> {
        let (sender, receiver) = oneshot::channel();
        let mut sender = Some(sender);
        win(
            "completion_handler",
            self.0.SetCompleted(&AsyncOperationCompletedHandler::new(
                move |operation: Option<&IAsyncOperation<T>>, _| {
                    if let Some(sender) = sender.take() {
                        let result = operation
                            .ok_or_else(|| {
                                "Windows authentication operation disappeared.".to_string()
                            })
                            .and_then(|operation| win("completion", operation.GetResults()));
                        let _ = sender.send(result);
                    }
                    Ok(())
                },
            )),
        )?;
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(CANCELLED.into()),
            result = tokio::time::timeout(timeout, receiver) => {
                let result = result.map_err(|_| "Windows authentication timed out.")?
                    .map_err(|_| "Windows authentication completion was unavailable.")?;
                if cancel.is_cancelled() { Err(CANCELLED.into()) } else { result }
            }
        }
    }
}

impl<T: RuntimeType + 'static> Drop for Pending<T> {
    fn drop(&mut self) {
        // Also runs when the owning Rust future is dropped. Late OS callbacks
        // can only send to the abandoned receiver, not commit credentials.
        let _ = self.0.Cancel();
    }
}

pub(super) struct PendingAction(IAsyncAction);

impl PendingAction {
    pub(super) fn new(operation: IAsyncAction) -> Self {
        Self(operation)
    }

    pub(super) async fn wait(
        self,
        cancel: &CancellationToken,
        timeout: Duration,
    ) -> Result<(), String> {
        let (sender, receiver) = oneshot::channel();
        let mut sender = Some(sender);
        win(
            "action_completion",
            self.0
                .SetCompleted(&AsyncActionCompletedHandler::new(move |operation, _| {
                    if let Some(sender) = sender.take() {
                        let result = operation
                            .ok_or_else(|| "Windows authentication action disappeared.".to_string())
                            .and_then(|operation| win("action_result", operation.GetResults()));
                        let _ = sender.send(result);
                    }
                    Ok(())
                })),
        )?;
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(CANCELLED.into()),
            result = tokio::time::timeout(timeout, receiver) => {
                result.map_err(|_| "Windows authentication action timed out.")?
                    .map_err(|_| "Windows authentication action completion was unavailable.".to_string())?
            }
        }
    }
}

impl Drop for PendingAction {
    fn drop(&mut self) {
        let _ = self.0.Cancel();
    }
}
