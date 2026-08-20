use super::crash_parser::{CrashAction, CrashDiagnosis, ExceptionEntry, RuleMatch};
pub(super) fn build_diagnosis(
    lang: &str,
    description: Option<&str>,
    exceptions: &[ExceptionEntry],
    crash_mod: Option<&str>,
    rule_match: &Option<RuleMatch>,
    corpus: &[String],
) -> CrashDiagnosis {
    let category = rule_match
        .as_ref()
        .map(|m| category_for_rule(&m.id).to_string())
        .unwrap_or_else(|| {
            if crash_mod.is_some() {
                "mod"
            } else if exceptions.is_empty() {
                "unknown"
            } else {
                "exception"
            }
            .to_string()
        });

    let confidence = if rule_match.is_some() {
        0.9
    } else if crash_mod.is_some() && !exceptions.is_empty() {
        0.68
    } else if !exceptions.is_empty() {
        0.52
    } else {
        0.25
    };

    let summary = if let Some(rule) = rule_match {
        rule.message.clone()
    } else if let Some(mod_name) = crash_mod {
        if lang == "en" {
            format!(
                "The crash may be related to `{mod_name}`. Update or temporarily disable that mod, then launch again."
            )
        } else {
            format!(
                "`{mod_name}` がクラッシュに関係している可能性があります。そのModを更新するか、一時的に無効化して再起動してください。"
            )
        }
    } else if let Some(first) = exceptions.first() {
        if lang == "en" {
            format!(
                "Minecraft crashed with `{}`. Check the evidence lines below and try updating the affected mods or Java runtime.",
                first.class
            )
        } else {
            format!(
                "`{}` により Minecraft がクラッシュしました。下の根拠行を確認し、関係するModまたはJavaの更新を試してください。",
                first.class
            )
        }
    } else if lang == "en" {
        "The launcher could not identify a specific crash cause from this log yet.".to_string()
    } else {
        "このログからは、まだ明確なクラッシュ原因を特定できませんでした。".to_string()
    };

    let dependency_hints = if category == "dependency" {
        extract_dependency_hints(corpus)
    } else {
        Vec::new()
    };
    let evidence = collect_evidence(description, exceptions, crash_mod, &dependency_hints);
    let actions = actions_for_category(lang, &category, crash_mod, dependency_hints.first());

    CrashDiagnosis {
        category,
        confidence,
        summary,
        evidence,
        actions,
    }
}

fn category_for_rule(rule_id: &str) -> &str {
    match rule_id {
        "out_of_memory" => "memory",
        "opengl" => "graphics",
        "duplicate_mod" => "duplicate_mod",
        "mod_dep" => "dependency",
        "mixin" => "mod_conflict",
        "native_library" => "game_files",
        "verify_error" => "java",
        "class_not_found" => "mod_compatibility",
        "server_tick" => "mod_or_world",
        "world_corrupt" => "world",
        "ssl" => "network",
        "access_widener" => "mod_file",
        "debug_crash" => "debug",
        _ => "known",
    }
}

fn collect_evidence(
    description: Option<&str>,
    exceptions: &[ExceptionEntry],
    crash_mod: Option<&str>,
    dependency_hints: &[DependencyHint],
) -> Vec<String> {
    let mut evidence = Vec::new();

    if let Some(desc) = description {
        evidence.push(format!("Description: {desc}"));
    }
    if let Some(mod_name) = crash_mod {
        evidence.push(format!("Suspected mod: {mod_name}"));
    }
    for hint in dependency_hints.iter().take(3) {
        if let Some(requester) = &hint.requester {
            evidence.push(format!(
                "Missing dependency: {} (required by {})",
                hint.missing, requester
            ));
        } else {
            evidence.push(format!("Missing dependency: {}", hint.missing));
        }
    }
    if let Some(first) = exceptions.first() {
        let mut line = first.class.clone();
        if let Some(message) = &first.message {
            line.push_str(": ");
            line.push_str(message);
        }
        evidence.push(line);
        evidence.extend(first.top_frames.iter().take(4).cloned());
    }

    evidence.truncate(8);
    evidence
}

#[derive(Debug, Clone)]
struct DependencyHint {
    missing: String,
    requester: Option<String>,
}

fn extract_dependency_hints(corpus: &[String]) -> Vec<DependencyHint> {
    let mut hints = Vec::new();

    for raw in corpus {
        let line = raw.trim();
        let lower = line.to_ascii_lowercase();

        if lower.contains("requires any version of") && lower.contains("missing") {
            let quotes = quoted_values(line);
            if quotes.len() >= 2 {
                push_dependency_hint(&mut hints, quotes[1].clone(), Some(quotes[0].clone()));
            }
            continue;
        }

        if lower.contains("requires version") && lower.contains(" of ") && lower.contains("missing")
        {
            let requester = quoted_values(line).first().cloned();
            if let Some(dep) = extract_dependency_after_of(line) {
                push_dependency_hint(&mut hints, dep, requester);
            }
            continue;
        }

        if let Some(dep) = extract_fix_add_dependency(line) {
            push_dependency_hint(&mut hints, dep, None);
            continue;
        }

        if let Some(dep) = extract_depends_token(line) {
            let requester = extract_word_after(line, "HARD_DEP_NO_CANDIDATE ");
            push_dependency_hint(&mut hints, dep, requester);
        }
    }

    hints
}

fn push_dependency_hint(
    hints: &mut Vec<DependencyHint>,
    missing: String,
    requester: Option<String>,
) {
    let missing = clean_dependency_name(&missing);
    if missing.is_empty() {
        return;
    }
    if let Some(existing) = hints.iter_mut().find(|hint| hint.missing == missing) {
        if existing.requester.is_none() {
            existing.requester = requester
                .map(|value| value.trim().to_string())
                .filter(|v| !v.is_empty());
        }
        return;
    }
    hints.push(DependencyHint {
        missing,
        requester: requester
            .map(|value| value.trim().to_string())
            .filter(|v| !v.is_empty()),
    });
}

fn quoted_values(value: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut remaining = value;
    while let Some(open) = remaining.find('\'') {
        remaining = &remaining[open + 1..];
        if let Some(close) = remaining.find('\'') {
            let item = remaining[..close].trim();
            if !item.is_empty() {
                result.push(item.to_string());
            }
            remaining = &remaining[close + 1..];
        } else {
            break;
        }
    }
    result
}

fn extract_dependency_after_of(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let start = lower.find(" of ")? + 4;
    let after = &line[start..];
    let dep: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    if dep.is_empty() { None } else { Some(dep) }
}

fn extract_fix_add_dependency(line: &str) -> Option<String> {
    let marker = "add:";
    let start = line.find(marker)? + marker.len();
    let after = &line[start..];
    let dep: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    if dep.is_empty() { None } else { Some(dep) }
}

fn extract_depends_token(line: &str) -> Option<String> {
    let marker = "{depends ";
    let start = line.find(marker)? + marker.len();
    let after = &line[start..];
    let dep: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    if dep.is_empty() { None } else { Some(dep) }
}

fn extract_word_after(line: &str, marker: &str) -> Option<String> {
    let start = line.find(marker)? + marker.len();
    let after = &line[start..];
    let word: String = after.chars().take_while(|c| !c.is_whitespace()).collect();
    if word.is_empty() { None } else { Some(word) }
}

fn clean_dependency_name(value: &str) -> String {
    value
        .trim()
        .trim_matches(|c: char| {
            c == ',' || c == '.' || c == '!' || c == ']' || c == ')' || c == '[' || c == '('
        })
        .to_string()
}

fn actions_for_category(
    lang: &str,
    category: &str,
    crash_mod: Option<&str>,
    dependency_hint: Option<&DependencyHint>,
) -> Vec<CrashAction> {
    let en = lang == "en";
    let mut actions = Vec::new();

    match category {
        "memory" => actions.push(action(
            "increase_memory",
            en,
            "Increase RAM",
            "メモリを増やす",
            "Raise the profile memory allocation, then launch again.",
            "プロファイルのメモリ割り当てを増やしてから、もう一度起動してください。",
            None,
        )),
        "graphics" => actions.push(action(
            "update_gpu_driver",
            en,
            "Update GPU driver",
            "GPUドライバーを更新",
            "Install the latest driver from your GPU or PC manufacturer.",
            "GPU または PC メーカーから最新ドライバーをインストールしてください。",
            None,
        )),
        "dependency" => {
            if let Some(hint) = dependency_hint {
                actions.push(action(
                    "install_dependency",
                    en,
                    &format!("Install {}", hint.missing),
                    &format!("{}を入れる", hint.missing),
                    &format!(
                        "Install {} for the current loader and Minecraft version.",
                        hint.missing
                    ),
                    &format!(
                        "現在のローダーとMinecraftバージョンに合う{}をインストールしてください。",
                        hint.missing
                    ),
                    Some(hint.missing.clone()),
                ));
            } else {
                actions.push(action(
                    "install_dependencies",
                    en,
                    "Install dependencies",
                    "依存Modを入れる",
                    "Find and install the missing dependencies for the current loader and Minecraft version.",
                    "現在のローダーと Minecraft バージョンに合う不足依存Modを探してインストールしてください。",
                    None,
                ));
            }
            actions.push(update_mods_action(en));
        }
        "duplicate_mod" => actions.push(action(
            "remove_duplicate_mods",
            en,
            "Remove duplicates",
            "重複Modを削除",
            "Open the mods folder and remove duplicate jar files.",
            "mods フォルダーを開き、重複している jar ファイルを削除してください。",
            None,
        )),
        "java" => actions.push(action(
            "select_compatible_java",
            en,
            "Select compatible Java",
            "互換Javaを選ぶ",
            "Use the Java version required by this Minecraft version.",
            "この Minecraft バージョンに必要な Java を選択してください。",
            None,
        )),
        "world" => actions.push(action(
            "restore_world_backup",
            en,
            "Restore backup",
            "バックアップを復元",
            "Restore the world from a backup, or test with a new world.",
            "バックアップからワールドを復元するか、新規ワールドで再現するか確認してください。",
            None,
        )),
        "debug" => actions.push(action(
            "ignore_debug_crash",
            en,
            "No action needed",
            "対応不要",
            "This crash was triggered intentionally and does not indicate a game failure.",
            "意図的に発生させたクラッシュなので、ゲーム異常ではありません。",
            None,
        )),
        _ => {
            actions.push(update_mods_action(en));
            if let Some(mod_name) = crash_mod {
                actions.push(action(
                    "disable_suspected_mod",
                    en,
                    "Disable suspected mod",
                    "疑わしいModを無効化",
                    "Temporarily disable the suspected mod and launch again.",
                    "疑わしいModを一時的に無効化して、もう一度起動してください。",
                    Some(mod_name.to_string()),
                ));
            }
        }
    }

    actions
}

fn update_mods_action(en: bool) -> CrashAction {
    action(
        "update_mods",
        en,
        "Update mods",
        "Modを更新",
        "Update installed mods to versions compatible with this profile.",
        "この起動構成に対応するバージョンへ Mod を更新してください。",
        None,
    )
}

fn action(
    kind: &str,
    en: bool,
    label_en: &str,
    label_ja: &str,
    detail_en: &str,
    detail_ja: &str,
    target: Option<String>,
) -> CrashAction {
    CrashAction {
        kind: kind.to_string(),
        label: if en { label_en } else { label_ja }.to_string(),
        detail: if en { detail_en } else { detail_ja }.to_string(),
        target,
    }
}
