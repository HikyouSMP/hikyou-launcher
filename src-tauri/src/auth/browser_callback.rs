//! Callback waiting is independent of WebView and OS APIs so every host can test it.

use std::time::Duration;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const TIMED_OUT: &str = "authentication timed out after 10 minutes";

pub async fn receive_code(
    receiver: oneshot::Receiver<Result<Zeroizing<String>, String>>,
    cancellation: &CancellationToken,
    timeout: Duration,
) -> Result<Zeroizing<String>, String> {
    let result = tokio::select! {
        biased;
        _ = cancellation.cancelled() => return Err(super::session::CANCELLED.into()),
        result = tokio::time::timeout(timeout, receiver) => result,
    };
    result
        .map_err(|_| TIMED_OUT.to_string())?
        .map_err(|_| super::session::CANCELLED.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn completed_callback_returns_its_owned_code() {
        let (sender, receiver) = oneshot::channel();
        sender.send(Ok(Zeroizing::new("test-code".into()))).unwrap();
        let code = receive_code(receiver, &CancellationToken::new(), Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(code.as_str(), "test-code");
    }

    #[tokio::test]
    async fn cancelled_operation_rejects_even_an_already_available_code() {
        let (sender, receiver) = oneshot::channel();
        sender.send(Ok(Zeroizing::new("test-code".into()))).unwrap();
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert_eq!(
            receive_code(receiver, &cancellation, Duration::from_secs(1))
                .await
                .unwrap_err(),
            super::super::session::CANCELLED
        );
    }

    #[tokio::test]
    async fn callback_failure_and_closed_channel_remain_distinct_from_timeout() {
        let (sender, receiver) = oneshot::channel();
        sender.send(Err("callback rejected".into())).unwrap();
        assert_eq!(
            receive_code(receiver, &CancellationToken::new(), Duration::from_secs(1))
                .await
                .unwrap_err(),
            "callback rejected"
        );
        let (sender, receiver) = oneshot::channel();
        drop(sender);
        assert_eq!(
            receive_code(receiver, &CancellationToken::new(), Duration::from_secs(1))
                .await
                .unwrap_err(),
            super::super::session::CANCELLED
        );
    }

    #[tokio::test]
    async fn pending_callback_expires() {
        let (_sender, receiver) = oneshot::channel();
        assert_eq!(
            receive_code(receiver, &CancellationToken::new(), Duration::ZERO)
                .await
                .unwrap_err(),
            TIMED_OUT
        );
    }
}
