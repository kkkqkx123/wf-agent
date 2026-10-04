//! Built-in JSON assets embedded at compile time.
//!
//! Every built-in JSON asset lives under `configs/`, is embedded here with
//! `include_str!` and is parsed at most once per process; callers borrow
//! the cached value instead of re-parsing the embedded text. Keeping the
//! embedding, the schema and the cache in one module means an asset is
//! written, embedded and validated in exactly one place, and a new
//! consumer of an existing asset costs no extra parse.
//!
//! Prompt text assets share one shape — a flat `{"prompts": {"<id>":
//! "<text>"}}` object — so they are served through [`agent_prompt`],
//! [`spec_workflow_prompt`] and [`approval_reviewer_prompt`]. List assets
//! (prompt templates, fragments, tool visibility templates) are served as
//! borrowed slices.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use wf_types::Template;

/// Prompt-text asset: id-keyed prompt bodies.
#[derive(Debug, Deserialize)]
struct PromptTextFile {
    prompts: BTreeMap<String, String>,
}

/// Prompt-template asset: ordered template list.
#[derive(Debug, Deserialize)]
struct PromptTemplateFile {
    prompts: Vec<Template>,
}

/// Tool visibility asset: ordered template list.
#[derive(Debug, Deserialize)]
struct ToolVisibilityFile {
    templates: Vec<Template>,
}

/// One built-in system prompt fragment as declared in the fragments asset.
#[derive(Debug, Clone, Deserialize)]
pub struct FragmentEntry {
    pub id: String,
    pub category: String,
    pub content: String,
    pub description: Option<String>,
}

/// Fragment asset: ordered fragment list.
#[derive(Debug, Deserialize)]
struct FragmentFile {
    fragments: Vec<FragmentEntry>,
}

const AGENT_PROMPTS: &str = include_str!("../configs/agent_prompts.json");
const APPROVAL_PROMPTS: &str = include_str!("../configs/approval_prompts.json");
const FRAGMENTS: &str = include_str!("../configs/fragments.json");
const PROMPTS: &str = include_str!("../configs/prompts.json");
const SPEC_WORKFLOW_PROMPTS: &str = include_str!("../configs/spec_workflow_prompts.json");
const TOOL_VISIBILITY: &str = include_str!("../configs/tool_visibility.json");

/// Prompt id of the built-in tool-call safety reviewer.
const APPROVAL_REVIEWER_PROMPT_ID: &str = "@standard/approval-reviewer";

fn parse<T: DeserializeOwned>(asset: &str, raw: &str) -> T {
    serde_json::from_str(raw).unwrap_or_else(|e| panic!("embedded {asset} is not valid: {e}"))
}

fn agent_prompt_file() -> &'static PromptTextFile {
    static ASSET: OnceLock<PromptTextFile> = OnceLock::new();
    ASSET.get_or_init(|| parse("agent_prompts.json", AGENT_PROMPTS))
}

fn spec_workflow_prompt_file() -> &'static PromptTextFile {
    static ASSET: OnceLock<PromptTextFile> = OnceLock::new();
    ASSET.get_or_init(|| parse("spec_workflow_prompts.json", SPEC_WORKFLOW_PROMPTS))
}

fn approval_prompt_file() -> &'static PromptTextFile {
    static ASSET: OnceLock<PromptTextFile> = OnceLock::new();
    ASSET.get_or_init(|| parse("approval_prompts.json", APPROVAL_PROMPTS))
}

fn prompt_text(file: &'static PromptTextFile, id: &str) -> &'static str {
    match file.prompts.get(id) {
        Some(text) => text.as_str(),
        None => panic!("embedded prompts have no entry for '{id}'"),
    }
}

/// Built-in prompt for the agent template `template_id`.
///
/// `template_id` is the agent template id (e.g. `@standard/main`); a
/// template id without a matching prompt is a build-time authoring
/// mistake, so it panics instead of silently shipping an agent without a
/// system prompt.
pub fn agent_prompt(template_id: &str) -> &'static str {
    prompt_text(agent_prompt_file(), template_id)
}

/// Built-in prompt for the spec-workflow stage template `template_id`,
/// with its `{{variable}}` placeholders still unrendered.
pub fn spec_workflow_prompt(template_id: &str) -> &'static str {
    prompt_text(spec_workflow_prompt_file(), template_id)
}

/// System prompt of the built-in LLM tool-call safety reviewer. Shared by
/// every host that reviews pending tool calls with a model instead of a
/// human.
pub fn approval_reviewer_prompt() -> &'static str {
    prompt_text(approval_prompt_file(), APPROVAL_REVIEWER_PROMPT_ID)
}

/// Built-in prompt templates.
pub fn prompt_templates() -> &'static [Template] {
    static ASSET: OnceLock<PromptTemplateFile> = OnceLock::new();
    &ASSET.get_or_init(|| parse("prompts.json", PROMPTS)).prompts
}

/// Built-in system prompt fragments.
pub fn fragments() -> &'static [FragmentEntry] {
    static ASSET: OnceLock<FragmentFile> = OnceLock::new();
    &ASSET
        .get_or_init(|| parse("fragments.json", FRAGMENTS))
        .fragments
}

/// Built-in tool visibility templates.
pub fn tool_visibility_templates() -> &'static [Template] {
    static ASSET: OnceLock<ToolVisibilityFile> = OnceLock::new();
    &ASSET
        .get_or_init(|| parse("tool_visibility.json", TOOL_VISIBILITY))
        .templates
}

/// Parse a fragment document supplied by a host at runtime. The document
/// uses the built-in asset shape, so host overrides are validated against
/// exactly the schema the embedded fragments satisfy.
pub fn parse_fragments(raw: &str) -> Result<Vec<FragmentEntry>, serde_json::Error> {
    Ok(serde_json::from_str::<FragmentFile>(raw)?.fragments)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AGENT_TEMPLATE_IDS: &[&str] = &[
        "@standard/main",
        "@standard/explorer",
        "@standard/worker",
        "@standard/goal-review-executor",
        "@standard/goal-review-reviewer",
        "@standard/goal-review-planner",
        "@standard/llm-summary",
        "@standard/code-context-prefetch",
    ];

    #[test]
    fn every_agent_template_id_resolves_to_a_prompt() {
        for id in AGENT_TEMPLATE_IDS {
            assert!(
                !agent_prompt(id).trim().is_empty(),
                "agent prompt '{id}' is empty"
            );
        }
    }

    #[test]
    fn unknown_agent_prompt_id_panics() {
        let result = std::panic::catch_unwind(|| agent_prompt("@standard/absent"));
        assert!(result.is_err(), "missing prompt id must not resolve");
    }

    #[test]
    fn spec_workflow_prompts_keep_their_placeholders() {
        for id in [
            "prompt.spec-workflow.specify",
            "prompt.spec-workflow.plan",
            "prompt.spec-workflow.tasks",
        ] {
            let text = spec_workflow_prompt(id);
            assert!(!text.trim().is_empty(), "spec prompt '{id}' is empty");
            assert!(
                text.contains("{{change_path}}"),
                "spec prompt '{id}' lost its change_path placeholder"
            );
        }
    }

    #[test]
    fn approval_reviewer_prompt_states_both_verdicts() {
        let text = approval_reviewer_prompt();
        assert!(text.contains("ALLOW"), "{text}");
        assert!(text.contains("DENY"), "{text}");
    }

    #[test]
    fn list_assets_expose_entries() {
        assert!(!prompt_templates().is_empty());
        assert!(!tool_visibility_templates().is_empty());
        assert!(!fragments().is_empty());
        for fragment in fragments() {
            assert!(!fragment.id.trim().is_empty());
            assert!(!fragment.category.trim().is_empty());
            assert!(!fragment.content.trim().is_empty());
        }
    }

    #[test]
    fn repeated_access_returns_the_same_allocation() {
        let first = agent_prompt("@standard/main") as *const str;
        let second = agent_prompt("@standard/main") as *const str;
        assert_eq!(first, second, "prompt text must be parsed once");
    }
}
