// ── 自然言語メッセージ生成 ────────────────────────────────────────────────────

/// 行からシングルクォートで囲まれた文字列を全て順番に取得する
fn all_quoted(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut remaining = s;
    while let Some(open) = remaining.find('\'') {
        remaining = &remaining[open + 1..];
        if let Some(close) = remaining.find('\'') {
            let val = remaining[..close].trim().to_string();
            if !val.is_empty() {
                result.push(val);
            }
            remaining = &remaining[close + 1..];
        } else {
            break;
        }
    }
    result
}

pub(super) fn build_mod_dep_message(lines: &[String], lang: &str) -> String {
    // ── 収集バケツ ────────────────────────────────────────────────────────────
    // Fabric "Replace mod" 提案: (mod表示名, 現在バージョン, 必要条件)
    // Fabricが「ユーザーが実際に更新すべきMod」として示すもの。
    let mut replace_mods: Vec<(String, String, String)> = Vec::new();
    // MC バージョン不一致（全依存関係、JAR-in-JAR含む）: (mod表示名, mod現在バージョン, 必要なMCバージョン)
    let mut mc_mismatch: Vec<(String, String, String)> = Vec::new();
    // 現在のMCバージョン（"but only X is present" から）
    let mut current_mc: Option<String> = None;
    // 依存Mod不足: (不足Mod名, 要求元Mod名)
    let mut missing_dep: Vec<(String, String)> = Vec::new();
    // その他バージョン不一致: (対象Mod, 要求Mod, 必要バージョン)
    let mut other_mismatch: Vec<(String, String, String)> = Vec::new();

    for line in lines {
        // HTML タグを除去してプレーンテキストにする
        let t = strip_html(line.trim());
        let quotes = all_quoted(&t);

        // ── パターン1: Fabric "Replace mod 'X' (id) ver with any version..." ──
        // Fabricが解決策として提示するMod（ユーザーが実際に更新すべきもの）
        // JAR-in-JARで内包されたModはここには現れず、親Modのみが列挙される
        if t.contains("Replace mod") && t.contains("with any version") {
            if let Some(mod_name) = quotes.first().cloned() {
                let mod_ver = extract_version_in_parens(&t);
                let requirement = extract_replace_requirement(&t).unwrap_or_default();
                if !replace_mods.iter().any(|(n, _, _)| n == &mod_name) {
                    replace_mods.push((mod_name, mod_ver.unwrap_or_default(), requirement));
                }
            }
            continue;
        }

        // ── パターン2: MCバージョン不一致 ────────────────────────────────────
        // "Mod 'X' (id) ver requires version Y of 'Minecraft' (minecraft),
        //  but only the wrong version is present: Z!"
        if t.contains("requires version")
            && (t.to_lowercase().contains("of 'minecraft'")
                || t.to_lowercase().contains("of \"minecraft\""))
            && t.contains("but only")
        {
            // 対象Mod名: quotes[0]
            // 必要なMCバージョン: "requires version Y of" の Y
            // 現在のMCバージョン: "present: Z" の Z
            if let Some(mod_name) = quotes.first().cloned() {
                let needed_mc = extract_word_between(&t, "requires version ", " of");
                let actual_mc = extract_word_after_str(&t, "present: ");
                if let Some(mc) = &actual_mc
                    && current_mc.is_none()
                {
                    current_mc = Some(mc.trim_end_matches('!').to_string());
                }
                let mod_ver = extract_version_in_parens(&t);
                if !mc_mismatch.iter().any(|(n, _, _)| n == &mod_name) {
                    mc_mismatch.push((
                        mod_name,
                        mod_ver.unwrap_or_default(),
                        needed_mc.unwrap_or_default(),
                    ));
                }
            }
            continue;
        }

        // ── パターン3: 依存Mod不足 ────────────────────────────────────────────
        // "Mod 'X' requires any version of 'Y', which is missing!"
        if t.contains("requires any version of") && t.contains("missing") {
            if quotes.len() >= 2 {
                let requester = quotes[0].clone();
                let missing = quotes[1].clone();
                if !missing_dep
                    .iter()
                    .any(|(m, req)| m == &missing && req == &requester)
                {
                    missing_dep.push((missing, requester));
                }
            }
            continue;
        }

        // "Mod 'X' requires version 16.0.0 or later of cloth-config, which is missing!"
        if t.contains("requires version") && t.contains(" of ") && t.contains("missing") {
            let requester = quotes.first().cloned().unwrap_or_default();
            if let Some(missing) = extract_dependency_after_of(&t)
                && !missing_dep
                    .iter()
                    .any(|(m, req)| m == &missing && req == &requester)
            {
                missing_dep.push((missing, requester));
            }
            continue;
        }

        // Fabric solver summary: "Fix: add [add:cloth-config 16.0.0 ...]"
        if t.contains("Fix: add")
            && let Some(missing) = extract_fix_add_dependency(&t)
            && !missing_dep.iter().any(|(m, _)| m == &missing)
        {
            missing_dep.push((missing, String::new()));
        }

        // ── パターン4: Mod間バージョン不一致 ─────────────────────────────────
        // "'X' requires version >=Y of 'Z'" (minecraft以外)
        if (t.contains("requires version") || t.contains("requires ^") || t.contains("requires ~"))
            && t.contains(" of ")
            && !t.to_lowercase().contains("minecraft")
        {
            if quotes.len() < 2 {
                continue;
            }
            let requester = quotes[0].clone();
            let Some(target) = quotes.last().cloned() else {
                continue;
            };
            let ver = t
                .split_whitespace()
                .find(|w| {
                    w.starts_with(">=")
                        || w.starts_with('>')
                        || w.starts_with('^')
                        || w.starts_with('~')
                        || w.starts_with('[')
                })
                .map(|v| v.trim_end_matches(',').to_string())
                .unwrap_or_default();
            let entry = (target, requester, ver);
            if !other_mismatch.iter().any(|(t, _, _)| t == &entry.0) {
                other_mismatch.push(entry);
            }
        }

        // ── パターン5: Forge/NeoForge 不足 ───────────────────────────────────
        if t.contains("requires version")
            && t.contains("not installed")
            && let Some(dep) = quotes.first().cloned()
        {
            let requester = quotes.get(1).cloned().unwrap_or_default();
            if !missing_dep
                .iter()
                .any(|(m, req)| m == &dep && req == &requester)
            {
                missing_dep.push((dep, requester));
            }
        }
    }

    // ── メッセージ生成 ────────────────────────────────────────────────────────
    let en = lang == "en";
    if !replace_mods.is_empty() {
        let replace_names: Vec<String> = replace_mods
            .iter()
            .map(|(name, _, _)| name.to_ascii_lowercase())
            .collect();
        missing_dep.retain(|(missing, _)| {
            !replace_names
                .iter()
                .any(|name| name == &missing.to_ascii_lowercase())
        });
        other_mismatch.retain(|(target, _, _)| {
            !replace_names
                .iter()
                .any(|name| name == &target.to_ascii_lowercase())
        });
    }

    if replace_mods.is_empty()
        && mc_mismatch.is_empty()
        && missing_dep.is_empty()
        && other_mismatch.is_empty()
    {
        return if en {
            "Mod resolution failed: a mod version is incompatible or a required mod is missing. \
Check that each mod supports your Minecraft version and install or update any missing dependencies."
        } else {
            "Modのバージョンが合っていないか、必須のModが不足しているため起動できません。\
各Modが対応しているMinecraftバージョンを確認し、必要なModをインストールまたは更新してください。"
        }
        .to_string();
    }

    let mut parts: Vec<String> = Vec::new();

    // Replace mod グループ
    let mc_ver = current_mc.as_deref().unwrap_or(if en {
        "your Minecraft version"
    } else {
        "現在のMinecraftバージョン"
    });
    if !replace_mods.is_empty() {
        let mod_lines: Vec<String> = replace_mods
            .iter()
            .take(6)
            .map(|(name, ver, requirement)| {
                if en {
                    if ver.is_empty() && requirement.is_empty() {
                        format!("• {} — update this mod", name)
                    } else if requirement.is_empty() {
                        format!("• {} {} — update this mod", name, ver)
                    } else if ver.is_empty() {
                        format!("• {} — install {}", name, requirement)
                    } else {
                        format!("• {} {} — install {}", name, ver, requirement)
                    }
                } else if ver.is_empty() && requirement.is_empty() {
                    format!("・{} — 更新してください", name)
                } else if requirement.is_empty() {
                    format!("・{} {} — 更新してください", name, ver)
                } else if ver.is_empty() {
                    format!("・{} — {} を入れてください", name, requirement)
                } else {
                    format!("・{} {} — {} を入れてください", name, ver, requirement)
                }
            })
            .collect();
        parts.push(if en {
            format!(
                "The following mod versions are incompatible. Replace them with a compatible version:\n{}",
                mod_lines.join("\n")
            )
        } else {
            format!(
                "以下のModのバージョンが要求と合っていません。対応版に置き換えてください:\n{}",
                mod_lines.join("\n")
            )
        });
    } else if !mc_mismatch.is_empty() {
        let mod_lines: Vec<String> = mc_mismatch
            .iter()
            .take(6)
            .map(|(name, ver, needed)| {
                if en {
                    if ver.is_empty() && needed.is_empty() {
                        format!("• {}", name)
                    } else if needed.is_empty() {
                        format!("• {} ({})", name, ver)
                    } else {
                        format!(
                            "• {} {} — requires Minecraft {} (current: {})",
                            name, ver, needed, mc_ver
                        )
                    }
                } else if ver.is_empty() && needed.is_empty() {
                    format!("・{}", name)
                } else if needed.is_empty() {
                    format!("・{} ({})", name, ver)
                } else {
                    format!(
                        "・{} {} — Minecraft {} 用（現在: {}）",
                        name, ver, needed, mc_ver
                    )
                }
            })
            .collect();

        let all_same_needed = mc_mismatch.len() > 1 && {
            let first = &mc_mismatch[0].2;
            mc_mismatch.iter().all(|(_, _, needed)| needed == first)
        };
        let jar_in_jar_note = if all_same_needed {
            if en {
                "\n※ Mods not visible in your mods folder may be bundled (JAR-in-JAR) inside a parent mod — updating the parent mod may resolve all issues at once."
            } else {
                "\n※ Modsフォルダーに見当たらないModはメインModに内包（JAR-in-JAR）されている場合があります。その場合はメインModを更新するだけで全て解決します。"
            }
        } else {
            ""
        };

        parts.push(if en {
            format!(
                "The following mods are not compatible with Minecraft {}. \
Find and download the {} version of each mod on Modrinth and replace the old files:\n{}{}",
                mc_ver, mc_ver, mod_lines.join("\n"), jar_in_jar_note
            )
        } else {
            format!(
                "以下のModが Minecraft {} に対応していません。Modrinthで各Modの {} 対応版を探してダウンロードし、古いファイルと置き換えてください:\n{}{}",
                mc_ver, mc_ver, mod_lines.join("\n"), jar_in_jar_note
            )
        });
    }

    // 依存Mod不足グループ
    if !missing_dep.is_empty() {
        let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
        for (missing, requester) in &missing_dep {
            if let Some((_, requesters)) = grouped.iter_mut().find(|(name, _)| name == missing) {
                if !requester.is_empty() && !requesters.iter().any(|item| item == requester) {
                    requesters.push(requester.clone());
                }
            } else {
                grouped.push((
                    missing.clone(),
                    if requester.is_empty() {
                        Vec::new()
                    } else {
                        vec![requester.clone()]
                    },
                ));
            }
        }
        let dep_lines: Vec<String> = grouped
            .iter()
            .take(4)
            .map(|(missing, requesters)| {
                if en {
                    if requesters.is_empty() {
                        format!("• {}", missing)
                    } else {
                        format!("• {} (required by {})", missing, requesters.join(", "))
                    }
                } else if requesters.is_empty() {
                    format!("・{}", missing)
                } else {
                    format!("・{} （{}に必要）", missing, requesters.join("、"))
                }
            })
            .collect();
        parts.push(if en {
            format!(
                "The following required mods are missing from your mods folder. Download and add them:\n{}",
                dep_lines.join("\n")
            )
        } else {
            format!(
                "以下の必須ModがModsフォルダーにありません。Modrinthからダウンロードして追加してください:\n{}",
                dep_lines.join("\n")
            )
        });
    }

    // Mod間バージョン不一致
    if !other_mismatch.is_empty() {
        let lines_out: Vec<String> = other_mismatch
            .iter()
            .take(3)
            .map(|(target, req, ver)| {
                if en {
                    if ver.is_empty() {
                        format!(
                            "• {} is incompatible with the version required by {}",
                            target, req
                        )
                    } else {
                        format!(
                            "• {} {} or later is required (requested by {})",
                            target, ver, req
                        )
                    }
                } else if ver.is_empty() {
                    format!("・{} のバージョンが {} の要求と合いません", target, req)
                } else {
                    format!("・{} {} 以上が必要です（{} から要求）", target, ver, req)
                }
            })
            .collect();
        parts.push(if en {
            format!(
                "The following mod versions are incompatible. Update them to their latest versions:\n{}",
                lines_out.join("\n")
            )
        } else {
            format!(
                "以下のModのバージョンが合っていません。最新版に更新してください:\n{}",
                lines_out.join("\n")
            )
        });
    }

    parts.join("\n\n")
}

// ── 文字列ユーティリティ ──────────────────────────────────────────────────────

/// HTML タグ（<br>、<html> 等）を除去する
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// "prefix TOKEN rest" から TOKEN を返す（スペース区切り）
fn extract_word_between(s: &str, after: &str, before: &str) -> Option<String> {
    let start = s.find(after)? + after.len();
    let sub = &s[start..];
    let end = sub.find(before).unwrap_or(sub.len());
    let word = sub[..end].trim().to_string();
    if word.is_empty() { None } else { Some(word) }
}

/// "marker TOKEN" の TOKEN を返す（スペース区切り1単語）
fn extract_word_after_str(s: &str, marker: &str) -> Option<String> {
    let pos = s.find(marker)?;
    let after = s[pos + marker.len()..].trim();
    let word: String = after
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '!')
        .collect();
    if word.is_empty() { None } else { Some(word) }
}

fn extract_dependency_after_of(s: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    let start = lower.find(" of ")? + 4;
    let dep: String = s[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    if dep.is_empty() { None } else { Some(dep) }
}

fn extract_fix_add_dependency(s: &str) -> Option<String> {
    let marker = "add:";
    let start = s.find(marker)? + marker.len();
    let dep: String = s[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    if dep.is_empty() { None } else { Some(dep) }
}

fn extract_replace_requirement(s: &str) -> Option<String> {
    let marker = "with any version ";
    let start = s.find(marker)? + marker.len();
    let tail = s[start..].trim();
    let end = tail
        .find(" that is compatible with")
        .or_else(|| tail.find('\n'))
        .unwrap_or(tail.len());
    let requirement = tail[..end].trim().trim_end_matches('!').to_string();
    if requirement.is_empty() {
        None
    } else {
        Some(requirement)
    }
}

/// "(modid) 1.2.3" の "1.2.3" を返す（括弧の後の最初のバージョン文字列）
fn extract_version_in_parens(s: &str) -> Option<String> {
    let close = s.find(')')?;
    let after = s[close + 1..].trim();
    let ver: String = after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '+')
        .collect();
    if ver.is_empty()
        || !ver
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
    {
        None
    } else {
        Some(ver)
    }
}

/// corpus には生ログ行と例外メッセージが統合されているため、
/// 別途 exceptions を受け取る必要がない。
pub(super) fn build_mixin_message(corpus: &[String], lang: &str) -> String {
    let en = lang == "en";
    let mut failed_mods: Vec<String> = Vec::new();

    for text in corpus {
        let lower = text.to_lowercase();
        if !lower.contains("mixin apply failed") && !lower.contains("mixinapplyerror") {
            continue;
        }
        if let Some(pos) = text.find("from mod ") {
            let after = text[pos + 9..].trim_start();
            let id: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if !id.is_empty() && !failed_mods.contains(&id) {
                failed_mods.push(id);
            }
        }
    }

    if failed_mods.is_empty() {
        return if en {
            "A mod failed to load. Try removing recently added or updated mods one by one to identify the cause."
        } else {
            "あるModの読み込みに失敗しました。\
最後に追加・更新したModを一つずつ外して、起動できるか確認してください。"
        }.to_string();
    }

    let mod_lines: Vec<String> = failed_mods
        .iter()
        .take(5)
        .map(|id| {
            if en {
                format!("• {}", id)
            } else {
                format!("・{}", id)
            }
        })
        .collect();

    if en {
        format!(
            "The following mods failed to load. They may be outdated or conflicting with another mod. \
Try updating them or removing them one by one:\n{}",
            mod_lines.join("\n")
        )
    } else {
        format!(
            "以下のModの読み込みに失敗しました。バージョンが合っていないか、他のModと競合している可能性があります。\
最新版に更新するか、一時的に外して起動できるか確認してください:\n{}",
            mod_lines.join("\n")
        )
    }
}

pub(super) fn build_duplicate_message(lines: &[String], lang: &str) -> String {
    let en = lang == "en";
    for line in lines {
        let t = line.trim();
        if !t.contains("Duplicate mod") && !t.contains("DuplicateModException") {
            continue;
        }
        let quotes = all_quoted(t);
        if let Some(name) = quotes.first() {
            return if en {
                format!(
                    "'{}' is installed more than once. Open your mods folder and remove the duplicate '{}' file.",
                    name, name
                )
            } else {
                format!(
                    "'{}' が重複してインストールされています。\
Modsフォルダーを開いて '{}' のファイルが2つ以上ないか確認し、古い方を削除してください。",
                    name, name
                )
            };
        }
    }
    if en {
        "A mod is installed more than once. Open your mods folder and remove any duplicate mod files.".to_string()
    } else {
        "同じModが重複してインストールされています。Modsフォルダーを確認して重複しているファイルを削除してください。".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::build_mod_dep_message;

    #[test]
    fn replace_requirement_keeps_dotted_version_ranges() {
        let message = build_mod_dep_message(
            &[String::from(
                "Replace mod 'Sodium' (sodium) 0.9.1-beta.3+mc26.2 with any version between 0.9.1-beta.3 (inclusive) and 0.10- (exclusive) that is compatible with:",
            )],
            "en",
        );

        assert!(message.contains("between 0.9.1-beta.3 (inclusive) and 0.10- (exclusive)"));
        assert!(!message.contains("between 0\n"));
    }

    #[test]
    fn missing_dependency_includes_requesting_mod() {
        let message = build_mod_dep_message(
            &[
                String::from("Mod resolution failed"),
                String::from(
                    "Mod 'More Culling' (moreculling) 1.4.0-beta.1 requires version 16.0.0 or later of cloth-config, which is missing!",
                ),
            ],
            "en",
        );

        assert!(message.contains("cloth-config"));
        assert!(message.contains("required by More Culling"));
    }

    #[test]
    fn replace_message_names_the_mod_to_replace_once() {
        let message = build_mod_dep_message(
            &[
                String::from(
                    "Replace mod 'Sodium' (sodium) 0.9.0+mc26.2 with any version between 0.9.1-beta.3 (inclusive) and 0.10- (exclusive).",
                ),
                String::from(
                    "Mod 'Reese's Sodium Options' (reeses-sodium-options) 2.2.2+mc26.2 requires version 0.9.1-beta.3 or later of mod 'Sodium' (sodium), but only the wrong version is present: 0.9.0+mc26.2!",
                ),
            ],
            "en",
        );

        assert!(message.contains("Sodium 0.9.0+mc26.2"));
        assert!(message.contains("between 0.9.1-beta.3 (inclusive) and 0.10- (exclusive)"));
        assert!(!message.contains("• sodium\n"));
    }
}
