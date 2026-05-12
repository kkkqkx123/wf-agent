/**
 * State Managers Module Export
 * Provides state management classes for workflow execution.
 */

// Unified VariableManager
export { VariableManager, type VariableManagerSnapshot } from "./variable-manager.js";

export { ExecutionState, type SubgraphContext } from "./execution-state.js";
export {
  AsyncCompletionManager,
  type CompletionHandler,
} from "./promise-resolution-manager.js";
export { TriggerState, type TriggerRuntimeState } from "./trigger-state.js";
export {
  WorkflowExecutionState,
  type WorkflowExecutionStateSnapshot,
  type OperationState,
} from "./workflow-execution-state.js";
export {
  WorkflowStateCoordinator,
  type WorkflowStateSnapshot,
  type WorkflowStateCoordinatorConfig,
} from "./workflow-state-coordinator.js";
