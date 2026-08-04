//! Runtime assembly for the event-driven trigger listener.
//!
//! Implements the wf-workflow listener traits over the runtime's own pieces:
//!
//! - [`ResourceTriggerRegistry`]: trigger templates from the wf-resource
//!   registrar (predefined `context_compression_trigger` et al.);
//! - [`WorkflowRunner`]: triggered sub-workflows executed through the
//!   `WorkflowCoordinator` (predefined `llm_summary_workflow`);
//! - write-back registry: wf-workflow's [`ExecutionContextRegistry`], into
//!   which every started workflow execution registers its variable map
//!   (register at start, unregister at end — see [`WorkflowRunner::run`]).
//!
//! `start_trigger_listener` wires them together and spawns the listener
//! background task; the returned handle's shutdown token stops the loop.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use wf_core::registry::Registry;
use wf_core::EventBus;
use wf_execution_shared::context::ExecutorContext;
use wf_llm::LlmGateway;
use wf_resource::registrar::Registries;
use wf_types::node::StaticNodeType;
use wf_types::trigger::TriggerTemplate;
use wf_types::workflow::WorkflowTemplate;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_workflow::error::{WorkflowError, WorkflowResult};
use wf_workflow::handler::NodeHandler;
use wf_workflow::trigger_listener::{
    SubworkflowRunner, TriggerEventListener, TriggerTemplateRegistry,
};
use wf_workflow::{WorkflowCoordinator, WorkflowExecutionEntity};

/// Trigger template registry backed by the wf-resource registrar.
pub struct ResourceTriggerRegistry {
    registries: Arc<Registries>,
}

impl ResourceTriggerRegistry {
    pub fn new(registries: Arc<Registries>) -> Self {
        Self { registries }
    }
}

impl TriggerTemplateRegistry for ResourceTriggerRegistry {
    fn templates(&self) -> Vec<TriggerTemplate> {
        self.registries
            .trigger_templates
            .list()
            .iter()
            .filter_map(|key| {
                self.registries
                    .trigger_templates
                    .get(key)
                    .map(|template| template.as_ref().clone())
            })
            .collect()
    }
}

/// Convert a workflow template into an executable graph structure.
///
/// The predefined templates are flat (no subgraph expansion): nodes map
/// directly, the first node is the start (START_FROM_TRIGGER) and the last
/// node the end (CONTINUE_FROM_TRIGGER) of the summary workflow.
pub fn template_to_graph(template: &WorkflowTemplate) -> WorkflowGraphStructure {
    let nodes: Vec<WorkflowNode> = template
        .definition
        .nodes
        .iter()
        .map(|node| WorkflowNode {
            id: node.id.clone(),
            name: node.name.clone(),
            node_type: serde_json::to_string(&node.node_type)
                .map(|s| s.trim_matches('"').to_string())
                .unwrap_or_default(),
            inner: node.config.clone().unwrap_or(Value::Null),
        })
        .collect();
    let edges: Vec<WorkflowEdge> = template
        .definition
        .edges
        .iter()
        .map(|edge| WorkflowEdge {
            id: edge.id.clone(),
            source_node_id: edge.source_node_id.clone(),
            target_node_id: edge.target_node_id.clone(),
            r#type: edge.r#type.clone(),
            condition: edge.condition.clone(),
            label: edge.label.clone(),
            description: edge.description.clone(),
        })
        .collect();
    WorkflowGraphStructure {
        start_node_id: nodes.first().map(|node| node.id.clone()),
        end_node_ids: nodes
            .last()
            .map(|node| vec![node.id.clone()])
            .unwrap_or_default(),
        nodes,
        edges,
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
    }
}

/// Sub-workflow runner over the workflow coordinator.
pub struct WorkflowRunner {
    registries: Arc<Registries>,
    event_bus: Arc<EventBus>,
    handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
    /// Write-back registry of live workflow executions: every execution
    /// registers its variable map at start and unregisters at end.
    contexts: Arc<ExecutionContextRegistry>,
    /// Optional skill loader injected into builtin tool executors.
    skill_loader: Option<Arc<wf_tools::SkillLoader>>,
    /// Shared tool registry (builtin handlers + skills + MCP tools). When
    /// absent, a fresh registry is created per run.
    tool_registry: Option<Arc<wf_tools::registry::ToolRegistry>>,
}

impl WorkflowRunner {
    pub fn new(
        registries: Arc<Registries>,
        event_bus: Arc<EventBus>,
        gateway: Arc<LlmGateway>,
        contexts: Arc<ExecutionContextRegistry>,
    ) -> Self {
        Self::with_skill_loader(registries, event_bus, gateway, contexts, None)
    }

    pub fn with_skill_loader(
        registries: Arc<Registries>,
        event_bus: Arc<EventBus>,
        gateway: Arc<LlmGateway>,
        contexts: Arc<ExecutionContextRegistry>,
        skill_loader: Option<Arc<wf_tools::SkillLoader>>,
    ) -> Self {
        Self {
            registries,
            event_bus,
            handlers: wf_workflow::create_default_handlers(gateway),
            contexts,
            skill_loader,
            tool_registry: None,
        }
    }

    /// Like [`WorkflowRunner::with_skill_loader`], but uses a caller-provided
    /// shared tool registry (skills and MCP tools pre-wired) for every run.
    pub fn with_tool_registry(
        registries: Arc<Registries>,
        event_bus: Arc<EventBus>,
        gateway: Arc<LlmGateway>,
        contexts: Arc<ExecutionContextRegistry>,
        tool_registry: Option<Arc<wf_tools::registry::ToolRegistry>>,
    ) -> Self {
        Self {
            registries,
            event_bus,
            handlers: wf_workflow::create_default_handlers(gateway),
            contexts,
            skill_loader: None,
            tool_registry,
        }
    }
}

#[async_trait]
impl SubworkflowRunner for WorkflowRunner {
    async fn run(&self, workflow_id: &str, input: Value) -> WorkflowResult<Value> {
        let template = self
            .registries
            .workflows
            .get(workflow_id)
            .ok_or_else(|| {
                WorkflowError::TriggerError(format!(
                    "Triggered workflow '{}' not found in resource registries",
                    workflow_id
                ))
            })?
            .as_ref()
            .clone();
        let graph = template_to_graph(&template);

        let max_execution_time = template
            .definition
            .triggered_subworkflow_config
            .as_ref()
            .and_then(|config| config.timeout);
        let options = WorkflowExecutionOptions {
            input: Some(input),
            max_steps: None,
            timeout: None,
            max_execution_time,
            enable_checkpoints: Some(false),
            node_timeout: None,
            max_pause_duration: None,
            retry_budget: None,
            on_failure: None,
            max_retries: None,
            retry_delay_ms: None,
            exponential_backoff: None,
            fallback_output: None,
        };

        let tool_registry = match &self.tool_registry {
            Some(shared) => shared.clone(),
            None => {
                let fresh = Arc::new(wf_tools::create_default_tool_registry());
                if let Some(loader) = &self.skill_loader {
                    fresh.set_skill_loader(loader.clone());
                }
                fresh
            }
        };

        let exec_ctx = ExecutorContext::new(
            wf_common::generate_id(),
            wf_common::generate_id(),
            Some(self.event_bus.clone()),
            tool_registry,
            options,
        );
        // Lifecycle wiring (compression chain closure): the execution's
        // variable map is the write-back target of its named message arrays;
        // registered at start and unregistered at end, so the trigger
        // listener can write compressed arrays back even while the execution
        // continues (or after it finished, harmlessly).
        let execution_id = exec_ctx.execution_id.clone();
        self.contexts
            .register_workflow(execution_id.clone(), exec_ctx.variables.clone());
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        let mut coordinator =
            WorkflowCoordinator::new(exec_ctx, graph, self.handlers.clone())?.with_entity(entity);
        let outcome = coordinator.execute().await;
        self.contexts.unregister(&execution_id);
        outcome
    }
}

/// Registry of live workflow execution write-back targets, keyed by
/// execution id. Lives in wf-workflow next to the trigger listener and the
/// message contexts it writes back into; wf-runtime only wires executions
/// into it during assembly (see [`WorkflowRunner`]).
pub use wf_workflow::execution_context::ExecutionContextRegistry;

/// Running trigger listener plus its shutdown token and task handle.
pub struct TriggerListenerHandle {
    pub listener: Arc<TriggerEventListener>,
    pub shutdown: CancellationToken,
    pub handle: tokio::task::JoinHandle<()>,
}

/// Wire the listener traits together and spawn the listener loop.
pub fn start_trigger_listener(
    event_bus: Arc<EventBus>,
    registries: Arc<Registries>,
    gateway: Arc<LlmGateway>,
    contexts: Arc<ExecutionContextRegistry>,
) -> TriggerListenerHandle {
    start_trigger_listener_with_skills(event_bus, registries, gateway, contexts, None)
}

/// Like `start_trigger_listener`, but injects the runtime skill loader into
/// the builtin tool executor of triggered sub-workflows.
pub fn start_trigger_listener_with_skills(
    event_bus: Arc<EventBus>,
    registries: Arc<Registries>,
    gateway: Arc<LlmGateway>,
    contexts: Arc<ExecutionContextRegistry>,
    skill_loader: Option<Arc<wf_tools::SkillLoader>>,
) -> TriggerListenerHandle {
    let runner: Arc<dyn SubworkflowRunner> = Arc::new(WorkflowRunner::with_skill_loader(
        registries.clone(),
        event_bus.clone(),
        gateway,
        contexts.clone(),
        skill_loader,
    ));
    spawn_listener(event_bus, registries, contexts, runner)
}

/// Like `start_trigger_listener`, but uses a caller-provided shared tool
/// registry (builtin handlers + skills + MCP tools) for every triggered
/// sub-workflow run.
pub fn start_trigger_listener_with_registry(
    event_bus: Arc<EventBus>,
    registries: Arc<Registries>,
    gateway: Arc<LlmGateway>,
    contexts: Arc<ExecutionContextRegistry>,
    tool_registry: Option<Arc<wf_tools::registry::ToolRegistry>>,
) -> TriggerListenerHandle {
    let runner: Arc<dyn SubworkflowRunner> = Arc::new(WorkflowRunner::with_tool_registry(
        registries.clone(),
        event_bus.clone(),
        gateway,
        contexts.clone(),
        tool_registry,
    ));
    spawn_listener(event_bus, registries, contexts, runner)
}

fn spawn_listener(
    event_bus: Arc<EventBus>,
    registries: Arc<Registries>,
    contexts: Arc<ExecutionContextRegistry>,
    runner: Arc<dyn SubworkflowRunner>,
) -> TriggerListenerHandle {
    let registry: Arc<dyn TriggerTemplateRegistry> =
        Arc::new(ResourceTriggerRegistry::new(registries));
    let shutdown = CancellationToken::new();
    let listener = Arc::new(TriggerEventListener::new(
        event_bus,
        registry,
        runner,
        contexts,
        shutdown.clone(),
    ));
    let handle = tokio::spawn({
        let listener = listener.clone();
        async move { listener.run().await }
    });
    TriggerListenerHandle {
        listener,
        shutdown,
        handle,
    }
}

/// Stop the listener loop and await its task.
pub async fn stop_trigger_listener(handle: TriggerListenerHandle) {
    handle.shutdown.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle.handle).await;
    let _ = handle.listener;
}

/// Best-effort shutdown of an optional listener; used by the runtime teardown.
pub async fn shutdown_trigger_listener(handle: Option<TriggerListenerHandle>) {
    if let Some(handle) = handle {
        warn!("Stopping event-driven trigger listener");
        stop_trigger_listener(handle).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashmap::DashMap;
    use std::time::Duration;
    use wf_llm::mock::{LlmResponseSpec, MockLlmClient};
    use wf_resource::registrar::Options;
    use wf_types::events::EventType;
    use wf_types::message::{Message, MessageContentValue, MessageRole};
    use wf_types::workflow::EdgeType;

    fn text_message(role: MessageRole, text: &str) -> Message {
        Message {
            id: wf_common::generate_id(),
            role,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    /// Wait until the bus sees the expected number of receivers (the
    /// listener subscribes on its first poll). Bounded: a wrong expectation
    /// must fail loudly instead of spinning forever.
    async fn wait_for_listener(bus: &EventBus, expected_receivers: usize) {
        for _ in 0..200 {
            if bus.receiver_count() >= expected_receivers {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!(
            "expected {} receivers within 2s, got {}",
            expected_receivers,
            bus.receiver_count()
        );
    }

    /// Poll a condition until it holds (2s budget).
    async fn wait_until(cond: impl Fn() -> bool) {
        for _ in 0..200 {
            if cond() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("condition not reached within budget");
    }

    fn node(id: &str, node_type: &str, inner: Value) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner,
        }
    }

    fn edge(source: &str, target: &str) -> WorkflowEdge {
        WorkflowEdge {
            id: format!("{}-{}", source, target),
            source_node_id: source.to_string(),
            target_node_id: target.to_string(),
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
        }
    }

    fn workflow_options() -> WorkflowExecutionOptions {
        WorkflowExecutionOptions {
            input: None,
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: Some(false),
            node_timeout: None,
            max_pause_duration: None,
            retry_budget: None,
            on_failure: None,
            max_retries: None,
            retry_delay_ms: None,
            exponential_backoff: None,
            fallback_output: None,
        }
    }

    #[tokio::test]
    async fn context_compression_chain_end_to_end() {
        // 1. Components: bus + predefined resources.
        let bus = Arc::new(EventBus::new(256));
        let mut sub = bus.subscribe();

        let registries = Arc::new(Registries::new());
        let opts = Options::default();
        wf_resource::predefined::triggers::register(&registries, &opts);
        wf_resource::predefined::workflows::register(&registries, &opts);

        // 2. Mock LLM: "main" for the emitting node, "DEFAULT" for the
        // llm_summary_workflow node.
        let gateway = Arc::new(LlmGateway::new());
        let main_mock = Arc::new(MockLlmClient::new());
        main_mock.default(LlmResponseSpec::text("main answer").with_usage(100, 20));
        gateway.register_mock("main", main_mock);
        let summary_mock = Arc::new(MockLlmClient::new());
        summary_mock.default(LlmResponseSpec::text("compressed summary").with_usage(50, 30));
        gateway.register_mock("DEFAULT", summary_mock.clone());

        // 3. Start the event-driven trigger listener.
        let contexts = Arc::new(ExecutionContextRegistry::new());
        let listener = start_trigger_listener(
            bus.clone(),
            registries.clone(),
            gateway.clone(),
            contexts.clone(),
        );
        wait_for_listener(&bus, 2).await;

        // 4. Main workflow: an LLM node reading the "chat" named context
        // whose estimated token count exceeds the node-level limit.
        let execution_id = wf_common::generate_id();
        let variables = Arc::new(DashMap::new());
        let mut chat_messages = Vec::new();
        for i in 0..40 {
            chat_messages.push(text_message(
                MessageRole::User,
                &format!("long message {} {}", i, "x".repeat(200)),
            ));
        }
        wf_workflow::append_context(&variables, "chat", chat_messages);
        contexts.register_workflow(execution_id.clone(), variables.clone());

        let graph = WorkflowGraphStructure {
            nodes: vec![
                node("start", "START", Value::Null),
                node(
                    "llm",
                    "LLM",
                    serde_json::json!({
                        "profile_id": "main",
                        "context_id": "chat",
                        "token_limit": 1000,
                        "output_context": "chat_output",
                    }),
                ),
                node("end", "END", Value::Null),
            ],
            edges: vec![edge("start", "llm"), edge("llm", "end")],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        };
        let handlers = wf_workflow::create_default_handlers(gateway.clone());
        let exec_ctx = ExecutorContext::new(
            execution_id.clone(),
            wf_common::generate_id(),
            Some(bus.clone()),
            Arc::new(wf_tools::create_default_tool_registry()),
            workflow_options(),
        );
        let mut exec_ctx = exec_ctx;
        exec_ctx.variables = variables.clone();
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, handlers)
            .unwrap()
            .with_entity(entity);
        assert!(coordinator.execute().await.is_ok());

        // 5a. CONTEXT_COMPRESSION_REQUESTED names the "chat" array and
        // carries its message snapshot.
        let requested = loop {
            match sub.recv().await {
                Ok(event) if event.r#type == EventType::ContextCompressionRequested => break event,
                Ok(_) => continue,
                Err(_) => panic!("event bus closed"),
            }
        };
        assert_eq!(
            requested.execution_id.as_deref(),
            Some(execution_id.as_str())
        );
        let requested_meta = wf_llm::ContextCompressionRequestedMeta::try_from(&requested).unwrap();
        assert_eq!(requested_meta.target_context_id, "chat");
        assert_eq!(
            requested_meta.messages.len(),
            40,
            "event must carry the array snapshot"
        );

        // 5b. The summary workflow ran over the conversation payload.
        wait_until(|| summary_mock.recorded_count() >= 1).await;
        let summary_request = summary_mock.last_request().unwrap();
        assert!(
            summary_request.messages.len() >= 40,
            "summary workflow must receive the full conversation"
        );

        // 5c. The compressed array was written back to the named context.
        wait_until(|| {
            let written = wf_workflow::get_context(&variables, "chat");
            written.len() == 1
        })
        .await;
        let written = wf_workflow::get_context(&variables, "chat");
        assert_eq!(written[0].role, MessageRole::Assistant);
        assert_eq!(
            written[0].content,
            MessageContentValue::Text("compressed summary".to_string())
        );

        // 5d. CONTEXT_COMPRESSION_COMPLETED carries the compressed array.
        let completed = loop {
            match sub.recv().await {
                Ok(event) if event.r#type == EventType::ContextCompressionCompleted => break event,
                Ok(_) => continue,
                Err(_) => panic!("event bus closed"),
            }
        };
        let completed_meta = wf_llm::ContextCompressionCompletedMeta::try_from(&completed).unwrap();
        assert_eq!(completed_meta.target_context_id, "chat");
        assert_eq!(completed_meta.messages.len(), 1);
        assert_eq!(
            completed_meta.messages[0].content,
            MessageContentValue::Text("compressed summary".to_string())
        );
        assert!(
            completed_meta.tokens_after < 1000,
            "compressed array must be far below the limit"
        );

        contexts.unregister(&execution_id);
        stop_trigger_listener(listener).await;
    }

    #[tokio::test]
    async fn no_compression_event_when_named_array_within_limit() {
        let bus = Arc::new(EventBus::new(64));
        let mut sub = bus.subscribe();

        let registries = Arc::new(Registries::new());
        let opts = Options::default();
        wf_resource::predefined::triggers::register(&registries, &opts);
        wf_resource::predefined::workflows::register(&registries, &opts);

        let gateway = Arc::new(LlmGateway::new());
        let main_mock = Arc::new(MockLlmClient::new());
        main_mock.default(LlmResponseSpec::text("main answer").with_usage(100, 20));
        gateway.register_mock("main", main_mock);
        let summary_mock = Arc::new(MockLlmClient::new());
        summary_mock.default(LlmResponseSpec::text("compressed summary"));
        gateway.register_mock("DEFAULT", summary_mock.clone());

        let contexts = Arc::new(ExecutionContextRegistry::new());
        let listener = start_trigger_listener(
            bus.clone(),
            registries.clone(),
            gateway.clone(),
            contexts.clone(),
        );
        wait_for_listener(&bus, 2).await;

        // A short array stays within the limit: no compression requested.
        let execution_id = wf_common::generate_id();
        let variables = Arc::new(DashMap::new());
        wf_workflow::append_context(
            &variables,
            "chat",
            vec![text_message(MessageRole::User, "short")],
        );
        contexts.register_workflow(execution_id.clone(), variables.clone());

        let graph = WorkflowGraphStructure {
            nodes: vec![
                node("start", "START", Value::Null),
                node(
                    "llm",
                    "LLM",
                    serde_json::json!({
                        "profile_id": "main",
                        "context_id": "chat",
                        "token_limit": 1000,
                        "output_context": "chat_output",
                    }),
                ),
                node("end", "END", Value::Null),
            ],
            edges: vec![edge("start", "llm"), edge("llm", "end")],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        };
        let handlers = wf_workflow::create_default_handlers(gateway.clone());
        let exec_ctx = ExecutorContext::new(
            execution_id.clone(),
            wf_common::generate_id(),
            Some(bus.clone()),
            Arc::new(wf_tools::create_default_tool_registry()),
            workflow_options(),
        );
        let mut exec_ctx = exec_ctx;
        exec_ctx.variables = variables.clone();
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, handlers)
            .unwrap()
            .with_entity(entity);
        assert!(coordinator.execute().await.is_ok());

        // No compression request within the observation window.
        let compression_observed = tokio::time::timeout(Duration::from_millis(300), async {
            loop {
                match sub.recv().await {
                    Ok(event) if event.r#type == EventType::ContextCompressionRequested => {
                        return true
                    }
                    Ok(_) => continue,
                    Err(_) => return false,
                }
            }
        })
        .await
        .unwrap_or(false);
        assert!(
            !compression_observed,
            "in-limit array must not trigger compression"
        );
        assert_eq!(summary_mock.recorded_count(), 0);

        contexts.unregister(&execution_id);
        stop_trigger_listener(listener).await;
    }

    fn template_with_nodes() -> WorkflowTemplate {
        use wf_types::node::BaseStaticNode;
        use wf_types::workflow::{
            Edge, TriggeredSubworkflowConfig, WorkflowDefinition, WorkflowMetadata,
        };
        WorkflowTemplate {
            id: "t_flow".to_string(),
            name: "T Flow".to_string(),
            description: "test".to_string(),
            definition: WorkflowDefinition {
                id: "t_flow".to_string(),
                name: "T Flow".to_string(),
                description: Some("test".to_string()),
                r#type: None,
                version: None,
                nodes: vec![
                    BaseStaticNode {
                        id: "start".into(),
                        node_type: StaticNodeType::StartFromTrigger,
                        name: Some("Start".into()),
                        description: None,
                        config: None,
                        execution_config: None,
                    },
                    BaseStaticNode {
                        id: "llm".into(),
                        node_type: StaticNodeType::Llm,
                        name: Some("LLM".into()),
                        description: None,
                        config: None,
                        execution_config: None,
                    },
                    BaseStaticNode {
                        id: "end".into(),
                        node_type: StaticNodeType::ContinueFromTrigger,
                        name: Some("End".into()),
                        description: None,
                        config: None,
                        execution_config: None,
                    },
                ],
                edges: vec![
                    Edge {
                        id: "e1".into(),
                        source_node_id: "start".into(),
                        target_node_id: "llm".into(),
                        r#type: EdgeType::Default,
                        condition: None,
                        label: None,
                        description: None,
                        weight: None,
                        metadata: None,
                    },
                    Edge {
                        id: "e2".into(),
                        source_node_id: "llm".into(),
                        target_node_id: "end".into(),
                        r#type: EdgeType::Default,
                        condition: None,
                        label: None,
                        description: None,
                        weight: None,
                        metadata: None,
                    },
                ],
                config: None,
                variables: None,
                triggers: None,
                triggered_subworkflow_config: Some(TriggeredSubworkflowConfig {
                    enable_checkpoints: Some(false),
                    timeout: Some(5000),
                    max_retries: Some(0),
                }),
                metadata: Some(WorkflowMetadata {
                    author: None,
                    tags: None,
                    category: None,
                }),
                available_tools: None,
                created_at: 0,
                updated_at: 0,
            },
            template_category: None,
            template_tags: None,
            is_public: None,
            enabled: None,
        }
    }

    #[test]
    fn template_to_graph_maps_nodes_edges_and_endpoints() {
        let template = template_with_nodes();
        let graph = template_to_graph(&template);

        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.nodes[0].node_type, "START_FROM_TRIGGER");
        assert_eq!(graph.nodes[1].node_type, "LLM");
        assert_eq!(graph.nodes[2].node_type, "CONTINUE_FROM_TRIGGER");
        assert_eq!(graph.start_node_id.as_deref(), Some("start"));
        assert_eq!(graph.end_node_ids, vec!["end".to_string()]);
        assert_eq!(graph.edges.len(), 2);
        assert_eq!(graph.edges[0].source_node_id, "start");
        assert_eq!(graph.edges[1].target_node_id, "end");
    }

    #[tokio::test]
    async fn execution_context_registry_writes_back_to_registered_execution() {
        use wf_workflow::execution_context::WriteBackError;

        let registry = ExecutionContextRegistry::new();
        assert!(!registry.registered("exec-1"));
        assert!(matches!(
            registry.write_context("exec-1", "chat", vec![], 0).await,
            Err(WriteBackError::NotRegistered)
        ));

        let variables = Arc::new(DashMap::new());
        registry.register_workflow("exec-1", variables.clone());
        assert!(registry.registered("exec-1"));

        let msg = Message {
            id: wf_common::generate_id(),
            role: wf_types::message::MessageRole::Assistant,
            content: wf_types::message::MessageContentValue::Text("summary".to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };
        // An array the execution never created is not writable.
        assert!(matches!(
            registry
                .write_context("exec-1", "chat", vec![msg.clone()], 0)
                .await,
            Err(WriteBackError::ContextNotFound)
        ));
        // Versioned write-back of a tracked array succeeds.
        wf_workflow::append_context(&variables, "chat", vec![msg.clone()]);
        let version = wf_workflow::message_context::array_version(&variables, "chat");
        assert!(registry
            .write_context("exec-1", "chat", vec![msg.clone()], version)
            .await
            .is_ok());
        let written = wf_workflow::get_context(&variables, "chat");
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].content, msg.content);

        registry.unregister("exec-1");
        assert!(!registry.registered("exec-1"));
        assert!(matches!(
            registry.write_context("exec-1", "chat", vec![], 0).await,
            Err(WriteBackError::NotRegistered)
        ));
    }

    #[tokio::test]
    async fn versioned_write_back_discards_stale_compression() {
        use wf_workflow::execution_context::WriteBackError;

        let registry = ExecutionContextRegistry::new();
        let variables = Arc::new(DashMap::new());
        wf_workflow::append_context(
            &variables,
            "chat",
            vec![Message {
                id: wf_common::generate_id(),
                role: wf_types::message::MessageRole::User,
                content: wf_types::message::MessageContentValue::Text("old".to_string()),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            }],
        );
        registry.register_workflow("exec-2", variables.clone());
        let emitted_version = wf_workflow::message_context::array_version(&variables, "chat");
        assert!(emitted_version > 0);

        // New messages appended after the event was emitted: the array moved
        // past the event version, the compressed result must be discarded.
        wf_workflow::append_context(
            &variables,
            "chat",
            vec![Message {
                id: wf_common::generate_id(),
                role: wf_types::message::MessageRole::User,
                content: wf_types::message::MessageContentValue::Text("newer".to_string()),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            }],
        );
        assert!(matches!(
            registry
                .write_context(
                    "exec-2",
                    "chat",
                    vec![Message {
                        id: wf_common::generate_id(),
                        role: wf_types::message::MessageRole::Assistant,
                        content: wf_types::message::MessageContentValue::Text(
                            "summary".to_string()
                        ),
                        timestamp: wf_common::now(),
                        tool_call_id: None,
                        tool_name: None,
                        tool_calls: None,
                        thinking: None,
                        metadata: None,
                    }],
                    emitted_version,
                )
                .await,
            Err(WriteBackError::VersionMismatch { .. })
        ));
        assert_eq!(wf_workflow::get_context(&variables, "chat").len(), 2);

        // At the current version the write-back succeeds.
        let current = wf_workflow::message_context::array_version(&variables, "chat");
        assert!(registry
            .write_context(
                "exec-2",
                "chat",
                vec![Message {
                    id: wf_common::generate_id(),
                    role: wf_types::message::MessageRole::Assistant,
                    content: wf_types::message::MessageContentValue::Text("summary".to_string()),
                    timestamp: wf_common::now(),
                    tool_call_id: None,
                    tool_name: None,
                    tool_calls: None,
                    thinking: None,
                    metadata: None,
                }],
                current,
            )
            .await
            .is_ok());
        assert_eq!(wf_workflow::get_context(&variables, "chat").len(), 1);
    }
}
