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

pub(super) fn load_rules() -> Vec<RuleDef> {
    serde_json::from_str(RULES_JSON).expect("failed to parse crash_rules.json")
}
