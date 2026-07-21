/**
 * Workflow Resources - Public API
 * Exports all resource APIs
 */

export { WorkflowRegistryAPI, type WorkflowFilter, type WorkflowSummary } from "./workflow-registry-api.js";
export {
  WorkflowExecutionRegistryAPI,
  type WorkflowExecutionFilter,
  type WorkflowExecutionSummary,
} from "./workflow-execution-registry-api.js";
export {
  CheckpointResourceAPI,
  type CheckpointFilter,
  type CheckpointSummary,
  type CheckpointTransition,
  type CheckpointChainAnalysis,
} from "./checkpoint-resource-api.js";
export {
  FileCheckpointResourceAPI,
  type FileCheckpointFilter,
} from "./file-checkpoint-resource-api.js";
export {
  MessageResourceAPI,
  type MessageFilter,
  type MessageStats,
} from "./message-resource-api.js";
export {
  NodeRegistryAPI,
  type NodeTemplateFilter,
  type NodeTemplateSummary,
} from "./node-template-registry-api.js";
export {
  HookTemplateRegistryAPI,
  type HookTemplateFilter,
  type HookTemplateSummary,
} from "./hook-template-registry-api.js";
export {
  TriggerTemplateRegistryAPI,
  type TriggerTemplateFilter,
  type TriggerTemplateSummary,
} from "./trigger-template-registry-api.js";
export {
  TriggerResourceAPI,
  type TriggerFilter,
} from "./trigger-resource-api.js";
export {
  UserInteractionResourceAPI,
  type UserInteractionConfig,
  type UserInteractionFilter,
} from "./user-interaction-resource-api.js";
export {
  VariableResourceAPI,
  type VariableFilter,
  type VariableUpdateOptions,
  type VariableDefinition,
} from "./variable-resource-api.js";

export {
  WorkflowIterationAnalysisAPI,
  type ExtendedNodeExecutionRecord,
  type ExecutionPathStep,
  type LLMNodeMetadata,
  type QualityMetrics,
  type ExtendedNodeExecutionFilter,
  type NodeExecutionStats,
  type OptimizationOpportunity,
  // Re-export types from Agent API for consistency
  type ToolDependency,
  type ExecutionPath,
  type LLMReasoningRecord,
} from "./workflow-iteration-analysis-api.js";

export {
  WorkflowExecutionContextAPI,
  type VariableDefinitionWithScope,
  type VariableValueSnapshot,
  type VariableSnapshot,
  type VariableHistoryEntry,
  type VariableHistory,
  type NodeInputContext,
  type ExecutionContextSnapshot,
  type ContextStateTransition,
  type ContextEvolution,
  type VariableSnapshotFilter,
  type ContextEvolutionFilter,
  type WorkflowStackFrame,
  type WorkflowCallStack,
  type WorkflowStateTransitionAnalysis,
} from "./workflow-execution-context-api.js";

export {
  WorkflowExecutionGraphQueryAPI,
  type WorkflowExecutionNode,
  type WorkflowExecutionEdge,
  type WorkflowExecutionGraph,
  type WorkflowExecutionPathSummary,
  type WorkflowExecutionGraphFilter,
  type AlternativeDecision,
  type DecisionPoint,
  type WorkflowDecisionAnalysis,
  type PathProbabilityAnalysis,
} from "./workflow-execution-graph-query-api.js";

export {
  WorkflowPerformanceAnalysisAPI,
  type WorkflowExecutionPerformanceProfile,
  type NodeExecutionPerformance,
  type WorkflowNodeComparison,
  type OperationMetrics,
  type PerformanceTier,
  type PerformanceBottleneck,
  type PerformanceSummary,
  type PerformanceTrend,
  type ExecutionTimelineEntry,
  type ExecutionTimelineType,
} from "./workflow-performance-analysis-api.js";

export {
  WorkflowTemplateRegistryAPI,
  type WorkflowTemplateFilter,
  type WorkflowTemplateSummary,
  type WorkflowTemplateDefinition,
} from "./workflow-template-registry-api.js";
