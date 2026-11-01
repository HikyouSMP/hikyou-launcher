use std::collections::HashMap;
use zeroize::Zeroizing;

use crate::core::manifest::{ArgumentValue, StringOrVec};

use super::launcher::LaunchRequest;
// ────────────────────────────────────────────────────────────────────────────
// 引数組み立て
// ────────────────────────────────────────────────────────────────────────────

pub(super) fn build_jvm_args(
    req: &LaunchRequest<'_>,
    vars: &HashMap<&str, String>,
    tuning: &super::jvm_policy::Plan,
) -> Vec<String> {
    let mut args = manifest_jvm_args(req, vars);
    let manual = req.jvm_flags_override.filter(|s| !s.trim().is_empty());
    args.extend(memory_arguments(
        req.memory_max_mb,
        tuning.initial_heap_mb,
        manual,
    ));
    if let Some(flags) = manual {
        for flag in flags.split_whitespace() {
            args.push(flag.to_string());
        }
    } else {
        args.extend(tuning.flags.iter().cloned());
    }

    args.push("-Dfile.encoding=UTF-8".to_string());
    if let Some(prepared) = req
        .prepared_mods
        .filter(|target| target.loader() == "fabric")
    {
        args.push(format!(
            "-Dfabric.modsFolder={}",
            prepared.mods_dir().display()
        ));
    }

    if req.version_json.arguments.is_none() {
        let natives_dir = vars.get("natives_directory").cloned().unwrap_or_default();
        args.push(format!("-Djava.library.path={}", natives_dir));
        args.push("-Dminecraft.launcher.brand=hikyou-launcher".to_string());
        args.push(format!(
            "-Dminecraft.launcher.version={}",
            env!("CARGO_PKG_VERSION")
        ));
    }

    args
}

fn manifest_jvm_args(req: &LaunchRequest<'_>, vars: &HashMap<&str, String>) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(arguments) = &req.version_json.arguments {
        for arg in &arguments.jvm {
            collect_arg_value(arg, &mut args, |template| expand_template(template, vars));
        }
        args.retain(|arg| {
            !arg.starts_with("-XX:HeapDumpPath=MojangTricksIntelDrivers") && arg != "-Xss1M"
        });
    }
    args
}

pub(super) fn external_gc(req: &LaunchRequest<'_>, vars: &HashMap<&str, String>) -> bool {
    req.jvm_flags_override
        .is_some_and(|flags| !flags.trim().is_empty())
        || manifest_jvm_args(req, vars).iter().any(|arg| {
            arg.strip_prefix("-XX:+Use")
                .is_some_and(|name| name.ends_with("GC"))
        })
}

fn memory_arguments(maximum: u32, initial: u32, manual: Option<&str>) -> Vec<String> {
    let manual_max = manual.is_some_and(|flags| {
        flags
            .split_whitespace()
            .any(|flag| flag.starts_with("-Xmx"))
    });
    let manual_initial = manual.is_some_and(|flags| {
        flags
            .split_whitespace()
            .any(|flag| flag.starts_with("-Xms"))
    });
    let mut args = Vec::new();
    if !manual_max {
        args.push(format!("-Xmx{maximum}M"));
    }
    if !manual_max && !manual_initial {
        args.push(format!("-Xms{initial}M"));
    }
    args
}

/// ゲーム引数を組み立てる。
/// 1.13+ は arguments.game を、1.12以前は minecraftArguments を使う。
pub(super) fn build_game_args(
    req: &LaunchRequest<'_>,
    vars: &HashMap<&str, String>,
) -> Vec<Zeroizing<String>> {
    let mut args: Vec<Zeroizing<String>> = Vec::new();

    if let Some(arguments) = &req.version_json.arguments {
        // 1.13+: arguments.game を展開
        for arg in &arguments.game {
            collect_arg_value(arg, &mut args, |template| {
                expand_game_template(template, vars, &req.auth.access_token)
            });
        }
    } else if let Some(mc_args) = &req.version_json.minecraft_arguments {
        // 1.12以前: スペース区切り文字列をテンプレート展開
        for token in mc_args.split_whitespace() {
            args.push(expand_game_template(token, vars, &req.auth.access_token));
        }
    }

    // ── カスタム解像度 ────────────────────────────────────────────────────────
    // has_custom_resolution フィーチャールールは collect_arg_value でスキップされるため、
    // ここで明示的に追加する
    if let (Some(w), Some(h)) = (req.window_width, req.window_height) {
        args.push(Zeroizing::new("--width".to_string()));
        args.push(Zeroizing::new(w.to_string()));
        args.push(Zeroizing::new("--height".to_string()));
        args.push(Zeroizing::new(h.to_string()));
    }

    args
}

/// ArgumentValue を展開して args に追加する。
/// - Simple: テンプレート変数を置換して追加
/// - Conditional: rules に feature フィールドがある場合はスキップ（デモモード等）
///   rules に os フィールドがある場合は OS チェックして展開
fn collect_arg_value<T>(arg: &ArgumentValue, args: &mut Vec<T>, expand: impl Fn(&str) -> T) {
    match arg {
        ArgumentValue::Simple(s) => {
            args.push(expand(s));
        }
        ArgumentValue::Conditional(cond) => {
            // feature ルール（is_demo_user, has_custom_resolution など）はスキップ
            let has_feature_rule = cond.rules.iter().any(|r| r.os.is_none());
            if has_feature_rule {
                return;
            }

            // OS ルールがある場合はチェック
            let current_os = match std::env::consts::OS {
                "windows" => "windows",
                "macos" => "osx",
                "linux" => "linux",
                _ => return,
            };

            let allowed = cond.rules.iter().all(|r| {
                let os_matches =
                    r.os.as_ref()
                        .and_then(|o| o.name.as_deref())
                        .map(|n| n == current_os)
                        .unwrap_or(true);
                match r.action {
                    crate::core::manifest::RuleAction::Allow => os_matches,
                    crate::core::manifest::RuleAction::Disallow => !os_matches,
                }
            });

            if allowed {
                match &cond.value {
                    StringOrVec::Single(s) => args.push(expand(s)),
                    StringOrVec::Multiple(sv) => {
                        for s in sv {
                            args.push(expand(s));
                        }
                    }
                }
            }
        }
    }
}

/// `${variable_name}` 形式のテンプレートを vars で置換する
fn expand_template(s: &str, vars: &HashMap<&str, String>) -> String {
    let mut result = s.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("${{{}}}", key), value);
    }
    result
}

/// Game arguments can contain the Minecraft access token. Build each expanded
/// argument in one owned buffer so the launcher can zeroize it after spawning
/// Java, rather than producing replacement intermediates for the secret.
fn expand_game_template(
    template: &str,
    vars: &HashMap<&str, String>,
    access_token: &str,
) -> Zeroizing<String> {
    let mut expanded = String::with_capacity(template.len() + access_token.len());
    let mut remaining = template;

    while let Some(start) = remaining.find("${") {
        expanded.push_str(&remaining[..start]);
        let after_start = &remaining[start + 2..];
        let Some(end) = after_start.find('}') else {
            expanded.push_str(&remaining[start..]);
            return Zeroizing::new(expanded);
        };

        let key = &after_start[..end];
        match key {
            "auth_access_token" => expanded.push_str(access_token),
            "auth_session" => {
                expanded.push_str("token:");
                expanded.push_str(access_token);
            }
            _ => {
                if let Some(value) = vars.get(key) {
                    expanded.push_str(value);
                } else {
                    expanded.push_str("${");
                    expanded.push_str(key);
                    expanded.push('}');
                }
            }
        }
        remaining = &after_start[end + 1..];
    }

    expanded.push_str(remaining);
    Zeroizing::new(expanded)
}

#[cfg(test)]
mod tests {
    use super::{expand_game_template, memory_arguments};
    use std::collections::HashMap;

    #[test]
    fn manual_heaps_are_not_overridden_or_combined_with_an_oversized_initial_heap() {
        assert_eq!(
            memory_arguments(8192, 1024, None),
            ["-Xmx8192M", "-Xms1024M"]
        );
        assert!(memory_arguments(8192, 1024, Some("-Xmx128M")).is_empty());
        assert_eq!(
            memory_arguments(8192, 1024, Some("-Xms256M")),
            ["-Xmx8192M"]
        );
    }

    #[test]
    fn game_template_expands_tokens_without_putting_them_in_common_variables() {
        let vars = HashMap::from([("auth_player_name", "Hikyou".to_string())]);
        let expanded = expand_game_template(
            "--accessToken=${auth_access_token};--session=${auth_session};--name=${auth_player_name}",
            &vars,
            "secret-token",
        );

        assert_eq!(
            &*expanded,
            "--accessToken=secret-token;--session=token:secret-token;--name=Hikyou"
        );
        assert!(!vars.values().any(|value| value.contains("secret-token")));
    }

    #[test]
    fn game_template_preserves_unknown_placeholders() {
        let vars = HashMap::new();
        let expanded = expand_game_template("--value=${unknown}", &vars, "secret-token");

        assert_eq!(&*expanded, "--value=${unknown}");
    }
}
