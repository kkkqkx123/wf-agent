/**
 * Agent Hook Processor Module
 *
 * Implement Agent specific Hook execution logic based on sdk/core/hooks generic framework.
 * Refer to the Graph module's hook-handler.ts design.
 *
 * Supported Hook Types:
 * - BEFORE_ITERATION: before iteration starts
 * - AFTER_ITERATION: After iteration is finished
 * - BEFORE_TOOL_CALL: before tool call starts
 * - AFTER_TOOL_CALL: after tool call ends
 * - BEFORE_LLM_CALL: before LLM call starts
 * - AFTER_LLM_CALL: After LLM call ends
 */

import type { AgentLoopEntity } from "../../../entities/agent-loop-entity.js";
import type { AgentHook, AgentHookType, AgentHookTriggeredEvent } from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";
import {
  filterAndSortHooks,
  executeHooks,
  type BaseHookDefinition,
  type BaseHookContext,
  type HookHandler,
} from "../../../../core/hooks/index.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import { buildAgentHookEvaluationContext, convertToEvaluationContext } from "./context-builder.js";
import { emitAgentHookEvent } from "./event-emitter.js";

// Logger is available for future use
void createContextualLogger({ component: "AgentHookHandler" });

/**
 * Agent Hook Execution Context
 *
 * Extends BaseHookContext to add agent-specific context data
 */
export interface AgentHookExecutionContext extends BaseHookContext {
  /** Agent Loop Entity */
  entity: AgentLoopEntity;
  /** Current tool call information (available BEFORE/AFTER_TOOL_CALL) */
  toolCall?: {
    id: string;
    name: string;
    arguments: unknown;
    result?: unknown;
    error?: string;
  };
  /** LLM response information (available when AFTER_LLM_CALL) */
  llmResponse?: {
    content: string;
    toolCalls?: unknown[];
  };
}

/**
 * Agent Hook Definition
 *
 * The AgentHook extends the BaseHookDefinition.
 */
export type AgentHookDefinition = AgentHook & BaseHookDefinition;

/**
 * Build an Agent Hook to evaluate the context
 */
function buildAgentEvalContext(context: AgentHookExecutionContext): Record<string, unknown> {
  const hookEvalContext = buildAgentHookEvaluationContext(context.entity, context.toolCall);
  return convertToEvaluationContext(hookEvalContext);
}

/**
 * Create a custom handler
 */
function createCustomHandler(): HookHandler<AgentHookExecutionContext> {
  return async (context, hook, eventData) => {
    const customHandler = hook.eventPayload?.["handler"];
    if (customHandler && typeof customHandler === "function") {
      try {
        await customHandler(context, hook as AgentHook, eventData);
      } catch (error) {
        throw new ExecutionError(
          "Agent custom handler execution failed",
          context.entity.nodeId,
          undefined,
          {
            eventName: hook.eventName,
            agentLoopId: context.entity.id,
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
  hookType: AgentHookType,
  emitEvent: (event: AgentHookTriggeredEvent) => Promise<void>,
): HookHandler<AgentHookExecutionContext> {
  return async (context, hook, eventData) => {
    await emitAgentHookEvent(context.entity, hookType, hook.eventName, eventData, emitEvent);
  };
}

/**
 * Execute the specified type of Agent Hook
 *
 * @param entity: Agent Loop entity
 * @param hookType: Hook type
 * @param emitEvent: Event emission function
 * @param toolCallInfo: Tool call information (optional)
 * @param llmResponse: LLM response information (optional)
 */
export async function executeAgentHook(
  entity: AgentLoopEntity,
  hookType: AgentHookType,
  emitEvent: (event: AgentHookTriggeredEvent) => Promise<void>,
  toolCallInfo?: {
    id: string;
    name: string;
    arguments: unknown;
    result?: unknown;
    error?: string;
  },
  llmResponse?: {
    content: string;
    toolCalls?: unknown[];
  },
): Promise<void> {
  const config = entity.config;

  // Check if the configuration contains any Hooks.
  if (!config.hooks || config.hooks.length === 0) {
    return;
  }

  // Use a generic framework for filtering and sorting hooks
  const hooks = filterAndSortHooks(config.hooks as AgentHookDefinition[], hookType);

  if (hooks.length === 0) {
    return;
  }

  // Constructing the execution context
  const context: AgentHookExecutionContext = {
    workflowExecutionId: entity.id,
    entity,
    toolCall: toolCallInfo,
    llmResponse,
  };

  // Create a processor chain
  const handlers: HookHandler<AgentHookExecutionContext>[] = [
    createCustomHandler(),
    createEventEmitterHandler(hookType, emitEvent),
  ];

  // Execute a Hook using a general framework.
  await executeHooks(
    hooks,
    context,
    buildAgentEvalContext,
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

// Export context builder functions
export {
  buildAgentHookEvaluationContext,
  convertToEvaluationContext,
  type AgentHookEvaluationContext,
} from "./context-builder.js";

// Export event emitter functions
export { emitAgentHookEvent, type AgentHookEventData } from "./event-emitter.js";
