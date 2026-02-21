use crate::skills::PromptSkill;

/// Select which prompt skills to inject for a given user message.
///
/// Rules:
/// - Disabled skills (`enabled: false`) are always excluded.
/// - Skills with no triggers are always included (always-on).
/// - Skills with triggers are included only when at least one trigger
///   phrase appears in the input (case-insensitive substring match via
///   [`PromptSkill::matches_trigger`]).
pub fn resolve_skills<'a>(all_skills: &'a [PromptSkill], user_input: &str) -> Vec<&'a PromptSkill> {
    all_skills
        .iter()
        .filter(|skill| {
            if !skill.enabled {
                return false;
            }
            if skill.triggers.is_empty() {
                return true;
            }
            skill.matches_trigger(user_input)
        })
        .collect()
}

/// Return only always-on skills (enabled, no triggers).
///
/// Used for background LLM calls (goals, self-reflection) where there is
/// no user message to match triggers against.
pub fn always_on_skills(all_skills: &[PromptSkill]) -> Vec<&PromptSkill> {
    all_skills
        .iter()
        .filter(|s| s.enabled && s.triggers.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_skill(name: &str, enabled: bool, triggers: Vec<&str>) -> PromptSkill {
        PromptSkill {
            name: name.to_string(),
            description: String::new(),
            enabled,
            triggers: triggers.into_iter().map(str::to_string).collect(),
            body: String::new(),
            references: HashMap::new(),
        }
    }

    #[test]
    fn always_on_skill_is_always_included() {
        let skills = [make_skill("always", true, vec![])];
        let result = resolve_skills(&skills, "anything at all");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "always");
    }

    #[test]
    fn disabled_skill_is_never_included() {
        let skills = [make_skill("off", false, vec![])];
        let result = resolve_skills(&skills, "anything");
        assert!(result.is_empty());
    }

    #[test]
    fn triggered_skill_activates_on_match() {
        let skills = [make_skill("refactor", true, vec!["simplify code"])];
        assert_eq!(resolve_skills(&skills, "please simplify code now").len(), 1);
        assert!(resolve_skills(&skills, "unrelated message").is_empty());
    }

    #[test]
    fn trigger_match_is_case_insensitive() {
        let skills = [make_skill("s", true, vec!["simplify"])];
        assert_eq!(resolve_skills(&skills, "SIMPLIFY this").len(), 1);
    }

    #[test]
    fn mix_of_always_on_and_triggered() {
        let skills = [
            make_skill("always", true, vec![]),
            make_skill("triggered", true, vec!["refactor"]),
        ];

        let r = resolve_skills(&skills, "refactor this");
        assert_eq!(r.len(), 2);

        let r = resolve_skills(&skills, "hello world");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "always");
    }

    #[test]
    fn disabled_triggered_skill_is_excluded() {
        let skills = [make_skill("disabled-trigger", false, vec!["refactor"])];
        assert!(resolve_skills(&skills, "refactor this").is_empty());
    }

    #[test]
    fn always_on_fn_excludes_triggered_skills() {
        let skills = [
            make_skill("always", true, vec![]),
            make_skill("triggered", true, vec!["refactor"]),
            make_skill("off", false, vec![]),
        ];

        let r = always_on_skills(&skills);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "always");
    }
}
