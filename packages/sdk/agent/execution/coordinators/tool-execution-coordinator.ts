/**
 * ToolExecutionCoordinator - Tool Execution Coordinator
 *
 * Coordinates the execution of tool calls within an agent loop iteration.
 * Handles approval workflows, batch processing, and event emission.
 *
 * Responsibilities:
 * - Execute tool calls with approval coordination
 * - Handle auto-approved and confirmation-required tools
 * - Emit tool execution events
 * - Manage tool call lifecycle (start/end)
 *
 * Design Principles:
 * - Single responsibility: Only handles tool execution
 * - Delegation pattern: Uses ToolApprovalCoordinator for approval logic
 * - Event-driven: Emits events for tool execution lifecycle
 */

import type {
  AgentStreamEvent,
  LLMToolCall,
  ToolApprovalOptions,
  ToolApprovalHandler,
  AgentHookTriggeredEvent,
} from "@wf-agent/types";
import type { TimeoutHandle } from "../../../shared/types/timeout.js";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { ConversationSession } from "../../../shared/messaging/conversation-session.js";
import type { ToolCallExecutor, ToolExecutionResult } from "../../../services/executors/tool-call-executor.js";
import type { EventRegistry } from "../../../shared/registry/event-registry.js";
import { ToolApprovalCoordinator } from "../../../shared/coordinators/tool-approval-coordinator.js";
import { executeAgentHook } from "../handlers/hook-handlers/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { combineAbortSignals } from "../../../shared/utils/interruption/index.js";
import type { AgentStateCoordinator } from "../../state-managers/agent-state-coordinator.js";

const logger = createContextualLogger({ component: "ToolExecutionCoordinator" });

/**
 * ToolExecutionCoordinator Dependencies
 */
export interface ToolExecutionCoordinatorDependencies {
  /** Tool Call Executor */
  toolCallExecutor: ToolCallExecutor;
  /** Event Registry (optional) */
  eventManager?: EventRegistry;
  /** Tool Approval Handler (optional) */
  toolApprovalHandler?: ToolApprovalHandler;
  /** Custom event emitter for agent hooks (optional) */
  emitEvent?: (event: AgentHookTriggeredEvent) => Promise<void>;
  /** Per-tool execution timeout in milliseconds (optional, default: no timeout) */
  toolTimeout?: number;
}

/**
 * ToolExecutionCoordinator
 *
 * Coordinates tool execution within agent loop iterations
 */
export class ToolExecutionCoordinator {
  private readonly toolCallExecutor: ToolCallExecutor;
  private readonly approvalCoordinator: ToolApprovalCoordinator;
  private readonly eventManager?: EventRegistry;
  private readonly toolApprovalHandler?: ToolApprovalHandler;
  private readonly emitEvent?: (event: AgentHookTriggeredEvent) => Promise<void>;
  private readonly stateCoordinator: AgentStateCoordinator;
  private readonly toolTimeout?: number;

  constructor(deps: ToolExecutionCoordinatorDependencies & { stateCoordinator: AgentStateCoordinator }) {
    this.toolCallExecutor = deps.toolCallExecutor;
    this.eventManager = deps.eventManager;
    this.toolApprovalHandler = deps.toolApprovalHandler;
    this.emitEvent = deps.emitEvent;
    this.stateCoordinator = deps.stateCoordinator;
    this.toolTimeout = deps.toolTimeout;

    // Initialize approval coordinator
    this.approvalCoordinator = new ToolApprovalCoordinator(deps.eventManager);
  }

  /**
   * Execute tool calls (sync mode)
   *
   * @param entity Agent loop entity
   * @param conversationManager Conversation manager
   * @param toolCalls Array of tool calls to execute
   * @returns Array of tool execution results with toolCallId, success, result, error
   */
  async executeToolCalls(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolCalls: Array<{ id: string; name: string; arguments: string }>,
  ): Promise<ToolExecutionResult[]> {
    const agentLoopId = entity.id;
    const iteration = entity.state.currentIteration;

    // Inject tool failure protection from entity (Task #4)
    if (entity.toolFailureProtection) {
      this.toolCallExecutor.setToolFailureProtection(entity.toolFailureProtection);
    }

    logger.debug("Executing tool calls", {
      agentLoopId,
      iteration,
      toolCallCount: toolCalls.length,
    });

    const results: ToolExecutionResult[] = [];

    // When no tool approval handler is configured, execute all tools directly
    // without going through the approval coordinator. This enables agent loops
    // with tool calls to work in testing/dev scenarios without requiring a
    // tool approval handler to be registered.
    if (!this.toolApprovalHandler) {
      for (const toolCall of toolCalls) {
        entity.state.recordToolCallStart(toolCall.id, toolCall.name, toolCall.arguments);
        const result = await this.executeSingleApprovedTool(entity, conversationManager, toolCall, {
          success: true,
          result: {},
        });
        results.push(result);
      }
      return results;
    }

    // Convert to LLMToolCall format for batch processing
    const llmToolCalls = toolCalls.map(tc => ({
      id: tc.id,
      type: "function" as const,
      function: {
        name: tc.name,
        arguments: tc.arguments,
      },
    }));

    // Get approval config from agent configuration
    const approvalOptions = this.getApprovalOptions(entity);

    // Process batch through approval coordinator
    const batchResult = await this.approvalCoordinator.processToolBatch(
      llmToolCalls,
      approvalOptions,
      entity.id,
      entity.nodeId || "unknown",
      {
        requestApproval: async request => {
          return this.requestAgentApproval(request, entity);
        },
      },
      this.eventManager,
    );

    // Execute auto-approved tools
    for (let i = 0; i < batchResult.autoExecuted.length; i++) {
      const autoResult = batchResult.autoExecuted[i];
      // Auto-executed tools correspond to the first N tools in the batch
      if (i < toolCalls.length && autoResult) {
        const originalToolCall = toolCalls[i]!;
        entity.state.recordToolCallStart(
          originalToolCall.id,
          originalToolCall.name,
          originalToolCall.arguments,
        );
        const result = await this.executeSingleApprovedTool(
          entity,
          conversationManager,
          originalToolCall,
          autoResult,
        );
        results.push(result);
      }
    }

    // Handle confirmation-required tool
    if (batchResult.confirmationRequired && batchResult.confirmationResult?.approved) {
      const confirmedToolCall = toolCalls.find(
        tc => tc.id === batchResult.confirmationRequired?.id,
      );
      if (confirmedToolCall) {
        // Apply edited parameters if provided
        if (batchResult.confirmationResult.editedParameters) {
          confirmedToolCall.arguments = JSON.stringify(
            batchResult.confirmationResult.editedParameters,
          );
        }

        // Add user instruction if provided
        if (batchResult.confirmationResult.userInstruction) {
          conversationManager.addMessage({
            role: "user",
            content: batchResult.confirmationResult.userInstruction,
          });
        }

        // Create placeholder result for execution
        const placeholderResult = {
          success: true,
          result: {},
          executionTime: 0,
          retryCount: 0,
        };

        entity.state.recordToolCallStart(
          confirmedToolCall.id,
          confirmedToolCall.name,
          confirmedToolCall.arguments,
        );
        const result = await this.executeSingleApprovedTool(
          entity,
          conversationManager,
          confirmedToolCall,
          placeholderResult,
        );
        results.push(result);
      }

      // Continue with remaining queue if flag is set
      if (
        batchResult.confirmationResult.continueBatch !== false &&
        batchResult.remainingQueue.length > 0
      ) {
        const remainingToolCalls = batchResult.remainingQueue.map(tc => ({
          id: tc.id,
          name: tc.function?.name || "unknown",
          arguments: tc.function?.arguments || "{}",
        }));

        // Recursively process remaining tools with results from nested calls
        const remainingResults = await this.executeToolCalls(entity, conversationManager, remainingToolCalls);
        results.push(...remainingResults);
      }
    }

    logger.debug("Tool calls execution completed", {
      agentLoopId,
      iteration,
      autoExecutedCount: batchResult.autoExecuted.length,
      hasConfirmation: !!batchResult.confirmationRequired,
    });

    return results;
  }

  /**
   * Execute tool calls (stream mode)
   *
   * @param entity Agent loop entity
   * @param conversationManager Conversation manager
   * @param toolCalls Array of tool calls to execute
   * @returns Stream event generator
   */
  async *executeToolCallsStream(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolCalls: Array<{ id: string; function: { name: string; arguments: string } }>,
  ): AsyncGenerator<AgentStreamEvent> {
    const agentLoopId = entity.id;

    // Inject tool failure protection from entity (Task #4)
    if (entity.toolFailureProtection) {
      this.toolCallExecutor.setToolFailureProtection(entity.toolFailureProtection);
    }

    logger.debug("Executing tool calls in stream", {
      agentLoopId,
      iteration: entity.state.currentIteration,
      toolCallCount: toolCalls.length,
    });

    const executorToolCalls = toolCalls.map(tc => ({
      id: tc.id,
      name: tc.function.name,
      arguments: tc.function.arguments,
    }));

    const toolResults = await this.toolCallExecutor.executeToolCalls(
      executorToolCalls,
      conversationManager,
      entity.id,
      entity.nodeId,
      { abortSignal: entity.getAbortSignal() },
    );

    for (const result of toolResults) {
      const toolCall = toolCalls.find(tc => tc.id === result.toolCallId);
      const args = toolCall ? JSON.parse(toolCall.function.arguments) : {};
      const toolCallInfo = {
        id: result.toolCallId,
        name: toolCall?.function.name || "",
        arguments: args,
      };

      await executeAgentHook(
        entity,
        "BEFORE_TOOL_CALL",
        this.emitAgentEvent.bind(this),
        this.stateCoordinator,
        toolCallInfo,
      );

      const startEvent: AgentStreamEvent = {
        type: "tool_execution_start",
        timestamp: Date.now(),
        agentLoopId,
        toolCallId: result.toolCallId,
        toolName: toolCallInfo.name,
        args,
        iteration: entity.state.currentIteration,
      };
      yield startEvent;
      await this.emitToRegistry(startEvent, entity);

      entity.state.recordToolCallStart(result.toolCallId, toolCallInfo.name, args);

      if (result.success) {
        entity.state.recordToolCallEnd(result.toolCallId, result.result);
        await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent.bind(this), this.stateCoordinator, {
          ...toolCallInfo,
          result: result.result,
        });

        const endEvent: AgentStreamEvent = {
          type: "tool_execution_end",
          timestamp: Date.now(),
          agentLoopId,
          toolCallId: result.toolCallId,
          toolName: toolCallInfo.name,
          result: {
            success: true,
            result: result.result,
            executionTime: result.executionTime,
            retryCount: 0,
          },
          duration: result.executionTime,
        };
        yield endEvent;
        await this.emitToRegistry(endEvent, entity);
      } else {
        entity.state.recordToolCallEnd(result.toolCallId, undefined, result.error);
        await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent.bind(this), this.stateCoordinator, {
          ...toolCallInfo,
          error: result.error,
        });

        const endEvent: AgentStreamEvent = {
          type: "tool_execution_end",
          timestamp: Date.now(),
          agentLoopId,
          toolCallId: result.toolCallId,
          toolName: toolCallInfo.name,
          result: {
            success: false,
            error: result.error,
            executionTime: result.executionTime,
            retryCount: 0,
          },
          duration: result.executionTime,
        };
        yield endEvent;
        await this.emitToRegistry(endEvent, entity);
      }
    }
  }

  /**
   * Execute a single approved tool call
   */
  private async executeSingleApprovedTool(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolCall: { id: string; name: string; arguments: string },
    executionResult: { success: boolean; result?: unknown; error?: string },
  ): Promise<ToolExecutionResult> {
    const agentLoopId = entity.id;

    const toolCallInfo = {
      id: toolCall.id,
      name: toolCall.name,
      arguments: toolCall.arguments ? JSON.parse(toolCall.arguments) : {},
    };

    await executeAgentHook(
      entity,
      "BEFORE_TOOL_CALL",
      this.emitAgentEvent.bind(this),
      this.stateCoordinator,
      toolCallInfo,
    );

    if (executionResult.success) {
      logger.debug("Tool call succeeded", {
        agentLoopId,
        toolCallId: toolCall.id,
        toolName: toolCallInfo.name,
      });

      // Phase 1: Tool Execution Timeout
      // Register a per-tool timeout on the entity's TimeoutManager
      let toolTimeoutHandle: TimeoutHandle | undefined;
      const toolTimeoutController = new AbortController();

      if (this.toolTimeout && this.toolTimeout > 0) {
        toolTimeoutHandle = entity.timeoutManager.register({
          id: `tool-${toolCall.id}`,
          duration: this.toolTimeout,
          onTimeout: () => {
            logger.warn(`Tool '${toolCallInfo.name}' timed out after ${this.toolTimeout}ms`, {
              agentLoopId,
              toolCallId: toolCall.id,
              toolName: toolCallInfo.name,
            });
            toolTimeoutController.abort(
              `Tool ${toolCallInfo.name} timed out after ${this.toolTimeout}ms`,
            );
          },
          tag: "tool-execution",
          metadata: {
            toolName: toolCallInfo.name,
            toolCallId: toolCall.id,
          },
          interruptionState: entity.getInterruptionState(),
        });
      }

      // Execute actual tool call with combined abort signal
      try {
        const combinedSignal = combineAbortSignals([
          entity.getAbortSignal(),
          toolTimeoutController.signal,
        ]);

        const toolResults = await this.toolCallExecutor.executeToolCalls(
          [toolCall],
          conversationManager,
          entity.id,
          entity.nodeId,
          { abortSignal: combinedSignal },
        );

        const result = toolResults[0]!;
        if (result?.success) {
          entity.state.recordToolCallEnd(toolCall.id, result.result);
          await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent.bind(this), this.stateCoordinator, {
            ...toolCallInfo,
            result: result.result,
          });
        } else {
          entity.state.recordToolCallEnd(
            toolCall.id,
            undefined,
            result?.error || "Tool execution failed",
          );
          await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent.bind(this), this.stateCoordinator, {
            ...toolCallInfo,
            error: result?.error,
          });
        }

        return result;
      } catch (error) {
        // Check if the error was caused by tool timeout
        if (toolTimeoutController.signal.aborted) {
          const timeoutError = `Tool '${toolCallInfo.name}' timed out after ${this.toolTimeout}ms`;
          logger.error(timeoutError, { agentLoopId, toolCallId: toolCall.id });

          entity.state.recordToolCallEnd(toolCall.id, undefined, timeoutError);

          return {
            toolCallId: toolCall.id,
            toolId: "",
            success: false,
            error: timeoutError,
            executionTime: this.toolTimeout || 0,
          };
        }
        // Re-throw non-timeout errors
        throw error;
      } finally {
        if (toolTimeoutHandle) {
          toolTimeoutHandle.cancel();
        }
      }
    } else {
      logger.warn("Tool call rejected or failed", {
        agentLoopId,
        toolCallId: toolCall.id,
        toolName: toolCallInfo.name,
        error: executionResult.error,
      });
      entity.state.recordToolCallEnd(
        toolCall.id,
        undefined,
        executionResult.error || "Rejected by user",
      );
      await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent.bind(this), this.stateCoordinator, {
        ...toolCallInfo,
        error: executionResult.error,
      });

      return {
        toolCallId: toolCall.id,
        toolId: "",
        success: false,
        error: executionResult.error || "Rejected by user",
        executionTime: 0,
      };
    }
  }

  /**
   * Get approval options from agent configuration
   */
  private getApprovalOptions(_entity: AgentLoopEntity): ToolApprovalOptions {
    // When no tool approval handler is configured, auto-approve all tools.
    // This enables agent loops with tool calls to work in testing/dev scenarios.
    // When a handler is configured, it controls the approval policy.
    return {
      autoApprovalEnabled: !this.toolApprovalHandler,
    };
  }

  /**
   * Request tool approval for agent mode
   */
  private async requestAgentApproval(
    request: { toolCall: { id: string; function?: { name?: string; arguments?: string } } },
    entity: AgentLoopEntity,
  ): Promise<{
    approved: boolean;
    toolCallId: string;
    editedParameters?: Record<string, unknown>;
    userInstruction?: string;
    rejectionReason?: string;
  }> {
    // Use registered handler if available
    if (this.toolApprovalHandler) {
      try {
        // Construct full LLMToolCall object
        const llmToolCall: LLMToolCall = {
          id: request.toolCall.id,
          type: "function",
          function: {
            name: request.toolCall.function?.name || "unknown",
            arguments: request.toolCall.function?.arguments || "{}",
          },
        };

        // Pause the TimeoutManager while waiting for user approval
        entity.timeoutManager.pause();
        try {
          const result = await this.toolApprovalHandler.requestApproval({
            toolCall: llmToolCall,
            batchId: undefined,
            toolIndex: 0,
            totalTools: 1,
            pendingQueue: [],
            contextId: entity.id,
            nodeId: entity.nodeId || "unknown",
            interactionId: `approval-${Date.now()}-${request.toolCall.id}`,
          });

          return {
            approved: result.approved,
            toolCallId: result.toolCallId,
            editedParameters: result.editedParameters,
            userInstruction: result.userInstruction,
            rejectionReason: result.rejectionReason,
          };
        } finally {
          // Resume the TimeoutManager after user approval
          entity.timeoutManager.resume();
        }
      } catch (error) {
        logger.error("Tool approval handler failed", {
          agentLoopId: entity.id,
          toolCallId: request.toolCall.id,
          error,
        });

        return {
          approved: false,
          toolCallId: request.toolCall.id,
          rejectionReason: `Handler error: ${error instanceof Error ? error.message : String(error)}`,
        };
      }
    }

    // Fallback to current behavior when no handler configured
    const toolName = request.toolCall.function?.name || "unknown";

    logger.warn("No tool approval handler configured, rejecting by default", {
      agentLoopId: entity.id,
      toolCallId: request.toolCall.id,
      toolName,
    });

    return {
      approved: false,
      toolCallId: request.toolCall.id,
      rejectionReason: `No approval handler configured. Tool "${toolName}" requires manual approval but no handler is registered.`,
    };
  }

  /**
   * Emit agent hook event
   * Uses the injected emitEvent callback if available, otherwise falls through.
   */
  private async emitAgentEvent(event: AgentHookTriggeredEvent): Promise<void> {
    if (this.emitEvent) {
      await this.emitEvent(event);
    } else {
      logger.debug("emitAgentEvent called (no emitter configured)", { eventType: event?.type });
    }
  }

  /**
   * Emit event to registry
   */
  private async emitToRegistry(event: AgentStreamEvent, _entity: AgentLoopEntity): Promise<void> {
    if (!this.eventManager) return;

    try {
      // Note: Event conversion would need to be handled by parent coordinator
      // For now, we'll skip detailed conversion
      logger.debug("Event emitted to registry", { eventType: event.type });
    } catch (error) {
      logger.warn("Failed to emit event to EventRegistry", {
        eventType: event.type,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }
}
