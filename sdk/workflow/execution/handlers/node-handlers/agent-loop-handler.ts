/**
 * Agent Loop Node Processor
 *
 * Responsible for processing AgentLoop nodes within the Graph execution engine.
 * Uses the new AgentLoopCoordinator architecture, which supports pause/resume functionality.
 * Allows for the reference of predefined system prompt templates via the systemPromptTemplateId.
 */

import type { Node, WorkflowExecution, AgentLoopNodeConfig, LLMMessage } from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";
import { now, diffTimestamp, getErrorOrNew } from "@wf-agent/common-utils";
import { resolveSystemPrompt } from "../../../../core/prompt/system-prompt-resolver.js";
import type { ConversationSession } from "../../../../core/messaging/conversation-session.js";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import { AgentLoopCoordinator, AgentLoopExecutor } from "../../../../agent/index.js";
import type { AgentLoopRegistry } from "../../../../agent/index.js";
import { emit } from "../../utils/index.js";
import {
  buildMessageAddedEvent,
  buildConversationStateChangedEvent,
} from "../../utils/event/index.js";
import { LLMExecutor } from "../../../../core/executors/llm-executor.js";
import { ToolRegistry } from "../../../../core/registry/tool-registry.js";
import { getContainer } from "../../../../core/di/index.js";
import * as Identifiers from "../../../../core/di/service-identifiers.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "AgentLoopHandler" });

/**
 * Agent Loop node execution results
 */
export interface AgentLoopExecutionResult {
  /** Execution Status */
  status: "COMPLETED" | "FAILED" | "ABORTED" | "PAUSED";
  /** The final LLM response content */
  content?: string;
  /** Actual number of iterations */
  iterations?: number;
  /** Number of tool calls */
  toolCallCount?: number;
  /** Did it end because the maximum number of iterations was reached? */
  hitIterationLimit?: boolean;
  /** Error message (in case of failure) */
  error?: Error;
  /** Execution time (in milliseconds) */
  executionTime: number;
  /** Agent Loop instance ID (used for pausing/resuming) */
  loopId?: string;
}

/**
 * Agent Loop Processor Context
 */
export interface AgentLoopHandlerContext {
  /** LLM Executor */
  llmExecutor: LLMExecutor;
  /** Tool Services */
  toolService: ToolRegistry;
  /** Dialogue Manager */
  conversationManager: ConversationSession;
  /** Event Manager */
  eventManager: EventRegistry;
  /** Agent Loop registry (optional, used for cross-request management) */
  agentLoopRegistry?: AgentLoopRegistry;
  /** Execution Registry (used for checking interrupts) */
  executionRegistry?: unknown;
}

/**
 * Create an AgentLoopCoordinator instance
 */
function createCoordinator(context: AgentLoopHandlerContext): AgentLoopCoordinator {
  // Get AgentLoopRegistry from DI container
  const container = getContainer();
  const registry =
    context.agentLoopRegistry ??
    (container.get(Identifiers.AgentLoopRegistry) as AgentLoopRegistry);
  const executor = new AgentLoopExecutor({
    llmExecutor: context.llmExecutor,
    toolService: context.toolService,
    eventManager: context.eventManager,
  });

  return new AgentLoopCoordinator(registry, executor);
}

/**
 * Agent Loop Node Processor
 */
export async function agentLoopHandler(
  execution: WorkflowExecution,
  node: Node,
  context: AgentLoopHandlerContext,
): Promise<AgentLoopExecutionResult> {
  const config = node.config as AgentLoopNodeConfig;
  const startTime = now();

  if (!config.profileId) {
    return {
      status: "FAILED",
      error: new ExecutionError("AgentLoop node requires profileId", node.id),
      executionTime: 0,
    };
  }

  try {
    // 1. Prepare the initial message
    const initialMessages: LLMMessage[] = [];
    const inputPrompt =
      execution.variableScopes?.workflowExecution?.["input"] || execution.variableScopes?.workflowExecution?.["prompt"];

    if (inputPrompt && typeof inputPrompt === "string") {
      initialMessages.push({ role: "user", content: inputPrompt });

      // Trigger message addition event
      try {
        await emit(
          context.eventManager,
          buildMessageAddedEvent({
            executionId: execution.id,
            role: "user",
            content: inputPrompt,
            nodeId: node.id,
          }),
        );
      } catch (error) {
        logger.debug("Failed to emit MESSAGE_ADDED event", { error });
      }
    }

    // 2. Create a Coordinator and execute it.
    const coordinator = createCoordinator(context);

    const result = await coordinator.execute(
      {
        profileId: config.profileId,
        systemPrompt: resolveSystemPrompt(config),
        initialMessages,
        tools: config.tools,
        maxIterations: config.maxIterations,
      },
      {
        conversationManager: context.conversationManager,
        parentExecutionId: execution.id,
        nodeId: node.id,
      },
    );

    if (!result.success) {
      throw result.error || new Error("Agent loop failed");
    }

    // 3. Synchronize messages to ConversationSession
    // In the new architecture, messages have been automatically synchronized to the ConversationSession through the AgentLoopEntity
    // Here only the event needs to be triggered
    if (result.content) {
      try {
        await emit(
          context.eventManager,
          buildMessageAddedEvent({
            executionId: execution.id,
            role: "assistant",
            content: result.content,
            nodeId: node.id,
          }),
        );
      } catch (error) {
        logger.debug("Failed to emit MESSAGE_ADDED event", { error });
      }
    }

    // Trigger a dialog state change event
    try {
      await emit(
        context.eventManager,
        buildConversationStateChangedEvent({
          executionId: execution.id,
          messageCount: context.conversationManager.getMessages().length,
          tokenUsage: 0, // Not counting the total consumption for now.
          nodeId: node.id,
        }),
      );
    } catch (error) {
      logger.debug("Failed to emit CONVERSATION_STATE_CHANGED event", { error });
    }

    // 4. Update the variable
    if (execution.variableScopes?.workflowExecution) {
      execution.variableScopes.workflowExecution["output"] = result.content;
      execution.variableScopes.workflowExecution["agentLoopIterations"] = result.iterations;
      execution.variableScopes.workflowExecution["agentLoopToolCallCount"] = result.toolCallCount;
    }

    return {
      status: "COMPLETED",
      content: result.content,
      iterations: result.iterations,
      toolCallCount: result.toolCallCount,
      executionTime: diffTimestamp(startTime, now()),
    };
  } catch (error) {
    return {
      status: "FAILED",
      error: getErrorOrNew(error),
      executionTime: diffTimestamp(startTime, now()),
    };
  }
}

/**
 * Stream Agent Loop Node Processor
 */
export async function* agentLoopStreamHandler(
  execution: WorkflowExecution,
  node: Node,
  context: AgentLoopHandlerContext,
): AsyncGenerator<unknown, AgentLoopExecutionResult, unknown> {
  const config = node.config as AgentLoopNodeConfig;
  const startTime = now();

  if (!config.profileId) {
    return {
      status: "FAILED",
      error: new ExecutionError("AgentLoop node requires profileId", node.id),
      executionTime: 0,
    };
  }

  try {
    // 1. Prepare the initial message
    const initialMessages: LLMMessage[] = [];
    const inputPrompt =
      execution.variableScopes?.workflowExecution?.["input"] || execution.variableScopes?.workflowExecution?.["prompt"];

    if (inputPrompt && typeof inputPrompt === "string") {
      initialMessages.push({ role: "user", content: inputPrompt });
    }

    // 2. Create a Coordinator to execute tasks in parallel.
    const coordinator = createCoordinator(context);

    for await (const event of coordinator.executeStream(
      {
        profileId: config.profileId,
        systemPrompt: resolveSystemPrompt(config),
        initialMessages,
        tools: config.tools,
        maxIterations: config.maxIterations,
      },
      {
        conversationManager: context.conversationManager,
        parentExecutionId: execution.id,
        nodeId: node.id,
      },
    )) {
      // Forward streaming events
      yield {
        type: "agent_loop_event",
        executionId: execution.id,
        nodeId: node.id,
        event,
      };
    }

    // 3. Obtain the execution results
    const entity = coordinator.getRunning()[0] || coordinator.getPaused()[0];
    const iterations = entity?.state.currentIteration ?? 0;
    const toolCallCount = entity?.state.toolCallCount ?? 0;
    const content = entity
      ?.getMessages()
      .filter((m: { role: string; content: unknown }) => m.role === "assistant")
      .pop()?.content;

    // 4. Update the variable
    if (execution.variableScopes?.workflowExecution) {
      execution.variableScopes.workflowExecution["output"] = content;
      execution.variableScopes.workflowExecution["agentLoopIterations"] = iterations;
      execution.variableScopes.workflowExecution["agentLoopToolCallCount"] = toolCallCount;
    }

    return {
      status: entity?.isPaused() ? "PAUSED" : "COMPLETED",
      content: typeof content === "string" ? content : undefined,
      iterations,
      toolCallCount,
      executionTime: diffTimestamp(startTime, now()),
      loopId: entity?.id,
    };
  } catch (error) {
    return {
      status: "FAILED",
      error: getErrorOrNew(error),
      executionTime: diffTimestamp(startTime, now()),
    };
  }
}
