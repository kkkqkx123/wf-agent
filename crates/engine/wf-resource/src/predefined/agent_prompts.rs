//! Built-in `@standard/*` agent prompts: the single key source plus their
//! registration into the template registry.
//!
//! Every `@standard/*` prompt key used across the workspace is declared here
//! once; consumers (agent templates, resource assemblers, workflow children)
//! re-export or reference these constants instead of redefining the string
//! literals, so a renamed JSON key fails the parity tests below instead of
//! panicking at runtime.
//!
//! The prompt texts are also registered into the template registry with
//! `skip_if_exists` semantics, so a user-defined prompt template with the
//! same id overrides the built-in text at the registration edge.

use crate::embedded_assets;
use crate::registry::{register_template, RegisterOptions, ResourceRegistries};
use crate::result::Summary;
use wf_types::Template;

pub const MAIN_AGENT_PROMPT_KEY: &str = "@standard/main";
pub const EXPLORER_AGENT_PROMPT_KEY: &str = "@standard/explorer";
pub const WORKER_AGENT_PROMPT_KEY: &str = "@standard/worker";
pub const GOAL_REVIEW_EXECUTOR_PROMPT_KEY: &str = "@standard/goal-review-executor";
pub const GOAL_REVIEW_REVIEWER_PROMPT_KEY: &str = "@standard/goal-review-reviewer";
pub const GOAL_REVIEW_PLANNER_PROMPT_KEY: &str = "@standard/goal-review-planner";
pub const LLM_SUMMARY_PROMPT_KEY: &str = "@standard/llm-summary";
pub const CODE_CONTEXT_PREFETCH_PROMPT_KEY: &str = "@standard/code-context-prefetch";

/// Prompt id of the built-in tool-call safety reviewer (separate asset file,
/// not part of `agent_prompts.json`).
pub const APPROVAL_REVIEWER_PROMPT_KEY: &str = "@standard/approval-reviewer";

/// All `@standard/*` keys served from `agent_prompts.json`.
pub const AGENT_PROMPT_KEYS: &[&str] = &[
    MAIN_AGENT_PROMPT_KEY,
    EXPLORER_AGENT_PROMPT_KEY,
    WORKER_AGENT_PROMPT_KEY,
    GOAL_REVIEW_EXECUTOR_PROMPT_KEY,
    GOAL_REVIEW_REVIEWER_PROMPT_KEY,
    GOAL_REVIEW_PLANNER_PROMPT_KEY,
    LLM_SUMMARY_PROMPT_KEY,
    CODE_CONTEXT_PREFETCH_PROMPT_KEY,
];

/// Human-readable names for the registered prompt templates, keyed by
/// prompt key.
fn display_name(key: &str) -> &'static str {
    match key {
        MAIN_AGENT_PROMPT_KEY => "Main Agent Prompt",
        EXPLORER_AGENT_PROMPT_KEY => "Explorer Agent Prompt",
        WORKER_AGENT_PROMPT_KEY => "Worker Agent Prompt",
        GOAL_REVIEW_EXECUTOR_PROMPT_KEY => "Goal Review Executor Prompt",
        GOAL_REVIEW_REVIEWER_PROMPT_KEY => "Goal Review Reviewer Prompt",
        GOAL_REVIEW_PLANNER_PROMPT_KEY => "Goal Review Planner Prompt",
        LLM_SUMMARY_PROMPT_KEY => "LLM Summary Prompt",
        CODE_CONTEXT_PREFETCH_PROMPT_KEY => "Code Context Prefetch Prompt",
        _ => "Built-in Agent Prompt",
    }
}

/// The built-in `@standard/*` prompt texts as plain system-prompt templates.
pub fn builtin_agent_prompt_templates() -> Vec<Template> {
    AGENT_PROMPT_KEYS
        .iter()
        .map(|key| Template {
            id: (*key).into(),
            name: display_name(key).into(),
            description: Some(format!("Built-in system prompt for {key}")),
            category: "system".into(),
            content: embedded_assets::agent_prompt(key).to_string(),
            variables: None,
            fragments: None,
        })
        .collect()
}

/// Register the built-in `@standard/*` prompts into the template registry.
/// `skip_if_exists` lets a user-defined prompt with the same id win.
pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();
    for template in builtin_agent_prompt_templates() {
        total.merge(register_template(regs, template, opts.skip_if_exists));
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::RegisterOptions;
    use wf_core::registry::Registry;

    #[test]
    fn every_declared_key_resolves_to_a_nonempty_prompt() {
        for key in AGENT_PROMPT_KEYS {
            let text = embedded_assets::agent_prompt(key);
            assert!(!text.trim().is_empty(), "prompt '{key}' is empty");
        }
    }

    #[test]
    fn declared_keys_match_the_asset_exactly() {
        let mut declared: Vec<&str> = AGENT_PROMPT_KEYS.to_vec();
        let mut embedded: Vec<String> = embedded_assets::agent_prompt_keys();
        declared.sort_unstable();
        embedded.sort();
        let embedded: Vec<&str> = embedded.iter().map(String::as_str).collect();
        assert_eq!(
            declared, embedded,
            "agent_prompts.json keys and declared constants drifted apart"
        );
    }

    #[test]
    fn register_exposes_all_prompts_as_templates() {
        let regs = ResourceRegistries::new();
        let summary = register(&regs, &RegisterOptions::default());
        assert!(summary.failed.is_empty(), "{:?}", summary.failed);
        for key in AGENT_PROMPT_KEYS {
            let template = regs
                .templates
                .get(key)
                .unwrap_or_else(|| panic!("prompt '{key}' not registered"));
            assert_eq!(template.content, embedded_assets::agent_prompt(key));
        }
    }

    #[test]
    fn registry_override_wins_over_embedded_default() {
        let regs = ResourceRegistries::new();
        let override_text = "custom system prompt".to_string();
        crate::registry::register_item_skip(
            &regs.templates,
            WORKER_AGENT_PROMPT_KEY.into(),
            Template {
                id: WORKER_AGENT_PROMPT_KEY.into(),
                name: "Override".into(),
                description: None,
                category: "system".into(),
                content: override_text.clone(),
                variables: None,
                fragments: None,
            },
        );
        register(&regs, &RegisterOptions::default());
        assert_eq!(
            regs.templates
                .get(WORKER_AGENT_PROMPT_KEY)
                .map(|template| template.content.clone())
                .as_deref(),
            Some(override_text.as_str())
        );
        assert_eq!(
            regs.templates
                .get(MAIN_AGENT_PROMPT_KEY)
                .map(|template| template.content.clone())
                .as_deref(),
            Some(embedded_assets::agent_prompt(MAIN_AGENT_PROMPT_KEY))
        );
    }
}
