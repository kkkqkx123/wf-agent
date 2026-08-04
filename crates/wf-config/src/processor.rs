/// Configuration processors: pure functions for validation, transformation,
/// merging, and parameter substitution. This is the single source of truth
/// for config semantics, used by wf-api (save paths), wf-workflow (graph
/// validation), and wf-resource (custom resource registration).
///
/// ## Wiring status (Rust migration)
///
/// **Fully wired (consumed externally):**
/// - `workflow` — validate_workflow_definition (wf-api save/rollback)
/// - `node_config` — validate_node_config (wf-workflow, wf-api)
/// - `llm_profile` — validate/transform (wf-runtime bootstrap)
/// - `infrastructure` — merge_* functions (wf-runtime SdkOptions)
/// - `trigger` — validate_trigger_template (wf-resource custom registration)
/// - `prompt` — validate_prompt_template (wf-resource custom registration)
///
/// **Pending wiring (kept for future consumers):**
/// - `agent_loop` — validate_agent_definition (future: agent definition API)
/// - `checkpoint` / `file_checkpoint` — merge/validate (future: checkpoint strategy resolution)
/// - `hook` — validate_hook_template (future: hook registration)
/// - `node_template` — validate/transform/export (future: node template API)
/// - `presets` — merge_presets_with_defaults (future: preset loading)
/// - `sandbox_global` — validate_sandbox_global (future: global sandbox config API)
/// - `script*` — validate_script_* (future: script config API)
/// - `substitute` — parameter substitution (future: workflow/agent import paths)
/// - `trigger` — transform/export (future: trigger template API)
/// - `prompt` — merge_prompt_template_config (future: prompt template API)
///
/// Modules not listed above (env, index, loader, parser) provide supporting
/// infrastructure; some functions are used internally, others are pending.
pub mod agent_loop;
pub mod checkpoint;
pub mod file_checkpoint;
pub mod hook;
pub mod infrastructure;
pub mod llm_profile;
pub mod node_config;
pub mod node_template;
pub mod presets;
pub mod prompt;
pub mod sandbox_global;
pub mod script;
pub mod script_flow;
pub mod script_interactive;
pub mod substitute;
pub mod trigger;
pub mod workflow;
