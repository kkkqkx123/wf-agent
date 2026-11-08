use wf_execution_shared::hooks::types::HookDefinition;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_types::agent_execution::TemporaryMode;
use wf_types::message::{Message, MessageRole};
use wf_types::Id;

use super::AgentLoopCoordinator;
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

impl AgentLoopCoordinator {
    /// Force the engine-side temporary policy onto a run config.
    ///
    /// A temporary mode overrides the caller-supplied exposure lists,
    /// never merges with them: the pool becomes exactly the tier
    /// allowlist, nothing starts gated or discoverable (everything allowed
    /// is visible upfront, so a short Q&A needs no discovery dance),
    /// formally activated tools outside the tier are dropped, the caller's
    /// hidden list is kept and extended with the tier denylist, and the
    /// `general` batch proxy is switched off so no inner invocation can
    /// widen the cage. `None` returns the config untouched: full runs keep
    /// caller-supplied exposure.
    fn forced_temporary_config(config: &AgentLoopConfig) -> AgentLoopConfig {
        let (allowed, denied) = match config.temporary_mode {
            None => return config.clone(),
            Some(TemporaryMode::Restricted) => (
                wf_tools::predefined::risk::temporary_allowed_tools(),
                wf_tools::predefined::risk::temporary_denied_tools(),
            ),
            Some(TemporaryMode::Ephemeral) => (
                wf_tools::predefined::risk::ephemeral_allowed_tools(),
                wf_tools::predefined::risk::ephemeral_denied_tools(),
            ),
        };
        let mut forced = config.clone();
        forced.available_tool_names = Some(allowed.iter().map(|id| id.to_string()).collect());
        forced.initial_tool_names = Vec::new();
        forced.discoverable_tool_names = Vec::new();
        forced.activated_tool_names = config
            .activated_tool_names
            .iter()
            .filter(|name| allowed.contains(&name.as_str()))
            .cloned()
            .collect();
        let mut hidden = config.hidden_tool_names.clone();
        for id in denied {
            if !hidden.iter().any(|name| name == id) {
                hidden.push(id.to_string());
            }
        }
        forced.hidden_tool_names = hidden;
        forced.enable_general_tool = Some(false);
        forced
    }
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
        let empty_tools: Vec<String> = Vec::new();
        let resolution = wf_tools::resolve_tool_exposure(wf_tools::ExposureInput {
            registry,
            available_names: config.available_tool_names.as_ref().unwrap_or(&empty_tools),
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
        // Temporary modes are enforced engine-side: the forced config
        // replaces the caller lists before anything else reads them
        // (entity exposure, effective-config snapshot, inbound history
        // normalization), so a client cannot widen its own cage and the
        // persisted loop config records what actually ran.
        let forced = Self::forced_temporary_config(config);
        let config = &forced;
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
            .with_directory(self.directory.clone())
            .with_definition_id(config.agent_id.clone())
            .with_hooks(hooks)
            .with_model(config.model.clone());

        // Parent association: the coordinator's typed parent fields are the
        // sole channel. The parent hierarchy manager is the single source.
        // When no manager handle was injected, the registry parent entity
        // lends its manager for a derive; a parent id with no live parent is
        // an explicit error instead of a half-linked legacy record. An empty
        // id carries no identity and means "no parent".
        let parent_execution_id = self.parent_execution_id.clone().filter(|id| !id.is_empty());
        if let Some(parent_id) = parent_execution_id {
            let parent_manager = self.parent_hierarchy_manager.clone().or_else(|| {
                self.entity_registry
                    .as_ref()
                    .and_then(|registry| registry.get(&parent_id))
                    .map(|parent| parent.hierarchy_manager())
            });
            match parent_manager {
                Some(parent_manager) => {
                    let child_manager = parent_manager
                        .derive_child(
                            entity.id().clone(),
                            wf_types::execution::ExecutionType::Agent,
                            None,
                        )
                        .map_err(|e| {
                            if e.is_hierarchy_depth_exceeded() {
                                crate::error::AgentError::HierarchyLimitReached(e.to_string())
                            } else {
                                crate::error::AgentError::Validation(e.to_string())
                            }
                        })?;
                    entity = entity.with_hierarchy_manager(child_manager);
                }
                None => {
                    return Err(crate::error::AgentError::Validation(format!(
                        "parent execution '{}' has no live hierarchy manager: the parent is unknown, already terminated, or belongs to an engine without a shared registry. Pass an empty parent execution id to run as a root agent, or dispatch through a live parent execution",
                        parent_id
                    )));
                }
            }
        }
        entity = entity.with_effective_config(config.clone());

        if let Some(tools) = config.available_tool_names.clone() {
            entity = entity.with_available_tool_names(tools);
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
            // A zero budget only disables proactive compression checks here;
            // the fail-loudly termination fires where compression becomes
            // necessary (a context-length rejection with no window ends the
            // run as `ContextBudgetExhausted` instead of looping).
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
            // Compaction derives its thresholds from the same model
            // window; a zero limit leaves compaction disabled and the
            // legacy single-summary path applies.
            if let Some(window) = profile.as_ref().and_then(|p| p.context_window_size) {
                entity.conversation().write().await.set_compaction_window(
                    u64::from(window),
                    &wf_execution_shared::process_compaction_limits(),
                );
            }
        }

        // The opening user turn: plain text when the turn carries no
        // images, rich blocks (text plus `ImageUrl` entries) otherwise.
        // Images are re-validated here so direct engine entries (workflow
        // dispatch, triggers) fail loudly instead of storing a broken turn;
        // the API gates report the same failure earlier with field detail.
        if let Err(reason) = wf_types::message::validate_user_images(&input.images) {
            return Err(crate::error::AgentError::Validation(format!(
                "invalid agent images: {reason}"
            )));
        }
        if let Some(content) = wf_types::message::user_content(&input.message, &input.images) {
            let msg = Message {
                id: wf_common::generate_id(),
                role: MessageRole::User,
                content,
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                thinking_signature: None,
                metadata: None,
            };
            entity.conversation().write().await.add_message(msg);
        }

        Ok(entity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> AgentLoopConfig {
        AgentLoopConfig {
            agent_id: Id::from("agent-1"),
            model: "mock".to_string(),
            file_scope: None,
            max_iterations: None,
            max_execution_time: None,
            hooks: Vec::new(),
            available_tool_names: None,
            initial_tool_names: Vec::new(),
            discoverable_tool_names: Vec::new(),
            activated_tool_names: Vec::new(),
            hidden_tool_names: Vec::new(),
            enable_general_tool: None,
            tool_call_protocol: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
            checkpoint_message_interval: None,
            general_description: None,
            discoverable_metadata_block: None,
            history_normalization: false,
            context_schema: None,
            context_strict: None,
            temporary_mode: None,
        }
    }

    #[test]
    fn full_runs_keep_caller_exposure() {
        let config = base_config();
        let forced = AgentLoopCoordinator::forced_temporary_config(&config);
        assert_eq!(forced, config);
    }

    #[test]
    fn restricted_mode_overrides_caller_lists() {
        let mut config = base_config();
        config.temporary_mode = Some(TemporaryMode::Restricted);
        // A client-supplied pool claiming write tools must not survive.
        config.available_tool_names = Some(vec!["write_file".to_string()]);
        config.enable_general_tool = Some(true);
        config.hidden_tool_names = vec!["custom_blocked".to_string()];
        let forced = AgentLoopCoordinator::forced_temporary_config(&config);
        let available = forced.available_tool_names.expect("pool is forced");
        assert!(available.contains(&"read_file".to_string()));
        assert!(available.contains(&"web_search".to_string()));
        assert!(!available.contains(&"write_file".to_string()));
        assert!(forced.initial_tool_names.is_empty());
        assert!(forced.discoverable_tool_names.is_empty());
        assert!(forced.hidden_tool_names.contains(&"write_file".to_string()));
        assert!(forced
            .hidden_tool_names
            .contains(&"custom_blocked".to_string()));
        assert_eq!(forced.enable_general_tool, Some(false));
        assert_eq!(forced.temporary_mode, Some(TemporaryMode::Restricted));
    }

    #[test]
    fn ephemeral_mode_is_local_only() {
        let mut config = base_config();
        config.temporary_mode = Some(TemporaryMode::Ephemeral);
        let forced = AgentLoopCoordinator::forced_temporary_config(&config);
        let available = forced.available_tool_names.expect("pool is forced");
        assert!(available.contains(&"read_file".to_string()));
        assert!(available.contains(&"ask_followup_question".to_string()));
        assert!(!available.contains(&"web_search".to_string()));
        assert!(forced.hidden_tool_names.contains(&"web_search".to_string()));
        assert_eq!(forced.enable_general_tool, Some(false));
    }
}
