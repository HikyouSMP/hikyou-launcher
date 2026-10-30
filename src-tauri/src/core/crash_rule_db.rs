#[derive(serde::Deserialize)]
pub(super) struct RuleDef {
    pub(super) id: String,
    pub(super) keywords: Vec<String>,
    #[serde(default)]
    pub(super) handler: Option<String>,
    pub(super) message_ja: String,
    #[serde(default)]
    pub(super) message_en: Option<String>,
}

static RULES_JSON: &str = include_str!("../../crash_rules.json");

pub(super) fn load_rules() -> Result<Vec<RuleDef>, String> {
    serde_json::from_str(RULES_JSON)
        .map_err(|e| format!("embedded crash-rule database is invalid: {e}"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_database_is_valid() -> Result<(), String> {
        let rules = super::load_rules()?;
        assert!(!rules.is_empty());
        Ok(())
    }
}
