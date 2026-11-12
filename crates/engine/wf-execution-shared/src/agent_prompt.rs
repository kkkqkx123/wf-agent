//! Single owner for agent prompt assembly: stable header plus volatile tail.
//!
//! Both the workflow `AGENT_LOOP` handler and the direct composition boundary
//! call this module so one agent definition renders identically on both
//! entries. The module holds no executor and knows nothing about session
//! origins; it only turns a [`wf_types::agent::AgentConfig`] plus a narrow variable view
//! into texts and into the messages that carry them.
//!
//! Stable header: inline `system_prompt` wins, `system_prompt_template_id`
//! renders next, stable dynamic sections (environment, skills, workflows,
//! custom sections) prepend, then skill metadata and MCP summary enrich.
//! Time, todo, pinned and workspace content never enter here.
//!
//! The static skills section is skipped when the `skill` tool is available:
//! progressive disclosure then owns skill presentation through the injected
//! skill metadata block, so skills are never announced twice.
//!
//! Volatile tail: current time plus todo, pinned, workspace tree and custom
//! data read through [`VariableSource`]. Missing or misshapen values warn
//! and skip; the engine never fabricates data.

pub mod exposure;
pub mod stable_header;
pub mod tail_ops;
pub mod variables;
pub mod volatile_tail;

#[cfg(test)]
mod tests;

pub use exposure::{build_exposure_artifacts, ExposureArtifacts};
pub use stable_header::{build_stable_header, resolve_system_prompt_text};
pub use tail_ops::{
    apply_assembled_prompt, dynamic_context_message, has_stable_system_message,
    is_dynamic_context_message, is_tool_change_message, is_tool_visibility_message,
    split_trailing_dynamic_tail, stable_system_message, strip_dynamic_context_messages,
    strip_tool_announcements, tool_change_message, DynamicTailBearing,
    DYNAMIC_CONTEXT_MESSAGE_TYPE, TOOL_CHANGE_MESSAGE_TYPE, TOOL_VISIBILITY_MESSAGE_TYPE,
};
pub use variables::{PromptEnvironment, VariableSource};
pub use volatile_tail::build_volatile_tail;

/// Stable header plus volatile tail texts for one run.
pub struct AssembledPrompt {
    pub stable_header: Option<String>,
    pub volatile_tail: Option<String>,
}

/// Assemble both outputs for one run. `working_directory` carries the
/// execution workdir into the stable environment section so relative
/// tool paths resolve against a model-visible base; `None` keeps the
/// historical process-cwd fallback.
pub fn assemble_agent_prompt(
    agent_config: Option<&wf_types::agent::AgentConfig>,
    vars: &impl VariableSource,
    env: &PromptEnvironment,
    available: wf_tools::tool_exposure::AvailablePool<'_>,
    working_directory: Option<&str>,
) -> AssembledPrompt {
    AssembledPrompt {
        stable_header: build_stable_header(agent_config, env, available, working_directory),
        volatile_tail: build_volatile_tail(agent_config, vars),
    }
}

/// Normalize a scope workdir for prompt display: absolute paths pass
/// through, relative paths anchor against the process cwd, and missing or
/// unresolvable values fall back to `None` (the renderer then reports the
/// process cwd itself).
pub fn normalize_prompt_workdir(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let candidate = std::path::Path::new(raw);
    if candidate.is_absolute() {
        return Some(raw.to_string());
    }
    std::env::current_dir()
        .ok()
        .map(|base| base.join(candidate).to_string_lossy().to_string())
}
