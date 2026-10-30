use serde::{Deserialize, Serialize};

pub const ISSUE_URL: &str = "https://github.com/HikyouSMP/hikyou-launcher/issues/new";
const MAX_ERROR_BYTES: usize = 16 * 1024;
const MAX_URL_BYTES: usize = 6000;
const MAX_COUNTER: u32 = 1_000_000;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    LaunchFailed,
    ModResolutionLimit,
    ModResolutionUnsatisfiable,
    ModResolutionUnsupported,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitKind {
    SearchStates,
    SearchStatesProjects,
    Projects,
    WallTime,
    Other,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SolverCounters {
    pub states: Option<u32>,
    pub projects: Option<u32>,
    pub roots: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReportContext {
    pub code: ErrorCode,
    pub limit: Option<LimitKind>,
    pub counters: Option<SolverCounters>,
}

#[derive(Debug, Serialize)]
pub struct ReportPreview {
    pub context: ReportContext,
    pub title: String,
    pub body: String,
}

// Raw messages are classification input only, never report content. This is not
// arbitrary-text redaction: unknown details are deliberately omitted.
pub fn prepare(error: &str) -> Result<ReportPreview, String> {
    if error.is_empty() || error.len() > MAX_ERROR_BYTES {
        return Err("launch_report_invalid_input".into());
    }
    let error = error.strip_prefix("Error: ").map_or(error, |value| value);
    let mut counters = None;
    let (code, limit) = if let Some(detail) = error.strip_prefix("mod_resolution_limit:") {
        let kind = detail.split(';').next().map_or("", str::trim);
        let limit = match kind {
            "search states" => LimitKind::SearchStates,
            "search states/projects" => LimitKind::SearchStatesProjects,
            "projects" => LimitKind::Projects,
            "wall time" => LimitKind::WallTime,
            _ => LimitKind::Other,
        };
        counters = parse_counters(detail)?;
        (ErrorCode::ModResolutionLimit, Some(limit))
    } else if error.starts_with("mod_resolution_unsatisfiable:") {
        (ErrorCode::ModResolutionUnsatisfiable, None)
    } else if error.starts_with("mod_resolution_unsupported:") {
        (ErrorCode::ModResolutionUnsupported, None)
    } else {
        (ErrorCode::LaunchFailed, None)
    };
    render(ReportContext {
        code,
        limit,
        counters,
    })
}

fn parse_counters(detail: &str) -> Result<Option<SolverCounters>, String> {
    let mut counters = SolverCounters::default();
    let mut found = false;
    for field in detail.split(';').skip(1) {
        let Some((key, value)) = field.trim().split_once('=') else {
            continue;
        };
        let target = match key {
            "states" => &mut counters.states,
            "projects" => &mut counters.projects,
            "roots" => &mut counters.roots,
            _ => continue,
        };
        if target.is_some()
            || value.is_empty()
            || value.len() > 7
            || !value.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err("launch_report_invalid_counters".into());
        }
        let count = value
            .parse::<u32>()
            .map_err(|_| "launch_report_invalid_counters")?;
        if count > MAX_COUNTER {
            return Err("launch_report_invalid_counters".into());
        }
        *target = Some(count);
        found = true;
    }
    Ok(if found { Some(counters) } else { None })
}

pub fn render(context: ReportContext) -> Result<ReportPreview, String> {
    if (context.code == ErrorCode::ModResolutionLimit) != context.limit.is_some() {
        return Err("launch_report_invalid_context".into());
    }
    let mut counts = String::new();
    if let Some(counters) = &context.counters {
        if context.code != ErrorCode::ModResolutionLimit {
            return Err("launch_report_invalid_context".into());
        }
        for (name, value) in [
            ("states", counters.states),
            ("projects", counters.projects),
            ("roots", counters.roots),
        ] {
            if let Some(count) = value {
                if count > MAX_COUNTER {
                    return Err("launch_report_invalid_counters".into());
                }
                counts.push_str(&format!("Solver {name}: {count}\n"));
            }
        }
    }
    let code = match context.code {
        ErrorCode::LaunchFailed => "launch_failed",
        ErrorCode::ModResolutionLimit => "mod_resolution_limit",
        ErrorCode::ModResolutionUnsatisfiable => "mod_resolution_unsatisfiable",
        ErrorCode::ModResolutionUnsupported => "mod_resolution_unsupported",
    };
    let limit = match context.limit {
        Some(LimitKind::SearchStates) => "search states",
        Some(LimitKind::SearchStatesProjects) => "search states/projects",
        Some(LimitKind::Projects) => "projects",
        Some(LimitKind::WallTime) => "wall time",
        Some(LimitKind::Other) => "other bounded resolution work",
        None => "not applicable",
    };
    let outcome = if context.code == ErrorCode::ModResolutionLimit {
        "Resolution stopped at a safety limit; no partial solution accepted."
    } else {
        "Launch did not complete."
    };
    Ok(ReportPreview {
        title: format!("Launch error: {code}"),
        body: format!(
            "## Launch error\n\nError code: {code}\nResolution limit: {limit}\n{counts}{outcome}\n\n\
             ## Technical context\n\nLauncher: {}\nOS: {}\nArchitecture: {}\n\
             Target Minecraft version: not captured\n\n\
             Raw messages, logs, account details and local paths are not included.\n\n\
             ## Reproduction\n\nPlease describe what you tried here after reviewing for private information.\n",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        context,
    })
}

pub fn issue_url(context: ReportContext, reviewed: bool) -> Result<String, String> {
    if !reviewed {
        return Err("launch_report_review_required".into());
    }
    let report = render(context)?;
    let mut url = url::Url::parse(ISSUE_URL).map_err(|_| "launch_report_invalid_destination")?;
    url.query_pairs_mut()
        .append_pair("title", &report.title)
        .append_pair("body", &report.body);
    if url.as_str().len() > MAX_URL_BYTES {
        return Err("launch_report_url_too_long".into());
    }
    Ok(url.into())
}

pub fn open_issue(
    context: ReportContext,
    reviewed: bool,
    opener: impl FnOnce(&str) -> Result<(), String>,
) -> Result<(), String> {
    let url = issue_url(context, reviewed)?;
    opener(&url).map_err(|_| "launch_report_open_failed".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn private_text_is_never_report_content() -> Result<(), String> {
        let secrets = [
            "Bearer eyJhbGciOiJIUzI1NiJ9.secret",
            "access_token=secret-token",
            "C:\\private-test\\worlds",
            "/private-test/worlds",
            "player=PrivatePerson uuid=123e4567-e89b-12d3-a456-426614174000",
            "https://evil.invalid/?token=secret-token",
        ];
        for secret in secrets {
            for prefix in ["", "mod_resolution_limit: search states/projects; "] {
                let report = prepare(&format!("{prefix}{secret}"))?;
                let url = issue_url(report.context.clone(), true)?;
                assert!(!report.body.contains(secret));
                assert!(!url.contains("PrivatePerson"));
                assert!(!url.contains("secret-token"));
                assert!(!url.contains("123e4567"));
            }
        }
        let report =
            prepare("mod_resolution_limit: search states/projects; no partial solution accepted")?;
        assert_eq!(report.context.limit, Some(LimitKind::SearchStatesProjects));
        assert!(report.body.contains("no partial solution accepted"));
        Ok(())
    }

    #[test]
    fn rejects_oversized_and_untrusted_inputs() -> Result<(), String> {
        assert!(prepare("").is_err());
        assert!(prepare(&"x".repeat(MAX_ERROR_BYTES + 1)).is_err());
        assert!(prepare(&"\u{754c}".repeat(MAX_ERROR_BYTES / 3 + 1)).is_err());
        assert!(
            serde_json::from_str::<ReportContext>(
                r#"{"code":"https://evil.invalid","limit":null}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<ReportContext>(
                r#"{"code":"launch_failed","limit":null,"body":"secret"}"#
            )
            .is_err()
        );
        assert!(
            render(ReportContext {
                code: ErrorCode::LaunchFailed,
                limit: Some(LimitKind::Projects),
                counters: None,
            })
            .is_err()
        );
        let report = prepare("mod_resolution_limit: C:\\private-test; token=secret-token")?;
        assert_eq!(report.context.limit, Some(LimitKind::Other));
        assert!(!report.body.contains("PrivatePerson"));
        Ok(())
    }

    #[test]
    fn only_bounded_typed_solver_counters_are_reported() -> Result<(), String> {
        for (reason, kind) in [
            ("search states", LimitKind::SearchStates),
            ("projects", LimitKind::Projects),
            ("search states/projects", LimitKind::SearchStatesProjects),
        ] {
            let report = prepare(&format!(
                "mod_resolution_limit: {reason}; no partial solution accepted; states=20001; projects=128; roots=12; token=private-token"
            ))?;
            assert_eq!(report.context.limit, Some(kind));
            assert!(
                report
                    .body
                    .contains("Solver states: 20001\nSolver projects: 128\nSolver roots: 12\n")
            );
            assert!(!report.body.contains("private-token"));
            assert!(issue_url(report.context, true)?.len() <= MAX_URL_BYTES);
        }
        for bad in [
            "states=1000001",
            "states=-1",
            "states=1.5",
            "states=1e3",
            "states=+1",
            "states=",
            "states=2; states=3",
            "roots=/private-test/worlds",
            "projects=123e4567-e89b-12d3-a456-426614174000",
        ] {
            assert!(prepare(&format!("mod_resolution_limit: search states; {bad}")).is_err());
        }
        assert!(
            render(ReportContext {
                code: ErrorCode::ModResolutionLimit,
                limit: Some(LimitKind::Projects),
                counters: Some(SolverCounters {
                    states: Some(MAX_COUNTER + 1),
                    projects: None,
                    roots: None
                }),
            })
            .is_err()
        );
        assert!(serde_json::from_str::<ReportContext>(r#"{"code":"mod_resolution_limit","limit":"projects","counters":{"states":"private"}}"#).is_err());
        Ok(())
    }

    #[test]
    fn fixed_bounded_destination_and_explicit_consent() -> Result<(), String> {
        let report = prepare("mod_resolution_limit: projects; no partial solution accepted")?;
        let called = Cell::new(false);
        assert!(
            open_issue(report.context.clone(), false, |_| {
                called.set(true);
                Ok(())
            })
            .is_err()
        );
        assert!(!called.get());
        let url = issue_url(report.context.clone(), true)?;
        let parsed = url::Url::parse(&url).map_err(|_| "test URL parse failed")?;
        assert_eq!(parsed.origin().ascii_serialization(), "https://github.com");
        assert_eq!(parsed.path(), "/HikyouSMP/hikyou-launcher/issues/new");
        assert!(url.len() <= MAX_URL_BYTES);
        let pairs: Vec<_> = parsed.query_pairs().collect();
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[1].1, report.body);
        assert_eq!(
            open_issue(report.context, true, |_| Err("private OS failure".into())),
            Err("launch_report_open_failed".into())
        );
        Ok(())
    }
}
