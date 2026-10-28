use std::collections::HashMap;

use wf_types::checkpoint::workflow::cap_node_payload;
use wf_types::events::EventType;

use crate::entity::{PendingVariableInjection, VariableInjectionPolicy, WorkflowExecutionEntity};

use super::WorkflowCoordinator;

impl WorkflowCoordinator {
    /// Apply pending `NextBoundary` injections at a node boundary.
    ///
    /// Runs at the top of the coordinator loop after the pause/cancel
    /// check passes, so a paused execution keeps its pending items and
    /// conditional edges downstream observe the injected values. Same
    /// variable submitted several times collapses last-writer-wins in
    /// submission order; one audit event is published per variable.
    pub(super) async fn apply_pending_variable_injections(&self, entity: &WorkflowExecutionEntity) {
        use wf_execution_shared::types::execution_entity::ExecutionEntity;
        if entity.status().is_terminal() {
            return;
        }
        // Error-branch isolation: the branch runs on an overlay cloned from
        // the entry snapshot while the main map stays frozen. Injected
        // values written now would bypass the overlay and lose to the merge,
        // so hold them back until the scope exits.
        if self.error_scope.is_some() {
            return;
        }
        let drained = entity.drain_variable_injections(VariableInjectionPolicy::NextBoundary);
        self.apply_drained_injections(entity, &drained, VariableInjectionPolicy::NextBoundary)
            .await;
    }

    /// Apply pending `OnResume` injections during an explicit resume, before
    /// the coordinator re-drives the remaining graph. Called once by the
    /// application resume path, never by the node loop.
    pub async fn apply_on_resume_variable_injections(&self, entity: &WorkflowExecutionEntity) {
        use wf_execution_shared::types::execution_entity::ExecutionEntity;
        if entity.status().is_terminal() {
            return;
        }
        let drained = entity.drain_variable_injections(VariableInjectionPolicy::OnResume);
        self.apply_drained_injections(entity, &drained, VariableInjectionPolicy::OnResume)
            .await;
    }

    async fn apply_drained_injections(
        &self,
        entity: &WorkflowExecutionEntity,
        drained: &[PendingVariableInjection],
        policy: VariableInjectionPolicy,
    ) {
        if drained.is_empty() {
            return;
        }
        // Last-writer-wins per variable in submission order; the drain is
        // already ordered, so later items overwrite earlier ones.
        let mut winning: HashMap<&str, &PendingVariableInjection> = HashMap::new();
        let mut coalesced: HashMap<&str, usize> = HashMap::new();
        for item in drained {
            if winning.insert(item.variable_name.as_str(), item).is_some() {
                *coalesced.entry(item.variable_name.as_str()).or_insert(0) += 1;
            }
        }
        let mut names: Vec<&str> = winning.keys().copied().collect();
        names.sort_unstable();
        let applied_at = wf_common::now();
        let boundary_node_id = self.current_node_id.clone();
        let event_bus = self.ctx.event_bus.clone();
        for name in names {
            let item = winning[name];
            let old_value = entity.get_variable(name);
            entity.set_variable(name, item.value.clone());
            // The loop reads `ctx.variables`; production shares the entity
            // map Arc, but branch/test coordinators may hold a separate one.
            if !std::sync::Arc::ptr_eq(&self.ctx.variables, entity.variables()) {
                self.ctx
                    .variables
                    .insert(name.to_string(), item.value.clone());
            }
            let mut metadata = serde_json::json!({
                "injection_id": item.id,
                "variable_name": name,
                "new_value": cap_node_payload(&item.value),
                "source": item.source,
                "submitted_at": item.submitted_at,
                "applied_at": applied_at,
                "policy": policy.label(),
            });
            if let Some(old) = old_value {
                metadata["old_value"] = cap_node_payload(&old);
            }
            if let Some(node_id) = &boundary_node_id {
                metadata["boundary_node_id"] = serde_json::Value::String(node_id.clone());
            }
            if let Some(dropped) = coalesced.get(name) {
                metadata["coalesced"] = serde_json::Value::from(*dropped as u64);
            }
            self.emit_event(
                event_bus.as_deref(),
                EventType::WorkflowVariableInjected,
                entity,
                &metadata,
            )
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
    use wf_tools::registry::ToolRegistry;
    use wf_types::node::StaticNodeType;
    use wf_types::workflow::EdgeType;
    use wf_types::workflow_execution::{
        WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
    };

    use crate::handler::NodeHandler;

    fn node(id: &str, node_type: &str) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner: serde_json::Value::Null,
        }
    }

    fn edge(source: &str, target: &str, condition: Option<&str>) -> WorkflowEdge {
        WorkflowEdge {
            id: format!("{source}-{target}"),
            source_node_id: source.to_string(),
            target_node_id: target.to_string(),
            r#type: EdgeType::Default,
            condition: condition.map(|c| c.to_string()),
            label: None,
            description: None,
            error_route: None,
        }
    }

    fn options() -> WorkflowExecutionOptions {
        WorkflowExecutionOptions {
            file_scope: None,
            input: None,
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: Some(false),
            node_timeout: None,
            max_pause_duration: None,
            max_navigation_multiplier: None,
            loop_max_iterations_cap: None,
        }
    }

    struct ProbeHandler {
        seen: Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    }

    #[async_trait::async_trait]
    impl NodeHandler for ProbeHandler {
        fn node_type(&self) -> StaticNodeType {
            StaticNodeType::Variable
        }

        async fn execute(
            &self,
            ctx: &mut NodeExecutionContext,
        ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
            let observed = ctx.get_variable("speed").unwrap_or(serde_json::Value::Null);
            self.seen.lock().unwrap().push(observed.clone());
            Ok(NodeExecutionResult::simple(observed))
        }
    }

    fn handlers(probe: ProbeHandler) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
        let mut map: HashMap<StaticNodeType, Box<dyn NodeHandler>> = HashMap::new();
        map.insert(
            StaticNodeType::Start,
            Box::new(crate::handler::start_end::StartHandler),
        );
        map.insert(
            StaticNodeType::End,
            Box::new(crate::handler::start_end::EndHandler),
        );
        map.insert(StaticNodeType::Variable, Box::new(probe));
        Arc::new(map)
    }

    fn graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            nodes,
            edges,
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
            error_default: None,
        }
    }

    #[tokio::test]
    async fn next_boundary_injection_applies_before_routing_and_emits_audit() {
        let bus = Arc::new(wf_core::EventBus::new(64));
        let mut sub = bus.subscribe();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let probe = ProbeHandler { seen: seen.clone() };
        let g = graph(
            vec![
                node("start", "START"),
                node("probe", "VARIABLE"),
                node("end", "END"),
            ],
            vec![edge("start", "probe", None), edge("probe", "end", None)],
        );
        let exec_ctx = wf_execution_shared::context::ExecutorContext::new(
            wf_common::generate_id(),
            wf_common::generate_id(),
            Some(bus),
            Arc::new(ToolRegistry::new()),
            options(),
        );
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        entity
            .submit_variable_injection(
                "speed",
                serde_json::json!(7),
                Some("cli"),
                Some(VariableInjectionPolicy::NextBoundary),
            )
            .expect("submit");
        let mut coordinator =
            WorkflowCoordinator::new(exec_ctx, g, handlers(probe)).expect("coordinator");
        // Share the entity variable map the way the application layer does.
        coordinator = coordinator.with_entity(entity);
        coordinator.ctx.variables = coordinator
            .entity
            .as_ref()
            .expect("entity")
            .variables()
            .clone();
        let output = coordinator.execute().await.expect("execute");
        assert_eq!(output, serde_json::json!(7));
        assert_eq!(seen.lock().unwrap().as_slice(), &[serde_json::json!(7)]);
        let entity = coordinator.entity.as_ref().expect("entity");
        assert_eq!(entity.get_variable("speed"), Some(serde_json::json!(7)));
        assert!(entity.list_variable_injections().is_empty());

        let mut found = None;
        while let Ok(event) = sub.try_recv() {
            if event.r#type == EventType::WorkflowVariableInjected {
                found = Some(event);
            }
        }
        let event = found.expect("injection audit event");
        let metadata = event.metadata.expect("metadata");
        assert_eq!(metadata["variable_name"], serde_json::json!("speed"));
        assert_eq!(metadata["new_value"], serde_json::json!(7));
        assert_eq!(metadata["source"], serde_json::json!("cli"));
        assert_eq!(metadata["policy"], serde_json::json!("next_boundary"));
        assert_eq!(metadata["boundary_node_id"], serde_json::json!("start"));
    }

    #[tokio::test]
    async fn on_resume_injection_waits_out_the_node_loop() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let probe = ProbeHandler { seen: seen.clone() };
        let g = graph(
            vec![
                node("start", "START"),
                node("probe", "VARIABLE"),
                node("end", "END"),
            ],
            vec![edge("start", "probe", None), edge("probe", "end", None)],
        );
        let exec_ctx = wf_execution_shared::context::ExecutorContext::new(
            wf_common::generate_id(),
            wf_common::generate_id(),
            None,
            Arc::new(ToolRegistry::new()),
            options(),
        );
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        entity
            .submit_variable_injection(
                "speed",
                serde_json::json!(3),
                None,
                Some(VariableInjectionPolicy::OnResume),
            )
            .expect("submit");
        let mut coordinator =
            WorkflowCoordinator::new(exec_ctx, g, handlers(probe)).expect("coordinator");
        coordinator = coordinator.with_entity(entity);
        coordinator.ctx.variables = coordinator
            .entity
            .as_ref()
            .expect("entity")
            .variables()
            .clone();
        let output = coordinator.execute().await.expect("execute");
        assert_eq!(output, serde_json::Value::Null);
        let entity = coordinator.entity.as_ref().expect("entity");
        assert_eq!(entity.get_variable("speed"), None);
        assert_eq!(entity.list_variable_injections().len(), 1);
    }

    #[tokio::test]
    async fn injected_values_drive_conditional_routing() {
        struct MarkerHandler;

        #[async_trait::async_trait]
        impl NodeHandler for MarkerHandler {
            fn node_type(&self) -> StaticNodeType {
                StaticNodeType::Variable
            }

            async fn execute(
                &self,
                ctx: &mut NodeExecutionContext,
            ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult>
            {
                Ok(NodeExecutionResult::simple(serde_json::json!(ctx
                    .node_id
                    .clone())))
            }
        }

        let g = WorkflowGraphStructure {
            nodes: vec![
                node("start", "START"),
                node("gate", "VARIABLE"),
                node("fast", "VARIABLE"),
                node("slow", "VARIABLE"),
                node("end", "END"),
            ],
            edges: vec![
                edge("start", "gate", None),
                edge("gate", "fast", Some("isTrue(flag)")),
                edge("gate", "slow", None),
                edge("fast", "end", None),
                edge("slow", "end", None),
            ],
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
            error_default: None,
        };
        let exec_ctx = wf_execution_shared::context::ExecutorContext::new(
            wf_common::generate_id(),
            wf_common::generate_id(),
            None,
            Arc::new(ToolRegistry::new()),
            options(),
        );
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        entity
            .submit_variable_injection(
                "flag",
                serde_json::json!(true),
                Some("web"),
                Some(VariableInjectionPolicy::NextBoundary),
            )
            .expect("submit");
        let mut map: HashMap<StaticNodeType, Box<dyn NodeHandler>> = HashMap::new();
        map.insert(
            StaticNodeType::Start,
            Box::new(crate::handler::start_end::StartHandler),
        );
        map.insert(
            StaticNodeType::End,
            Box::new(crate::handler::start_end::EndHandler),
        );
        map.insert(StaticNodeType::Variable, Box::new(MarkerHandler));
        let mut coordinator =
            WorkflowCoordinator::new(exec_ctx, g, Arc::new(map)).expect("coordinator");
        coordinator = coordinator.with_entity(entity);
        coordinator.ctx.variables = coordinator
            .entity
            .as_ref()
            .expect("entity")
            .variables()
            .clone();
        let output = coordinator.execute().await.expect("execute");
        assert_eq!(output, serde_json::json!("fast"));
    }
}
