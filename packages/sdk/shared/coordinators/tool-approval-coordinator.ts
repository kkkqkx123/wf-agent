/**
 * Tool Approval Coordinator
 * Coordinates tool approval process
 *
 * Core Responsibilities:
 * 1. Check if tool requires approval (with auto-approval support)
 * 2. Request user approval
 * 3. Process approval result
 * 4. Support parameter editing and user instructions
 *
 * Design Principles:
 * - Pure approval coordination logic
 * - No business-specific features (checkpoint, execution management)
 * - Reusable across modules (Graph, Agent, etc.)
 * - Layer-independent: uses generic event types from types package
 */

import type {
  ToolApprovalOptions,
  ToolApprovalResult,
  ToolApprovalCoordinatorParams,
  ToolApprovalRequest,
  Tool,
  ToolRiskLevel,
  LLMToolCall,
  ToolBatchResult,
  PendingToolCall,
  ToolExecutionResult,
  ToolApprovalHandler,
  BaseEvent,
} from "@wf-agent/types";
import { generateId } from "@wf-agent/common-utils";
import type { EventRegistry } from "../registry/event-registry.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import {
  checkAutoApproval,
  extractContextFromParameters,
} from "../../services/auto-approval/index.js";
import {
  buildToolApprovalRequestedEvent,
  buildProgressiveToolExecutionStartEvent,
  buildProgressiveToolExecutionEndEvent,
  buildToolQueueUpdateEvent,
  buildToolApprovalAnnotatedEvent,
} from "../events/builders/interaction-events.js";

const logger = createContextualLogger();

/**
 * Audit log entry structure
 */
interface AuditLogEntry {
  timestamp: string;
  contextId: string;
  toolCallId: string;
  toolName: string;
  decision:
    | "AUTO_APPROVED"
    | "AUTO_DENIED"
    | "MANUALLY_APPROVED"
    | "MANUALLY_REJECTED"
    | "TIMEOUT_AUTO_RESPONSE"
    | "ERROR"
    | "USAGE_LIMIT_EXCEEDED";
  riskLevel?: string;
  reason?: string;
  editedParameters?: Record<string, unknown>;
  userInstruction?: string;
  requiresManualReview?: boolean;
}

/**
 * Extended parameters for tool approval coordinator
 */
export interface ExtendedToolApprovalCoordinatorParams extends ToolApprovalCoordinatorParams {
  /** Tool definition (for risk level and metadata) */
  tool?: Tool;
  /** Optional risk level override (decouples from tool definition) */
  riskLevel?: string;
}

/**
 * Tool Approval Coordinator Class
 *
 * Responsibilities:
 * - Check if tool requires approval
 * - Check auto-approval rules
 * - Request user approval
 * - Process approval result
 *
 * Design Principles:
 * - Pure coordination logic
 * - Reusable across modules
 */
export class ToolApprovalCoordinator {
  /**
   * Constructor
   *
   * @param eventManager Event manager for triggering events
   */
  constructor(private eventManager?: EventRegistry) {}

  /**
   * Process tool approval
   *
   * This method coordinates the complete tool approval process:
   * 1. Check auto-approval rules (with preset support)
   * 2. Request user approval (if required)
   * 3. Return approval result
   *
   * @param params Approval parameters
   * @returns Approval result
   */
  async processToolApproval(
    params: ExtendedToolApprovalCoordinatorParams,
  ): Promise<ToolApprovalResult> {
    const { toolCall, options, contextId, toolDescription, tool, riskLevel } = params;

    // Validate tool or riskLevel is provided
    if (!tool && !riskLevel) {
      return {
        approved: false,
        toolCallId: toolCall.id,
        rejectionReason: "Tool definition or riskLevel must be provided for approval check",
      };
    }

    // 1. Check auto-approval (unified logic with presets)
    try {
      let parameters: Record<string, unknown> = {};
      try {
        parameters = JSON.parse(toolCall.function?.arguments ?? "{}");
      } catch (parseError) {
        logger.warn("Failed to parse tool arguments, using empty parameters", {
          toolId: tool?.id,
          error: parseError instanceof Error ? parseError.message : String(parseError),
        });
        // Continue with empty parameters - extractContextFromParameters will handle safety
      }

      // Use provided riskLevel or extract from tool
      const effectiveRiskLevel = (riskLevel || tool?.metadata?.riskLevel) as
        | ToolRiskLevel
        | undefined;

      if (!effectiveRiskLevel && !tool) {
        logger.error("Cannot determine risk level - both tool and riskLevel are missing", {
          toolCallId: toolCall.id,
          toolName: toolCall.function?.name,
        });
        return {
          approved: false,
          toolCallId: toolCall.id,
          rejectionReason: "Cannot determine tool risk level for approval check",
        };
      }

      const effectiveTool = tool
        ? { ...tool, metadata: { ...tool.metadata, riskLevel: effectiveRiskLevel } }
        : undefined;

      const context = extractContextFromParameters(tool?.id || "unknown", parameters);
      const decision = checkAutoApproval({
        options: options || {},
        tool:
          effectiveTool ||
          ({
            id: "unknown",
            type: "STATELESS" as const,
            description: "Unknown tool",
            parameters: { type: "object" as const, properties: {} },
            metadata: { riskLevel: effectiveRiskLevel || "WRITE" },
          } as Tool),
        context,
      });

      if (decision.decision === "approve") {
        logger.debug("Tool auto-approved", {
          toolId: tool?.id || "unknown",
          riskLevel: effectiveRiskLevel,
        });

        // Log audit trail
        this.logAuditTrail({
          contextId,
          toolCallId: toolCall.id,
          toolName: toolCall.function?.name || "unknown",
          decision: "AUTO_APPROVED",
          riskLevel: effectiveRiskLevel,
          reason: "Passed auto-approval checks",
        });

        return { approved: true, toolCallId: toolCall.id };
      }

      if (decision.decision === "deny") {
        logger.info("Tool auto-denied by policy", {
          toolId: tool?.id || "unknown",
          riskLevel: effectiveRiskLevel,
          reason: decision.reason,
        });

        // Log audit trail
        this.logAuditTrail({
          contextId,
          toolCallId: toolCall.id,
          toolName: toolCall.function?.name || "unknown",
          decision: "AUTO_DENIED",
          riskLevel: effectiveRiskLevel,
          reason: decision.reason,
        });

        return {
          approved: false,
          toolCallId: toolCall.id,
          rejectionReason: decision.reason,
        };
      }

      if (decision.decision === "timeout") {
        logger.debug("Tool approval timeout with auto-response", {
          toolId: tool?.id || "unknown",
          timeoutMs: decision.timeout,
        });

        // Log audit trail
        this.logAuditTrail({
          contextId,
          toolCallId: toolCall.id,
          toolName: toolCall.function?.name || "unknown",
          decision: "TIMEOUT_AUTO_RESPONSE",
          riskLevel: effectiveRiskLevel,
          reason: `Timeout (${decision.timeout}ms) with auto-response`,
        });

        return {
          approved: true,
          toolCallId: toolCall.id,
          userInstruction: String(decision.autoResponse),
        };
      }
    } catch (error) {
      // Enhanced error handling with actionable messages
      const errorMessage = error instanceof Error ? error.message : String(error);
      const errorStack = error instanceof Error ? error.stack : undefined;

      logger.error("Auto-approval check failed with exception", {
        error: errorMessage,
        stack: errorStack,
        toolId: tool?.id,
        toolName: toolCall.function?.name,
        contextId,
      });

      // Provide actionable error message to user
      const userMessage = "Auto-approval system encountered an error. ";

      if (errorMessage.includes("Invalid or missing")) {
        logger.warn(
          userMessage + "Tool parameters are invalid. Please review the tool call and try again.",
        );
      } else if (errorMessage.includes("path") || errorMessage.includes("command")) {
        logger.warn(
          userMessage + "Required parameter is missing or invalid. Check the tool configuration.",
        );
      } else {
        logger.warn(
          userMessage +
            "Manual approval required due to system error. Please review this tool call carefully.",
        );
      }

      // Log audit trail for error case
      this.logAuditTrail({
        contextId,
        toolCallId: toolCall.id,
        toolName: toolCall.function?.name || "unknown",
        decision: "ERROR",
        reason: `System error: ${errorMessage}`,
        requiresManualReview: true,
      });

      // Fall back to manual approval with clear explanation
      return this.requestUserApproval({
        ...params,
        toolDescription: `${toolDescription || "Tool"} (approval system error - manual review required)`,
      });
    }

    // 2. Request user approval
    const manualResult = await this.requestUserApproval(params);

    // Log audit trail for manual approval
    this.logAuditTrail({
      contextId,
      toolCallId: toolCall.id,
      toolName: toolCall.function?.name || "unknown",
      decision: manualResult.approved ? "MANUALLY_APPROVED" : "MANUALLY_REJECTED",
      riskLevel: riskLevel || tool?.metadata?.riskLevel,
      reason: manualResult.rejectionReason || "User approved",
      editedParameters: manualResult.editedParameters,
      userInstruction: manualResult.userInstruction,
    });

    return manualResult;
  }

  /**
   * Process a batch of tool calls with sequential execution
   *
   * This method implements the auto-approval prefix pattern:
   * 1. Auto-execute safe tools at the beginning of the batch
   * 2. Pause at first tool requiring approval
   * 3. Request user approval with full batch context
   * 4. Continue based on user's continueBatch flag
   *
   * @param toolCalls Array of tool calls to process
   * @param options Approval options
   * @param contextId Execution context ID
   * @param nodeId Node ID (optional)
   * @param approvalHandler Handler for requesting user approval
   * @param eventManager Event manager for emitting progressive events
   * @returns Batch result with auto-executed tools and approval status
   */
  async processToolBatch(
    toolCalls: LLMToolCall[],
    options: ToolApprovalOptions,
    contextId: string,
    nodeId: string,
    approvalHandler: ToolApprovalHandler,
    eventManager?: EventRegistry,
  ): Promise<ToolBatchResult> {
    const batchId = generateId();
    logger.info("Starting batch tool approval", {
      batchId,
      contextId,
      nodeId,
      totalTools: toolCalls.length,
    });

    // Step 1: Split into auto-execute prefix and first confirmation tool
    const { autoPrefix, firstConfirmTool, remainingAfterConfirm } = this.splitToolBatch(
      toolCalls,
      options,
      contextId,
    );

    logger.debug("Tool batch split", {
      batchId,
      autoCount: autoPrefix.length,
      hasConfirmation: !!firstConfirmTool,
      remainingCount: remainingAfterConfirm.length,
    });

    // Step 2: Execute auto-approved prefix with progress events
    const autoResults: ToolExecutionResult[] = [];
    for (let i = 0; i < autoPrefix.length; i++) {
      const call = autoPrefix[i]!;

      // Emit start event
      if (eventManager) {
        await this.safeEmit(
          eventManager,
          buildProgressiveToolExecutionStartEvent({
            executionId: contextId,
            nodeId,
            toolCallId: call.id,
            toolName: call.function?.name || "unknown",
            batchId,
            toolIndex: i,
            totalTools: toolCalls.length,
            pendingQueue: this.buildPendingQueue(
              autoPrefix.slice(i + 1),
              firstConfirmTool,
              remainingAfterConfirm,
            ),
          }),
        );
      }

      // Note: Tool execution is handled by the caller (e.g., LLMExecutionCoordinator)
      // This coordinator only handles approval logic, not actual tool execution
      const result: ToolExecutionResult = {
        success: true,
        result: {},
        executionTime: 0,
        retryCount: 0,
      };

      autoResults.push(result);

      // Emit end event
      if (eventManager) {
        await this.safeEmit(
          eventManager,
          buildProgressiveToolExecutionEndEvent({
            executionId: contextId,
            nodeId,
            toolCallId: call.id,
            toolName: call.function?.name || "unknown",
            batchId,
            status: result.success ? "success" : "error",
            result,
            executionTime: 0,
          }),
        );

        // Emit queue update
        await this.safeEmit(
          eventManager,
          buildToolQueueUpdateEvent({
            executionId: contextId,
            nodeId,
            batchId,
            completedCount: i + 1,
            totalCount: toolCalls.length,
            pendingQueue: this.buildPendingQueue(
              autoPrefix.slice(i + 1),
              firstConfirmTool,
              remainingAfterConfirm,
            ),
          }),
        );
      }
    }

    // Step 3: If confirmation needed, pause and request approval
    if (firstConfirmTool) {
      const approvalResult = await this.requestUserApprovalForBatch(
        firstConfirmTool,
        options,
        contextId,
        nodeId,
        approvalHandler,
        batchId,
        autoPrefix.length,
        toolCalls.length,
        remainingAfterConfirm,
        autoResults,
        eventManager,
      );

      logger.info("User approval received", {
        batchId,
        approved: approvalResult.approved,
        continueBatch: approvalResult.continueBatch,
      });

      // Determine remaining queue based on approval decision
      const finalRemaining = approvalResult.continueBatch !== false ? remainingAfterConfirm : [];

      return {
        batchId,
        autoExecuted: autoResults,
        confirmationRequired: firstConfirmTool,
        confirmationResult: approvalResult,
        remainingQueue: finalRemaining,
        allCompleted: !approvalResult.continueBatch || remainingAfterConfirm.length === 0,
      };
    }

    // All tools auto-executed
    logger.info("All tools auto-executed", {
      batchId,
      count: autoResults.length,
    });

    return {
      batchId,
      autoExecuted: autoResults,
      confirmationRequired: null,
      remainingQueue: [],
      allCompleted: true,
    };
  }

  /**
   * Request user approval
   *
   * @param params Approval parameters
   * @returns Approval result
   */
  private async requestUserApproval(
    params: ExtendedToolApprovalCoordinatorParams,
  ): Promise<ToolApprovalResult> {
    const { toolCall, contextId, nodeId, toolDescription, approvalHandler } = params;

    // Generate interaction ID
    const interactionId = generateId();

    // Create approval request
    const request: ToolApprovalRequest = {
      toolCall,
      toolDescription,
      contextId,
      nodeId,
      interactionId,
    };

    try {
      // Trigger USER_INTERACTION_REQUESTED event
      if (this.eventManager) {
        await this.triggerApprovalRequestedEvent(request);
      }

      // Request approval
      const result = await approvalHandler.requestApproval(request);

      return result;
    } catch (error) {
      // Return rejection on error
      return {
        approved: false,
        toolCallId: toolCall.id,
        rejectionReason: error instanceof Error ? error.message : "Unknown error",
      };
    }
  }

  /**
   * Trigger approval requested event
   */
  private async triggerApprovalRequestedEvent(
    request: ToolApprovalRequest,
  ): Promise<void> {
    try {
      await this.eventManager!.emit(
        buildToolApprovalRequestedEvent({
          executionId: request.contextId || "",
          interactionId: request.interactionId,
          toolCall: request.toolCall,
          toolDescription: request.toolDescription,
          contextId: request.contextId,
          nodeId: request.nodeId,
          batchId: request.batchId,
          toolIndex: request.toolIndex,
          totalTools: request.totalTools,
          pendingQueue: request.pendingQueue,
        }),
      );
    } catch (error) {
      logger.warn("Failed to trigger approval requested event", {
        contextId: request.contextId,
        error,
      });
    }
  }

  // ============================================================================
  // Batch Processing Helper Methods
  // ============================================================================

  /**
   * Split a batch of tools into auto-executable prefix and first confirmation tool
   */
  private splitToolBatch(
    toolCalls: LLMToolCall[],
    options: ToolApprovalOptions,
    contextId: string,
  ): {
    autoPrefix: LLMToolCall[];
    firstConfirmTool: LLMToolCall | null;
    remainingAfterConfirm: LLMToolCall[];
  } {
    const autoPrefix: LLMToolCall[] = [];
    let firstConfirmTool: LLMToolCall | null = null;
    let firstConfirmIndex = -1;

    for (let i = 0; i < toolCalls.length; i++) {
      const call = toolCalls[i]!;
      const tool = undefined;

      if (this.requiresConfirmation(tool, options, contextId)) {
        if (!firstConfirmTool) {
          firstConfirmTool = call;
          firstConfirmIndex = i;
          break; // Stop at first confirmation-required tool
        }
      } else {
        autoPrefix.push(call);
      }
    }

    const remainingAfterConfirm =
      firstConfirmIndex >= 0 ? toolCalls.slice(firstConfirmIndex + 1) : [];

    return { autoPrefix, firstConfirmTool, remainingAfterConfirm };
  }

  /**
   * Check if a tool requires user confirmation
   */
  private requiresConfirmation(
    tool: Tool | undefined,
    options: ToolApprovalOptions,
    contextId: string,
  ): boolean {
    if (!options.autoApprovalEnabled) {
      return true; // All tools require approval if disabled
    }

    // Use existing auto-approval logic
    try {
      const decision = checkAutoApproval({
        tool:
          tool ||
          ({
            id: "unknown",
            type: "STATELESS" as const,
            description: "Unknown tool",
            parameters: { type: "object" as const, properties: {} },
            metadata: { riskLevel: "WRITE" },
          } as Tool),
        options,
        context: this.extractAutoApprovalContext(tool, contextId),
      });

      return decision.decision !== "approve";
    } catch (error) {
      // On error, require manual approval for safety
      logger.warn("Auto-approval check failed, requiring manual approval", {
        error: error instanceof Error ? error.message : String(error),
      });
      return true;
    }
  }

  /**
   * Request user approval with batch context
   */
  private async requestUserApprovalForBatch(
    toolCall: LLMToolCall,
    options: ToolApprovalOptions,
    contextId: string,
    nodeId: string,
    approvalHandler: ToolApprovalHandler,
    batchId: string,
    toolIndex: number,
    totalTools: number,
    pendingQueue: LLMToolCall[],
    autoExecutedResults: ToolExecutionResult[],
    eventManager?: EventRegistry,
  ): Promise<ToolApprovalResult> {
    const interactionId = generateId();

    // Create approval request with batch context and configuration
    const request: ToolApprovalRequest = {
      toolCall,
      toolDescription: undefined,
      contextId,
      nodeId,
      interactionId,
      batchId,
      toolIndex,
      totalTools,
      pendingQueue,
      autoExecutedResults,
      // Pass configuration from options
      securityPreset: options.securityPreset,
    };

    // Delegate to handler
    const result = await approvalHandler.requestApproval(request);

    // Emit annotation event if provided
    if (result.annotation && eventManager) {
      await this.safeEmit(
        eventManager,
        buildToolApprovalAnnotatedEvent({
          executionId: contextId,
          nodeId,
          interactionId,
          toolCallId: toolCall.id,
          toolName: toolCall.function?.name || "unknown",
          annotation: result.annotation,
          approved: result.approved,
        }),
      );
    }

    return result;
  }

  /**
   * Build pending queue for events
   * Includes remaining auto-executable tools, confirmation tool, and subsequent tools
   */
  private buildPendingQueue(
    remainingAuto: LLMToolCall[],
    confirmTool: LLMToolCall | null,
    remainingAfterConfirm: LLMToolCall[],
  ): PendingToolCall[] {
    const queue: PendingToolCall[] = [];

    // Add remaining auto-executable tools first
    queue.push(
      ...remainingAuto.map(call => ({
        id: call.id,
        name: call.function?.name || "unknown",
        arguments: call.function?.arguments,
      })),
    );

    // Add confirmation-required tool
    if (confirmTool) {
      queue.push({
        id: confirmTool.id,
        name: confirmTool.function?.name || "unknown",
        arguments: confirmTool.function?.arguments,
      });
    }

    // Add remaining tools after confirmation
    queue.push(
      ...remainingAfterConfirm.map(call => ({
        id: call.id,
        name: call.function?.name || "unknown",
        arguments: call.function?.arguments,
      })),
    );

    return queue;
  }

  /**
   * Extract context for auto-approval check
   */
  private extractAutoApprovalContext(
    _tool: Tool | undefined,
    _contextId: string,
  ): import("../../services/auto-approval/index.js").AutoApprovalContext {
    // Extract workspace boundary context from tool parameters
    // For now, return minimal context - workspace boundary checking
    // is a future enhancement when ToolContext provides workingDirectory
    return {
      isOutsideWorkspace: false,
      isProtected: false,
      domain: undefined,
    };
  }

  /**
   * Get tool description
   */

  /**
   * Safe event emission with error handling
   */
  private async safeEmit(eventManager: EventRegistry, event: BaseEvent): Promise<void> {
    try {
      await eventManager.emit(event);
    } catch (error) {
      logger.warn("Failed to emit event", {
        eventType: event.type,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  // ============================================================================
  // Audit Logging
  // ============================================================================

  /**
   * Log approval decision to audit trail
   *
   * This provides a complete history of all approval decisions for:
   * - Security review and compliance
   * - Debugging approval issues
   * - Analyzing approval patterns
   * - Detecting potential abuse
   */
  private logAuditTrail(entry: Omit<AuditLogEntry, "timestamp">): void {
    const auditEntry: AuditLogEntry = {
      ...entry,
      timestamp: new Date().toISOString(),
    };

    // Log to structured logger (can be extended to write to file/database)
    logger.info("Approval Audit Trail", {
      timestamp: auditEntry.timestamp,
      contextId: auditEntry.contextId,
      toolCallId: auditEntry.toolCallId,
      toolName: auditEntry.toolName,
      decision: auditEntry.decision,
      riskLevel: auditEntry.riskLevel,
      reason: auditEntry.reason,
      requiresManualReview: auditEntry.requiresManualReview,
    });
  }
}
