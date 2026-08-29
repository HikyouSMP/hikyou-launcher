use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityMeasurement {
    pub status: &'static str,
    pub detail: String,
    pub evidence: String,
}

impl SecurityMeasurement {
    pub fn new(
        status: &'static str,
        detail: impl Into<String>,
        evidence: impl Into<String>,
    ) -> Self {
        Self {
            status,
            detail: detail.into(),
            evidence: evidence.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureStorageDiagnostics {
    pub backend: String,
    pub measured_at_unix_ms: u128,
    pub provider: Option<String>,
    pub key_scope: SecurityMeasurement,
    pub hardware_backing: SecurityMeasurement,
    pub export_policy: SecurityMeasurement,
    pub user_presence: SecurityMeasurement,
    pub pcr_binding: SecurityMeasurement,
    pub access_control: SecurityMeasurement,
}

impl SecureStorageDiagnostics {
    pub fn generic(backend: String) -> Self {
        Self {
            backend,
            measured_at_unix_ms: now_unix_ms(),
            provider: None,
            key_scope: SecurityMeasurement::new(
                "unavailable",
                "The backend does not expose a structured key-scope measurement.",
                "No platform-specific measurement was provided.",
            ),
            hardware_backing: SecurityMeasurement::new(
                "unavailable",
                "Hardware backing was not measured for this backend.",
                "No platform-specific measurement was provided.",
            ),
            export_policy: SecurityMeasurement::new(
                "unavailable",
                "Private-key export policy is not exposed by this backend.",
                "No platform-specific measurement was provided.",
            ),
            user_presence: SecurityMeasurement::new(
                "unavailable",
                "User-presence policy is not exposed by this backend.",
                "No platform-specific measurement was provided.",
            ),
            pcr_binding: SecurityMeasurement::new(
                "not_applicable",
                "PCR binding does not apply to this backend.",
                "This is not a Windows Platform Crypto Provider measurement.",
            ),
            access_control: SecurityMeasurement::new(
                "unavailable",
                "Key access control was not measured for this backend.",
                "No platform-specific measurement was provided.",
            ),
        }
    }
}

pub fn now_unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
