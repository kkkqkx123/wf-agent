/**
 * Graph Hook Processor Module
 *
 * Implements Graph-specific Hook execution logic based on the sdk/core/hooks general framework.
 * The timing of execution is managed by higher-level stateful modules (such as ThreadExecutor).
 */

import type { Node, NodeHook, NodeExecutionResult, NodeCustomEvent } from "@wf-agent/types";
import { HookType, ExecutionError } from "@wf-agent/types";
import type { CheckpointDependencies } from "../../../checkpoint/utils/checkpoint-utils.js";
import { createCheckpoint } from "../../../checkpoint/utils/checkpoint-utils.js";
import {
  filterAndSortHooks,
  executeHooks,
  type BaseHookDefinition,
  type BaseHookContext,
  type HookHandler,
} from "../../../../core/hooks/index.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import { buildHookEvaluationContext, convertToEvaluationContext } from "./context-builder.js";
import { emitHookEvent } from "./event-emitter.js";
import type { ThreadEntity } from "../../../entities/index.js";

const logger = createContextualLogger();

/**
 * Graph Hook Execution Context
 *
 * Extends BaseHookContext to add Graph-specific context data.
 */
export interface HookExecutionContext extends BaseHookContext {
  /** ThreadEntity instance */
  threadEntity: ThreadEntity;
  /** Node Definition */
  node: Node;
  /** Node execution results (available at AFTER_EXECUTE) */
  result?: NodeExecutionResult;
  /** Checkpoint dependencies (optional) */
  checkpointDependencies?: CheckpointDependencies;
}

/**
 * Graph Hook Definition
 *
 * The NodeHook extends the BaseHookDefinition.
 */
export type GraphHookDefinition = NodeHook & BaseHookDefinition;

/**
 * Constructing a Graph Hook to evaluate the context
 */
function buildGraphEvalContext(context: HookExecutionContext): Record<string, unknown> {
  const hookEvalContext = buildHookEvaluationContext(context);
  return convertToEvaluationContext(hookEvalContext);
}

/**
 * Create a checkpoint handler
 */
function createCheckpointHandler(): HookHandler<HookExecutionContext> {
  return async (context, hook) => {
    // Convert the hook to a NodeHook to access specific Graph properties.
    const nodeHook = hook as NodeHook;
    if (!nodeHook.createCheckpoint || !context.checkpointDependencies) {
      return;
    }

    try {
      await createCheckpoint(
        {
          threadId: context.workflowExecutionEntity.id,
          nodeId: context.node.id,
          description: nodeHook.checkpointDescription || `Hook: ${hook.eventName}`,
        },
        context.checkpointDependencies,
      );
    } catch (error) {
      logger.warn(
        "Failed to create checkpoint for hook",
        {
          eventName: hook.eventName,
          nodeId: context.node.id,
          threadId: context.workflowExecutionEntity.id,
          workflowId: context.threadEntity.getWorkflowId(),
          operation: "checkpoint_creation",
          suggestion: "Check checkpoint storage configuration and retry",
        },
        undefined,
        getErrorOrNew(error),
      );
    }
  };
}

/**
 * Create a custom processor
 */
function createCustomHandler(): HookHandler<HookExecutionContext> {
  return async (context, hook, eventData) => {
    const customHandler = hook.eventPayload?.["handler"];
    if (customHandler && typeof customHandler === "function") {
      try {
        await customHandler(context, hook as NodeHook, eventData);
      } catch (error) {
        throw new ExecutionError(
          "Custom handler execution failed",
          context.node.id,
          context.threadEntity.getWorkflowId(),
          {
            eventName: hook.eventName,
            nodeId: context.node.id,
            operation: "custom_handler_execution",
          },
          getErrorOrNew(error),
          "error",
        );
      }
    }
  };
}

/**
 * Create an event emitter handler
 */
function createEventEmitterHandler(
  emitEvent: (event: NodeCustomEvent) => Promise<void>,
): HookHandler<HookExecutionContext> {
  return async (context, hook, eventData) => {
    await emitHookEvent(context, hook.eventName, eventData || {}, emitEvent);
  };
}

/**
 * Execute the specified type of Hook
 *
 * @param context Hook execution context
 * @param hookType Hook type (BEFORE_EXECUTE or AFTER_EXECUTE)
 * @param emitEvent Event emission function
 */
export async function executeHook(
  context: HookExecutionContext,
  hookType: HookType,
  emitEvent: (event: NodeCustomEvent) => Promise<void>,
): Promise<void> {
  const { node } = context;

  // Check if the node has a Hook configuration.
  if (!node.hooks || node.hooks.length === 0) {
    return;
  }

  // Using a generic framework for filtering and sorting hooks
  const hooks = filterAndSortHooks(node.hooks as GraphHookDefinition[], hookType);

  if (hooks.length === 0) {
    return;
  }

  // Create a processor chain
  const handlers: HookHandler<HookExecutionContext>[] = [
    createCheckpointHandler(),
    createCustomHandler(),
    createEventEmitterHandler(emitEvent),
  ];

  // Execute Hook using a generic framework
  await executeHooks(
    hooks,
    context,
    buildGraphEvalContext,
    handlers,
    async () => {
      // The event has been processed by createEventEmitterHandler.
    },
    {
      parallel: true,
      continueOnError: true,
      warnOnConditionFailure: true,
    },
  );
}

// Export context builder functions and types
export {
  buildHookEvaluationContext,
  convertToEvaluationContext,
  type HookEvaluationContext,
} from "./context-builder.js";

// Export event emitter functions
export { emitHookEvent } from "./event-emitter.js";

// Export payload generator functions
export { generateHookEventData, resolvePayloadTemplate } from "./payload-generator.js";
