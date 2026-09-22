//! Centralized filesystem layout for the configuration subsystem.
//!
//! Every well-known directory / file name used when locating configuration
//! sources lives here so that path conventions are defined in exactly one
//! place. Domain modules (mcp, skill, preset, orchestrator, ...) compose
//! these constants instead of hardcoding their own path fragments.

/// Per-project hidden state directory: `{project_root}/.wf`.
pub const PROJECT_WF_DIR: &str = ".wf";

/// Project-root-relative root of all bundled config families:
/// `{project_root}/configs`.
pub const CONFIGS_DIR: &str = "configs";

/// Directory name of each config family under `configs/`.
pub mod family {
    /// MCP preset definitions (`configs/mcp`).
    pub const MCP: &str = "mcp";
    /// Skill collection definitions (`configs/skills`).
    pub const SKILLS: &str = "skills";
    /// Infrastructure config files and presets (`configs/infrastructure`).
    pub const INFRASTRUCTURE: &str = "infrastructure";
    /// LLM provider definitions (`configs/llm-providers`).
    pub const LLM_PROVIDERS: &str = "llm-providers";
    /// LLM profile definitions (`configs/llm-profiles`).
    pub const LLM_PROFILES: &str = "llm-profiles";
    /// Workflow definitions (`configs/workflows`).
    pub const WORKFLOWS: &str = "workflows";
    /// Agent loop definitions (`configs/agent-loops`).
    pub const AGENT_LOOPS: &str = "agent-loops";
    /// Node template definitions (`configs/node-templates`).
    pub const NODE_TEMPLATES: &str = "node-templates";
    /// Prompt template definitions (`configs/prompt-templates`).
    pub const PROMPT_TEMPLATES: &str = "prompt-templates";
    /// Script definitions (`configs/scripts`).
    pub const SCRIPTS: &str = "scripts";
}

/// Default global settings file names inside `{settings_dir}`.
pub mod settings_file {
    /// Global MCP settings (`mcp-settings.json`).
    pub const MCP: &str = "mcp-settings.json";
    /// Global skill settings (`skill-settings.json`).
    pub const SKILL: &str = "skill-settings.json";
}

/// Join a project root with a config family directory:
/// `{project_root}/configs/{family}`.
pub fn family_dir(project_root: &std::path::Path, family: &str) -> std::path::PathBuf {
    project_root.join(CONFIGS_DIR).join(family)
}
