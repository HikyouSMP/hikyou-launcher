//! Minecraft クラッシュログの構造化パーサー + ルールマッチャー
//!
//! # パイプライン
//! ```text
//! raw log lines
//!   → parse()       : 例外・スタック・Mod一覧・バージョンを抽出
//!   → match_rules() : crash_rules.json の既知パターンと照合（一致すれば即答）
//! ```
//!
//! # ルールの拡張
//! `src-tauri/crash_rules.json` を編集してルールを追加・変更できます。
//! ビルド時に埋め込まれるため、アプリの再ビルドが必要です。

use super::crash_diagnosis::build_diagnosis;
use super::crash_rules::match_rules;

// ─────────────────────────────────────────────────────────────────────────────
// 公開型
// ─────────────────────────────────────────────────────────────────────────────

/// ログから抽出した構造化クラッシュ情報
#[derive(Debug, serde::Serialize)]
pub struct ParsedCrash {
    /// クラッシュ時に何をしていたか ("Exception in server tick loop" 等)
    pub description: Option<String>,
    /// 例外チェーン（主例外 + Caused by）
    pub exceptions: Vec<ExceptionEntry>,
    /// クラッシュを起こした Mod のパッケージ/名前（推定）
    pub crash_mod: Option<String>,
    /// ロード済み Mod 一覧（表示名 + バージョン、最大20件）
    pub mod_list: Vec<String>,
    /// Minecraft バージョン
    pub mc_version: Option<String>,
    /// Java バージョン
    pub java_version: Option<String>,
    /// Mod ローダー種別（Fabric / Forge / Quilt / NeoForge 等）
    pub loader: Option<String>,
    /// 正式クラッシュレポート形式か（通常ゲームログかの区別）
    pub is_crash_report: bool,
    /// 既知パターンに一致した場合の解決策
    pub rule_match: Option<RuleMatch>,
    /// UI が原因・根拠・次の操作を分けて表示するための診断結果
    pub diagnosis: CrashDiagnosis,
}

#[derive(Debug, serde::Serialize)]
pub struct ExceptionEntry {
    pub class: String,
    pub message: Option<String>,
    /// 上位スタックフレーム（最大8件）
    pub top_frames: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RuleMatch {
    /// crash_rules.json の rule id
    pub id: String,
    /// 自然言語での解説（原因 + 解決方法）
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CrashDiagnosis {
    /// Stable English category for UI grouping and future analytics.
    pub category: String,
    /// 0.0-1.0 confidence score. Rule matches are high, inferred guesses are lower.
    pub confidence: f32,
    /// Human-readable short explanation.
    pub summary: String,
    /// Key log lines or extracted facts used by the diagnosis.
    pub evidence: Vec<String>,
    /// Recommended next steps. UI can later map `kind` to buttons.
    pub actions: Vec<CrashAction>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CrashAction {
    /// Stable action id, for example "update_mods" or "increase_memory".
    pub kind: String,
    pub label: String,
    pub detail: String,
    /// Optional machine-readable target such as a mod id/name.
    pub target: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// エントリポイント
// ─────────────────────────────────────────────────────────────────────────────

/// ログを解析して構造化クラッシュ情報を返す。
///
/// `lang` は将来の多言語対応用。現在は `"ja"` のみサポート。
/// 他の言語コードを渡しても動作するが、メッセージは日本語で返る。
pub fn parse(lines: &[String], lang: &str) -> ParsedCrash {
    let is_crash_report = lines
        .iter()
        .any(|l| l.contains("---- Minecraft Crash Report ----"));

    let description = extract_description(lines);
    let exceptions = extract_exceptions(lines);
    let (mc_version, java_version, loader, mod_list) = extract_system_details(lines);
    let crash_mod = infer_crash_mod(&exceptions, &mod_list);

    // ── 統一コーパス ──────────────────────────────────────────────────────────
    // キーワードマッチとデータ抽出を同じテキストに対して行うことで
    // 「マッチしたのに取り出せない」バグを構造上なくす。
    // 内容: 生ログ行 + 例外クラス名 + 例外メッセージ（これが特に重要）
    let corpus: Vec<String> = {
        let mut v = lines.to_vec();
        for exc in &exceptions {
            // "ClassName: message" の形で 1 行として追加
            let mut line = exc.class.clone();
            if let Some(msg) = &exc.message {
                line.push_str(": ");
                line.push_str(msg);
            }
            v.push(line);
        }
        v
    };

    let rule_match = match_rules(&corpus, lang);
    let diagnosis = build_diagnosis(
        lang,
        description.as_deref(),
        &exceptions,
        crash_mod.as_deref(),
        &rule_match,
        &corpus,
    );

    ParsedCrash {
        description,
        exceptions,
        crash_mod,
        mod_list,
        mc_version,
        java_version,
        loader,
        is_crash_report,
        rule_match,
        diagnosis,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// パーサー内部関数
// ─────────────────────────────────────────────────────────────────────────────

fn extract_description(lines: &[String]) -> Option<String> {
    for line in lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Description:") {
            let desc = rest.trim();
            if !desc.is_empty() {
                return Some(desc.to_string());
            }
        }
    }
    None
}

fn extract_exceptions(lines: &[String]) -> Vec<ExceptionEntry> {
    let mut entries: Vec<ExceptionEntry> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // 例外行の検出: "SomeException: message" または "Caused by: SomeException"
        let exception_line = if let Some(rest) = line.strip_prefix("Caused by: ") {
            Some(rest)
        } else if is_exception_line(line) {
            Some(line)
        } else {
            None
        };

        if let Some(exc_line) = exception_line {
            let (class, message) = split_exception_line(exc_line);
            let mut frames = Vec::new();

            // 直後のスタックフレームを収集（最大8件）
            let mut j = i + 1;
            while j < lines.len() && frames.len() < 8 {
                let fl = lines[j].trim();
                if fl.starts_with("at ") {
                    frames.push(fl.trim_start_matches("at ").to_string());
                    j += 1;
                } else if fl.starts_with("...") {
                    // "... N more" は省略行
                    j += 1;
                } else {
                    break;
                }
            }

            entries.push(ExceptionEntry {
                class,
                message,
                top_frames: frames,
            });
        }

        i += 1;
    }

    // 主例外 + Caused byチェーン（最大4件）
    entries.truncate(4);
    entries
}

fn is_exception_line(line: &str) -> bool {
    // "java.lang.XXX:", "net.minecraft.XXX:", "com.example.XXX:" 等
    // ただし "at " で始まる行は除外
    if line.starts_with("at ") || line.starts_with('\t') {
        return false;
    }
    // コロンがあり、その前の部分にドット区切りのクラス名パターンがある
    if let Some(colon_pos) = line.find(':') {
        let before_colon = &line[..colon_pos];
        // クラス名らしき条件: ドットを含む OR 大文字で始まりException/Errorで終わる
        if before_colon.contains('.')
            && before_colon
                .chars()
                .next()
                .map(|c| c.is_alphabetic())
                .unwrap_or(false)
        {
            return true;
        }
        if before_colon.ends_with("Exception") || before_colon.ends_with("Error") {
            return true;
        }
    }
    // コロンなしで Exception/Error で終わる行（スタックトレースの最初の行）
    if line.ends_with("Exception") || line.ends_with("Error") {
        return true;
    }
    false
}

fn split_exception_line(line: &str) -> (String, Option<String>) {
    if let Some(pos) = line.find(": ") {
        (line[..pos].to_string(), Some(line[pos + 2..].to_string()))
    } else if let Some(pos) = line.find(':') {
        let after = line[pos + 1..].trim();
        if after.is_empty() {
            (line[..pos].to_string(), None)
        } else {
            (line[..pos].to_string(), Some(after.to_string()))
        }
    } else {
        (line.to_string(), None)
    }
}

fn extract_system_details(
    lines: &[String],
) -> (Option<String>, Option<String>, Option<String>, Vec<String>) {
    let mut mc_version = None;
    let mut java_version = None;
    let mut loader = None;
    let mut mod_list = Vec::new();

    let mut in_details = false;
    let mut in_mods = false;

    for line in lines {
        let trimmed = line.trim();

        if trimmed == "-- System Details --" || trimmed.contains("-- System Details --") {
            in_details = true;
            in_mods = false;
            continue;
        }

        // Fabric ローダー検出
        if trimmed.contains("fabric-loader") || trimmed.contains("fabricloader") {
            if loader.is_none() {
                loader = Some("Fabric".to_string());
            }
        } else if trimmed.contains("forge") && trimmed.contains("net.minecraftforge") {
            if loader.is_none() {
                loader = Some("Forge".to_string());
            }
        } else if trimmed.contains("neoforge") {
            if loader.is_none() {
                loader = Some("NeoForge".to_string());
            }
        } else if (trimmed.contains("quiltmc") || trimmed.contains("quilt_loader"))
            && loader.is_none()
        {
            loader = Some("Quilt".to_string());
        }

        if !in_details {
            continue;
        }

        if let Some(v) = trimmed.strip_prefix("Minecraft Version:") {
            mc_version = Some(v.trim().to_string());
        } else if let Some(v) = trimmed.strip_prefix("Java Version:") {
            // "21.0.3, Eclipse Adoptium" → "21.0.3"
            let ver = v.trim().split(',').next().unwrap_or("").trim();
            java_version = Some(ver.to_string());
        } else if trimmed.starts_with("Mod List:") || trimmed == "Mods:" {
            in_mods = true;
        } else if in_mods {
            // セクション終了を検出
            if trimmed.starts_with("--") || (trimmed.is_empty() && line.len() < 2) {
                in_mods = false;
                continue;
            }
            if trimmed.is_empty() {
                continue;
            }
            // Mod行（"modname | version" や "modname-version.jar" 等）
            if let Some(entry) = parse_mod_line(trimmed)
                && mod_list.len() < 20
            {
                mod_list.push(entry);
            }
        }
    }

    // Fabric の場合は別形式でも検出
    if mc_version.is_none()
        && let Some(v) = lines
            .iter()
            .map(|line| line.trim())
            .find_map(|t| t.strip_prefix("Minecraft Version: "))
    {
        mc_version = Some(v.to_string());
    }

    (mc_version, java_version, loader, mod_list)
}

fn parse_mod_line(line: &str) -> Option<String> {
    // "modname-1.2.3.jar" 形式
    if line.ends_with(".jar") {
        let name = line.trim_end_matches(".jar");
        return Some(name.to_string());
    }
    // "| modname | version |" （Fabric クラッシュレポート形式）
    if line.starts_with('|') {
        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if parts.len() >= 3 {
            let name = parts[1];
            let ver = parts[2];
            if !name.is_empty() && name != "Mod" {
                return Some(format!("{} {}", name, ver));
            }
        }
        return None;
    }
    // "modname: version" 形式
    if let Some(colon_pos) = line.find(':') {
        let name = line[..colon_pos].trim();
        let ver = line[colon_pos + 1..].trim();
        if !name.is_empty() && !name.contains(' ') {
            return Some(format!("{} {}", name, ver));
        }
    }
    // その他: 空白区切りで短い場合のみ
    if line.len() < 60 && !line.contains("Exception") && !line.contains("at ") {
        return Some(line.to_string());
    }
    None
}

fn infer_crash_mod(exceptions: &[ExceptionEntry], mod_list: &[String]) -> Option<String> {
    // スタックフレームのパッケージ名からModを推定
    // Minecraft本体や標準ライブラリを除外し、残ったものを返す
    let known_mod_markers = [
        ("lithium", "lithium"),
        ("sodium", "sodium"),
        ("moonrise", "moonrise"),
        ("ferritecore", "ferritecore"),
        ("ferrite_core", "ferritecore"),
        ("immediatelyfast", "immediatelyfast"),
        ("entityculling", "entityculling"),
        ("dynamic_fps", "dynamic_fps"),
        ("modernfix", "modernfix"),
        ("nvidium", "nvidium"),
    ];

    for exc in exceptions {
        for frame in &exc.top_frames {
            let lower = frame.to_lowercase();
            for (marker, mod_id) in known_mod_markers {
                if lower.contains(marker) {
                    return Some(resolve_mod_name(mod_id, mod_list));
                }
            }
            if let Some(mixin_mod) = extract_mixin_redirect_owner(&lower) {
                return Some(resolve_mod_name(&mixin_mod, mod_list));
            }
        }
    }

    let skip_prefixes = [
        "net.minecraft.",
        "com.mojang.",
        "java.",
        "javax.",
        "sun.",
        "jdk.",
        "org.lwjgl.",
        "net.fabricmc.loader.",
        "org.spongepowered.asm.",  // Mixin framework
        "net.minecraftforge.fml.", // Forge FML
    ];

    for exc in exceptions {
        for frame in &exc.top_frames {
            if skip_prefixes.iter().any(|p| frame.starts_with(p)) {
                continue;
            }
            // "com.example.mymod.XXX" → "mymod" を推定
            let parts: Vec<&str> = frame.split('.').collect();
            if parts.len() >= 3 {
                let candidate = parts[2]; // 3番目のセグメント
                if !candidate.is_empty()
                    && candidate
                        .chars()
                        .next()
                        .map(|c| c.is_lowercase())
                        .unwrap_or(false)
                {
                    // Mod一覧と照合
                    let lower = candidate.to_lowercase();
                    for m in mod_list {
                        if m.to_lowercase().contains(&lower) {
                            return Some(candidate.to_string());
                        }
                    }
                    return Some(candidate.to_string());
                }
            }
        }
    }
    None
}

fn extract_mixin_redirect_owner(frame: &str) -> Option<String> {
    let dollar = frame.find('$')?;
    let rest = &frame[dollar + 1..];
    let marker_start = rest.find('$')?;
    let rest = &rest[marker_start + 1..];
    let owner_end = rest.find('$')?;
    let owner = &rest[..owner_end];
    if owner.is_empty() || !owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(owner.to_string())
}

fn resolve_mod_name(mod_id: &str, mod_list: &[String]) -> String {
    let normalized = mod_id.replace('_', "").to_lowercase();
    mod_list
        .iter()
        .find_map(|entry| {
            let entry_lower = entry.replace('_', "").to_lowercase();
            if entry_lower.contains(&normalized) {
                entry.split_whitespace().next().map(|s| s.to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| mod_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn builds_structured_diagnosis_for_memory_crash() {
        let lines = vec![
            "---- Minecraft Crash Report ----".to_string(),
            "Description: Rendering screen".to_string(),
            "java.lang.OutOfMemoryError: Java heap space".to_string(),
        ];

        let parsed = parse(&lines, "en");

        assert_eq!(parsed.diagnosis.category, "memory");
        assert!(parsed.diagnosis.confidence > 0.8);
        assert!(
            parsed
                .diagnosis
                .actions
                .iter()
                .any(|action| action.kind == "increase_memory")
        );
        assert!(
            parsed
                .diagnosis
                .evidence
                .iter()
                .any(|line| line.contains("Rendering screen"))
        );
    }

    #[test]
    fn inferred_mod_crash_gets_lower_confidence_and_disable_action() {
        let lines = vec![
            "java.lang.RuntimeException: boom".to_string(),
            "\tat com.example.coolmod.Client.init(Client.java:12)".to_string(),
            "\tat net.minecraft.client.Minecraft.run(Minecraft.java:100)".to_string(),
            "coolmod: 1.0.0".to_string(),
        ];

        let parsed = parse(&lines, "en");

        assert!(parsed.diagnosis.confidence < 0.8);
        assert!(parsed.crash_mod.is_some());
        assert!(
            parsed
                .diagnosis
                .actions
                .iter()
                .any(|action| action.kind == "disable_suspected_mod")
        );
    }

    #[test]
    fn infers_lithium_from_mixin_redirect_frame() {
        let lines = vec![
            "java.lang.NullPointerException: Cannot invoke getEntityManager()".to_string(),
            "\tat net.minecraft.class_3215.redirect$bem000$lithium$iterateEntitiesChunkAware(class_3215.java:9000)".to_string(),
            "\tat net.minecraft.class_3215.method_14161(class_3215.java:300)".to_string(),
            "lithium: Lithium 0.17.0+mc1.21.6".to_string(),
        ];

        let parsed = parse(&lines, "en");

        assert_eq!(parsed.crash_mod.as_deref(), Some("lithium"));
    }

    #[test]
    fn dependency_crash_names_missing_mod_and_requester() {
        let lines = vec![
            "[main/WARN]: Mod resolution failed".to_string(),
            "[main/INFO]: Fix: add [add:cloth-config 16.0.0 ([[16.0.0,∞)])], remove [], replace []".to_string(),
            "[main/ERROR]: Incompatible mods found!".to_string(),
            "net.fabricmc.loader.impl.FormattedException: Some of your mods are incompatible with the game or each other!".to_string(),
            "Mod 'More Culling' (moreculling) 1.4.0-beta.1 requires version 16.0.0 or later of cloth-config, which is missing!".to_string(),
        ];

        let parsed = parse(&lines, "en");

        assert_eq!(parsed.diagnosis.category, "dependency");
        assert!(parsed.diagnosis.summary.contains("cloth-config"));
        assert!(
            parsed
                .diagnosis
                .evidence
                .iter()
                .any(|line| line.contains("cloth-config") && line.contains("More Culling"))
        );
        assert!(
            parsed
                .diagnosis
                .actions
                .iter()
                .any(|action| action.target.as_deref() == Some("cloth-config"))
        );
    }
}
