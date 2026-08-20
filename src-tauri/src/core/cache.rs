//! SQLite ベースのディスク永続 API キャッシュ
//!
//! Modrinth App と同等の仕組み:
//!   - SQLite データベースにシリアライズした JSON を保存
//!   - `(data_type, alias)` を複合主キーとして一意性を保証
//!   - `expires_at` (Unix 秒) でコンテンツの有効期限を管理
//!   - アプリ再起動後もキャッシュが有効
//!   - `clear_all()` で手動パージが可能
//!
//! キャッシュ TTL の目安:
//!   - ローダーバージョン一覧 (Fabric/Quilt/Forge/NeoForge) : 1 時間
//!   - Modrinth 検索結果                                     : 5 分
//!   - Modrinth Mod バージョン一覧                            : 6 時間
//!   - ModPack バージョン一覧                                : 30 分

use rusqlite::{Connection, params};
use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

// ── グローバルシングルトン ────────────────────────────────────────────────────

static CACHE: OnceLock<Arc<DiskCache>> = OnceLock::new();

/// アプリ起動時に一度だけ呼び出す。以降は `get()` でアクセス。
pub fn init(db_path: &Path) -> Result<(), String> {
    if CACHE.get().is_some() {
        return Ok(()); // 二重初期化を防ぐ
    }
    let cache =
        DiskCache::new(db_path).map_err(|e| format!("failed to initialize cache DB: {}", e))?;
    CACHE.set(Arc::new(cache)).ok();
    log::info!("[Cache] SQLite cache initialized: {:?}", db_path);
    Ok(())
}

/// 初期化済みのキャッシュインスタンスを返す。
/// setup() 前に呼ばれた場合は None。
pub fn get() -> Option<&'static Arc<DiskCache>> {
    CACHE.get()
}

// ── DiskCache 実装 ────────────────────────────────────────────────────────────

pub struct DiskCache {
    conn: Mutex<Connection>,
}

impl DiskCache {
    fn new(db_path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(db_path)?;

        // WAL モードで同時読み書き性能を向上
        // synchronous=NORMAL: WAL 使用時はこれで十分安全かつ高速
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS api_cache (
                data_type  TEXT NOT NULL,
                alias      TEXT NOT NULL,
                data       TEXT NOT NULL,
                expires_at INTEGER NOT NULL,
                etag       TEXT,
                PRIMARY KEY (data_type, alias)
            );
            CREATE INDEX IF NOT EXISTS idx_cache_expiry ON api_cache(expires_at);",
        )?;
        // 旧スキーマに etag カラムを追加（初回マイグレーション）
        let _ = conn.execute_batch("ALTER TABLE api_cache ADD COLUMN etag TEXT;");

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// キャッシュからデータを取得する（TTL 切れは None）。
    pub async fn get<T: DeserializeOwned>(&self, data_type: &str, alias: &str) -> Option<T> {
        let now = unix_now();
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT data FROM api_cache WHERE data_type = ?1 AND alias = ?2 AND expires_at > ?3",
            params![data_type, alias, now],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(json) => serde_json::from_str(&json).ok(),
            Err(_) => None,
        }
    }

    /// データをキャッシュに保存する。`ttl_secs` 秒後に期限切れになる。
    pub async fn set<T: Serialize>(
        &self,
        data_type: &str,
        alias: &str,
        value: &T,
        ttl_secs: i64,
    ) -> Result<(), String> {
        self.set_with_etag(data_type, alias, value, ttl_secs, None)
            .await
    }

    /// データをキャッシュに保存する（ETag 付き）。
    pub async fn set_with_etag<T: Serialize>(
        &self,
        data_type: &str,
        alias: &str,
        value: &T,
        ttl_secs: i64,
        etag: Option<&str>,
    ) -> Result<(), String> {
        let json =
            serde_json::to_string(value).map_err(|e| format!("failed to serialize JSON: {}", e))?;
        let expires_at = unix_now() + ttl_secs;
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO api_cache (data_type, alias, data, expires_at, etag)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![data_type, alias, json, expires_at, etag],
        )
        .map_err(|e| format!("failed to write cache: {}", e))?;
        Ok(())
    }

    /// TTL に関わらず（期限切れでも）データと ETag を返す。
    /// ETag による条件付きリクエストのために使用する。
    pub async fn get_stale_with_etag<T: DeserializeOwned>(
        &self,
        data_type: &str,
        alias: &str,
    ) -> Option<(T, Option<String>)> {
        let conn = self.conn.lock().await;
        let result = conn.query_row(
            "SELECT data, etag FROM api_cache WHERE data_type = ?1 AND alias = ?2",
            params![data_type, alias],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        );
        match result {
            Ok((json, etag)) => serde_json::from_str(&json).ok().map(|v| (v, etag)),
            Err(_) => None,
        }
    }

    /// 期限切れエントリを削除する。
    pub async fn prune_expired(&self) -> Result<usize, String> {
        let now = unix_now();
        let conn = self.conn.lock().await;
        let n = conn
            .execute("DELETE FROM api_cache WHERE expires_at <= ?1", params![now])
            .map_err(|e| format!("failed to prune cache: {}", e))?;
        Ok(n)
    }

    /// 全エントリを削除する（手動キャッシュパージ）。
    pub async fn clear_all(&self) -> Result<(), String> {
        let conn = self.conn.lock().await;
        conn.execute_batch("DELETE FROM api_cache;")
            .map_err(|e| format!("failed to clear cache: {}", e))?;
        log::info!("[Cache] All cache cleared");
        Ok(())
    }

    /// キャッシュの統計情報（エントリ数・サイズ目安）を返す。
    pub async fn stats(&self) -> CacheStats {
        let conn = self.conn.lock().await;
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM api_cache", [], |r| r.get(0))
            .unwrap_or(0);
        let valid: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM api_cache WHERE expires_at > ?1",
                params![unix_now()],
                |r| r.get(0),
            )
            .unwrap_or(0);
        CacheStats {
            total_entries: total as usize,
            valid_entries: valid as usize,
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub valid_entries: usize,
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
