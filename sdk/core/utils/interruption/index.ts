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
  type InterruptionCheckResult,
} from "./abort-signal-utils.js";

// Execution-specific interruption utilities (generic layer)
export {
  checkExecutionInterruption,
  shouldContinueExecution,
  getExecutionInterruptionType,
  getExecutionInterruptionDescription,
  type ExecutionInterruptionCheckResult,
} from "./execution-interruption-utils.js";

// Unified interruption handler (new architecture)
export {
  executeWithInterruptionHandling,
  iterateWithInterruptionHandling,
  type StreamInterruptionConfig,
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

// NOTE: InterruptionTimeoutManager has been removed.
// Use TimeoutManager from sdk/core/state-managers/timeout-manager.js instead.
// See docs/refactoring/phase2-migration-guide.md for migration guide.

// Interruption history manager
export {
  InterruptionHistoryManager,
  type InterruptionHistoryEntry,
  type HistoryFilter,
} from "./interruption-history-manager.js";

// Recovery strategy manager
export {
  RecoveryStrategyManager,
  type RecoveryStrategy,
  type RecoveryContext,
  createAutoSaveStrategy,
} from "./recovery-strategy-manager.js";
