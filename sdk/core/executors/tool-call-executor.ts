/**
 * Tool Call Executor
 * Specializes in handling tool call executions
 *
 * Core Responsibilities:
 * 1. Execute an array of tool calls
 * 2. Process individual tool calls
 * 3. Manage the results of tool executions
 * 4. Trigger related events
 * 5. Support streaming progress updates (NEW)
 *
 * Design Principles:
 * - Dedicated logic for tool execution
 * - Integration with the event coordinator
 * - Unified error handling
 * - Optional dependency injection for reusability in various scenarios
 * - Streaming progress support (inspired by pi-agent-core)
 *
 * Location:
 * Located in sdk/core/execution/executors, serving as a general-purpose executor
 * - Can be reused by modules such as Graph and Agent
 * - Graph-specific features are available through optional dependency injection
 */

import { isAbortError, checkInterruption, getErrorOrNew } from "@wf-agent/common-utils";
import type { ToolRegistry } from "../registry/tool-registry.js";
import type { EventRegistry } from "../registry/event-registry.js";
import type { Tool, ID, Event } from "@wf-agent/types";
import { now, diffTimestamp, generateId } from "@wf-agent/common-utils";
import type { ConversationSession } from "../messaging/conversation-session.js";
import { WorkflowExecutionInterruptedException, CheckpointError } from "@wf-agent/types";
import { MessageBuilder } from "../messaging/message-builder.js";
import type { CheckpointDependencies } from "../../workflow/checkpoint/utils/checkpoint-utils.js";
import type { ToolVisibilityStore } from "../../workflow/stores/tool-visibility-store.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ToolCallExecutor" });

/**
 * Tool streaming progress callback
 *
 * Called during tool execution to report partial results.
 * Inspired by pi-agent-core's onUpdate pattern.
 *
 * @param toolCallId Tool call ID
 * @param partialResult Partial result
 */
export type ToolProgressCallback = (toolCallId: string, partialResult: unknown) => void;

/**
 * Tool Call Task Information
 * Used to track the lifecycle of a single tool call
 */
export interface ToolCallTaskInfo {
  /** Task ID (unique identifier) */
  taskId: string;
  /** Batch ID (a identifier for a group of parallel calls) */
  batchId: string;
  /** Tool call ID (from LLM response) */
  toolCallId: string;
  /** Tool Name */
  toolName: string;
  /** Start time */
  startTime: number;
  /** End time */
  endTime?: number;
  /** Status */
  status: "running" | "completed" | "failed";
  /** Error message (in case of failure) */
  error?: string;
}

/**
 * Tool execution results
 */
export interface ToolExecutionResult {
  /** Tool call ID */
  toolCallId: string;
  /** Tool ID */
  toolId: ID;
  /** Whether it was successful */
  success: boolean;
  /** Execution result */
  result?: unknown;
  /** Error message */
  error?: string;
  /** Execution time (in milliseconds) */
  executionTime: number;
}

/**
 * Event Builder Interface
 * The implementation of event building is provided by the caller.
 */
export interface EventBuilder {
  buildMessageAddedEvent(params: unknown): Event;
  buildToolCallStartedEvent(params: unknown): Event;
  buildToolCallCompletedEvent(params: unknown): Event;
  buildToolCallFailedEvent(params: unknown): Event;
}

/**
 * Checkpoint creation function type
 */
export type CheckpointCreator = (
  options: { workflowExecutionId: string; toolId?: string; description?: string },
  dependencies: CheckpointDependencies,
) => Promise<string>;

/**
 * Tool Call Executor Class
 */
export class ToolCallExecutor {
  constructor(
    private toolService: ToolRegistry,
    private eventManager?: EventRegistry,
    private checkpointDependencies?: CheckpointDependencies,
    private toolVisibilityStore?: ToolVisibilityStore,
    private eventBuilder?: EventBuilder,
    private createCheckpointFn?: CheckpointCreator,
    private safeEmitFn?: (
      eventManager: EventRegistry | undefined,
      event: Event,
    ) => void | Promise<void>,
  ) {}

  /**
   * Array of tool calls to execute
   *
   * @param toolCalls - Array of tool calls to be executed
   * @param conversationState - Conversation manager
   * @param executionId - Execution ID
   * @param nodeId - Node ID
   * @param options - Execution options (including AbortSignal and progress callback)
   * @returns - Array of execution results
   */
  async executeToolCalls(
    toolCalls: Array<{ id: string; name: string; arguments: string }>,
    conversationState: ConversationSession,
    executionId?: string,
    nodeId?: string,
    options?: {
      abortSignal?: AbortSignal;
      /** Streaming progress callback (NEW) */
      onProgress?: ToolProgressCallback;
    },
  ): Promise<ToolExecutionResult[]> {
    logger.debug("Tool calls execution started", {
      executionId,
      nodeId,
      toolCallCount: toolCalls.length,
      toolNames: toolCalls.map(tc => tc.name),
    });

    // Check the interrupt signal.
    if (options?.abortSignal && options.abortSignal.aborted) {
      const result = checkInterruption(options.abortSignal);
      if (result.type === "paused" || result.type === "stopped") {
        throw new WorkflowExecutionInterruptedException(
          "Tool execution interrupted",
          result.type === "paused" ? "PAUSE" : "STOP",
          result.executionId || executionId || "",
          result.nodeId || nodeId || "",
        );
      }
      throw new WorkflowExecutionInterruptedException("Tool execution aborted", "STOP");
    }

    // Generate a batch ID (used to track this batch of parallel tool calls)
    const batchId = `batch_${generateId()}`;

    // Call the pre-generated task ID and task information for each tool.
    const taskInfos: Map<string, ToolCallTaskInfo> = new Map();
    for (const toolCall of toolCalls) {
      const taskId = `task_${generateId()}`;
      taskInfos.set(toolCall.id, {
        taskId,
        batchId,
        toolCallId: toolCall.id,
        toolName: toolCall.name,
        startTime: now(),
        status: "running",
      });
    }

    // Use Promise.allSettled to perform all tool calls in parallel
    // Even if some tool calls fail, the execution of other tool calls can continue.
    const executionPromises = toolCalls.map(toolCall =>
      this.executeSingleToolCall(
        toolCall,
        conversationState,
        executionId,
        nodeId,
        batchId,
        taskInfos.get(toolCall.id)!,
        options,
      ),
    );

    const settledResults = await Promise.allSettled(executionPromises);

    const successCount = settledResults.filter(
      r => r.status === "fulfilled" && r.value.success,
    ).length;
    const failedCount = settledResults.length - successCount;

    logger.debug("Tool calls execution completed", {
      batchId,
      executionId,
      nodeId,
      totalCount: toolCalls.length,
      successCount,
      failedCount,
    });

    // 转换结果为统一的 ToolExecutionResult[] 格式
    return settledResults.map((result, index) => {
      if (result.status === "fulfilled") {
        return result.value;
      } else {
        // Handling rejected cases
        const toolCall = toolCalls[index];
        if (!toolCall) {
          throw new Error(`Tool call at index ${index} is undefined`);
        }

        const error = result.reason;
        const errorMessage = error instanceof Error ? error.message : String(error);
        const executionTime = 0;

        // Building a failure message result using MessageBuilder failed.
        const toolMessage = MessageBuilder.buildToolMessage(toolCall.id, {
          success: false,
          error: errorMessage || "Tool execution failed",
          executionTime: 0,
          retryCount: 0,
        });
        conversationState.addMessage(toolMessage);

        // Trigger message addition event
        if (this.eventManager && this.eventBuilder && this.safeEmitFn) {
          const messageEvent = this.eventBuilder.buildMessageAddedEvent({
            executionId: executionId || "",
            role: toolMessage.role,
            content:
              typeof toolMessage.content === "string"
                ? toolMessage.content
                : JSON.stringify(toolMessage.content),
            nodeId,
          });
          this.safeEmitFn(this.eventManager, messageEvent);
        }

        // Retrieve task information from taskInfos.
        const taskInfo = taskInfos.get(toolCall.id)!;

        // Triggering the tool call failed event.
        if (this.eventManager && this.eventBuilder && this.safeEmitFn) {
          const failedEvent = this.eventBuilder.buildToolCallFailedEvent({
            executionId: executionId || "",
            nodeId: nodeId || "",
            toolId: toolCall.name,
            toolName: toolCall.name,
            error: new Error(errorMessage),
            taskId: taskInfo.taskId,
            batchId: taskInfo.batchId,
          });
          this.safeEmitFn(this.eventManager, failedEvent);
        }

        return {
          toolCallId: toolCall.id,
          toolId: toolCall.name,
          success: false,
          error: errorMessage,
          executionTime,
        };
      }
    });
  }

  /**
   * Execute a single tool call
   *
   * @param toolCall: The tool call to be executed
   * @param conversationState: The conversation manager
   * @param executionId: The execution ID
   * @param nodeId: The node ID
   * @param batchId: The batch ID (used for tracking parallel calls)
   * @param taskInfo: Task information (including a pre-generated taskId, etc.)
   * @param options: Execution options (including the AbortSignal)
   * @returns: The execution result
   */
  private async executeSingleToolCall(
    toolCall: { id: string; name: string; arguments: string },
    conversationState: ConversationSession,
    executionId: string | undefined,
    nodeId: string | undefined,
    batchId: string,
    taskInfo: ToolCallTaskInfo,
    options?: { abortSignal?: AbortSignal },
  ): Promise<ToolExecutionResult> {
    const startTime = taskInfo.startTime;

    // Obtain tool configuration
    let toolConfig: Tool | undefined;
    try {
      toolConfig = this.toolService.getTool(toolCall.name);
    } catch (error) {
      logger.warn(
        `Tool '${toolCall.name}' not found in registry, will fail during execution`,
        {
          toolCallId: toolCall.id,
          toolName: toolCall.name,
          executionId,
          nodeId,
        },
        undefined,
        getErrorOrNew(error),
      );
    }

    // Check whether the tool is within the current visibility context.
    if (executionId && this.toolVisibilityStore) {
      if (!this.toolVisibilityStore.isToolVisible(executionId, toolCall.name)) {
        const visibleTools = this.toolVisibilityStore.getVisibleTools(executionId);
        const errorMessage = `工具 '${toolCall.name}' 在当前作用域不可用。当前可用工具：[${Array.from(visibleTools).join(", ")}]`;

        // Failed to construct the tool result message using MessageBuilder.
        const toolMessage = MessageBuilder.buildToolMessage(toolCall.id, {
          success: false,
          error: errorMessage,
          executionTime: diffTimestamp(startTime, now()),
          retryCount: 0,
        });
        conversationState.addMessage(toolMessage);

        // Trigger message addition event
        if (this.eventManager && this.eventBuilder && this.safeEmitFn) {
          const messageEvent = this.eventBuilder.buildMessageAddedEvent({
            executionId: executionId || "",
            role: toolMessage.role,
            content:
              typeof toolMessage.content === "string"
                ? toolMessage.content
                : JSON.stringify(toolMessage.content),
            nodeId,
          });
          await this.safeEmitFn(this.eventManager, messageEvent);
        }

        // Triggering the tool invocation failed event.
        if (this.eventManager && this.eventBuilder && this.safeEmitFn) {
          const failedEvent = this.eventBuilder.buildToolCallFailedEvent({
            executionId: executionId || "",
            nodeId: nodeId || "",
            toolId: toolCall.name,
            toolName: toolCall.name,
            error: new Error(errorMessage),
            taskId: taskInfo.taskId,
            batchId,
          });
          await this.safeEmitFn(this.eventManager, failedEvent);
        }

        return {
          toolCallId: toolCall.id,
          toolId: toolCall.name,
          success: false,
          error: errorMessage,
          executionTime: diffTimestamp(startTime, now()),
        };
      }
    }

    // Create a checkpoint before the tool call (if configured).
    if (
      toolConfig?.createCheckpoint &&
      this.checkpointDependencies &&
      executionId &&
      this.createCheckpointFn
    ) {
      const checkpointConfig = toolConfig.createCheckpoint;
      if (
        checkpointConfig === true ||
        checkpointConfig === "before" ||
        checkpointConfig === "both"
      ) {
        try {
          await this.createCheckpointFn(
            {
              workflowExecutionId: executionId,
              toolId: toolCall.name,
              description:
                toolConfig.checkpointDescriptionTemplate || `Before tool: ${toolCall.name}`,
            },
            this.checkpointDependencies,
          );
        } catch (error) {
          throw new CheckpointError(
            `Failed to create checkpoint before tool "${toolCall.name}"`,
            "create",
            undefined,
            undefined,
            undefined,
            {
              toolId: toolCall.name,
              originalError: error instanceof Error ? error : new Error(String(error)),
            },
          );
        }
      }
    }

    // Trigger the tool call start event.
    if (this.eventManager && this.eventBuilder && this.safeEmitFn) {
      const startedEvent = this.eventBuilder.buildToolCallStartedEvent({
        executionId: executionId || "",
        nodeId: nodeId || "",
        toolId: toolCall.name,
        toolName: toolCall.name,
        toolArguments: toolCall.arguments,
        taskId: taskInfo.taskId,
        batchId,
      });
      await this.safeEmitFn(this.eventManager, startedEvent);
    }

    // Build execution options that support reading from tool configuration.
    const executionOptions: {
      timeout: number;
      retries: number;
      retryDelay: number;
      signal?: AbortSignal;
    } = {
      timeout: (toolConfig?.config as { timeout?: number })?.timeout || 30000,
      retries: (toolConfig?.config as { maxRetries?: number })?.maxRetries || 0,
      retryDelay: (toolConfig?.config as { retryDelay?: number })?.retryDelay || 1000,
      signal: options?.abortSignal, // Pass the AbortSignal
    };

    // Call ToolRegistry to execute the tool.
    const result = await this.toolService.execute(
      toolCall.name,
      JSON.parse(toolCall.arguments),
      executionOptions,
      executionId,
    );

    const executionTime = diffTimestamp(startTime, now());

    if (result.isErr()) {
      const error = result.error;
      const errorMessage = error.message;

      // Handle AbortError
      if (isAbortError(error)) {
        const result = checkInterruption(options?.abortSignal);
        // Only PAUSE or STOP will convert to WorkflowExecutionInterruptedException.
        if (result.type === "paused" || result.type === "stopped") {
          throw new WorkflowExecutionInterruptedException(
            "Tool execution interrupted",
            result.type === "paused" ? "PAUSE" : "STOP",
            result.executionId || executionId || "",
            result.nodeId || nodeId || "",
          );
        }
        // Aborted or continued, with the original error being re-thrown.
        throw error;
      }

      // Failed to construct the tool result message using MessageBuilder.
      const toolMessage = MessageBuilder.buildToolMessage(toolCall.id, {
        success: false,
        error: errorMessage || "Tool execution failed",
        executionTime,
        retryCount: 0,
      });
      conversationState.addMessage(toolMessage);

      // Trigger message addition event
      if (this.eventManager && this.eventBuilder && this.safeEmitFn) {
        const messageEvent = this.eventBuilder.buildMessageAddedEvent({
          executionId: executionId || "",
          role: toolMessage.role,
          content:
            typeof toolMessage.content === "string"
              ? toolMessage.content
              : JSON.stringify(toolMessage.content),
          nodeId,
        });
        await this.safeEmitFn(this.eventManager, messageEvent);
      }

      // Trigger tool call failure event
      if (this.eventManager && this.eventBuilder && this.safeEmitFn) {
        const failedEvent = this.eventBuilder.buildToolCallFailedEvent({
          executionId: executionId || "",
          nodeId: nodeId || "",
          toolId: toolCall.name,
          toolName: toolCall.name,
          error: new Error(errorMessage),
          taskId: taskInfo.taskId,
          batchId,
        });
        await this.safeEmitFn(this.eventManager, failedEvent);
      }

      return {
        toolCallId: toolCall.id,
        toolId: toolCall.name,
        success: false,
        error: errorMessage,
        executionTime,
      };
    }

    // Execution successful.
    const serviceResult = result.value;

    // Construct a successful tool result message using MessageBuilder.
    const toolMessage = MessageBuilder.buildToolMessage(toolCall.id, {
      success: serviceResult.success,
      result: serviceResult.result,
      error: serviceResult.error,
      executionTime,
      retryCount: 0,
    });
    conversationState.addMessage(toolMessage);

    // Trigger message addition event
    if (this.eventManager && this.eventBuilder && this.safeEmitFn) {
      const messageEvent = this.eventBuilder.buildMessageAddedEvent({
        executionId: executionId || "",
        role: toolMessage.role,
        content:
          typeof toolMessage.content === "string"
            ? toolMessage.content
            : JSON.stringify(toolMessage.content),
        nodeId,
      });
      await this.safeEmitFn(this.eventManager, messageEvent);
    }

    // A checkpoint is created after the tool is called (if configured).
    if (
      toolConfig?.createCheckpoint &&
      this.checkpointDependencies &&
      executionId &&
      this.createCheckpointFn
    ) {
      const checkpointConfig = toolConfig.createCheckpoint;
      if (checkpointConfig === "after" || checkpointConfig === "both") {
        try {
          await this.createCheckpointFn(
            {
              workflowExecutionId: executionId,
              toolId: toolCall.name,
              description:
                toolConfig.checkpointDescriptionTemplate || `After tool: ${toolCall.name}`,
            },
            this.checkpointDependencies,
          );
        } catch (error) {
          throw new CheckpointError(
            `Failed to create checkpoint after tool "${toolCall.name}"`,
            "create",
            undefined,
            undefined,
            undefined,
            {
              toolId: toolCall.name,
              originalError: error instanceof Error ? error : new Error(String(error)),
            },
          );
        }
      }
    }

    // Trigger the completion event for the tool invocation.
    if (this.eventManager && this.eventBuilder && this.safeEmitFn) {
      const completedEvent = this.eventBuilder.buildToolCallCompletedEvent({
        executionId: executionId || "",
        nodeId: nodeId || "",
        toolId: toolCall.name,
        toolName: toolCall.name,
        toolResult: serviceResult.result,
        executionTime,
        taskId: taskInfo.taskId,
        batchId,
      });
      await this.safeEmitFn(this.eventManager, completedEvent);
    }

    return {
      toolCallId: toolCall.id,
      toolId: toolCall.name,
      success: true,
      result: serviceResult.result,
      executionTime,
    };
  }
}
