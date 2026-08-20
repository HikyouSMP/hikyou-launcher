use super::crash_messages::{build_duplicate_message, build_mixin_message, build_mod_dep_message};
use super::crash_parser::RuleMatch;
use super::crash_rule_db::load_rules;

/// Match normalized crash corpus lines against the embedded crash rule database.
pub(super) fn match_rules(corpus: &[String], lang: &str) -> Option<RuleMatch> {
    let rules = load_rules();
    let full_text = corpus.join("\n");

    let use_en = lang == "en";

    for rule in &rules {
        if rule
            .keywords
            .iter()
            .any(|kw| full_text.contains(kw.as_str()))
        {
            let message = match rule.handler.as_deref() {
                Some("mod_dep") => build_mod_dep_message(corpus, lang),
                Some("mixin") => build_mixin_message(corpus, lang),
                Some("duplicate") => build_duplicate_message(corpus, lang),
                _ => {
                    if use_en {
                        rule.message_en
                            .clone()
                            .unwrap_or_else(|| rule.message_ja.clone())
                    } else {
                        rule.message_ja.clone()
                    }
                }
            };
            return Some(RuleMatch {
                id: rule.id.clone(),
                message,
            });
        }
    }
    None
}
