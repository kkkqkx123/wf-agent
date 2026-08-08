/// Configuration processors: pure functions for validation, transformation,
/// merging, and parameter substitution. This is the single source of truth
/// for config semantics, used by wf-api (save paths), wf-workflow (graph
/// validation), and wf-resource (custom resource registration).
///
/// ## Module overview
///
/// - `infrastructure` — `merge_*_with_defaults` for storage/timeout/metrics/output/sandbox
/// - `llm_profile` — validate/transform LLM profiles
/// - `workflow` — validate workflow definitions, transform nodes/edges
/// - `node_config` — validate per-node-type config (LLM, Script, Variable, Route, Fork, Join, etc.)
/// - `agent_loop` — validate/transform agent definitions
/// - `checkpoint` — merge/validate checkpoint policies
/// - `file_checkpoint` — merge/validate file checkpoint configs
/// - `hook` — validate/transform hook templates
/// - `node_template` — validate/transform node templates
/// - `presets` — merge presets with defaults
/// - `sandbox_global` — validate/transform global sandbox configs
/// - `script` — validate script executor configs
/// - `script_flow` — validate/transform script flows
/// - `script_interactive` — validate/transform interactive script configs
/// - `substitute` — `{{parameters.*}}` substitution in any serializable struct
/// - `trigger` — validate/transform trigger templates
/// - `prompt` — validate/merge/transform prompt templates
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
pub mod tools;
pub mod trigger;
pub mod workflow;
