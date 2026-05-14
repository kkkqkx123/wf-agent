/**
 * Interruption Utilities Module Export
 */

// Execution-specific interruption utilities (supports both workflow and agent)
export * from "./execution-interruption-utils.js";

// Unified interruption handler (new architecture)
export {
  executeWithInterruptionHandling,
  iterateWithInterruptionHandling,
} from "./interruption-handler.js";

// Backward compatibility exports (deprecated, use execution-* naming)
export {
  ExecutionInterruptionCheckResult as WorkflowInterruptionCheckResult,
  checkWorkflowInterruption,
  getWorkflowInterruptionType,
  getWorkflowNodeId,
  getWorkflowExecutionId,
  getWorkflowInterruptionDescription,
  createInterruptionAbortReason,
} from "./execution-interruption-utils.js";
