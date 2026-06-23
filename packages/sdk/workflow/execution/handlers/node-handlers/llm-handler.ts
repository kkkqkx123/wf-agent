/**
 * LLM Node Processor
 * Responsible for executing LLM nodes and handling LLM API calls
 *
 * Design Principles:
 * - Only includes the core execution logic
 * - Relyes on the LLMExecutionCoordinator for actual LLM calls
 * - Business logic such as tool approval is handled by the Graph module
 * - Returns the execution results
 * - Supports referencing predefined prompt templates via templateId
 */

import type {
  RuntimeNode,
  LLMNodeConfig,
  MessageContextRegistry,
  TransformContextFn,
} from "@wf-agent/types";
import type { WorkflowExecution, LLMMessage } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { now, diffTimestamp, getErrorOrNew } from "@wf-agent/common-utils";
import { LLMExecutionCoordinator } from "../../coordinators/llm-execution-coordinator.js";
import { LLMWrapper } from "../../../../services/llm/wrapper.js";
import type { EventRegistry } from "../../../../shared/registry/event-registry.js";
import type { ConversationSession } from "../../../../shared/messaging/conversation-session.js";

/**
 * LLM node execution results
 */
export interface LLMExecutionResult {
  /** Execution Status */
  status: "COMPLETED" | "FAILED";
  /** LLM response content */
  content?: string;
  /** Tool calls from the LLM response (available when LLM requested tool execution) */
  toolCalls?: Array<{
    id: string;
    name: string;
    arguments: unknown;
  }>;
  /** Error message (in case of failure) */
  error?: Error;
  /** Execution time (in milliseconds) */
  executionTime: number;
}

/**
 * LLM processor context
 */
export interface LLMHandlerContext {
  /** LLM Execution Coordinator */
  llmCoordinator: LLMExecutionCoordinator;
  /** LLM Wrapper */
  llmWrapper: LLMWrapper;
  /** Event Manager */
  eventManager: EventRegistry;
  /** Dialogue Manager */
  conversationManager: ConversationSession;
  /** Transform context function (for dynamic context injection, message compression, etc.) */
  transformContext?: TransformContextFn;
}

/**
 * Collect messages from a named context
 */
function collectMessagesFromContext(
  config: LLMNodeConfig,
  workflowExecution: WorkflowExecution,
): LLMMessage[] {
  // Get MessageContextRegistry from execution context
  const registry = (
    workflowExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }
  ).messageContextRegistry;

  if (!registry) {
    throw new RuntimeValidationError("MessageContextRegistry not found in execution context", {
      operation: "collectMessagesFromContext",
      field: "messageContextRegistry",
    });
  }

  // Use the specified contextId, or default to 'current'
  const contextId = config.contextId || "current";

  const namedContext = registry.get(contextId);

  if (!namedContext) {
    throw new RuntimeValidationError(`Context '${contextId}' not found`, {
      operation: "collectMessagesFromContext",
      field: "contextId",
      value: contextId,
    });
  }

  return [...namedContext.messages];
}

/**
 * LLM Node Processor
 * @param workflowExecution Workflow execution instance
 * @param node Node definition
 * @param context Processor context
 * @returns Execution result
 */
export async function llmHandler(
  workflowExecution: WorkflowExecution,
  node: RuntimeNode,
  context: LLMHandlerContext,
): Promise<LLMExecutionResult> {
  const config = node.config as LLMNodeConfig;
  const startTime = now();

  try {
    // 1. Collect messages from named context
    const messages = collectMessagesFromContext(config, workflowExecution);

    // 2. Add messages to conversation manager for this execution
    for (const message of messages) {
      context.conversationManager.addMessage(message);
    }

    // 3. Convert the configuration into executable data
    const executionData = {
      prompt: "", // Empty prompt - messages come from context references
      profileId: config.profileId,
      parameters: config.parameters || {},
      maxToolCallsPerRequest: config.maxToolCallsPerRequest,
      stream: false,
    };

    // 4. Call LLMExecutionCoordinator
    const result = await context.llmCoordinator.executeLLM(
      {
        executionId: workflowExecution.id,
        nodeId: node.id,
        prompt: executionData.prompt,
        profileId: executionData.profileId,
        parameters: executionData.parameters,
        maxToolCallsPerRequest: executionData.maxToolCallsPerRequest,
        transformContext: context.transformContext,
      },
      context.conversationManager,
    );

    const endTime = now();

    // 6. Append response to output context if specified
    if (result.success && config.outputContext) {
      const registry = (
        workflowExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }
      ).messageContextRegistry;
      if (registry) {
        const outputContextId = config.outputContext;
        const existingContext = registry.get(outputContextId);

        if (existingContext) {
          // Update existing context
          registry.update(outputContextId, [
            ...existingContext.messages,
            { role: "assistant", content: result.content || "" },
          ]);
        } else {
          // Create new context
          registry.register({
            id: outputContextId,
            messages: [{ role: "assistant", content: result.content || "" }],
            createdAt: Date.now(),
            updatedAt: Date.now(),
            metadata: { description: `Output from LLM node ${node.id}` },
          });
        }
      }
    }

    if (result.success) {
      return {
        status: "COMPLETED",
        content: result.content,
        toolCalls: result.toolCalls,
        executionTime: diffTimestamp(startTime, endTime),
      };
    } else {
      return {
        status: "FAILED",
        error: result.error,
        executionTime: diffTimestamp(startTime, endTime),
      };
    }
  } catch (error) {
    const endTime = now();
    return {
      status: "FAILED",
      error: getErrorOrNew(error),
      executionTime: diffTimestamp(startTime, endTime),
    };
  }
}
