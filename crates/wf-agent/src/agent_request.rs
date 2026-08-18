use wf_tools::general::GENERAL_TOOL_NAME;
use wf_tools::registry::ToolRegistry;
use wf_tools::tool_exposure::ExposureInput;
use wf_types::llm::LlmRequest;
use wf_types::message::MessageRole;

use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;
use crate::tool_router::ToolRouter;

/// Build the LLM request for one agent iteration.
///
/// Single request construction point shared by blocking and streaming
/// iteration modes. The visible tool set is recomputed per turn through the
/// tool router (whitelist + exposure layers + formal activation), so every
/// iteration observes the latest tool availability without ever mutating
/// the schema through runtime blocks (KV-cache friendly). Sampling
/// parameters are deliberately not hardcoded here: they come from the
/// referenced profile's `parameters` and are merged by the gateway.
///
/// `general_description` carries the assembly-time rendered description for
/// the `general` tool (from the `tool-visibility.general_description`
/// resource template); it is written into the routed tool copy so the
/// schema follows custom resource overrides while staying byte-stable
/// across turns. `None` keeps the builtin static description.
///
/// `discoverable_metadata_block` carries the assembly-time pre-rendered
/// discoverable-tool metadata (from the `tool-visibility.discoverable_metadata`
/// resource template). It is injected into the first system message at
/// request assembly time (placeholder replacement or tail append), exactly
/// when the `general` tool is exposed; `None` falls back to the built-in
/// metadata generation. The replacement is deterministic per turn, so the
/// system prompt stays byte-stable across turns (KV-cache friendly).
pub async fn build_agent_request(
    entity: &AgentLoopEntity,
    tool_registry: &ToolRegistry,
    stream: bool,
    general_description: Option<&str>,
    discoverable_metadata_block: Option<&str>,
) -> AgentResult<LlmRequest> {
    let mut messages = entity.conversation().read().await.messages().to_vec();
    let activated_tools = {
        let state = entity.state.read().await;
        state.tool_discovery().activated_tools.clone()
    };

    let exposure = wf_tools::resolve_tool_exposure(ExposureInput {
        registry: tool_registry,
        available_names: entity.available_tool_names(),
        initial_names: entity.initial_tool_names(),
        discoverable_names: entity.discoverable_tool_names(),
        hidden_names: entity.hidden_tool_names(),
        enable_general_tool: entity.enable_general_tool(),
        activated_tools: &activated_tools,
        exposure_overrides: &entity.exposure_overrides().iter().cloned().collect(),
    });
    let mut router: ToolRouter = exposure.clone().into();

    // The routed tool list is a clone, so the general description override
    // never touches the shared registry.
    if let Some(description) = general_description {
        if let Some(general) = router
            .visible
            .iter_mut()
            .find(|t| t.name == GENERAL_TOOL_NAME)
        {
            general.description = description.to_string();
        }
    }

    // Discoverable tool metadata injection: applied to the first system
    // message at request assembly time when the `general` tool is actually
    // exposed. The block is deterministic per turn (same exposure inputs,
    // same generation options), so the system prompt prefix stays
    // byte-stable across turns.
    if exposure.general_enabled && !exposure.discoverable.is_empty() {
        let block = match discoverable_metadata_block {
            Some(block) => block.to_string(),
            None => {
                let options = wf_tools::discoverable_metadata_options(entity.tool_call_format());
                wf_tools::generate_discoverable_tools_metadata_with_options(
                    &exposure.discoverable,
                    &options,
                )
            }
        };
        if let Some(system) = messages.iter_mut().find(|m| m.role == MessageRole::System) {
            if let wf_types::message::MessageContentValue::Text(text) = &mut system.content {
                *text = wf_tools::inject_tool_metadata_block(text, &block);
            }
        }
    }

    let tools = if router.visible.is_empty() {
        None
    } else {
        Some(router.visible)
    };

    Ok(LlmRequest {
        profile_id: entity.model().to_string(),
        messages,
        parameters: None,
        tools,
        tool_call_format: entity
            .tool_call_format()
            .map(|config| config.format.clone()),
        locked_tool_call_format: entity.tool_call_format().cloned(),
        violation_policy: None,
        execution_id: Some(entity.id().to_string()),
        stream: Some(stream),
        dead_loop_detection: None,
        protocol_auto_converted: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::message::{Message, MessageContentValue, MessageRole};

    fn make_tool(id: &str, name: &str) -> wf_types::tool::Tool {
        wf_types::tool::Tool {
            id: id.into(),
            name: name.into(),
            description: format!("tool {name}"),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        }
    }

    fn make_registry() -> ToolRegistry {
        let registry = ToolRegistry::new();
        registry.register_tool(wf_tools::predefined::general::GENERAL.tool_def());
        registry.register_tool(make_tool("web_search", "web_search"));
        registry
    }

    async fn make_entity(system_prompt: &str) -> AgentLoopEntity {
        let entity = AgentLoopEntity::new(wf_types::Id::from("request-test".to_string()))
            .with_model("mock".to_string())
            .with_available_tool_names(vec!["web_search".to_string()])
            .with_initial_tool_names(vec!["web_search".to_string()])
            .with_discoverable_tool_names(vec!["web_search".to_string()]);
        entity.conversation().write().await.add_message(Message {
            id: wf_types::Id::new(),
            role: MessageRole::System,
            content: MessageContentValue::Text(system_prompt.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        });
        entity
    }

    fn system_text(request: &LlmRequest) -> String {
        request
            .messages
            .iter()
            .find(|m| m.role == MessageRole::System)
            .and_then(|m| match &m.content {
                MessageContentValue::Text(t) => Some(t.clone()),
                MessageContentValue::Rich(_) => None,
            })
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn discoverable_metadata_replaces_placeholder_in_system_message() {
        let entity = make_entity("You are a coder.\n{DISCOVERABLE_TOOLS_METADATA}").await;
        let request = build_agent_request(&entity, &make_registry(), false, None, None)
            .await
            .expect("request must build");
        let text = system_text(&request);
        assert!(text.contains("web_search"), "metadata injected: {text}");
        assert!(
            !text.contains(wf_tools::DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER),
            "placeholder replaced: {text}"
        );
    }

    #[tokio::test]
    async fn discoverable_metadata_appends_when_placeholder_absent() {
        let entity = make_entity("You are a coder.").await;
        let request = build_agent_request(&entity, &make_registry(), false, None, None)
            .await
            .expect("request must build");
        let text = system_text(&request);
        assert!(
            text.starts_with("You are a coder."),
            "original prompt preserved: {text}"
        );
        assert!(
            text.contains("web_search"),
            "metadata appended at the tail: {text}"
        );
    }

    #[tokio::test]
    async fn discoverable_metadata_injection_is_byte_stable_across_turns() {
        let entity = make_entity("You are a coder.\n{DISCOVERABLE_TOOLS_METADATA}").await;
        let first = build_agent_request(&entity, &make_registry(), false, None, None)
            .await
            .expect("first request");
        let second = build_agent_request(&entity, &make_registry(), false, None, None)
            .await
            .expect("second request");
        assert_eq!(
            system_text(&first),
            system_text(&second),
            "system prompt must be byte-stable across turns"
        );
    }

    #[tokio::test]
    async fn config_rendered_block_wins_over_builtin_generation() {
        let entity = make_entity("You are a coder.").await;
        let custom = "Discoverable tools:\n- web_search (custom block)".to_string();
        let request = build_agent_request(&entity, &make_registry(), false, None, Some(&custom))
            .await
            .expect("request must build");
        let text = system_text(&request);
        assert!(text.contains("custom block"), "custom block used: {text}");
    }
}
