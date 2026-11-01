//! Optional tuning is a verified policy, never a prerequisite for launching Java.

use super::{java::JavaInstall, jvm_probe};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Collector {
    G1,
    Shenandoah,
    Zgc,
    JvmDefault,
    External,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct Plan {
    pub collector: Collector,
    pub flags: Vec<String>,
    pub initial_heap_mb: u32,
    pub reason: &'static str,
    pub fallback: bool,
    pub cache_hit: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Hardware {
    pub cores: Option<usize>,
    pub total_mb: u64,
    pub available_mb: u64,
}

impl Hardware {
    pub fn detect() -> Self {
        use sysinfo::{MemoryRefreshKind, RefreshKind, System};
        let system = System::new_with_specifics(
            RefreshKind::new().with_memory(MemoryRefreshKind::new().with_ram()),
        );
        let available = std::thread::available_parallelism().ok().map(usize::from);
        let physical = system.physical_core_count();
        Self {
            cores: match (physical, available) {
                (Some(a), Some(b)) => Some(a.min(b)),
                _ => None,
            },
            total_mb: system.total_memory() / 1024 / 1024,
            available_mb: system.available_memory() / 1024 / 1024,
        }
    }
}

fn initial_heap(heap: u32) -> u32 {
    (heap / 4).clamp(128, 1024).min(heap)
}

pub(super) fn validate_heap(heap: u32) -> Result<(), String> {
    if !(128..=1024 * 1024).contains(&heap) {
        return Err("Java maximum heap must be between 128 MiB and 1 TiB".into());
    }
    Ok(())
}

pub(super) fn external(heap: u32) -> Plan {
    Plan {
        collector: Collector::External,
        flags: Vec::new(),
        initial_heap_mb: initial_heap(heap),
        reason: "external_options",
        fallback: false,
        cache_hit: false,
    }
}

pub(super) fn environment_override() -> bool {
    ["JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "JDK_JAVA_OPTIONS"]
        .iter()
        .any(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()))
}

fn module_enabled(modules: Option<&str>, name: &str) -> bool {
    modules.is_none_or(|items| items.split(',').any(|item| item.trim() == name))
}

fn candidate(
    collector: Collector,
    major: u32,
    heap: u32,
    compact: bool,
    reason: &'static str,
) -> Plan {
    let mut flags: Vec<String> = match collector {
        Collector::G1 => vec!["-XX:+UseG1GC".into(), "-XX:MaxGCPauseMillis=50".into()],
        Collector::Shenandoah => vec!["-XX:+UseShenandoahGC".into()],
        Collector::Zgc => {
            let mut flags = vec!["-XX:+UseZGC".into()];
            if major == 21 || major == 22 {
                flags.push("-XX:+ZGenerational".into());
            }
            flags
        }
        Collector::JvmDefault | Collector::External => Vec::new(),
    };
    if compact {
        flags.push("-XX:+UseCompactObjectHeaders".into());
    }
    Plan {
        collector,
        flags,
        initial_heap_mb: initial_heap(heap),
        reason,
        fallback: false,
        cache_hit: false,
    }
}

fn candidates(
    major: u32,
    heap: u32,
    hardware: Hardware,
    lab: bool,
    modules: Option<&str>,
) -> Vec<Plan> {
    let roomy = hardware.cores.is_some_and(|cores| cores >= 6)
        && hardware.total_mb >= 16 * 1024
        && heap >= 4096
        && hardware.available_mb >= u64::from(heap) + 2048;
    let concurrent = hardware.cores.is_some_and(|cores| cores >= 4)
        && hardware.total_mb >= 8192
        && heap >= 2048
        && hardware.available_mb >= u64::from(heap) + 1024;
    let low_latency = !lab || module_enabled(modules, "lowLatencyGc");
    let mut result = Vec::new();
    if low_latency && major >= 21 && roomy {
        result.push(candidate(
            Collector::Zgc,
            major,
            heap,
            false,
            "latency_headroom",
        ));
    } else if low_latency && ((major == 17 && roomy) || (major == 21 && concurrent)) {
        result.push(candidate(
            Collector::Shenandoah,
            major,
            heap,
            false,
            "concurrent_headroom",
        ));
    }
    if major >= 25 {
        result.push(candidate(
            Collector::G1,
            major,
            heap,
            true,
            "compact_footprint",
        ));
    }
    result.push(candidate(Collector::G1, major, heap, false, "balanced"));
    if lab {
        for plan in &mut result {
            plan.flags.push("-XX:+DisableExplicitGC".into());
            if major >= 17 && module_enabled(modules, "codeCache") {
                plan.flags.push("-XX:ReservedCodeCacheSize=384M".into());
            }
            if major >= 17 && module_enabled(modules, "aggressiveJit") {
                plan.flags.push("-XX:AllocatePrefetchStyle=1".into());
            }
            if plan.collector == Collector::G1 && module_enabled(modules, "g1Client") {
                plan.flags.push("-XX:+UnlockExperimentalVMOptions".into());
                plan.flags.push("-XX:G1NewSizePercent=20".into());
                plan.flags.push("-XX:G1ReservePercent=20".into());
            }
        }
        // An unsupported lab option must not make the conservative policy unusable.
        result.push(candidate(
            Collector::G1,
            major,
            heap,
            false,
            "lab_unavailable",
        ));
    }
    result.push(candidate(
        Collector::JvmDefault,
        major,
        heap,
        false,
        "jvm_defaults",
    ));
    result
}

async fn select<F, Fut>(plans: Vec<Plan>, mut verify: F) -> Result<Plan, String>
where
    F: FnMut(Vec<String>, Collector) -> Fut,
    Fut: std::future::Future<Output = Result<Option<bool>, String>>,
{
    for (index, mut plan) in plans.into_iter().enumerate() {
        if let Some(cache_hit) = verify(plan.flags.clone(), plan.collector).await? {
            plan.fallback = index > 0;
            plan.cache_hit = cache_hit;
            if plan.fallback {
                log::warn!(
                    "[java] Preferred JVM tuning unavailable; selected {:?}",
                    plan.collector
                );
            }
            return Ok(plan);
        }
    }
    Err("Java preflight failed even with JVM defaults; no game process was started".into())
}

pub(super) async fn prepare(
    java: &JavaInstall,
    heap: u32,
    hardware: Hardware,
    mode: Option<&str>,
    modules: Option<&str>,
) -> Result<Plan, String> {
    validate_heap(heap)?;
    let lab = match mode {
        None | Some("smooth") => false,
        Some("performance_lab") => true,
        Some(_) => return Err("Unknown JVM tuning mode".into()),
    };
    if environment_override() {
        return Ok(external(heap));
    }
    select(
        candidates(java.major_version, heap, hardware, lab, modules),
        |flags, collector| jvm_probe::verify(&java.path, flags, collector),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hardware() -> Hardware {
        Hardware {
            cores: Some(8),
            total_mb: 32768,
            available_mb: 24576,
        }
    }
    #[test]
    fn automatic_policy_respects_runtime_and_actual_headroom() {
        for (major, collector) in [
            (8, Collector::G1),
            (17, Collector::Shenandoah),
            (21, Collector::Zgc),
            (25, Collector::Zgc),
        ] {
            let plans = candidates(major, 6144, hardware(), false, None);
            assert_eq!(plans[0].collector, collector);
            assert!(
                !plans[0]
                    .flags
                    .iter()
                    .any(|flag| flag.contains("AlwaysPreTouch")
                        || flag.contains("DisableExplicitGC"))
            );
            assert!(plans[0].initial_heap_mb < 6144);
        }
        for facts in [
            Hardware {
                cores: None,
                ..hardware()
            },
            Hardware {
                available_mb: 1024,
                ..hardware()
            },
            Hardware {
                total_mb: 0,
                ..hardware()
            },
        ] {
            let plans = candidates(25, 6144, facts, false, None);
            assert_eq!(plans[0].collector, Collector::G1);
            assert!(
                plans[0]
                    .flags
                    .iter()
                    .any(|flag| flag == "-XX:+UseCompactObjectHeaders")
            );
        }
        assert_eq!(
            candidates(21, 3072, hardware(), false, None)[0].collector,
            Collector::Shenandoah
        );
    }
    #[test]
    fn removed_flags_and_compact_headers_never_mix_with_zgc() {
        for major in [21, 23, 24, 25] {
            let plans = candidates(major, 6144, hardware(), false, None);
            let first = &plans[0];
            assert_eq!(
                first.flags.iter().any(|flag| flag == "-XX:+ZGenerational"),
                major == 21
            );
            assert!(
                !first
                    .flags
                    .iter()
                    .any(|flag| flag.contains("CompactObjectHeaders"))
            );
        }
        assert_eq!(initial_heap(256), 128);
        assert_eq!(initial_heap(8192), 1024);
    }
    #[tokio::test]
    async fn unsupported_optional_policy_falls_back_but_io_failure_does_not_lie()
    -> Result<(), String> {
        let plans = candidates(25, 6144, hardware(), false, None);
        let result = select(plans.clone(), |_, collector| async move {
            Ok((collector == Collector::G1).then_some(false))
        })
        .await?;
        assert_eq!(result.collector, Collector::G1);
        assert!(result.fallback);
        assert!(
            select(plans, |_, _| async { Err("probe I/O failed".into()) })
                .await
                .is_err()
        );
        assert!(
            select(candidates(8, 2048, hardware(), false, None), |_, _| async {
                Ok(None)
            })
            .await
            .is_err()
        );
        Ok(())
    }
}
