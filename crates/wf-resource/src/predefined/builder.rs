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

impl PromptType {
    /// Composition label resolved against the labeled builtin composition
    /// list (data-driven prompt assembly).
    pub fn composition_kind(&self) -> &'static str {
        match self {
            PromptType::Assistant => "assistant",
            PromptType::Coder => "coder",
        }
    }
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
    // Data-driven composition: the fragment list comes from the labeled
    // builtin composition table instead of a hardcoded match, so prompt
    // variants stay single-sourced with `builtin_compositions()`.
    let composition =
        crate::predefined::fragments::builtin_composition_for(prompt_type.composition_kind())
            .unwrap_or(wf_types::FragmentCompositionConfig {
                fragment_ids: Vec::new(),
                separator: None,
                prefix: None,
                suffix: None,
            });
    let separator = composition.separator.clone().unwrap_or_else(|| "\n\n".into());

    let cfg = compose::Config {
        fragment_ids: composition.fragment_ids,
        separator: Some(separator),
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
