use wf_execution_shared::agent_prompt::PromptEnvironment;
use wf_tools::callback::AgentLoopConfig;
use wf_types::agent::AgentConfig;
use wf_types::message::Message;

pub fn seed_initial_messages(conversation: &mut Vec<Message>, cfg: &AgentConfig) {
    let Some(initial) = cfg.initial_messages.as_ref() else {
        return;
    };
    if !conversation.is_empty() || initial.is_empty() {
        return;
    }
    conversation.extend(initial.iter().cloned());
}

/// Render tool exposure blocks from the resolved buckets. Runs for both
/// templated and untemplated configs so the direct path never falls back to
/// the builtin generation while the workflow path uses custom resources.
pub fn attach_exposure_artifacts(env: &PromptEnvironment, config: &mut AgentLoopConfig) {
    // `None` = unset: exposure blocks render against the definition-layer
    // fallback applied earlier, so the empty slice is the right view here.
    let available_tools = config.available_tool_names.clone().unwrap_or_default();
    let artifacts = wf_execution_shared::agent_prompt::build_exposure_artifacts(
        env,
        config.tool_call_protocol.as_ref(),
        &available_tools,
        &config.initial_tool_names,
        &config.discoverable_tool_names,
        &config.hidden_tool_names,
        config.enable_general_tool,
    );
    if let Some(description) = artifacts.general_description {
        config.general_description = Some(description);
    }
    if let Some(block) = artifacts.discoverable_metadata_block {
        config.discoverable_metadata_block = Some(block);
    }
}

/// Assemble the stable header and volatile tail for the resolved template and
/// land them in the round conversation. The header seeds the leading system
/// message once and the tail travels as a separate marked user message; stale
/// tails are dropped inside the apply step so no caller-side cleanup is
/// needed. The user task message stays pure input.
pub fn attach_prompt_assembly(
    env: &PromptEnvironment,
    agent_config: Option<&AgentConfig>,
    config: &AgentLoopConfig,
    input: &mut wf_tools::callback::AgentLoopInput,
) {
    use wf_execution_shared::agent_prompt::{
        apply_assembled_prompt, assemble_agent_prompt, normalize_prompt_workdir, DynamicTailBearing,
    };
    let workdir = config
        .file_scope
        .as_ref()
        .and_then(|scope| scope.workdir.as_deref());
    let assembled = assemble_agent_prompt(
        agent_config,
        &input.context,
        env,
        &config.available_tool_names.clone().unwrap_or_default(),
        normalize_prompt_workdir(workdir).as_deref(),
    );
    apply_assembled_prompt(
        &mut input.conversation,
        &assembled,
        DynamicTailBearing::SeparateUserMessage,
    );
}
