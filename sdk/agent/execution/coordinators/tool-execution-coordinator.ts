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
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { ConversationSession } from "../../../core/messaging/conversation-session.js";
import type { ToolCallExecutor } from "../../../core/executors/tool-call-executor.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import { ToolApprovalCoordinator } from "../../../core/coordinators/tool-approval-coordinator.js";
import { executeAgentHook } from "../handlers/hook-handlers/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

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

  constructor(deps: ToolExecutionCoordinatorDependencies) {
    this.toolCallExecutor = deps.toolCallExecutor;
    this.eventManager = deps.eventManager;
    this.toolApprovalHandler = deps.toolApprovalHandler;

    // Initialize approval coordinator
    this.approvalCoordinator = new ToolApprovalCoordinator(deps.eventManager);
  }

  /**
   * Execute tool calls (sync mode)
   *
   * @param entity Agent loop entity
   * @param conversationManager Conversation manager
   * @param toolCalls Array of tool calls to execute
   */
  async executeToolCalls(
    entity: AgentLoopEntity,
    conversationManager: ConversationSession,
    toolCalls: Array<{ id: string; name: string; arguments: string }>,
  ): Promise<void> {
    const agentLoopId = entity.id;
    const iteration = entity.state.currentIteration;

    logger.debug("Executing tool calls", {
      agentLoopId,
      iteration,
      toolCallCount: toolCalls.length,
    });

    // When no tool approval handler is configured, execute all tools directly
    // without going through the approval coordinator. This enables agent loops
    // with tool calls to work in testing/dev scenarios without requiring a
    // tool approval handler to be registered.
    if (!this.toolApprovalHandler) {
      for (const toolCall of toolCalls) {
        entity.state.recordToolCallStart(toolCall.id, toolCall.name, toolCall.arguments);
        await this.executeSingleApprovedTool(
          entity,
          conversationManager,
          toolCall,
          { success: true, result: {} },
        );
      }
      return;
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
        await this.executeSingleApprovedTool(
          entity,
          conversationManager,
          originalToolCall,
          autoResult,
        );
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

        await this.executeSingleApprovedTool(
          entity,
          conversationManager,
          confirmedToolCall,
          placeholderResult,
        );
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

        // Recursively process remaining tools
        await this.executeToolCalls(entity, conversationManager, remainingToolCalls);
      }
    }

    logger.debug("Tool calls execution completed", {
      agentLoopId,
      iteration,
      autoExecutedCount: batchResult.autoExecuted.length,
      hasConfirmation: !!batchResult.confirmationRequired,
    });
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

      await executeAgentHook(entity, "BEFORE_TOOL_CALL", this.emitAgentEvent.bind(this), toolCallInfo);

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
        await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent.bind(this), {
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
        await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent.bind(this), {
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
  ): Promise<void> {
    const agentLoopId = entity.id;

    const toolCallInfo = {
      id: toolCall.id,
      name: toolCall.name,
      arguments: toolCall.arguments ? JSON.parse(toolCall.arguments) : {},
    };

    await executeAgentHook(entity, "BEFORE_TOOL_CALL", this.emitAgentEvent.bind(this), toolCallInfo);

    if (executionResult.success) {
      logger.debug("Tool call succeeded", {
        agentLoopId,
        toolCallId: toolCall.id,
        toolName: toolCallInfo.name,
      });

      // Execute actual tool call
      const toolResults = await this.toolCallExecutor.executeToolCalls(
        [toolCall],
        conversationManager,
        entity.id,
        entity.nodeId,
        { abortSignal: entity.getAbortSignal() },
      );

      const result = toolResults[0];
      if (result?.success) {
        entity.state.recordToolCallEnd(toolCall.id, result.result);
        await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent.bind(this), {
          ...toolCallInfo,
          result: result.result,
        });
      } else {
        entity.state.recordToolCallEnd(
          toolCall.id,
          undefined,
          result?.error || "Tool execution failed",
        );
        await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent.bind(this), {
          ...toolCallInfo,
          error: result?.error,
        });
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
      await executeAgentHook(entity, "AFTER_TOOL_CALL", this.emitAgentEvent.bind(this), {
        ...toolCallInfo,
        error: executionResult.error,
      });
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
   * Placeholder method for emitAgentEvent - will be injected from parent
   */
  private async emitAgentEvent(event: AgentHookTriggeredEvent): Promise<void> {
    // This will be overridden by the parent coordinator
    logger.debug("emitAgentEvent called (placeholder)", { eventType: event?.type });
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
