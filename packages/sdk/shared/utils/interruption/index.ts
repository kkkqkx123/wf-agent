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
  type InterruptionInfo,
  type InterruptionStateConfig,
} from "./interruption-state.js";

// Re-export InterruptionType from canonical location in core/types
export type { InterruptionType } from "../../types/interruption-types.js";

// NOTE: InterruptionPropagationProxy has been removed.
// Cascade propagation is now handled via EventRegistry:
//   childInterruptionState.setEventRegistry(eventRegistry);
//   childInterruptionState.connectToParent(parentExecutionId);
// See docs/architecture/interruption-event-driven.md for migration guide.

// NOTE: InterruptionTimeoutManager has been removed.
// Use TimeoutManager from s../shared/state-managers/timeout-manager.js instead.
// See docs/refactoring/phase2-migration-guide.md for migration guide.

// NOTE: InterruptionHistoryManager has been removed (Phase 8 cleanup).
// Interruption events are now published to ExecutionEventBus and persisted in CheckpointState.interruptionRecords.
// For querying interruptions, use CheckpointState.interruptionRecords or subscribe to ExecutionEventBus events.

// Recovery strategy manager
export {
  RecoveryStrategyManager,
  type RecoveryStrategy,
  type RecoveryContext,
  createAutoSaveStrategy,
} from "./recovery-strategy-manager.js";
