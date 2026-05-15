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

import { getErrorOrNew } from "@wf-agent/common-utils";
import {
  checkWorkflowInterruption,
  type ExecutionInterruptionCheckResult,
} from "../utils/interruption/index.js";
import type { ToolRegistry } from "../registry/tool-registry.js";
import type { EventRegistry } from "../registry/event-registry.js";
import type { Tool, ID, Event } from "@wf-agent/types";
import { now, diffTimestamp, generateId } from "@wf-agent/common-utils";
import type { ConversationSession } from "../messaging/conversation-session.js";
import { WorkflowCheckpointError } from "@wf-agent/types";
import { MessageBuilder } from "../messaging/message-builder.js";
import type { CheckpointDependencies } from "../../workflow/checkpoint/utils/checkpoint-utils.js";
import type { ToolVisibilityStore } from "../../workflow/stores/tool-visibility-store.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type { ToolFailureProtectionState } from "../state-managers/tool-failure-protection-state.js";
import type { ToolMetricsCollector } from "../metrics/tool-collector.js";

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
  buildToolCallBlockedEvent?(params: unknown): Event; // NEW - optional for backward compatibility
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
    private toolFailureProtection?: ToolFailureProtectionState,
    private metricsCollector?: ToolMetricsCollector,
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
      const result = checkWorkflowInterruption(options.abortSignal);
      
      // Return interruption info instead of throwing
      // This allows callers to handle interruption gracefully
      logger.info("Tool execution interrupted before starting", {
        executionId,
        nodeId,
        interruptionType: result.type,
        toolCallCount: toolCalls.length,
      });
      
      // Build cancelled results for all tool calls
      return toolCalls.map(toolCall => ({
        toolCallId: toolCall.id,
        toolId: toolCall.name,
        success: false,
        error: `Tool execution ${result.type === "paused" ? "paused" : "cancelled"}: ${result.type}`,
        executionTime: 0,
      }));
    }

    // Generate a batch ID (used to track this batch of parallel tool calls)
    const batchId = `batch_${generateId()}`;

    // Create a batch-level AbortController for coordinated cancellation
    // This ensures that if one tool is interrupted, all other tools in the batch can be notified
    const batchController = new AbortController();
    
    // Combine external abort signal with batch controller
    // If either signals abort, all tools will be notified
    let combinedSignal: AbortSignal;
    if (options?.abortSignal) {
      // Use combineAbortSignals from common-utils if available, otherwise manual combination
      const { combineAbortSignals } = await import("@wf-agent/common-utils");
      const result = combineAbortSignals([options.abortSignal, batchController.signal]);
      combinedSignal = result.signal;
    } else {
      combinedSignal = batchController.signal;
    }

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
    const executionPromises = toolCalls.map(async (toolCall) => {
      try {
        return await this.executeSingleToolCall(
          toolCall,
          conversationState,
          executionId,
          nodeId,
          batchId,
          taskInfos.get(toolCall.id)!,
          { ...options, abortSignal: combinedSignal }, // Pass combined signal
        );
      } catch (error) {
        // If a tool fails due to interruption, abort the entire batch
        if (error instanceof Error && error.name === "AbortError") {
          logger.info("Tool interrupted, cancelling remaining tools in batch", {
            batchId,
            toolCallId: toolCall.id,
            toolName: toolCall.name,
          });
          
          // Abort the batch controller to notify other tools
          if (!batchController.signal.aborted) {
            batchController.abort(error);
          }
        }
        throw error;
      }
    });

    const settledResults = await Promise.allSettled(executionPromises);

    // Clean up batch controller if not already aborted
    if (!batchController.signal.aborted) {
      batchController.abort();
    }

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

        // Record failed tool execution for failure protection (NEW)
        if (this.toolFailureProtection) {
          this.toolFailureProtection.recordFailure(toolCall.name, errorMessage || "Tool execution failed");
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

    // Record tool call start metrics
    if (executionId && this.metricsCollector) {
      this.metricsCollector.recordToolCallStart(toolCall.name, executionId);
    }

    // Track operation state for mid-node resume (if execution registry is available)
    const executionRegistry = this.checkpointDependencies?.workflowExecutionRegistry;
    let hasOperationState = false;
    
    if (executionRegistry && executionId) {
      const executionEntity = executionRegistry.get(executionId);
      if (executionEntity) {
        executionEntity.state.setCurrentOperation({
          type: "TOOL_EXECUTION",
          operationId: toolCall.id,
          nodeId: nodeId || "",
          startedAt: now(),
          progress: {
            itemsProcessed: 0,
            percentage: 0,
          },
          metadata: {
            toolName: toolCall.name,
            arguments: toolCall.arguments,
            batchId,
            taskId: taskInfo.taskId,
          },
        });
        hasOperationState = true;
      }
    }

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
        const errorMessage = `Tool '${toolCall.name}' Not available in the current scope. Currently available tools: [${Array.from(visibleTools).join(", ")}]`;

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

        // Record tool call failure metrics
        if (executionId && this.metricsCollector) {
          this.metricsCollector.recordToolCallComplete(
            toolCall.name,
            executionId,
            diffTimestamp(startTime, now()),
            false,
            JSON.stringify(toolCall.arguments).length,
          );
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

    // Check tool failure protection (NEW)
    if (executionId && this.toolFailureProtection) {
      const checkResult = this.toolFailureProtection.canExecuteTool(toolCall.name);
      
      if (!checkResult.allowed) {
        const errorMessage = checkResult.reason || `Tool '${toolCall.name}' is blocked due to consecutive failures`;
        
        logger.warn(`Tool call blocked by failure protection`, {
          toolCallId: toolCall.id,
          toolName: toolCall.name,
          executionId,
          nodeId,
          failureCount: checkResult.failureCount,
          remainingCooldown: checkResult.remainingCooldown,
          lastError: checkResult.lastError,
        });

        // Build tool result message for blocked tool
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

        // Trigger tool call blocked event (NEW)
        if (this.eventManager && this.eventBuilder?.buildToolCallBlockedEvent && this.safeEmitFn) {
          const blockedEvent = this.eventBuilder.buildToolCallBlockedEvent({
            executionId: executionId || "",
            nodeId: nodeId || "",
            toolId: toolCall.name,
            toolName: toolCall.name,
            failureCount: checkResult.failureCount,
            lastError: checkResult.lastError,
            remainingCooldown: checkResult.remainingCooldown,
            reason: checkResult.reason,
          });
          await this.safeEmitFn(this.eventManager, blockedEvent);
        }

        // Record tool call blocked metrics
        if (executionId && this.metricsCollector) {
          this.metricsCollector.recordToolCallComplete(
            toolCall.name,
            executionId,
            diffTimestamp(startTime, now()),
            false,
          );
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
          throw new WorkflowCheckpointError(
            `Failed to create checkpoint before tool "${toolCall.name}"`,
            "create",
            undefined,
            undefined,
            undefined,
            executionId,
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

    // Check if this is an interactive tool
    const isInteractiveTool = toolConfig?.metadata?.requiresUserInteraction === true;

    // Call ToolRegistry to execute the tool.
    try {
      // For interactive tools, pass context with event manager and execution info
      let context: Record<string, unknown> | undefined;
      if (isInteractiveTool) {
        context = {
          eventManager: this.eventManager,
          executionId,
          nodeId,
          parentExecutionEntity: this.checkpointDependencies?.workflowExecutionRegistry?.get(executionId || ""),
        };
      }

      const result = await this.toolService.execute(
        toolCall.name,
        JSON.parse(toolCall.arguments),
        executionOptions,
        executionId,
        context, // Pass context for interactive tools
      );

      const executionTime = diffTimestamp(startTime, now());

      // Clear operation state on success
      if (hasOperationState && executionRegistry && executionId) {
        const executionEntity = executionRegistry.get(executionId);
        executionEntity?.state.clearOperation();
      }

      if (result.isErr()) {
        const error = result.error;
        const errorMessage = error.message;

        // Handle InterruptionError - return interruption result instead of throwing
        if (error instanceof Error && error.name === "InterruptionError" && "interruption" in error) {
          const interruptionResult = (error as { interruption: ExecutionInterruptionCheckResult }).interruption;
          
          // Log interruption for observability
          logger.info("Tool execution interrupted during execution", {
            toolCallId: toolCall.id,
            toolName: toolCall.name,
            executionId,
            nodeId,
            interruptionType: interruptionResult.type,
          });
          
          // Clear operation state on interruption
          if (hasOperationState && executionRegistry && executionId) {
            const executionEntity = executionRegistry.get(executionId);
            executionEntity?.state.clearOperation();
          }
          
          // Build interruption result message
          const interruptionMessage = `Tool execution ${interruptionResult.type === "paused" ? "paused" : "cancelled"}`;
          const toolMessage = MessageBuilder.buildToolMessage(toolCall.id, {
            success: false,
            error: interruptionMessage,
            executionTime: diffTimestamp(startTime, now()),
            retryCount: 0,
          });
          conversationState.addMessage(toolMessage);
          
          // Return interruption result instead of throwing
          return {
            toolCallId: toolCall.id,
            toolId: toolCall.name,
            success: false,
            error: interruptionMessage,
            executionTime: diffTimestamp(startTime, now()),
          };
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

        // Record failed tool execution for failure protection (NEW)
        if (this.toolFailureProtection) {
          this.toolFailureProtection.recordFailure(toolCall.name, errorMessage || "Tool execution failed");
        }

        // Record tool call failure metrics
        if (executionId && this.metricsCollector) {
          this.metricsCollector.recordToolCallComplete(
            toolCall.name,
            executionId,
            executionTime,
            false,
            JSON.stringify(toolCall.arguments).length,
          );
        }

        return {
          toolCallId: toolCall.id,
          toolId: toolCall.name,
          success: false,
          error: errorMessage,
          executionTime,
        };
      }

      // Tool execution succeeded
      const successResult = result.value;

      // Create a checkpoint after the tool call (if configured).
      if (
        toolConfig?.createCheckpoint &&
        this.checkpointDependencies &&
        executionId &&
        this.createCheckpointFn
      ) {
        const checkpointConfig = toolConfig.createCheckpoint;
        if (
          checkpointConfig === true ||
          checkpointConfig === "after" ||
          checkpointConfig === "both"
        ) {
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
            throw new WorkflowCheckpointError(
              `Failed to create checkpoint after tool "${toolCall.name}"`,
              "create",
              undefined,
              undefined,
              undefined,
              executionId,
              {
                toolId: toolCall.name,
                originalError: error instanceof Error ? error : new Error(String(error)),
              },
            );
          }
        }
      }

      // Build a successful tool result message using MessageBuilder.
      const toolMessage = MessageBuilder.buildToolMessage(toolCall.id, {
        success: true,
        result: successResult,
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

      // Triggering the tool invocation completed the event.
      if (this.eventManager && this.eventBuilder && this.safeEmitFn) {
        const completedEvent = this.eventBuilder.buildToolCallCompletedEvent({
          executionId: executionId || "",
          nodeId: nodeId || "",
          toolId: toolCall.name,
          toolName: toolCall.name,
          result: successResult,
          executionTime,
          taskId: taskInfo.taskId,
          batchId,
        });
        await this.safeEmitFn(this.eventManager, completedEvent);
      }

      // Record successful tool execution for failure protection (NEW)
      if (this.toolFailureProtection) {
        this.toolFailureProtection.recordSuccess(toolCall.name);
      }

      // Record tool call completion metrics
      if (executionId && this.metricsCollector) {
        this.metricsCollector.recordToolCallComplete(
          toolCall.name,
          executionId,
          executionTime,
          true,
          JSON.stringify(toolCall.arguments).length,
          JSON.stringify(successResult).length,
        );
      }

      return {
        toolCallId: toolCall.id,
        toolId: toolCall.name,
        success: true,
        result: successResult,
        executionTime,
      };
    } catch (error) {
      // On exception, clear operation state if interrupted
      if (hasOperationState && executionRegistry && executionId) {
        const executionEntity = executionRegistry.get(executionId);
        if (error instanceof Error && error.name === "AbortError") {
          // Preserve operation state for resume
          logger.info("Tool execution interrupted, preserving operation state for resume", {
            executionId,
            toolCallId: toolCall.id,
          });
        } else {
          // Clear operation state on other errors
          executionEntity?.state.clearOperation();
        }
      }
      throw error;
    }
  }
}
