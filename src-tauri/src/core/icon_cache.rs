//! Mod アイコン画像のディスクキャッシュ
//!
//! URL のSHA1ハッシュをファイル名として `caches/icons/` に保存する。
//! キャッシュヒット時はファイルを読んで data URL を返す（ネットワーク不要）。
//! キャッシュミス時はダウンロードしてから保存・返却する。

use crate::core::paths::LauncherPaths;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha1_smol::Sha1;

/// アイコンを取得して `data:<mime>;base64,<b64>` 形式で返す。
/// 失敗時は Err（呼び出し側は元 URL にフォールバックすること）。
pub async fn fetch_icon_data_url(url: &str, paths: &LauncherPaths) -> Result<String, String> {
    let dir = paths.icons_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create icons directory: {}", e))?;

    // URL から拡張子を推定（クエリ文字列を除く）
    let url_path = url.split('?').next().unwrap_or(url);
    let ext = url_path
        .rsplit('.')
        .next()
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| ["png", "jpg", "jpeg", "webp", "gif"].contains(&e.as_str()))
        .unwrap_or_else(|| "png".to_string());

    let hash = Sha1::from(url.as_bytes()).digest().to_string();
    let cache_path = dir.join(format!("{}.{}", hash, ext));

    let bytes: Vec<u8> = if cache_path.exists() {
        std::fs::read(&cache_path).map_err(|e| format!("icon failed to read: {}", e))?
    } else {
        let resp = reqwest::Client::new()
            .get(url)
            .send()
            .await
            .map_err(|e| format!("failed to download icon: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("icon HTTP request failed: {}", resp.status()));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("failed to read icon body: {}", e))?
            .to_vec();

        // 失敗してもキャッシュを返すため、書き込みエラーは無視
        let _ = std::fs::write(&cache_path, &bytes);
        bytes
    };

    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/png",
    };

    Ok(format!("data:{};base64,{}", mime, STANDARD.encode(&bytes)))
}
