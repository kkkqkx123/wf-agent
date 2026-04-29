/**
 * Coordinator Module
 *
 * The coordinator is a stateless component responsible for coordinating the interactions between various managers.
 *
 * Design Principles:
 * - Coordination Logic: Encapsulate complex coordination logic.
 * - Dependency Injection: Receive dependent managers through the constructor.
 *
 * Included Coordinators:
 * - NodeExecutionCoordinator: Node execution coordinator
 * - TriggerCoordinator: Trigger coordinator
 * - LLMExecutionCoordinator: LLM execution coordinator
 * - WorkflowLifecycleCoordinator: Thread lifecycle coordinator
 * - WorkflowOperationCoordinator: Thread operation coordinator
 * - VariableCoordinator: Variable coordinator
 * - ToolVisibilityCoordinator: Tool visibility coordinator
 * - WorkflowStateTransitor: Thread state transitor (atomic state operations + cascade operations)
 */

export {
  NodeExecutionCoordinator,
  type NodeExecutionCoordinatorConfig,
} from "./node-execution-coordinator.js";
export { TriggerCoordinator } from "./trigger-coordinator.js";
export { LLMExecutionCoordinator } from "./llm-execution-coordinator.js";
export { WorkflowLifecycleCoordinator } from "./thread-lifecycle-coordinator.js";
export { WorkflowOperationCoordinator } from "./thread-operation-coordinator.js";
export { WorkflowExecutionCoordinator } from "./thread-execution-coordinator.js";
export { VariableCoordinator } from "./variable-coordinator.js";
export { WorkflowStateTransitor } from "./thread-state-transitor.js";

export { ToolVisibilityCoordinator } from "./tool-visibility-coordinator.js";
