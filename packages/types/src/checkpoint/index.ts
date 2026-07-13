/**
 * Unified Export of Checkpoint Type Definitions
 * Defining the structure and content of a checkpoint
 */

// Export base type (includes unified triggers, policies, and contexts)
export { CheckpointTypeEnum as CheckpointType } from "./base.js";
export type { CheckpointType as TCheckpointType } from "./base.js";
export {
  SnapshotVersion,
  SnapshotBase,
  CheckpointMetadata,
  CheckpointOptions,
  DeltaStorageConfig,
  DEFAULT_DELTA_STORAGE_CONFIG,
  CheckpointConfigResult,
  CheckpointListOptions,
  CheckpointConfigSource,
  WorkflowCheckpointTriggerType,
  AgentLoopCheckpointTriggerType,
  BaseCheckpointCore,
  BaseCheckpoint,
  FullCheckpoint,
  DeltaCheckpoint,
  AnyCheckpoint,
  CheckpointTrigger,
  CHECKPOINT_POLICY_MINIMAL,
  CHECKPOINT_POLICY_STANDARD,
  CHECKPOINT_POLICY_COMPREHENSIVE,
  CHECKPOINT_POLICY_NONE,
  CHECKPOINT_POLICIES,
} from "./base.js";
export type {
  CheckpointTriggerType,
  CheckpointContentConfig,
  CheckpointRetentionConfig,
  CheckpointErrorHandlingConfig,
  UnifiedCheckpointPolicy,
  CheckpointContext,
} from "./base.js";

// Export error handling types
export {
  CheckpointError,
  type CheckpointErrorStrategy,
  type CheckpointErrorContext,
  type CheckpointErrorCallback,
  type CheckpointErrorHandlerConfig,
  type CheckpointErrorHandlingResult,
} from "./error-handling.js";

// Export metrics types
export type {
  CheckpointCreationMetrics,
  CheckpointCleanupMetrics,
  CheckpointLoadMetrics,
  CheckpointChainLengthMetric,
  CheckpointMetricsAggregate,
  CheckpointMetricsConfig,
  CheckpointMetricsEvent,
  ICheckpointMetricsStorage,
} from "./metrics.js";

// Export version management types
export type {
  CheckpointFormatVersion,
  VersionCompatibility,
  VersionMigrationResult,
  VersionMigrationHandler,
  VersionMigrationRegistry,
  CheckpointVersionMetadata,
} from "./version.js";
export {
  CHECKPOINT_FORMAT_VERSIONS,
  CURRENT_CHECKPOINT_FORMAT_VERSION,
  COMPATIBILITY_RULES,
  versionFormatter,
} from "./version.js";

// Exporting Agent Loop Checkpoint Types
export * from "./agent/index.js";

// Export Workflow Checkpoint Types
export * from "./workflow/index.js";

// Export Checkpoint Variable State
export { CheckpointVariableState } from "./variable-state.js";

// Export Execution Event Types (Plan C: State-driven architecture)
export {
  EXECUTION_STATE_MAX_EVENTS,
  EXECUTION_STATE_MAX_ERROR_RECORDS,
  EXECUTION_STATE_MAX_INTERRUPTION_RECORDS,
} from "./execution-events.js";
export type {
  ExecutionErrorRecord,
  ExecutionInterruptionRecord,
  ExecutionEventRecord,
} from "./execution-events.js";

