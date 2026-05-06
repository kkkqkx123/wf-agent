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
} from "@wf-agent/types";
import { generateId } from "@wf-agent/common-utils";
import type { EventRegistry } from "../registry/event-registry.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import {
  buildUserInteractionRequestedEvent,
  buildUserInteractionProcessedEvent,
} from "../utils/event/builders/index.js";
import {
  checkAutoApproval,
  extractContextFromParameters,
  type AutoApprovalDecision,
} from "../../services/auto-approval/index.js";

const logger = createContextualLogger();

/**
 * Approval State for tracking usage limits per context
 */
interface ApprovalState {
  /** Number of consecutive auto-approved requests */
  consecutiveAutoApprovedCount: number;
  /** Last reset point (message index or timestamp) */
  lastResetIndex: number;
  /** Timestamp of last activity for cleanup */
  lastActivityAt: number;
}

/**
 * Audit log entry structure
 */
interface AuditLogEntry {
  timestamp: string;
  contextId: string;
  toolCallId: string;
  toolName: string;
  decision: "AUTO_APPROVED" | "AUTO_DENIED" | "MANUALLY_APPROVED" | "MANUALLY_REJECTED" | "TIMEOUT_AUTO_RESPONSE" | "ERROR" | "USAGE_LIMIT_EXCEEDED";
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
 * - Track usage limits (consecutive approvals, costs)
 * - Request user approval
 * - Process approval result
 *
 * Design Principles:
 * - Pure coordination logic
 * - Reusable across modules
 * - Stateful for usage limit tracking
 */
export class ToolApprovalCoordinator {
  /** State map for tracking approval limits per context */
  private approvalStates: Map<string, ApprovalState> = new Map();
  
  /** Cleanup interval for stale states (5 minutes) */
  private readonly STATE_CLEANUP_INTERVAL = 5 * 60 * 1000;

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
   * 1. Check usage limits (if configured)
   * 2. Check auto-approval rules (with preset support)
   * 3. Request user approval (if required)
   * 4. Return approval result
   *
   * @param params Approval parameters
   * @returns Approval result
   */
  async processToolApproval(
    params: ExtendedToolApprovalCoordinatorParams,
  ): Promise<ToolApprovalResult> {
    const { toolCall, options, contextId, nodeId, toolDescription, approvalHandler, tool, riskLevel } = params;

    // Validate tool or riskLevel is provided
    if (!tool && !riskLevel) {
      return {
        approved: false,
        toolCallId: toolCall.id,
        rejectionReason: "Tool definition or riskLevel must be provided for approval check",
      };
    }

    // 0. Check usage limits BEFORE auto-approval
    const state = this.getOrCreateState(contextId);
    const usageLimitCheck = this.checkUsageLimits(state, options || {});
    
    if (!usageLimitCheck.allowed) {
      logger.info("Usage limit exceeded, requiring manual approval", {
        contextId,
        reason: usageLimitCheck.reason,
        consecutiveCount: state.consecutiveAutoApprovedCount,
      });
      
      // Reset counters after requiring manual approval
      this.resetState(contextId);
      
      // Proceed to manual approval
      const manualResult = await this.requestUserApproval(params);
      
      // If user approves manually, reset the counter
      if (manualResult.approved) {
        this.resetState(contextId);
      }
      
      return manualResult;
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
      const effectiveRiskLevel = (riskLevel || tool?.metadata?.riskLevel) as ToolRiskLevel | undefined;
      
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
        tool: effectiveTool || { 
          id: "unknown", 
          type: "STATELESS" as const, 
          description: "Unknown tool", 
          parameters: { type: "object" as const, properties: {} }, 
          metadata: { riskLevel: effectiveRiskLevel || "WRITE" } 
        } as Tool,
        context,
      });

      if (decision.decision === "approve") {
        logger.debug("Tool auto-approved", { 
          toolId: tool?.id || "unknown",
          riskLevel: effectiveRiskLevel,
          consecutiveCount: state.consecutiveAutoApprovedCount + 1,
        });
        
        // Update state for auto-approved request
        this.updateStateAfterApproval(contextId);
        
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
        
        // Update state for timeout auto-response
        this.updateStateAfterApproval(contextId);
        
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
      let userMessage = "Auto-approval system encountered an error. ";
      
      if (errorMessage.includes("Invalid or missing")) {
        userMessage += "Tool parameters are invalid. Please review the tool call and try again.";
      } else if (errorMessage.includes("path") || errorMessage.includes("command")) {
        userMessage += "Required parameter is missing or invalid. Check the tool configuration.";
      } else {
        userMessage += "Manual approval required due to system error. Please review this tool call carefully.";
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
   * Request user approval
   *
   * @param params Approval parameters
   * @returns Approval result
   */
  private async requestUserApproval(
    params: ExtendedToolApprovalCoordinatorParams,
  ): Promise<ToolApprovalResult> {
    const { toolCall, options, contextId, nodeId, toolDescription, approvalHandler } = params;

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
        await this.triggerApprovalRequestedEvent(request, options?.approvalTimeout || 0);
      }

      // Request approval
      const result = await approvalHandler.requestApproval(request);

      // Trigger USER_INTERACTION_PROCESSED event
      if (this.eventManager) {
        await this.triggerApprovalProcessedEvent(request, result);
      }

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
    timeout: number,
  ): Promise<void> {
    try {
      await this.eventManager!.emit(
        buildUserInteractionRequestedEvent({
          executionId: request.contextId || "",
          interactionId: request.interactionId,
          operationType: "TOOL_APPROVAL" as const,
          prompt: `Approve tool call "${request.toolCall.function?.name ?? "unknown"}"?`,
          timeout,
          contextData: {
            toolName: request.toolCall.function?.name,
            toolDescription: request.toolDescription,
            toolCall: request.toolCall,
          },
        }),
      );
    } catch (error) {
      logger.warn("Failed to trigger approval requested event", {
        contextId: request.contextId,
        error,
      });
    }
  }

  /**
   * Trigger approval processed event
   */
  private async triggerApprovalProcessedEvent(
    request: ToolApprovalRequest,
    result: ToolApprovalResult,
  ): Promise<void> {
    try {
      await this.eventManager!.emit(
        buildUserInteractionProcessedEvent({
          executionId: request.contextId || "",
          interactionId: request.interactionId,
          operationType: "TOOL_APPROVAL" as const,
          results: result,
        }),
      );
    } catch (error) {
      logger.warn("Failed to trigger approval processed event", {
        contextId: request.contextId,
        error,
      });
    }
  }

  // ============================================================================
  // State Management Methods
  // ============================================================================

  /**
   * Get or create approval state for a context
   */
  private getOrCreateState(contextId: string): ApprovalState {
    let state = this.approvalStates.get(contextId);
    
    if (!state) {
      state = {
        consecutiveAutoApprovedCount: 0,
        lastResetIndex: 0,
        lastActivityAt: Date.now(),
      };
      this.approvalStates.set(contextId, state);
    } else {
      // Update activity timestamp
      state.lastActivityAt = Date.now();
    }
    
    // Periodic cleanup of stale states
    this.cleanupStaleStates();
    
    return state;
  }

  /**
   * Check if usage limits are exceeded
   */
  private checkUsageLimits(
    state: ApprovalState,
    options: ToolApprovalOptions,
  ): { allowed: boolean; reason?: string } {
    // Check consecutive auto-approved requests limit
    const maxRequests = options.maxAutoApprovedRequests;
    if (maxRequests !== undefined && maxRequests > 0) {
      if (state.consecutiveAutoApprovedCount >= maxRequests) {
        return {
          allowed: false,
          reason: `Exceeded maximum consecutive auto-approved requests (${maxRequests})`,
        };
      }
    }

    return { allowed: true };
  }

  /**
   * Update state after an auto-approved request
   */
  private updateStateAfterApproval(contextId: string): void {
    const state = this.approvalStates.get(contextId);
    if (state) {
      state.consecutiveAutoApprovedCount += 1;
      state.lastActivityAt = Date.now();
      
      logger.debug("Updated approval state", {
        contextId,
        consecutiveCount: state.consecutiveAutoApprovedCount,
      });
    }
  }

  /**
   * Reset state after manual approval or limit exceeded
   */
  private resetState(contextId: string): void {
    const state = this.approvalStates.get(contextId);
    if (state) {
      state.consecutiveAutoApprovedCount = 0;
      state.lastResetIndex += 1;
      state.lastActivityAt = Date.now();
      
      logger.debug("Reset approval state", {
        contextId,
        resetIndex: state.lastResetIndex,
      });
    }
  }

  /**
   * Clean up stale states to prevent memory leaks
   */
  private cleanupStaleStates(): void {
    const now = Date.now();
    
    for (const [contextId, state] of this.approvalStates.entries()) {
      if (now - state.lastActivityAt > this.STATE_CLEANUP_INTERVAL) {
        this.approvalStates.delete(contextId);
        logger.debug("Cleaned up stale approval state", { contextId });
      }
    }
  }

  /**
   * Get current approval state (for debugging/monitoring)
   */
  public getApprovalState(contextId: string): ApprovalState | undefined {
    return this.approvalStates.get(contextId);
  }

  /**
   * Manually reset approval state for a context
   */
  public resetApprovalState(contextId: string): void {
    this.resetState(contextId);
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

    // In production, this could be extended to:
    // - Write to persistent storage (database, file)
    // - Send to monitoring/analytics service
    // - Trigger alerts for suspicious patterns
  }
}
