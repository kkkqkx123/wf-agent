/**
 * Factory Module
 *
 * The factory is responsible for creating complex objects and contexts, primarily for use by the coordinator.
 *
 * Included factories:
 * - ThreadBuilder: Thread entity builder (main factory for thread creation)
 * - NodeHandlerContextFactory: Node handler context factory
 * - LLMContextFactory: LLM execution context factory
 *
 * Note: ThreadBuilder follows the factory pattern but keeps its name to reflect the builder pattern usage.
 * The naming difference from AgentLoopFactory reflects the different execution models.
 */

export { ThreadBuilder } from "./thread-builder.js";

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
