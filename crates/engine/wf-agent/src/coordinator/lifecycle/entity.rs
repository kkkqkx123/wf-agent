use wf_execution_shared::hooks::types::HookDefinition;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::Id;

use super::AgentLoopCoordinator;
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;
use wf_execution_shared::types::execution_entity::{child_ancestors, child_depth, child_root};

impl AgentLoopCoordinator {
    /// Normalize an inbound conversation to the target loop's exposure.
    ///
    /// Inbound histories carry the sender's bucket shapes (a direct call where
    /// this loop only discovers the tool, or a `general` wrap where this loop
    /// exposes it directly). Rewriting once at the boundary keeps the new schema
    /// and the replayed history consistent. Stored archives stay verbatim and the
    /// runtime gates remain authoritative over what may execute. Stale volatile
    /// tail messages are dropped while a freshly assembled trailing tail is
    /// preserved, so cross-round imports never accumulate old tails.
    pub(super) fn normalize_inbound_conversation(
        registry: &wf_tools::registry::ToolRegistry,
        conversation: &[Message],
        config: &AgentLoopConfig,
    ) -> Vec<Message> {
        if conversation.is_empty() {
            return Vec::new();
        }
        let (filtered, trailing) =
            wf_execution_shared::agent_prompt::split_trailing_dynamic_tail(conversation.to_vec());
        let activated_tools: std::collections::HashSet<String> =
            config.activated_tool_names.iter().cloned().collect();
        // Exposure overrides are intentionally empty here, matching the per-turn
        // resolution: the entity carries no overrides at build time (no producer
        // wires `with_exposure_overrides` yet), so both read the same empty
        // source and cannot drift. Thread a real overrides source through both
        // sites when one appears.
        let resolution = wf_tools::resolve_tool_exposure(wf_tools::ExposureInput {
            registry,
            available_names: &config.available_tool_names,
            initial_names: &config.initial_tool_names,
            discoverable_names: &config.discoverable_tool_names,
            hidden_names: &config.hidden_tool_names,
            enable_general_tool: config.enable_general_tool,
            activated_tools: &activated_tools,
            exposure_overrides: &std::collections::HashMap::new(),
        });
        let mut normalized =
            wf_tools::general_history::normalize_history_for_exposure(&filtered, &resolution);
        normalized.extend(trailing);
        normalized
    }

    pub(super) async fn build_entity(
        &self,
        config: &AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopEntity> {
        self.build_entity_with_forced_id(config, input, None).await
    }

    pub(super) async fn build_entity_with_forced_id(
        &self,
        config: &AgentLoopConfig,
        input: AgentLoopInput,
        forced_id: Option<Id>,
    ) -> AgentResult<AgentLoopEntity> {
        let hooks: Vec<HookDefinition> = config
            .hooks
            .iter()
            .map(|h| {
                // Single runtime normalization point (`HookDefinition::from`
                // clamps negative priorities and drops empty handler names
                // with a warning); only the id is assigned here so every
                // definition carries a fresh identity.
                let mut def = HookDefinition::from(h);
                def.id = wf_common::generate_id();
                def
            })
            .collect();

        // Every run gets a fresh agent loop id; the config's `agent_id` only
        // identifies the definition (persisted as `definition_id`). An
        // explicit `forced_id` (in-place resume) wins over the coordinator
        // preset so the continuation reuses the source execution id.
        let agent_loop_id = forced_id
            .or_else(|| self.agent_loop_id.clone())
            .unwrap_or_else(|| Id::from(wf_common::generate_id()));
        let mut entity = AgentLoopEntity::new(agent_loop_id)
            .with_definition_id(config.agent_id.clone())
            .with_hooks(hooks)
            .with_model(config.model.clone());

        // Parent association: typed field first, `input.context` fallback.
        let parent_execution_id = self.parent_execution_id.clone().or_else(|| {
            input
                .context
                .get("parent_execution_id")
                .and_then(|v| v.as_str())
                .map(Id::from)
        });
        if let Some(parent_id) = parent_execution_id {
            entity = entity.with_parent_execution_id(parent_id.clone());
            // Resolve hierarchy depth / root / ancestor chain from the
            // registered parent so `get_hierarchy_depth`,
            // `get_root_execution_id` and `get_ancestors` reflect the real
            // parent chain (root run keeps 0 / own id / empty).
            if let Some(ref registry) = self.entity_registry {
                if let Some(parent) = registry.get(&parent_id) {
                    let parent_ref = parent.as_ref();
                    entity = entity
                        .with_hierarchy_depth(child_depth(parent_ref))
                        .with_root_execution_id(child_root(parent_ref))
                        .with_ancestors(child_ancestors(parent_ref));
                }
            }
        }

        if !config.available_tool_names.is_empty() {
            entity = entity.with_available_tool_names(config.available_tool_names.clone());
        }

        if !config.initial_tool_names.is_empty() {
            entity = entity.with_initial_tool_names(config.initial_tool_names.clone());
        }

        if !config.discoverable_tool_names.is_empty() {
            entity = entity.with_discoverable_tool_names(config.discoverable_tool_names.clone());
        }

        if config.enable_general_tool.is_some() {
            entity = entity.with_enable_general_tool(config.enable_general_tool);
        }

        if !config.hidden_tool_names.is_empty() {
            entity = entity.with_hidden_tool_names(config.hidden_tool_names.clone());
        }

        if config.history_normalization {
            entity = entity.with_history_normalization(true);
        }

        // Seed formally activated tools (TOOL_VISIBILITY unblock markers from
        // the workflow) into the run's discovery state.
        if !config.activated_tool_names.is_empty() {
            let activated: std::collections::HashSet<String> =
                config.activated_tool_names.iter().cloned().collect();
            let state = entity.state.clone();
            {
                let mut guard = state.write().await;
                for name in &activated {
                    guard.tool_discovery_mut().activate_tool(name);
                }
            }
        }

        if let Some(ref format) = config.tool_call_protocol {
            entity = entity.with_tool_call_protocol(format.clone());
        }

        if let Some(duration) = self.max_pause_duration {
            entity = entity.with_max_pause_duration(duration);
        }
        if let Some(ref metrics) = self.metrics {
            entity = entity.with_timeout_metrics(metrics.timeout());
        }

        if let Some(ref bus) = self.event_bus {
            entity.interruption().set_event_bus(bus.clone());
        }

        // Loop-boundary history normalization: the inbound conversation
        // carries the sender's bucket shapes, so rewrite it once to this
        // loop's target exposure (config lists + activated tools) before it
        // becomes the session history. This covers every entry path —
        // workflow `AGENT_LOOP`, `call_agent` sub-agents, direct API use —
        // since all of them build the entity here. The workflow handler may
        // already have normalized; the conversion is idempotent under the
        // same resolution, so a second pass is a no-op.
        let inbound_conversation =
            Self::normalize_inbound_conversation(&self.tool_registry, &input.conversation, config);
        for msg in &inbound_conversation {
            entity.conversation().write().await.add_message(msg.clone());
        }

        if config.enable_token_tracking.unwrap_or(true) {
            if let Some(token_limit) = config.token_limit.filter(|&l| l > 0) {
                entity
                    .conversation()
                    .write()
                    .await
                    .set_token_limit(token_limit);
            }
            // Context budget comes from the model window only, never from
            // the task token limit: single-request input size is a model
            // capability, task length is a separate concern. A per-model
            // percent override lives in the profile metadata map.
            let profile = self.gateway.profile_registry().get(&config.model);
            let context_budget = wf_execution_shared::context_budget_from_profile(
                profile.as_ref().and_then(|p| p.context_window_size),
                profile.as_ref().and_then(|p| p.metadata.as_ref()),
            );
            if context_budget == 0 {
                tracing::warn!(
                    model = %config.model,
                    "no context window for model: compression and preflight checks disabled"
                );
            }
            entity
                .conversation()
                .write()
                .await
                .set_context_limit(context_budget);
        }

        if !input.message.is_empty() {
            let msg = Message {
                id: wf_common::generate_id(),
                role: MessageRole::User,
                content: MessageContentValue::Text(input.message),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            };
            entity.conversation().write().await.add_message(msg);
        }

        Ok(entity)
    }
}
