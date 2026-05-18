/**
 * Interruption Utilities Module Export
 */

// AbortSignal utilities (domain-agnostic)
export {
  isAborted,
  createNeverAbortSignal,
  combineAbortSignals,
  withAbortSignal,
  checkInterruption,
  shouldContinue,
  type InterruptionCheckResult,
} from "./abort-signal-utils.js";

// Execution-specific interruption utilities (supports both workflow and agent)
export {
  checkWorkflowInterruption,
  shouldContinue as shouldContinueExecution,
  getWorkflowInterruptionType,
  getWorkflowInterruptionDescription,
  createInterruptionAbortReason,
  type ExecutionInterruptionCheckResult,
} from "./execution-interruption-utils.js";

// Unified interruption handler (new architecture)
export {
  executeWithInterruptionHandling,
  iterateWithInterruptionHandling,
} from "./interruption-handler.js";
