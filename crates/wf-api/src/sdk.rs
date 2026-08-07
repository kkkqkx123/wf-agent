use std::sync::{Arc, RwLock};

use wf_resource::registrar::Registries;
use wf_resource::starter::BundleRegistry;
use wf_storage::context::StorageContext;

use crate::context::ApiContext;
use crate::error::ApiResult;

/// Lifecycle state of an [`SDKInstance`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdkLifecycleState {
    /// Created but `wait_for_ready` has not completed yet.
    Creating,
    /// Ready to serve API calls.
    Ready,
    /// Shutting down (final flush in progress).
    ShuttingDown,
    /// Fully destroyed; API calls are rejected.
    Destroyed,
}

/// Application-facing SDK entry (TS `SDKInstance` + `createSDK` counterpart).
///
/// Wraps an [`ApiContext`] (the shared storage / registries / engines /
/// persistence pieces) and exposes lifecycle management plus a unified API
/// slot facade ([`AllApis`]).
pub struct SDKInstance {
    ctx: Arc<ApiContext>,
    state: RwLock<SdkLifecycleState>,
}

/// Create a standalone SDK instance over fresh storage / registries.
pub fn create_sdk(
    storage: StorageContext,
    registries: Arc<Registries>,
    bundles: Arc<BundleRegistry>,
) -> SDKInstance {
    let ctx = ApiContext::new(storage, registries, bundles);
    SDKInstance::from_context(Arc::new(ctx))
}

impl SDKInstance {
    /// Build an SDK instance over an existing context (e.g. the runtime's
    /// shared `ApiContext`).
    pub fn from_context(ctx: Arc<ApiContext>) -> Self {
        Self {
            ctx,
            state: RwLock::new(SdkLifecycleState::Creating),
        }
    }

    /// The underlying API context (shared with the runtime when constructed
    /// from `wf-runtime`).
    pub fn context(&self) -> &Arc<ApiContext> {
        &self.ctx
    }

    /// Current lifecycle state.
    pub fn state(&self) -> SdkLifecycleState {
        *self.state.read().expect("sdk state lock poisoned")
    }

    pub fn is_ready(&self) -> bool {
        self.state() == SdkLifecycleState::Ready
    }

    /// Complete initialization (opens the persistence backend and starts the
    /// buffered flush task) and mark the instance ready.
    pub async fn wait_for_ready(&self) -> ApiResult<()> {
        if self.is_ready() {
            return Ok(());
        }
        self.ctx.persistence.initialize().await?;
        *self.state.write().expect("sdk state lock poisoned") = SdkLifecycleState::Ready;
        Ok(())
    }

    /// Graceful shutdown: flush pending persistence writes and close the
    /// backend.
    pub async fn shutdown(&self) -> ApiResult<()> {
        if self.state() == SdkLifecycleState::Destroyed {
            return Ok(());
        }
        *self.state.write().expect("sdk state lock poisoned") = SdkLifecycleState::ShuttingDown;
        self.ctx.persistence.shutdown().await?;
        *self.state.write().expect("sdk state lock poisoned") = SdkLifecycleState::Ready;
        Ok(())
    }

    /// Full teardown; the instance cannot be reused afterwards.
    pub async fn destroy(&self) -> ApiResult<()> {
        self.shutdown().await?;
        *self.state.write().expect("sdk state lock poisoned") = SdkLifecycleState::Destroyed;
        Ok(())
    }

    /// Unified API slot facade over the underlying context.
    pub fn apis(&self) -> AllApis {
        AllApis {
            ctx: self.ctx.clone(),
        }
    }
}

/// Unified API slot entry (TS `AllAPIs` 38-slot facade, converged onto the
/// current `wf-api` surface).
///
/// Accessors return per-domain API structs sharing one [`ApiContext`]; the
/// structs are cheap wrappers so every accessor is `O(1)`.
pub struct AllApis {
    ctx: Arc<ApiContext>,
}

impl AllApis {
    /// Workflow definition CRUD + execution entry.
    pub fn workflows(&self) -> crate::workflow_execution::WorkflowApi {
        crate::workflow_execution::WorkflowApi::new(self.ctx.clone())
    }

    /// Workflow execution registry queries (history / search input).
    pub fn executions(&self) -> ExecutionRegistryApi {
        ExecutionRegistryApi::new(self.ctx.clone())
    }

    /// Agent loop / execution entry.
    pub fn agents(&self) -> crate::agent_execution::AgentApi {
        crate::agent_execution::AgentApi::new(self.ctx.clone())
    }

    /// Tool execution + management.
    pub fn tools(&self) -> crate::tool::ToolApi {
        crate::tool::ToolApi::new(self.ctx.clone())
    }

    /// Script execution + validation.
    pub fn scripts(&self) -> crate::script::ScriptApi {
        crate::script::ScriptApi::new(self.ctx.clone())
    }

    /// Direct LLM generation.
    pub fn llm(&self) -> crate::llm::LlmApi {
        crate::llm::LlmApi::new(self.ctx.clone())
    }

    /// LLM profile registry.
    pub fn profiles(&self) -> crate::llm_profile::LlmProfileApi {
        crate::llm_profile::LlmProfileApi::new(self.ctx.clone())
    }

    /// Event history / timeline / stats / subscription.
    pub fn events(&self) -> crate::events::EventApi {
        crate::events::EventApi::new(self.ctx.clone())
    }

    /// Tool-approval / follow-up interaction coordinator.
    pub fn approvals(&self) -> crate::approval::ApprovalCoordinator {
        crate::approval::ApprovalCoordinator::new(self.ctx.clone())
    }

    /// User interaction registry + handler registration.
    pub fn user_interactions(&self) -> crate::user_interaction::UserInteractionApi {
        crate::user_interaction::UserInteractionApi::new(self.ctx.clone())
    }

    /// Variable registry.
    pub fn variables(&self) -> crate::variable::VariableApi {
        crate::variable::VariableApi::new(self.ctx.clone())
    }

    /// Message registry.
    pub fn messages(&self) -> crate::message::MessageApi {
        crate::message::MessageApi::new(self.ctx.clone())
    }

    /// Skill registry.
    pub fn skills(&self) -> crate::skill::SkillApi {
        crate::skill::SkillApi::new(self.ctx.clone())
    }

    /// Storage diagnostics.
    pub fn diagnostics(&self) -> crate::diagnostics::StorageDiagnosticsApi {
        crate::diagnostics::StorageDiagnosticsApi::new(self.ctx.clone())
    }

    /// Unified search across resource types.
    pub fn search(&self) -> crate::search::Searcher {
        crate::search::Searcher::new(self.ctx.clone())
    }

    /// Workflow execution graph analysis.
    pub fn graphs(&self) -> crate::execution_graph::ExecutionGraphApi {
        crate::execution_graph::ExecutionGraphApi::new(self.ctx.clone())
    }

    /// Workflow execution state / context snapshots.
    pub fn execution_state(&self) -> crate::execution_state::WorkflowExecutionStateApi {
        crate::execution_state::WorkflowExecutionStateApi::new(self.ctx.clone())
    }

    /// Agent execution state.
    pub fn agent_execution_state(&self) -> crate::execution_state::AgentExecutionStateApi {
        crate::execution_state::AgentExecutionStateApi::new(self.ctx.clone())
    }

    /// Workflow error analysis.
    pub fn error_analysis(&self) -> crate::error_analysis::ErrorAnalysisApi {
        crate::error_analysis::ErrorAnalysisApi::new(self.ctx.clone())
    }

    /// Workflow performance analysis.
    pub fn performance(&self) -> crate::performance::PerformanceApi {
        crate::performance::PerformanceApi::new(self.ctx.clone())
    }

    /// Workflow iteration analysis (node-level).
    pub fn workflow_iteration(&self) -> crate::workflow_iteration::WorkflowIterationAnalysisApi {
        crate::workflow_iteration::WorkflowIterationAnalysisApi::new(self.ctx.clone())
    }

    /// Template library (workflow / trigger / node templates).
    pub fn templates(&self) -> crate::template_library::TemplateLibraryApi {
        crate::template_library::TemplateLibraryApi::new(self.ctx.clone())
    }

    /// Trigger execution dispatcher.
    pub fn execution_triggers(&self) -> crate::execution_trigger::ExecutionTriggerApi {
        crate::execution_trigger::ExecutionTriggerApi::new(self.ctx.clone())
    }

    /// Agent loop registry (summary / status / history).
    pub fn agent_loops(&self) -> crate::agent_loop_registry::AgentLoopRegistryApi {
        crate::agent_loop_registry::AgentLoopRegistryApi::new(self.ctx.clone())
    }

    /// Agent loop checkpoints.
    pub fn agent_checkpoints(&self) -> crate::agent_checkpoint::AgentLoopCheckpointApi {
        crate::agent_checkpoint::AgentLoopCheckpointApi::new(self.ctx.clone())
    }

    /// Agent loop messages.
    pub fn agent_messages(&self) -> crate::agent_message::AgentLoopMessageApi {
        crate::agent_message::AgentLoopMessageApi::new(self.ctx.clone())
    }

    /// Agent iteration analysis.
    pub fn agent_iteration(&self) -> crate::iteration::IterationApi {
        crate::iteration::IterationApi::new(self.ctx.clone())
    }

    /// Agent variables.
    pub fn agent_variables(&self) -> crate::agent_variable::AgentVariableApi {
        crate::agent_variable::AgentVariableApi::new(self.ctx.clone())
    }

    /// Agent user interactions.
    pub fn agent_user_interactions(&self) -> crate::agent_user_interaction::AgentUserInteractionApi {
        crate::agent_user_interaction::AgentUserInteractionApi::new(self.ctx.clone())
    }

    /// Agent error analysis.
    pub fn agent_error_analysis(&self) -> crate::agent_error_analysis::AgentErrorAnalysisApi {
        crate::agent_error_analysis::AgentErrorAnalysisApi::new(self.ctx.clone())
    }

    /// Agent performance analysis.
    pub fn agent_performance(&self) -> crate::agent_performance::AgentPerformanceAnalysisApi {
        crate::agent_performance::AgentPerformanceAnalysisApi::new(self.ctx.clone())
    }

    /// Agent execution registry.
    pub fn agent_execution_registry(&self) -> crate::agent_execution_registry::AgentExecutionRegistryApi {
        crate::agent_execution_registry::AgentExecutionRegistryApi::new(self.ctx.clone())
    }

    /// Agent decision graph analysis.
    pub fn agent_graphs(&self) -> crate::agent_graph::AgentGraphApi {
        crate::agent_graph::AgentGraphApi::new(self.ctx.clone())
    }

    /// Agent triggers.
    pub fn agent_triggers(&self) -> crate::agent_trigger::AgentTriggerApi {
        crate::agent_trigger::AgentTriggerApi::new(self.ctx.clone())
    }

    /// Agent trigger templates.
    pub fn agent_trigger_templates(&self) -> crate::agent_trigger_template::AgentTriggerTemplateRegistryApi {
        crate::agent_trigger_template::AgentTriggerTemplateRegistryApi::new(self.ctx.clone())
    }

    /// Agent hook templates.
    pub fn agent_hook_templates(&self) -> crate::agent_hook_template::AgentHookTemplateRegistryApi {
        crate::agent_hook_template::AgentHookTemplateRegistryApi::new(self.ctx.clone())
    }

    /// Agent templates.
    pub fn agent_templates(&self) -> crate::agent_template::AgentTemplateRegistryApi {
        crate::agent_template::AgentTemplateRegistryApi::new(self.ctx.clone())
    }

    /// Config parsing / validation.
    pub fn config(&self) -> crate::config::ConfigApi {
        crate::config::ConfigApi
    }

    /// Task CRUD + stats + cleanup.
    pub async fn list_tasks(
        &self,
        options: Option<wf_storage::adapter::task::TaskListOptions>,
    ) -> ApiResult<Vec<wf_types::TaskStorageMetadata>> {
        crate::task::list_tasks(&self.ctx.storage, options).await
    }

    /// Node template CRUD.
    pub async fn list_node_templates(
        &self,
        options: Option<wf_storage::adapter::node_template::NodeTemplateListOptions>,
    ) -> ApiResult<Vec<wf_types::NodeTemplateStorageMetadata>> {
        crate::node_template::list_node_templates(&self.ctx.storage, options).await
    }

    /// The raw persistence layer (events / snapshots / metrics).
    pub fn persistence(&self) -> Arc<dyn crate::persistence::PersistenceLayer> {
        self.ctx.persistence.clone()
    }
}

/// Workflow execution registry query facade (history, status, cleanup).
pub struct ExecutionRegistryApi {
    ctx: Arc<ApiContext>,
}

impl ExecutionRegistryApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    pub async fn list(
        &self,
        options: Option<wf_storage::adapter::execution::WorkflowExecutionListOptions>,
    ) -> ApiResult<Vec<wf_types::WorkflowExecution>> {
        crate::workflow::list_executions(&self.ctx, options).await
    }

    pub async fn get(&self, id: &str) -> ApiResult<wf_types::WorkflowExecution> {
        crate::workflow::get_execution(&self.ctx, id).await
    }

    pub async fn delete(&self, id: &str) -> ApiResult<bool> {
        crate::workflow::delete_execution(&self.ctx, id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lifecycle_waits_ready_and_destroys() {
        let sdk = create_sdk(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        );
        assert_eq!(sdk.state(), SdkLifecycleState::Creating);
        assert!(!sdk.is_ready());

        sdk.wait_for_ready().await.unwrap();
        assert_eq!(sdk.state(), SdkLifecycleState::Ready);
        assert!(sdk.is_ready());

        sdk.destroy().await.unwrap();
        assert_eq!(sdk.state(), SdkLifecycleState::Destroyed);
        assert!(!sdk.is_ready());
    }

    #[tokio::test]
    async fn apis_facade_exposes_unified_slots() {
        let sdk = create_sdk(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        );
        sdk.wait_for_ready().await.unwrap();
        let apis = sdk.apis();

        // Every slot constructs over the shared context and stays functional.
        apis.workflows();
        apis.events().history(&crate::events::EventQueryOptions::default()).await.unwrap();
        apis.scripts();
        apis.profiles();
        apis.agent_loops();
        apis.agent_execution_registry();
        apis.agent_graphs();
    }

    #[tokio::test]
    async fn buffered_persistence_wires_through_sdk() {
        let sdk = create_sdk(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        );
        sdk.wait_for_ready().await.unwrap();

        let event = wf_types::events::BaseEvent {
            id: wf_common::generate_id(),
            r#type: wf_types::events::EventType::Heartbeat,
            timestamp: 1,
            workflow_id: None,
            execution_id: None,
            agent_loop_id: None,
            metadata: None,
        };
        sdk.apis().events().dispatch(event).await.unwrap();

        let history = sdk
            .apis()
            .events()
            .history(&crate::events::EventQueryOptions::default())
            .await
            .unwrap();
        assert_eq!(history.len(), 1);
    }
}
