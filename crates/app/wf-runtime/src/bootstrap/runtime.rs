use std::sync::Arc;

use wf_core::event::EventBus;
use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_llm::LlmGateway;
use wf_resource::registry::ResourceRegistries;
use wf_workflow::trigger::TriggerEventListener;

use crate::error::RuntimeResult;
use crate::lifecycle::{ShutdownHandle, ShutdownWaiter};
use crate::metrics::MetricsContext;
use crate::mode::ModeInfo;
use crate::storage_manager::StorageManager;
use crate::trigger_listener::{ExecutionContextRegistry, TimerBindingRegistry};

pub struct Runtime {
    pub storage_manager: StorageManager,
    pub mode_info: ModeInfo,
    pub shutdown_handle: ShutdownHandle,
    pub _shutdown_waiter: ShutdownWaiter,
    pub registries: Arc<ResourceRegistries>,
    pub skill_loader: Arc<wf_tools::SkillLoader>,
    /// Shared tool registry (builtin handlers + skill loader + MCP tools);
    /// injected into every execution through the trigger listener.
    pub tool_registry: Arc<wf_tools::registry::ToolRegistry>,
    /// Shared MCP connection manager; `None` when MCP is not configured.
    pub mcp_manager: Option<Arc<wf_tools::mcp::connection::McpConnectionManager>>,
    pub event_bus: Arc<EventBus>,
    pub metrics: Option<Arc<MetricsContext>>,
    pub llm_gateway: Arc<LlmGateway>,
    /// Shared sandbox runtime: global profiles and routing rules compiled
    /// at bootstrap, injected into every script handler.
    pub sandbox_runtime: Arc<wf_sandbox::SandboxRuntime>,
    /// Variable maps of live workflow executions (write-back target of the
    /// event-driven context compression chain).
    pub execution_contexts: Arc<ExecutionContextRegistry>,
    /// Background event-driven trigger listener (context compression).
    pub trigger_listener: Option<Arc<TriggerEventListener>>,
    pub(super) trigger_listener_shutdown: Option<tokio_util::sync::CancellationToken>,
    pub(super) trigger_listener_handle: Option<tokio::task::JoinHandle<()>>,
    /// Trigger runtime state registry: the listener records fired triggers
    /// here and checkpoints capture them as the `trigger_states` audit trail.
    pub trigger_state_registry: Arc<wf_workflow::TriggerStateRegistry>,
    /// Runtime mount table of execution-scoped schedule timers: whoever owns
    /// a timed execution binds its schedule name here so scheduler ticks name
    /// the live execution. Shared with the scheduler background task.
    pub timer_bindings: Arc<TimerBindingRegistry>,
    /// Shared hook handler registry: engine hook points and signals
    /// (context compression) fire through it; the compression service is
    /// registered on the `CONTEXT_COMPRESSION_REQUESTED` signal point.
    pub hook_handler_registry: Arc<HookHandlerRegistry>,
    /// Shared agent loop registry of the composite execution callback;
    /// injected into the API context so tool-dispatched executions appear in
    /// the server execution views.
    pub(super) agent_registry: std::sync::Arc<wf_agent::registry::AgentLoopRegistry>,
    #[cfg(feature = "plugins")]
    pub plugin_engine: Option<wf_plugin::PluginEngine>,
    /// Durable event persistence backend (buffered + store mirroring the
    /// runtime storage). `None` keeps events in memory only.
    pub(super) event_persistence: Option<Arc<dyn wf_api::PersistenceLayer>>,
    /// Durable checkpoint store backend (crash recovery): execution
    /// checkpoints written through `ApiContext::checkpoint_store` land in the
    /// same backend as the runtime storage. Falls back to in-memory when
    /// storage is memory-only or the backend cannot be opened.
    pub(super) checkpoint_store: Arc<wf_storage::backend::StorageBackend>,
    /// Lazily-created application-facing API context; shared so live execution
    /// handles (pause/resume/cancel) stay valid across calls.
    pub(super) api_ctx: std::sync::OnceLock<std::sync::Arc<wf_api::ApiContext>>,
    /// File checkpoint manager (layertwine-backed): execution file snapshots
    /// are created/restored through it and script handlers capture workspace
    /// changes when it is attached. `None` keeps file checkpointing disabled.
    pub(super) file_checkpoint_manager: Option<checkpoint_file::file::FileCheckpointManager>,
    /// Host default tool approval configuration, applied to the API context
    /// so executions launched through it route tool calls through the
    /// persisted interaction flow when enabled.
    pub(super) tool_approval: wf_types::config::tool_approval::ToolApprovalConfig,
    /// Resolved infrastructure values retained from bootstrap for downstream
    /// readers. `limits` drives the agent executor and trigger subsystem;
    /// `output`, `presets` and `tools` are resolved (file layer or
    /// programmatic) and retained here so hosts can observe the effective
    /// configuration.
    pub output: wf_types::config::output::OutputConfig,
    pub presets: wf_types::config::presets::PresetsConfig,
    pub tools: wf_config::orchestrator::ToolConfigs,
    pub limits: wf_types::config::limits::LimitsConfig,
    /// Manual change service: watches the workspace root and routes
    /// human/external file edits into the manual partition. Started when
    /// file checkpointing is enabled with a workspace root and `manual_watch`.
    pub(super) manual_change_service: Option<checkpoint_file::watcher::ManualChangeService>,
    /// Forwarder task from the file-checkpoint event bus onto the shared
    /// event bus (CheckpointFileChanged / CheckpointMergeConflicted with the
    /// `DeltaSummary` payload). Kept alive for the runtime lifetime.
    pub(super) checkpoint_event_bridge_handle: Option<tokio::task::JoinHandle<()>>,
    /// Optional periodic GC timer that runs `FileCheckpointManager::run_gc`
    /// at the configured `gc_interval_secs` interval. `None` when periodic
    /// GC is disabled (explicit `run_gc` / API only).
    pub(super) gc_timer_handle: Option<tokio::task::JoinHandle<()>>,
    /// Supervised local code-context service for managed transport. Kept
    /// alive for the runtime lifetime and stopped during shutdown.
    pub(super) code_context_sidecar: Option<wf_integration::RunningSidecar>,
}

impl Runtime {
    pub fn registries(&self) -> &ResourceRegistries {
        &self.registries
    }

    /// The attached file checkpoint manager (layertwine-backed), when file
    /// checkpointing is enabled with a storage backend.
    pub fn file_checkpoint_manager(&self) -> Option<&checkpoint_file::file::FileCheckpointManager> {
        self.file_checkpoint_manager.as_ref()
    }

    /// Shared skill loader; skills are scanned from configured paths at bootstrap.
    pub fn skill_loader(&self) -> &Arc<wf_tools::SkillLoader> {
        &self.skill_loader
    }

    /// Shared tool registry; injected into executions started by this runtime.
    pub fn tool_registry(&self) -> &Arc<wf_tools::registry::ToolRegistry> {
        &self.tool_registry
    }

    /// Shared MCP connection manager, when MCP settings were configured.
    pub fn mcp_manager(&self) -> Option<&Arc<wf_tools::mcp::connection::McpConnectionManager>> {
        self.mcp_manager.as_ref()
    }

    pub fn storage(&self) -> &StorageManager {
        &self.storage_manager
    }

    /// Recover incomplete (running/paused/created) workflow and agent
    /// executions left by a previous process: scans the execution stores and
    /// restores the latest checkpoint of each one through the API resume
    /// path. Targets without a usable checkpoint are reported as skipped,
    /// never as spuriously recovered.
    ///
    /// Without a persistent checkpoint store executions are reported as
    /// skipped, never as spuriously recovered.
    pub async fn recover_incomplete_executions(
        &self,
    ) -> RuntimeResult<crate::recovery::RecoveryResult> {
        use crate::recovery::RecoveryOrchestrator;
        use crate::recovery::RecoveryScanner;

        let Some(storage) = self.storage_manager.shared_context() else {
            return Err(crate::error::RuntimeError::NotInitialized);
        };
        let scanner = RecoveryScanner::new(storage.workflow_execution.clone())
            .with_agent_store(storage.agent_execution.clone());
        let ctx = self.api_context();

        RecoveryOrchestrator::new(scanner)
            .with_recovery_executor(Arc::new(crate::recovery::ApiRecoveryExecutor))
            .recover_all(ctx)
            .await
    }

    pub fn mode(&self) -> &ModeInfo {
        &self.mode_info
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_handle.is_shutting_down()
    }

    pub fn trigger_shutdown(&self) {
        self.shutdown_handle.trigger();
    }

    /// Optional metrics system; absent when metrics are disabled.
    pub fn metrics(&self) -> Option<&Arc<MetricsContext>> {
        self.metrics.as_ref()
    }

    /// Shared LLM gateway; workflow and agent execution are injected with
    /// this instance so all LLM calls resolve profiles from one registry.
    pub fn llm_gateway(&self) -> &Arc<LlmGateway> {
        &self.llm_gateway
    }

    /// Shared sandbox runtime (global profiles + routing rules compiled at
    /// bootstrap). Injected into the script handlers of every workflow
    /// execution started by this runtime.
    pub fn sandbox_runtime(&self) -> &Arc<wf_sandbox::SandboxRuntime> {
        &self.sandbox_runtime
    }

    #[cfg(feature = "plugins")]
    pub fn plugin_engine(&self) -> Option<&wf_plugin::PluginEngine> {
        self.plugin_engine.as_ref()
    }
}
