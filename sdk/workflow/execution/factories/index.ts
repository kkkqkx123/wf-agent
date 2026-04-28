/**
 * Factory Module
 *
 * The factory is responsible for creating complex objects and contexts, primarily for use by the coordinator.
 *
 * Included factories:
 * - WorkflowExecutionBuilder: Workflow execution entity builder (main factory for workflow execution creation)
 * - NodeHandlerContextFactory: Node handler context factory
 * - LLMContextFactory: LLM execution context factory
 *
 * Note: WorkflowExecutionBuilder follows the factory pattern but keeps its name to reflect the builder pattern usage.
 * The naming difference from AgentLoopFactory reflects the different execution models.
 */

export { WorkflowExecutionBuilder } from "./workflow-execution-builder.js";
// Backward compatibility
export { WorkflowExecutionBuilder as ThreadBuilder } from "./workflow-execution-builder.js";

export {
  NodeHandlerContextFactory,
  type NodeHandlerContextFactoryConfig,
} from "./node-handler-context-factory.js";

export {
  LLMContextFactory,
  type LLMContextFactoryConfig,
  type ToolApprovalContext,
  type InterruptionContext,
  type ToolExecutionContext,
  type LLMCallContext,
  type ToolVisibilityContext,
} from "./llm-context-factory.js";
