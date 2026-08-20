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
    is_liberica_nik: bool,
    java_major: u32,
    system_total_mb: u64,
) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    if let Some(arguments) = &req.version_json.arguments {
        for arg in &arguments.jvm {
            collect_arg_value(arg, &mut args, |template| expand_template(template, vars));
        }
        // Mojang が version JSON に埋め込む不要なフラグを除外
        args.retain(|a| {
            !a.starts_with("-XX:HeapDumpPath=MojangTricksIntelDrivers") && a != "-Xss1M"
        });
    }

    args.push(format!("-Xmx{}M", req.memory_max_mb));
    args.push(format!("-Xms{}M", req.memory_max_mb));

    // JVMフラグ上書きが指定されていればGCブロックをスキップして上書き
    let has_manual_flags = req.jvm_flags_override.filter(|s| !s.is_empty()).is_some();
    let performance_lab =
        matches!(req.jvm_tuning_mode, Some("performance_lab")) && !has_manual_flags;
    let lab_low_latency_gc = performance_lab && lab_module_enabled(req, "lowLatencyGc");
    let lab_g1_client = performance_lab && lab_module_enabled(req, "g1Client");
    let lab_aggressive_jit = performance_lab && lab_module_enabled(req, "aggressiveJit");
    let lab_code_cache = performance_lab && lab_module_enabled(req, "codeCache");
    let using_zgc =
        lab_low_latency_gc && should_use_zgc(req.memory_max_mb, system_total_mb, java_major);

    if let Some(flags) = req.jvm_flags_override.filter(|s| !s.is_empty()) {
        for flag in flags.split_whitespace() {
            args.push(flag.to_string());
        }
    } else if using_zgc {
        push_zgc_args(&mut args, java_major);
    } else if java_major >= 9 {
        push_g1_args(&mut args, java_major, lab_g1_client);
    } else {
        push_legacy_g1_args(&mut args);
    }

    if !has_manual_flags && should_pretouch_heap(req.memory_max_mb, system_total_mb) {
        args.push("-XX:+AlwaysPreTouch".to_string());
    }

    if is_liberica_nik && !has_manual_flags {
        push_liberica_nik_args(&mut args);
    }

    if performance_lab {
        push_performance_lab_args(
            &mut args,
            is_liberica_nik,
            java_major,
            using_zgc,
            lab_aggressive_jit,
            lab_code_cache,
        );
    }

    args.push("-Dfile.encoding=UTF-8".to_string());

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

fn should_use_zgc(memory_max_mb: u32, system_total_mb: u64, java_major: u32) -> bool {
    java_major >= 21 && memory_max_mb >= 4096 && (system_total_mb == 0 || system_total_mb >= 8192)
}

fn should_pretouch_heap(memory_max_mb: u32, system_total_mb: u64) -> bool {
    memory_max_mb >= 4096 && (system_total_mb == 0 || system_total_mb >= memory_max_mb as u64 * 2)
}

fn lab_module_enabled(req: &LaunchRequest<'_>, module: &str) -> bool {
    req.jvm_tuning_modules
        .filter(|s| !s.is_empty())
        .map(|modules| modules.split(',').any(|item| item.trim() == module))
        .unwrap_or(true)
}

fn push_zgc_args(args: &mut Vec<String>, java_major: u32) {
    args.push("-XX:+UseZGC".to_string());
    if java_major < 23 {
        args.push("-XX:+ZGenerational".to_string());
    }
    args.push("-XX:+DisableExplicitGC".to_string());
    args.push("-XX:+AlwaysActAsServerClassMachine".to_string());
    args.push("-XX:+PerfDisableSharedMem".to_string());
    if java_major >= 25 {
        args.push("-XX:+UseCompactObjectHeaders".to_string());
    }
}

fn push_g1_args(args: &mut Vec<String>, java_major: u32, lab_g1_client: bool) {
    args.push("-XX:+UseG1GC".to_string());
    args.push("-XX:MaxGCPauseMillis=50".to_string());
    args.push("-XX:+ParallelRefProcEnabled".to_string());
    args.push("-XX:+DisableExplicitGC".to_string());
    args.push("-XX:+AlwaysActAsServerClassMachine".to_string());
    args.push("-XX:+PerfDisableSharedMem".to_string());
    if lab_g1_client {
        args.push("-XX:+UnlockExperimentalVMOptions".to_string());
        args.push("-XX:G1NewSizePercent=40".to_string());
        args.push("-XX:G1MaxNewSizePercent=50".to_string());
        args.push("-XX:G1HeapRegionSize=16M".to_string());
        args.push("-XX:G1ReservePercent=15".to_string());
    }
    if java_major >= 25 {
        args.push("-XX:+UseCompactObjectHeaders".to_string());
    }
}

fn push_legacy_g1_args(args: &mut Vec<String>) {
    args.push("-XX:+UseG1GC".to_string());
    args.push("-XX:MaxGCPauseMillis=50".to_string());
    args.push("-XX:+ParallelRefProcEnabled".to_string());
    args.push("-XX:+DisableExplicitGC".to_string());
    args.push("-XX:+PerfDisableSharedMem".to_string());
}

fn push_liberica_nik_args(args: &mut Vec<String>) {
    args.push("-XX:+UseJVMCICompiler".to_string());
}

fn push_performance_lab_args(
    args: &mut Vec<String>,
    is_liberica_nik: bool,
    java_major: u32,
    using_zgc: bool,
    aggressive_jit: bool,
    code_cache: bool,
) {
    if code_cache && java_major >= 17 {
        args.push("-XX:ReservedCodeCacheSize=384M".to_string());
    }

    if using_zgc {
        args.push("-XX:-ZProactive".to_string());
    }

    if aggressive_jit && java_major >= 17 {
        args.push("-XX:AllocatePrefetchStyle=1".to_string());
    }

    if aggressive_jit && is_liberica_nik {
        args.push("-XX:+EagerJVMCI".to_string());
    }

    if aggressive_jit && is_liberica_nik && java_major < 25 {
        let graal_prefix = if java_major >= 21 {
            "jdk.graal"
        } else {
            "graal"
        };
        args.push(format!("-D{graal_prefix}.TuneInlinerExploration=1"));
        args.push(format!("-D{graal_prefix}.LoopRotation=true"));
        args.push(format!("-D{graal_prefix}.OptWriteMotion=true"));
    }
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
    use super::expand_game_template;
    use std::collections::HashMap;

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
