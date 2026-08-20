//! ランチャーが使用するすべてのディレクトリパスを一元管理する。

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct LauncherPaths {
    root: PathBuf,
}

impl LauncherPaths {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    // ── ルート ────────────────────────────────────────────────────────────

    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    // ── profile (旧: instances) ──────────────────────────────────────

    /// 全プロファイルを格納するディレクトリ
    pub fn profiles(&self) -> PathBuf {
        self.root.join("profiles")
    }

    /// Launcher-managed smart profiles such as Latest+ and Snapshot+.
    pub fn smart_profiles(&self) -> PathBuf {
        self.root.join("smart-profiles")
    }

    /// 特定プロファイルの `.minecraft` ディレクトリ (id = UUID)
    pub fn profile_game_dir(&self, id: &str) -> PathBuf {
        self.profiles().join(id).join(".minecraft")
    }

    pub fn profile_game_dir_for_ref(&self, profile_ref: &str) -> Result<PathBuf, String> {
        crate::core::profile::profile_game_dir_for_ref(&self.root, profile_ref)
    }

    pub fn checked_profile_game_dir(&self, id: &str) -> Result<PathBuf, String> {
        self.profile_game_dir_for_ref(id)
    }

    // ── メタデータ ────────────────────────────────────────────────────────

    /// バージョン・ライブラリ・アセット・Java を格納するメタディレクトリ
    pub fn meta(&self) -> PathBuf {
        self.root.join("meta")
    }

    /// バージョン JSON ファイル群
    pub fn versions(&self) -> PathBuf {
        self.meta().join("versions")
    }

    /// 共有ライブラリ
    pub fn libraries(&self) -> PathBuf {
        self.meta().join("libraries")
    }

    /// アセット (サウンド、テクスチャなど)
    pub fn assets(&self) -> PathBuf {
        self.meta().join("assets")
    }

    /// Java ランタイム (旧: runtime/)
    pub fn java_versions(&self) -> PathBuf {
        self.meta().join("java_versions")
    }

    /// 特定の JVM のディレクトリ (例: `meta/java_versions/zulu-21`)
    pub fn java_version_dir(&self, name: &str) -> PathBuf {
        self.java_versions().join(name)
    }

    // ── セットアップ ──────────────────────────────────────────────────────

    /// API レスポンスキャッシュ用 SQLite データベースファイル
    pub fn cache_db(&self) -> PathBuf {
        self.caches_dir().join("cache.db")
    }

    /// Launcher-owned state history database. This is not disposable API cache.
    pub fn launcher_state_db(&self) -> PathBuf {
        self.state_dir().join("launcher_state.db")
    }

    /// log4j2 設定ファイルを格納するディレクトリ
    pub fn log_configs_dir(&self) -> PathBuf {
        self.meta().join("log_configs")
    }

    // ── ランチャーログ ────────────────────────────────────────────────────

    /// ランチャーセッションログを格納するディレクトリ
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    // ── キャッシュ ────────────────────────────────────────────────────────

    /// キャッシュルートディレクトリ
    pub fn caches_dir(&self) -> PathBuf {
        self.root.join("caches")
    }

    /// Launcher-owned persistent state such as launch metrics and health history.
    pub fn state_dir(&self) -> PathBuf {
        self.root.join("state")
    }

    /// Mod アイコン画像キャッシュ
    pub fn icons_dir(&self) -> PathBuf {
        self.caches_dir().join("icons")
    }

    /// 自動インストールModリスト
    pub fn auto_mods_file(&self) -> PathBuf {
        self.root().join("auto_mods.json")
    }

    // ── ローダーキャッシュ ────────────────────────────────────────────────

    /// 全ローダーキャッシュの共通ルート
    pub fn loaders_dir(&self) -> PathBuf {
        self.meta().join("loaders")
    }

    /// Fabric profile JSON キャッシュ
    pub fn fabric_dir(&self) -> PathBuf {
        self.loaders_dir().join("fabric")
    }

    /// Quilt profile JSON キャッシュ
    pub fn quilt_dir(&self) -> PathBuf {
        self.loaders_dir().join("quilt")
    }

    /// Forge キャッシュ (version JSON + installer JAR + マーカーファイル)
    pub fn forge_dir(&self) -> PathBuf {
        self.loaders_dir().join("forge")
    }

    /// Forge インストーラー作業ディレクトリ
    pub fn forge_install_dir(&self) -> PathBuf {
        self.loaders_dir().join("forge-install")
    }

    /// NeoForge キャッシュ (version JSON + installer JAR + マーカーファイル)
    pub fn neoforge_dir(&self) -> PathBuf {
        self.loaders_dir().join("neoforge")
    }

    /// NeoForge インストーラー作業ディレクトリ
    pub fn neoforge_install_dir(&self) -> PathBuf {
        self.loaders_dir().join("neoforge-install")
    }

    /// すべての基本ディレクトリを作成する。起動時に必ず呼び出すこと。
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for path in [
            self.root(),
            self.profiles(),
            self.smart_profiles(),
            self.meta(),
            self.versions(),
            self.libraries(),
            self.assets(),
            self.java_versions(),
            self.caches_dir(),
            self.state_dir(),
        ] {
            std::fs::create_dir_all(path)?;
        }
        Ok(())
    }
}

/// OS ファイルマネージャーで開いてよいパスだけを許可する。
///
/// フロントエンドから任意パスを渡せる Tauri command の境界で使う。
/// ランチャーのデータルート配下にある既存ディレクトリだけを許可することで、
/// WebView 側のバグや XSS がユーザーの任意ディレクトリを開く経路を塞ぐ。
pub fn validate_open_dir(root: &std::path::Path, requested: &str) -> Result<PathBuf, String> {
    if requested.trim().is_empty() || requested.contains('\0') {
        return Err("invalid path".to_string());
    }

    let root = root
        .canonicalize()
        .map_err(|e| format!("failed to resolve launcher directory: {}", e))?;
    let requested = std::path::Path::new(requested)
        .canonicalize()
        .map_err(|e| format!("failed to resolve folder: {}", e))?;

    if !requested.is_dir() {
        return Err("path is not a directory".to_string());
    }
    if !requested.starts_with(&root) {
        return Err("cannot open folders outside the launcher directory".to_string());
    }

    Ok(requested)
}

#[cfg(test)]
mod tests {
    use super::validate_open_dir;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(0);

    fn test_root() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "hikyou-path-test-{}-{}",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("profiles").join("p1")).unwrap();
        root
    }

    #[test]
    fn allows_existing_dir_inside_launcher_root() {
        let root = test_root();
        let inside = root.join("profiles").join("p1");
        let validated = validate_open_dir(&root, inside.to_str().unwrap()).unwrap();
        assert!(validated.ends_with(std::path::Path::new("profiles").join("p1")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_dir_outside_launcher_root() {
        let root = test_root();
        let outside = std::env::temp_dir();
        let err = validate_open_dir(&root, outside.to_str().unwrap()).unwrap_err();
        assert!(err.contains("outside the launcher directory"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_files_inside_launcher_root() {
        let root = test_root();
        let file = root.join("settings.json");
        fs::write(&file, "{}").unwrap();
        let err = validate_open_dir(&root, file.to_str().unwrap()).unwrap_err();
        assert!(err.contains("path is not a directory"));
        let _ = fs::remove_dir_all(root);
    }
}
