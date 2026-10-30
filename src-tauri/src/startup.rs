//! Commands become available only after required startup resources are ready.

use std::sync::OnceLock;

#[derive(Default)]
pub(crate) struct Gate(OnceLock<Result<(), String>>);

impl Gate {
    pub(crate) fn complete(&self, result: Result<(), String>) -> Result<(), String> {
        self.0
            .set(result)
            .map_err(|_| "startup result was already published".into())
    }

    pub(crate) fn require_ready(&self) -> Result<(), String> {
        self.0
            .get()
            .cloned()
            .unwrap_or_else(|| Err("startup is not complete".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::Gate;

    #[test]
    fn commands_require_one_explicit_successful_completion() -> Result<(), String> {
        let pending = Gate::default();
        assert!(pending.require_ready().is_err());
        pending.complete(Ok(()))?;
        pending.require_ready()?;
        assert!(pending.complete(Err("late error".into())).is_err());
        pending.require_ready()?;
        let failed = Gate::default();
        failed.complete(Err("database unavailable".into()))?;
        assert_eq!(failed.require_ready(), Err("database unavailable".into()));
        assert!(failed.complete(Ok(())).is_err());
        assert!(failed.require_ready().is_err());
        Ok(())
    }
}
