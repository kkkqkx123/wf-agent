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
    /// Server transport and middleware config (`configs/server`).
    pub const SERVER: &str = "server";
    /// LLM provider definitions (`configs/llm-providers`).
    pub const LLM_PROVIDERS: &str = "llm-providers";
    /// LLM profile definitions (`configs/llm-profiles`).
    pub const LLM_PROFILES: &str = "llm-profiles";
    /// LLM authentication declarations (`configs/llm-auth` / user-level `llm-auth`).
    pub const LLM_AUTH: &str = "llm-auth";
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

/// Volatile machine-local metadata under `{project_root}/.wf/meta`:
/// session caches, diagnostics and other files that must never be
/// version-controlled or enter checkpoint scans.
pub const META_DIR: &str = "meta";

/// Volatile spill output under `{project_root}/.wf/tmp`: oversized
/// command output and other regenerable artifacts.
pub const TMP_DIR: &str = "tmp";

/// Content of the `.gitignore` maintained inside each volatile directory:
/// ignore everything but keep the ignore file itself tracked-ignored.
pub const VOLATILE_GITIGNORE_CONTENT: &str = "*\n!.gitignore\n";

/// Name of the ignore file maintained inside volatile directories.
pub const GITIGNORE_FILE: &str = ".gitignore";

/// Ensure the volatile `.wf` subdirectories exist with their internal
/// ignore files. Only `{project_root}/.wf/{meta,tmp}/.gitignore` are
/// created or written; the outer workspace `.gitignore` is never touched
/// so tracked `.wf` configuration keeps its sync behavior. A present
/// ignore file with custom content is left alone (never force-overwritten).
pub fn ensure_project_wf_volatile_dirs(project_root: &std::path::Path) -> std::io::Result<()> {
    for subdir in [META_DIR, TMP_DIR] {
        let dir = project_root.join(PROJECT_WF_DIR).join(subdir);
        std::fs::create_dir_all(&dir)?;
        let ignore = dir.join(GITIGNORE_FILE);
        if std::fs::read(&ignore).is_err() {
            std::fs::write(&ignore, VOLATILE_GITIGNORE_CONTENT)?;
        }
    }
    Ok(())
}

/// Join a project root with a config family directory:
/// `{project_root}/configs/{family}`.
pub fn family_dir(project_root: &std::path::Path, family: &str) -> std::path::PathBuf {
    project_root.join(CONFIGS_DIR).join(family)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volatile_dirs_are_created_with_internal_ignores() {
        let dir = tempfile::tempdir().unwrap();
        ensure_project_wf_volatile_dirs(dir.path()).unwrap();
        for subdir in [META_DIR, TMP_DIR] {
            let ignore = dir
                .path()
                .join(PROJECT_WF_DIR)
                .join(subdir)
                .join(GITIGNORE_FILE);
            assert_eq!(
                std::fs::read_to_string(&ignore).unwrap(),
                VOLATILE_GITIGNORE_CONTENT
            );
        }
        assert!(!dir.path().join(".gitignore").exists());
        // Second run is idempotent.
        ensure_project_wf_volatile_dirs(dir.path()).unwrap();
    }

    #[test]
    fn existing_custom_ignore_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let meta = dir.path().join(PROJECT_WF_DIR).join(META_DIR);
        std::fs::create_dir_all(&meta).unwrap();
        std::fs::write(meta.join(GITIGNORE_FILE), "custom\n").unwrap();
        ensure_project_wf_volatile_dirs(dir.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(meta.join(GITIGNORE_FILE)).unwrap(),
            "custom\n"
        );
        assert_eq!(
            std::fs::read_to_string(
                dir.path()
                    .join(PROJECT_WF_DIR)
                    .join(TMP_DIR)
                    .join(GITIGNORE_FILE)
            )
            .unwrap(),
            VOLATILE_GITIGNORE_CONTENT
        );
    }
}
