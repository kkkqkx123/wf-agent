pub mod agent;
pub mod analysis;
pub mod audit;
pub mod builder;
pub mod checkpoint;
pub mod entity;
pub mod infra;
pub mod llm;
pub mod query;
pub mod template;
pub mod trigger;
pub mod workflow;

pub use agent::agent_checkpoint::AgentCheckpointStatistics;
pub use agent::agent_draft::{
    delete_draft as delete_agent_draft, get_draft as get_agent_draft,
    list_drafts as list_agent_drafts, promote_draft as promote_agent_draft,
    save_draft as save_agent_draft,
};
pub use agent::agent_error_analysis::{
    AdvancedErrorAnalysis, AgentErrorStatistics, ErrorRecoveryProposal, ExecutionErrorRecord,
    RootCauseAnalysis,
};
pub use agent::agent_execution_registry::{AgentExecutionFilter, AgentExecutionSummary};
pub use agent::agent_graph::{
    AgentAlternativeDecisionView, AgentChosenDecisionView, AgentDecisionEdgeView,
    AgentDecisionGraph, AgentDecisionGraphView, AgentDecisionNode, AgentDecisionNodeView,
    AgentDecisionPatternsView, AgentDecisionRecordView, AgentDecisionSequenceView,
    AgentEfficiencyAnalysis, AgentExecutionPathStepView, AgentExecutionPathView,
    AgentIterationAlternativesView, AgentPathProbabilityAnalysisView,
    AgentPathProbabilityEntryView, AgentPathStatisticsView, ToolCallView,
};
pub use agent::agent_loop_registry::ExecutionPath as AgentExecutionPath;
pub use agent::agent_loop_registry::VariableHistoryEntry as AgentVariableHistoryEntry;
pub use agent::agent_loop_registry::{
    AgentExecutionStatistics, AgentLoopFilter, AgentLoopStatistics, AgentLoopSummary,
    ContextEvolutionEntry, ExecutionTimelineEntry, ExecutionTimelineEntryType, IterationDetail,
    IterationHistorySummary, ToolCallInPath, VariableChange,
};
pub use agent::agent_message::AgentLoopMessageStats;
pub use agent::agent_performance::{
    AgentIterationTiming, AgentPerformanceProfile, IterationComparison,
};
pub use agent::agent_user_interaction::{AgentUserInteractionEventRecord, UserInteractionHandler};
pub use agent::agent_variable::AgentVariableStatistics;
pub use agent::validation::AgentValidator;
pub use analysis::error_analysis::{
    analyze_root_cause, error_context, error_context_chain, AdvancedWorkflowErrorAnalysis,
    ErrorContextView, ErrorRecommendation, ErrorSubscription, ProblematicNode, RecoveryProposal,
    SimilarErrorGroup, WorkflowErrorHotspot, WorkflowErrorStats, WorkflowNodeRef,
    WorkflowRootCauseAnalysis,
};
pub use analysis::llm_metrics::{agent_llm_metrics, flush, AgentLlmMetrics, ModelTokenUsage};
pub use analysis::performance::{
    ExecutionComparison, ExecutionPerformanceProfile, NodeComparisonView, NodeRef,
    NodeTimelineEntry, PerformanceBottleneckView, PerformanceSummaryView, PerformanceTier,
    WorkflowPerformanceProfile,
};
pub use analysis::progress::{
    format_progress, get_progress, ProgressEventType, ProgressMetrics, ProgressTracker,
};
pub use analysis::search::{SearchOptions, SearchResourceType, SearchResult, SearchResultItem};
pub use audit::{
    audit_report, audit_summary, audit_timeline, list_iterations, list_llm_calls,
    list_node_executions, list_tool_calls, AuditReport, AuditSource, AuditSummary,
    AuditTimelineEntry, AuditTimelineEntryType, IterationAuditView, LlmCallAuditView,
    NodeExecutionAuditView, ToolCallAuditView,
};
pub use builder::{
    AgentDefinitionBuilder, AgentExecutionBuilder, AgentHookBuilder, AgentLoopConfigBuilder,
    AgentToolConfigBuilder, ExecutionBuilder, ExecutionResult, NodeBuilder, NodeTemplateBuilder,
    TriggerTemplateBuilder, WorkflowBuilder,
};
pub use entity::message::{MessageOrder, MessageStats};
pub use entity::resource::ResourceApi;
pub use entity::skill::{SkillFilter, SkillResourceEntry};
pub use entity::variable::{VariableHistoryEntry, VariableStatistics};
pub use infra::context::ApiContext;
pub use infra::dependency::{
    audit_all_workflows, check_update_impact, find_dependents, request_async_revalidation,
    DependencyKind, DependentEntry, DependentImpact, ImpactLevel, UpdateImpactReport,
};
pub use infra::diagnostics::{StorageDiagnosticReport, StorageDiagnosticsReport, StoreDiagnostic};
pub use infra::error::{with_timeout, ApiError, ApiResult};
pub use infra::events::{
    event_history_size, event_system_health, event_time_range, execution_listener_stats,
    execution_timeline_summary, EventQueryOptions, EventStats, EventSystemHealth,
    ExecutionListenerStats, ExecutionTimeline, ExecutionTimelinePhase, ExecutionTimelineSummary,
};
pub use infra::handler_chain::{
    node_type_name, NoopPluginHandlerSource, PluginHandlerSource, PluginMiddlewareBridge,
    PluginNodeAdapter, PluginNodeExecutor, TemplateSubgraphHandler,
};
pub use infra::persistence::{
    BufferedPersistenceLayer, NoOpPersistenceLayer, PersistenceHealth, PersistenceLayer,
    StorePersistenceLayer,
};
pub use infra::reference::{DeleteReference, ReferenceKind};
pub use infra::state_tracker::{
    clear_state, get_call_stack, get_memory_usage, get_most_changed_variables,
    get_peak_memory_usage, get_state_at_iteration, get_variable_history,
    get_variable_mutation_count, get_variable_snapshot, list_state_records, ExecutionStateAccessor,
    ExecutionStateRecord, StatePoint,
};
pub use infra::subscription::{
    spawn_event_subscription, wait_for_event, EventSubscription, EventSubscriptionOptions,
};
pub use infra::validation::{ValidationContext, ValidationError, ValidationResult};
pub use llm::llm_profile::{LlmProfileFilter, LlmProfileTemplate, MASKED_API_KEY};
pub use llm::script::{ScriptExecuteParams, ScriptValidation};
pub use llm::tool::{ToolParameterValidation, ToolReference};
pub use query::{
    aggregate, apply_filter_expressions, evaluate_expression, evaluate_json_expression,
    export_to_csv, export_to_format, export_to_xml, get_distinct, get_field_value, group_by_field,
    json_field_value, query, AggregationOp, AggregationResult, AggregationType, ExecutionRecord,
    ExportFormat, FilterCriteria, FilterExpression, FilterOperator, PaginationOptions,
    QueryBuilder, SortOptions,
};
pub use template::agent_template::AgentTemplateFilter;
pub use template::agent_trigger_template::{
    AgentTriggerTemplateFilter, AgentTriggerTemplateSummary,
};
pub use template::node_template::NodeTemplateSummary;
pub use template::template_library::{TemplateFilter, TemplateKind, TemplateSummary};
pub use trigger::validation::TriggerValidator;
pub use workflow::approval::{ApprovalResult, ApprovalStatus};
pub use workflow::draft::{
    delete_draft as delete_workflow_draft, get_draft as get_workflow_draft, hot_reload_to_draft,
    lifecycle_of as workflow_lifecycle_of, list_drafts as list_workflow_drafts, promote_all_drafts,
    promote_draft as promote_workflow_draft, save_draft as save_workflow_draft,
    validate_draft_complete as validate_workflow_draft_complete,
    validate_draft_internal as validate_workflow_draft_internal,
    LifecycleStatus as WorkflowLifecycleStatus,
};
pub use workflow::execution_graph::{
    analyze_decision_points, enumerate_paths, reachable_nodes, AlternativeDecision, DecisionPoint,
    EfficiencyAnalysis, ExecutionPath, ExecutionPathAnalysis, PathProbabilityAnalysis,
    PathProbabilityEntry, SlowNodeView,
};
pub use workflow::execution_state::{
    AgentLoopStateView, CommonTransitionView, ContextEvolutionView, ContextStateTransitionView,
    ExecutionContextSnapshotView, IterationRecordView, NodeExecutionRecordView,
    StateTransitionView, ToolCallRecordView, VariableSnapshotView, VariableValueSnapshotView,
    WorkflowCallStackView, WorkflowExecutionStateView, WorkflowStackFrameView,
    WorkflowStateTransitionAnalysisView,
};
pub use workflow::graph_query::{
    execution_graph_edges, execution_graph_node_neighbors, execution_graph_nodes,
    get_execution_graph, get_execution_path_statistics, get_graph, graph_analysis,
    graph_detect_cycles, graph_edges, graph_node_neighbors, graph_nodes, graph_nodes_by_type,
    graph_reachability, graph_summary, graph_topological_sort, list_graph_workflows,
    ExecutionPathStatsView, GraphEdgeView, GraphNeighborsView, GraphNodeView, GraphSummary,
};
pub use workflow::iteration::{AgentIterationAnalysis, ToolCallStat};
pub use workflow::workflow_execution::{ExecutionSummary, RestoredCheckpoint};
pub use workflow::workflow_iteration::{
    ExecutionPathStepView, ExtendedNodeExecutionFilter, ExtendedNodeExecutionRecordView,
    NodeExecutionStats, OptimizationOpportunity, ToolDependencyView, WorkflowExecutionPathView,
};
pub use workflow::WorkflowSummary;

pub use wf_storage::adapter::base::ListOptions;
pub use wf_storage::adapter::execution::WorkflowExecutionListOptions;
pub use wf_storage::domain::QueryFilter;

pub use wf_agent::approval::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};
pub use wf_agent::stream::AgentStreamEvent;
pub use wf_config::parser as config_parser;
pub use wf_llm::{LlmError, LlmGateway};
pub use wf_storage::adapter::base::BaseStorageAdapter;
pub use wf_storage::adapter::task::TaskListOptions;
pub use wf_storage::adapter::variable::VariableListOptions;
pub use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, AgentLoopOutput};
pub use wf_workflow::analysis::{analyze_reachability, get_reachable_nodes};

// Engine types re-exported for downstream consumers (wf-server, wf-cli).
// These types are used in ApiContext fields or engine APIs; re-exporting them
// lets consumers work with engine types without adding direct engine deps.

// -- wf-execution-shared --
pub use wf_execution_shared::context::{
    ExecutorContext, NodeExecutionContext, NodeExecutionResult, NodeInputShape,
};
pub use wf_execution_shared::error::{ExecutionSharedError, ExecutionSharedResult};
pub use wf_execution_shared::execution_state::ExecutionStateManager;
pub use wf_execution_shared::fork::{BranchRecord, BranchStatus, ForkRegistry};
pub use wf_execution_shared::handler::{NodeHandler, NodeHandlerRegistry};
pub use wf_execution_shared::hooks::{
    dispatch, evaluate_hook_condition, filter_and_sort_hooks, publish_hook_audit_event,
    HookContext, HookOutcome, HookReceiver, HookRegistry, ReceiverResult,
};
pub use wf_execution_shared::types::execution_entity::{ExecutionEntity, ExecutionStatus};
pub use wf_execution_shared::types::state_manager::StateManager;

// -- wf-tools --
pub use wf_tools::error::{ToolError, ToolResult};
pub use wf_tools::executor::ToolExecutor;
pub use wf_tools::filesystem::{FsToolConfig, FsToolHandlers};
pub use wf_tools::general::{GeneralToolInvoker, GENERAL_TOOL_NAME};
pub use wf_tools::handlers::{
    create_default_tool_registry, register_builtin_handlers, BuiltinHandlersConfig,
};
pub use wf_tools::registry::ToolRegistry;
pub use wf_tools::skill::{SkillLoader, SkillResourceContent};
pub use wf_tools::tool_call::ToolCallExecutor;
pub use wf_tools::tool_description_generator::{
    discoverable_metadata_options, generate_discoverable_tool_entries,
    generate_discoverable_tool_entries_with_options, generate_discoverable_tools_metadata,
    generate_discoverable_tools_metadata_with_options, inject_discoverable_tools_metadata,
    inject_tool_metadata_block, DescriptionStyle, DiscoverableMetadataOptions,
    ToolDescriptionGenerator, DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER,
};
pub use wf_tools::tool_exposure::{
    is_tool_callable, resolve_tool_exposure, ExposureInput, ExposureResolution,
};
pub use wf_tools::tool_schema_formatter::ToolSchemaFormatter;

// -- wf-agent --
pub use wf_agent::entity::AgentLoopEntity;
pub use wf_agent::executor::AgentLoopExecutor;
pub use wf_agent::registry::AgentLoopRegistry;
pub use wf_agent::state::AgentLoopState;
pub use wf_agent::stream::AgentEventStream;
pub use wf_agent::timeout::{AgentTimeoutManager, TimeoutHandle};
pub use wf_agent::capacity::AgentCapacityGate;
pub use wf_agent::factory::AgentLoopFactory;
pub use wf_agent::validation::AgentLoopValidator;
pub use wf_agent::checkpoint::{AgentCheckpointStrategy, AgentCheckpointTiming};
pub use wf_agent::error::{AgentError, AgentResult};
pub use wf_agent::trigger::{
    TriggeredAgentExecutionConfig, TriggeredAgentExecutionManager, TriggeredTaskSubmission,
};
pub use wf_agent::visibility::{
    collect_activated_tools, VariableBackedVisibilityStore, ACTIVATED_VARIABLE_PREFIX,
    BLOCKED_VARIABLE_PREFIX,
};
pub use wf_agent::conversation_compression::{
    apply_compression, apply_versioned_writeback, spawn_conversation_compression_consumer,
    ConversationWritebackOp,
};
pub use wf_agent::error_analysis::{
    analyze_error, analyze_error_pattern, find_root_cause, get_error_chain,
    get_recommended_recovery_action, llm_error_analysis, shared_error_analysis,
    tool_error_analysis, ErrorAnalysis,
};

// -- wf-workflow --
pub use wf_workflow::entity::WorkflowExecutionEntity;
pub use wf_workflow::executor::WorkflowExecutor;
pub use wf_workflow::handler::{
    HandlerRegistry, NodeHandlerResult,
};
pub use wf_workflow::registry::{
    create_execution_registry, create_graph_registry, lookup_graph, lookup_script, register_graph,
    register_script, ScriptDefinition, ScriptRegistry, WorkflowExecutionRegistry,
    WorkflowGraphRegistry,
};
pub use wf_workflow::state::WorkflowExecutionState;
pub use wf_workflow::variable::{
    convert_variable_type, create_variable_store, evaluate_expression as evaluate_variable_expression, ExprEvaluator,
    ExpressionError, VariableResolver, VariableStore,
};
pub use wf_workflow::graph::GraphTraversal;
pub use wf_workflow::loop_state::{
    current_item, current_loop, enter_loop, exit_loop, find_loop, iterable_len, loop_condition_met,
    mark_iteration_failed, update_loop, LoopState, MAX_ITERATIONS_CAP,
};
pub use wf_workflow::interaction::{
    complete_interaction, interaction_registry, register_interaction, InteractionRegistry,
    InteractionWait,
};
pub use wf_workflow::message_context::{
    append_context, get_context, has_context, register_context, DEFAULT_CONTEXT_ID,
};
pub use wf_workflow::trigger_listener::{
    SubworkflowRunner, TriggerActionRunner, TriggerEventListener, TriggerTemplateRegistry,
};
pub use wf_workflow::trigger_states::{TriggerStateRecord, TriggerStateRegistry};
pub use wf_workflow::validation::{format_validation_report, GraphValidator};
pub use wf_workflow::analysis::{
    analyze_graph, detect_cycles, get_nodes_reaching_to,
    topological_sort, CycleDetectionResult, GraphAnalysis, ReachabilityResult,
    TopologicalSortResult,
};
pub use wf_workflow::barrier::{BranchResult, FailureStrategy, ForkOutcome};
pub use wf_workflow::coordinator::{
    state_transitor::WorkflowStateTransitor, NodeCoordinator, WorkflowCoordinator,
    WorkflowExecutionParams, WorkflowLifecycleCoordinator,
};
pub use wf_workflow::error::{WorkflowError, WorkflowResult};
pub use wf_workflow::execution_callback::WorkflowExecutionCallback;
pub use wf_workflow::execution_context::{ExecutionContextRegistry, WriteBackError};
pub use wf_workflow::node_validation;
pub use wf_workflow::persistence::build_workflow_execution;
pub use wf_workflow::reference_closure::{ReferenceClosureReport, ReferenceContext, MAX_REFERENCE_DEPTH};
pub use wf_workflow::create_default_handlers;

// -- wf-checkpoint --
pub use wf_checkpoint::file::{
    FileCheckpoint, FileCheckpointManager, FileCheckpointMetadata, FileCheckpointOptions,
    FileContentEntry, FileContentStore, FileState, LayertwineFileContentStore,
    WorkspaceRestoreResult,
};
pub use wf_checkpoint::diff::{
    DiffEngine, DiffHunk, DiffOp, DiffOpKind, DiffResult, DiffStats, HunkLine, HunkLineKind,
};
pub use wf_checkpoint::scan::{ScanConfig, WorkspaceScan, WorkspaceScanner};
pub use wf_checkpoint::watcher::{FileChangeKind, FileChangeRecord, FileWatcher, ManualChangeService};
pub use wf_checkpoint::approval::{ConflictView, MergeOutcome, PendingApproval};
pub use wf_checkpoint::error::CheckpointError;
pub use wf_checkpoint::event::{CheckpointEvent, CheckpointEventBus};
pub use wf_checkpoint::cache::CheckpointCache;
pub use wf_checkpoint::serializer::{CheckpointCodec, CheckpointSerializer};
pub use wf_checkpoint::provenance::{DeltaSummary, FileDiffKind, FileDiffView, PartitionView, WorkspaceFile};
pub use wf_checkpoint::actor_id::{ActorId, ActorIdError, ActorKind};
pub use wf_checkpoint::file_merge::MergeCommitResult;
pub use wf_checkpoint::file_util::sha256_hex;
pub use wf_checkpoint::metadata_builder::{build_checkpoint_state, CheckpointMetadataBuilder};

pub use agent::agent_config::{
    build_agent_loop_config, DEFAULT_AGENT, DEFAULT_MAX_ITERATIONS, DEFAULT_MODEL,
};

pub(crate) use infra::error::not_found;
