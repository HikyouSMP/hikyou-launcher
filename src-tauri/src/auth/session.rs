//! One backend-owned authentication operation; account mutations and commits
//! share a lock so a completed logout cannot be undone by a late login result.

use std::sync::LazyLock;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::storage::{self, StoredAuth};

pub const CANCELLED: &str = "__user_cancelled__";
static STATE: LazyLock<Mutex<Sessions>> = LazyLock::new(|| Mutex::new(Sessions::default()));

#[derive(Default)]
struct Sessions {
    next: u64,
    active: Option<(u64, CancellationToken)>,
}

impl Sessions {
    fn begin(&mut self) -> Result<Operation, String> {
        if self
            .active
            .as_ref()
            .is_some_and(|(_, token)| !token.is_cancelled())
        {
            return Err("Microsoft authentication is already in progress.".into());
        }
        self.next = self
            .next
            .checked_add(1)
            .ok_or("Authentication session limit reached.")?;
        let token = CancellationToken::new();
        self.active = Some((self.next, token.clone()));
        Ok(Operation {
            id: self.next,
            token,
        })
    }

    fn cancel(&mut self) {
        if let Some((_, token)) = self.active.take() {
            token.cancel();
        }
    }

    fn accepts(&self, operation: &Operation) -> bool {
        !operation.token.is_cancelled()
            && self
                .active
                .as_ref()
                .is_some_and(|(id, _)| *id == operation.id)
    }
}

pub struct Operation {
    id: u64,
    pub token: CancellationToken,
}

impl Drop for Operation {
    fn drop(&mut self) {
        self.token.cancel();
    }
}

pub async fn begin() -> Result<Operation, String> {
    STATE.lock().await.begin()
}

pub async fn cancel() {
    STATE.lock().await.cancel();
}

pub async fn is_busy() -> bool {
    STATE
        .lock()
        .await
        .active
        .as_ref()
        .is_some_and(|(_, token)| !token.is_cancelled())
}

pub async fn commit(operation: &Operation, auth: StoredAuth) -> Result<StoredAuth, String> {
    commit_with(&STATE, operation, async {
        storage::save_auth(&auth).await?;
        Ok(auth)
    })
    .await
}

async fn commit_with<T>(
    state: &Mutex<Sessions>,
    operation: &Operation,
    write: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let state = state.lock().await;
    if !state.accepts(operation) {
        return Err(CANCELLED.into());
    }
    // Commit is the linearization point. Logout/switch/cancel wait for this
    // write, then take effect; none can acknowledge and be overwritten later.
    write.await
}

pub async fn logout() -> Result<(), String> {
    let mut state = STATE.lock().await;
    state.cancel();
    storage::delete_auth().await
}

pub async fn switch_account(uuid: String) -> Result<(), String> {
    let mut state = STATE.lock().await;
    state.cancel();
    let auth = storage::load_account_auth(uuid).await?;
    storage::save_auth(&auth).await
}

pub async fn delete_account(uuid: String) -> Result<(), String> {
    let mut state = STATE.lock().await;
    state.cancel();
    storage::delete_account_auth(uuid).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_login_is_rejected_and_cancelled_result_is_stale() {
        let mut sessions = Sessions::default();
        let first = sessions.begin().unwrap();
        assert!(sessions.begin().is_err());
        sessions.cancel();
        let second = sessions.begin().unwrap();
        assert!(!sessions.accepts(&first));
        assert!(sessions.accepts(&second));
        drop(second);
        assert!(sessions.begin().is_ok());
        assert!(!sessions.accepts(&first));
    }

    #[test]
    fn dropping_operation_cancels_its_os_work_and_allows_retry() {
        let mut sessions = Sessions::default();
        let operation = sessions.begin().unwrap();
        let token = operation.token.clone();
        drop(operation);
        assert!(token.is_cancelled());
        assert!(sessions.begin().is_ok());
    }

    #[tokio::test]
    async fn cancelled_or_superseded_completion_never_reaches_storage() {
        let state = Mutex::new(Sessions::default());
        let old = state.lock().await.begin().unwrap();
        state.lock().await.cancel();
        let _new = state.lock().await.begin().unwrap();
        let mut written = false;
        let result = commit_with(&state, &old, async {
            written = true;
            Ok(())
        })
        .await;
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert!(!written);
    }

    #[tokio::test]
    async fn cancellation_acknowledgement_is_ordered_after_an_inflight_commit() {
        use std::sync::Arc;
        let state = Arc::new(Mutex::new(Sessions::default()));
        let operation = state.lock().await.begin().unwrap();
        let (started, wait_started) = tokio::sync::oneshot::channel();
        let (finish, wait_finish) = tokio::sync::oneshot::channel();
        let writer = {
            let state = state.clone();
            tokio::spawn(async move {
                commit_with(&state, &operation, async {
                    started.send(()).unwrap();
                    wait_finish.await.unwrap();
                    Ok(())
                })
                .await
            })
        };
        wait_started.await.unwrap();
        assert!(state.try_lock().is_err());
        finish.send(()).unwrap();
        writer.await.unwrap().unwrap();
        state.lock().await.cancel();
        assert!(state.lock().await.active.is_none());
    }
}
