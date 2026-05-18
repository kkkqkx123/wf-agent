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

// Execution-specific interruption utilities (generic layer)
export {
  checkExecutionInterruption,
  shouldContinue as shouldContinueExecution,
  getExecutionInterruptionType,
  getExecutionInterruptionDescription,
  type ExecutionInterruptionCheckResult,
} from "./execution-interruption-utils.js";

// Unified interruption handler (new architecture)
export {
  executeWithInterruptionHandling,
  iterateWithInterruptionHandling,
} from "./interruption-handler.js";

// Interruption state management
export {
  InterruptionState,
  type InterruptionType,
  type InterruptionInfo,
  type InterruptionStateConfig,
} from "./interruption-state.js";

// Interruption propagation proxy
export {
  InterruptionPropagationProxy,
  type PropagationResult,
  InterruptionPropagationError,
} from "./interruption-propagation-proxy.js";
