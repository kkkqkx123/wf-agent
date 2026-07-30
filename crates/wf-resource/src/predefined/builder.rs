use std::collections::HashMap;

use wf_core::registry::{ConcurrentRegistry, Registry};
use wf_types::tool_description::ToolDescriptionData;
use wf_types::SystemPromptFragment;

use crate::compose;
use crate::predefined::render::{render_tool_descriptions, ToolFormat};

#[derive(Debug, Clone)]
pub enum PromptType {
    Assistant,
    Coder,
}

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub global_context: Option<String>,
    pub tool_descriptions: Vec<ToolDescriptionData>,
    pub tool_format: ToolFormat,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub fragment_variables: Option<HashMap<String, String>>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            global_context: None,
            tool_descriptions: Vec::new(),
            tool_format: ToolFormat::Xml,
            prefix: None,
            suffix: None,
            fragment_variables: None,
        }
    }
}

pub fn build_system_prompt(
    prompt_type: PromptType,
    opts: &BuildOptions,
    fragments: &ConcurrentRegistry<SystemPromptFragment>,
) -> String {
    let fragment_ids: Vec<String> = match prompt_type {
        PromptType::Assistant => vec![
            "fragments.role.assistant".into(),
            "fragments.capability.general".into(),
            "fragments.capability.general-principles".into(),
            "fragments.constraint.general".into(),
            "fragments.tool-usage.xml-summary".into(),
            "fragments.task-instruction.code-review".into(),
        ],
        PromptType::Coder => vec![
            "fragments.role.coder".into(),
            "fragments.capability.general".into(),
            "fragments.capability.coding".into(),
            "fragments.constraint.coding".into(),
            "fragments.constraint.code-safety".into(),
            "fragments.tool-usage.json-summary".into(),
            "fragments.task-instruction.code-review".into(),
        ],
    };

    let cfg = compose::Config {
        fragment_ids,
        separator: Some("\n\n".into()),
        prefix: opts.prefix.clone(),
        suffix: opts.suffix.clone(),
        variables: opts.fragment_variables.clone(),
    };

    let mut parts: Vec<String> = Vec::new();

    if let Some(ref ctx) = opts.global_context {
        parts.push(ctx.clone());
    }

    if let Ok(fragments_content) = compose::compose(&cfg, fragments) {
        parts.push(fragments_content);
    }

    if !opts.tool_descriptions.is_empty() {
        let tools_content = render_tool_descriptions(&opts.tool_descriptions, opts.tool_format);
        parts.push(format!("Available tools:\n{}", tools_content));
    }

    parts.join("\n\n")
}

pub fn build_minimal_system_prompt(
    prompt_type: PromptType,
    fragments: &ConcurrentRegistry<SystemPromptFragment>,
) -> String {
    let role_id = match prompt_type {
        PromptType::Assistant => "fragments.role.assistant",
        PromptType::Coder => "fragments.role.coder",
    };

    fragments
        .get(role_id)
        .map(|f| f.content.clone())
        .unwrap_or_default()
}
