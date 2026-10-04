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

use wf_config::processor::prompt::extract_template_placeholders;
use wf_core::registry::Registry;

use crate::embedded_assets;
use crate::predefined::resource_assembler::goal_review::GOAL_REVIEW_PLANNER_PROMPT_ID;
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

/// The system prompt for `key` as it should be used at the registration
/// edge: the registry override's content when a template with the same id
/// was registered, otherwise the embedded default. `None` means "keep the
/// default the caller already carries".
pub fn resolve_system_prompt(regs: &ResourceRegistries, key: &str) -> Option<String> {
    regs.templates
        .get(key)
        .map(|template| template.content.clone())
}

/// Outcome of consulting the registry for a bundle prompt's override at
/// install time.
pub enum BundlePromptOverride {
    /// No override registered, or the bundle prompt already carries it.
    KeepBundle,
    /// Registry override content that should replace the bundle prompt's.
    Patch(String),
    /// The override keeps unrendered placeholders; install-time patching
    /// cannot render them, so it is rejected with an explanatory message.
    Reject(String),
}

/// Bundle prompt ids whose text can come from an explicit assembler config
/// field: the embedded default is recognizable by content, so a registry
/// override only applies while the bundle still carries that default.
fn explicit_config_default(prompt_id: &str) -> Option<&'static str> {
    match prompt_id {
        GOAL_REVIEW_PLANNER_PROMPT_ID => {
            Some(embedded_assets::agent_prompt(GOAL_REVIEW_PLANNER_PROMPT_KEY))
        }
        _ => None,
    }
}

/// Resolve the registry override for a prompt carried in a resource bundle.
///
/// Rules, in order:
/// - no same-id template registered, or its content matches the bundle's:
///   keep the bundle prompt;
/// - the override keeps unrendered placeholders: reject it, because
///   install-time patching cannot render them (delayed rendering is the
///   longer-term fix, until then a placeholder override is a mistake worth
///   surfacing rather than silently leaking into the final prompt);
/// - the bundle id has an explicit assembler-config channel and the bundle
///   no longer carries the embedded default: the explicit config outranks
///   the override;
/// - otherwise the override content replaces the bundle prompt's.
pub fn bundle_prompt_override(regs: &ResourceRegistries, prompt: &Template) -> BundlePromptOverride {
    let Some(override_text) = resolve_system_prompt(regs, &prompt.id) else {
        return BundlePromptOverride::KeepBundle;
    };
    if override_text == prompt.content {
        return BundlePromptOverride::KeepBundle;
    }
    if !extract_template_placeholders(&override_text).is_empty() {
        return BundlePromptOverride::Reject(format!(
            "registry override for '{}' keeps unrendered placeholders, which install-time prompt patching cannot render",
            prompt.id
        ));
    }
    if let Some(default) = explicit_config_default(&prompt.id) {
        if prompt.content != default {
            return BundlePromptOverride::KeepBundle;
        }
    }
    BundlePromptOverride::Patch(override_text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::RegisterOptions;

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
        assert_eq!(declared, embedded, "agent_prompts.json keys and declared constants drifted apart");
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
            resolve_system_prompt(&regs, WORKER_AGENT_PROMPT_KEY).as_deref(),
            Some(override_text.as_str())
        );
        // Keys without an override still resolve to the embedded default.
        assert_eq!(
            resolve_system_prompt(&regs, MAIN_AGENT_PROMPT_KEY).as_deref(),
            Some(embedded_assets::agent_prompt(MAIN_AGENT_PROMPT_KEY))
        );
    }

    #[test]
    fn no_override_yields_none() {
        let regs = ResourceRegistries::new();
        assert_eq!(resolve_system_prompt(&regs, WORKER_AGENT_PROMPT_KEY), None);
    }

    fn bundle_prompt(id: &str, content: &str) -> Template {
        Template {
            id: id.into(),
            name: "Bundle Prompt".into(),
            description: None,
            category: "system".into(),
            content: content.into(),
            variables: None,
            fragments: None,
        }
    }

    fn register_override(regs: &ResourceRegistries, id: &str, content: &str) {
        crate::registry::register_item_skip(
            &regs.templates,
            id.into(),
            bundle_prompt(id, content),
        );
    }

    #[test]
    fn bundle_prompt_override_patches_a_rendered_prompt() {
        let regs = ResourceRegistries::new();
        register_override(&regs, "prompt.spec-workflow.specify", "static override");
        let bundle = bundle_prompt("prompt.spec-workflow.specify", "rendered default");
        assert!(matches!(
            bundle_prompt_override(&regs, &bundle),
            BundlePromptOverride::Patch(text) if text == "static override"
        ));
    }

    #[test]
    fn bundle_prompt_override_patches_planner_default() {
        let regs = ResourceRegistries::new();
        register_override(&regs, GOAL_REVIEW_PLANNER_PROMPT_ID, "planner override");
        let bundle = bundle_prompt(
            GOAL_REVIEW_PLANNER_PROMPT_ID,
            embedded_assets::agent_prompt(GOAL_REVIEW_PLANNER_PROMPT_KEY),
        );
        assert!(matches!(
            bundle_prompt_override(&regs, &bundle),
            BundlePromptOverride::Patch(text) if text == "planner override"
        ));
    }

    #[test]
    fn explicit_planner_config_outranks_the_override() {
        let regs = ResourceRegistries::new();
        register_override(&regs, GOAL_REVIEW_PLANNER_PROMPT_ID, "planner override");
        let bundle = bundle_prompt(GOAL_REVIEW_PLANNER_PROMPT_ID, "explicit config prompt");
        assert!(matches!(
            bundle_prompt_override(&regs, &bundle),
            BundlePromptOverride::KeepBundle
        ));
    }

    #[test]
    fn bundle_prompt_override_rejects_placeholder_overrides() {
        let regs = ResourceRegistries::new();
        register_override(
            &regs,
            "prompt.spec-workflow.specify",
            "override for {{change_path}}",
        );
        let bundle = bundle_prompt("prompt.spec-workflow.specify", "rendered default");
        assert!(matches!(
            bundle_prompt_override(&regs, &bundle),
            BundlePromptOverride::Reject(_)
        ));
    }

    #[test]
    fn bundle_prompt_without_override_keeps_the_bundle() {
        let regs = ResourceRegistries::new();
        let bundle = bundle_prompt("prompt.spec-workflow.specify", "rendered default");
        assert!(matches!(
            bundle_prompt_override(&regs, &bundle),
            BundlePromptOverride::KeepBundle
        ));
    }

    #[test]
    fn install_bundle_applies_prompt_override_to_bundle_prompts() {
        use crate::resource_assembler::{install_bundle, ResourceBundle};
        use wf_tools::registry::ToolRegistry;

        let regs = ResourceRegistries::new();
        register_override(&regs, "prompt.spec-workflow.specify", "static override");
        let mut bundle = ResourceBundle::new();
        bundle.prompts.push(bundle_prompt(
            "prompt.spec-workflow.specify",
            "rendered default",
        ));
        let summary = install_bundle(&regs, &ToolRegistry::new(), &bundle, true);
        assert!(summary.failed.is_empty(), "{:?}", summary.failed);
        assert_eq!(
            regs.templates
                .get("prompt.spec-workflow.specify")
                .map(|t| t.content.clone())
                .as_deref(),
            Some("static override")
        );
    }
}
