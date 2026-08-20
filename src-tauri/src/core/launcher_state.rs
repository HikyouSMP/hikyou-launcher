use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

static STATE: OnceLock<Arc<LauncherStateStore>> = OnceLock::new();

const MAX_LAUNCH_METRICS_ROWS: i64 = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchMetricStage {
    pub name: String,
    pub ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchMetricsRecord {
    #[serde(default)]
    pub id: Option<i64>,
    pub profile_id: String,
    pub version_id: String,
    #[serde(default)]
    pub created_at: Option<i64>,
    pub total_pre_spawn_ms: i64,
    pub java_spawn_ms: i64,
    pub stages: Vec<LaunchMetricStage>,
}

pub fn init(db_path: &Path) -> Result<(), String> {
    if STATE.get().is_some() {
        return Ok(());
    }
    let store = LauncherStateStore::new(db_path)
        .map_err(|e| format!("failed to initialize launcher state DB: {}", e))?;
    STATE.set(Arc::new(store)).ok();
    log::info!("[state] Launcher state DB initialized: {:?}", db_path);
    Ok(())
}

pub fn get() -> Option<&'static Arc<LauncherStateStore>> {
    STATE.get()
}

pub struct LauncherStateStore {
    conn: Mutex<Connection>,
}

impl LauncherStateStore {
    fn new(db_path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS launch_metrics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                profile_id TEXT NOT NULL,
                version_id TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                total_pre_spawn_ms INTEGER NOT NULL,
                java_spawn_ms INTEGER NOT NULL,
                stages_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_launch_metrics_profile_created
                ON launch_metrics(profile_id, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_launch_metrics_created
                ON launch_metrics(created_at DESC);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub async fn record_launch_metrics(
        &self,
        mut metrics: LaunchMetricsRecord,
    ) -> Result<(), String> {
        let stages_json = serde_json::to_string(&metrics.stages)
            .map_err(|e| format!("failed to serialize launch metric stages: {}", e))?;
        let created_at = metrics.created_at.take().unwrap_or_else(unix_now);
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO launch_metrics
             (profile_id, version_id, created_at, total_pre_spawn_ms, java_spawn_ms, stages_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                metrics.profile_id,
                metrics.version_id,
                created_at,
                metrics.total_pre_spawn_ms,
                metrics.java_spawn_ms,
                stages_json,
            ],
        )
        .map_err(|e| format!("failed to record launch metrics: {}", e))?;
        prune_launch_metrics_locked(&conn)?;
        Ok(())
    }

    pub async fn launch_metric_history(
        &self,
        limit: usize,
    ) -> Result<Vec<LaunchMetricsRecord>, String> {
        let limit = limit.clamp(1, 100) as i64;
        let conn = self.conn.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, profile_id, version_id, created_at, total_pre_spawn_ms,
                        java_spawn_ms, stages_json
                 FROM launch_metrics
                 ORDER BY created_at DESC, id DESC
                 LIMIT ?1",
            )
            .map_err(|e| format!("failed to prepare launch metric query: {}", e))?;
        let rows = stmt
            .query_map(params![limit], |row| {
                let stages_json: String = row.get(6)?;
                let stages = serde_json::from_str(&stages_json).unwrap_or_default();
                Ok(LaunchMetricsRecord {
                    id: row.get(0)?,
                    profile_id: row.get(1)?,
                    version_id: row.get(2)?,
                    created_at: row.get(3)?,
                    total_pre_spawn_ms: row.get(4)?,
                    java_spawn_ms: row.get(5)?,
                    stages,
                })
            })
            .map_err(|e| format!("failed to read launch metrics: {}", e))?;
        let mut metrics = Vec::new();
        for row in rows {
            metrics.push(row.map_err(|e| format!("failed to read launch metric row: {}", e))?);
        }
        Ok(metrics)
    }
}

fn prune_launch_metrics_locked(conn: &Connection) -> Result<(), String> {
    let cutoff_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM launch_metrics
             ORDER BY created_at DESC, id DESC
             LIMIT 1 OFFSET ?1",
            params![MAX_LAUNCH_METRICS_ROWS - 1],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("failed to find launch metrics prune cutoff: {}", e))?;
    if let Some(cutoff_id) = cutoff_id {
        conn.execute(
            "DELETE FROM launch_metrics WHERE id < ?1",
            params![cutoff_id],
        )
        .map_err(|e| format!("failed to prune launch metrics: {}", e))?;
    }
    Ok(())
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(0);

    fn test_db_path() -> std::path::PathBuf {
        std::env::temp_dir()
            .join("hikyou-launcher-state-tests")
            .join(format!(
                "state-{}-{}.db",
                std::process::id(),
                NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed)
            ))
    }

    #[tokio::test]
    async fn records_and_reads_launch_metrics() {
        let db_path = test_db_path();
        let store = LauncherStateStore::new(&db_path).unwrap();
        store
            .record_launch_metrics(LaunchMetricsRecord {
                id: None,
                profile_id: "smart:latest-plus".to_string(),
                version_id: "26.2".to_string(),
                created_at: Some(123),
                total_pre_spawn_ms: 456,
                java_spawn_ms: 7,
                stages: vec![LaunchMetricStage {
                    name: "assets".to_string(),
                    ms: 120,
                }],
            })
            .await
            .unwrap();

        let history = store.launch_metric_history(10).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].profile_id, "smart:latest-plus");
        assert_eq!(history[0].version_id, "26.2");
        assert_eq!(history[0].created_at, Some(123));
        assert_eq!(history[0].stages[0].name, "assets");

        if let Some(parent) = db_path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
}
