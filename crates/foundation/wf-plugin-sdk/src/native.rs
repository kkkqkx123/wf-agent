//! Native plugin C ABI v1 definitions (author-visible contract).
//!
//! Data structures and constants only — host-side loading logic
//! (`load_manifest`, `load_abi_info`, symbol resolution) stays in the host
//! crate `wf-plugin`. Plugin authors consume these types through the
//! `export_plugin!` macro (SDK-1) rather than implementing the ABI by hand.

use std::os::raw::c_char;

pub const WF_ABI_VERSION_V1: u32 = 1;

/// Host ABI versions supported by this host, highest first. Negotiation
/// selects the highest version present in both the plugin declaration and
/// this list.
pub const WF_SUPPORTED_ABI_VERSIONS: [u32; 1] = [WF_ABI_VERSION_V1];

#[repr(C)]
pub struct PluginContextC {
    pub abi_version: u32,
    pub plugin_id: *const c_char,
    pub config_json: *const c_char,
}

#[repr(C)]
pub struct ContributionRegistrarC {
    pub abi_version: u32,
    pub context: *mut std::ffi::c_void,
    pub register_node_type:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    pub register_tool_type:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    pub register_llm_provider:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    /// Register a provider authentication implementation under `name`;
    /// dispatched on the `llm-auth` handler type.
    pub register_llm_auth:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    pub register_event_handler:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, event_type: *const c_char) -> i32>,
    pub register_middleware: Option<
        extern "C" fn(ctx: *mut std::ffi::c_void, phase: *const c_char, priority: i32) -> i32,
    >,
    /// Declarative tool definition (`wf_types::tool::Tool` JSON, null-terminated).
    pub register_tool:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, tool_json: *const c_char) -> i32>,
    /// Declarative tool description (`ToolDescriptionData` JSON, null-terminated).
    pub register_tool_description:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, description_json: *const c_char) -> i32>,
    /// Declarative CLI command (`PluginCliCommand` JSON, null-terminated).
    pub register_cli_command:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, command_json: *const c_char) -> i32>,
    /// Static completion source (`PluginCompletionSource` JSON, null-terminated).
    pub register_completion_source:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, source_json: *const c_char) -> i32>,
    /// Settings row (`PluginSettingField` JSON, null-terminated).
    pub register_setting_field:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, field_json: *const c_char) -> i32>,
    /// Tool summary templates (`PluginToolSummary` JSON, null-terminated).
    pub register_tool_summary:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, summary_json: *const c_char) -> i32>,
    /// Markdown rewrite rule (`PluginMarkdownRule` JSON, null-terminated).
    pub register_markdown_rule:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, rule_json: *const c_char) -> i32>,
    /// Image generation implementation under `name`; dispatched on
    /// the `llm-images` handler type.
    pub register_llm_images:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    /// Declarative LLM connection template (`LlmProviderDefinition` JSON).
    pub register_llm_provider_definition:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, definition_json: *const c_char) -> i32>,
    /// Declarative workflow (`WorkflowTemplate` JSON, null-terminated).
    pub register_workflow:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, workflow_json: *const c_char) -> i32>,
    /// Declarative prompt (`Template` JSON, null-terminated).
    pub register_prompt:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, prompt_json: *const c_char) -> i32>,
    /// Declarative fragment (`SystemPromptFragment` JSON, null-terminated).
    pub register_fragment:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, fragment_json: *const c_char) -> i32>,
    /// Declarative agent template (`AgentTemplate` JSON, null-terminated).
    pub register_agent_template:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, agent_json: *const c_char) -> i32>,
    /// Declarative node template (`NodeTemplate` JSON, null-terminated).
    pub register_node_template:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, node_json: *const c_char) -> i32>,
    /// Declarative trigger (`TriggerTemplate` JSON, null-terminated).
    pub register_trigger:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, trigger_json: *const c_char) -> i32>,
    /// Compaction planning advice under `name`; dispatched on `compaction`.
    pub register_compaction_advisor:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    /// Full-text analyzer under `name`; dispatched on `text-analyzer`.
    pub register_text_analyzer:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    /// Retrieval shaping advice under `name`; dispatched on `retrieval`.
    pub register_retrieval_advisor:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    /// Vector encoding under `name`; dispatched on `vector-encoding`.
    pub register_vector_encoding:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    /// Snapshot policy (`name` plus arbitrary JSON policy document).
    pub register_snapshot_policy: Option<
        extern "C" fn(
            ctx: *mut std::ffi::c_void,
            name: *const c_char,
            policy_json: *const c_char,
        ) -> i32,
    >,
    /// Ingest metadata enrichment under `name`; dispatched on `metadata-extractor`.
    pub register_metadata_extractor:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    /// Semantic chunk splitting under `name`; dispatched on `chunk-splitter`.
    pub register_chunk_splitter:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    /// Semantic route selection under `name`; dispatched on `route-selector`.
    pub register_route_selector:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    /// Diversity postprocessing under `name`; dispatched on `postprocessor`.
    pub register_postprocessor:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
    /// Answer scoring under `name`; dispatched on `eval-judge`.
    pub register_eval_judge:
        Option<extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> i32>,
}

/// Host dispatch function: calls a registered handler by type and name.
/// Returns 0 on success, non-zero on error.
/// `output_len` is in/out: on input it holds buffer capacity, on output it holds bytes written.
pub type DispatchFn = extern "C" fn(
    handler_type: *const c_char,
    handler_name: *const c_char,
    input_json: *const c_char,
    output_buf: *mut u8,
    output_len: *mut usize,
) -> i32;
