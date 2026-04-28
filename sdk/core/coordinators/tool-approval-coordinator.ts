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
 * - No business-specific features (checkpoint, thread management)
 * - Reusable across modules (Graph, Agent, etc.)
 * - Layer-independent: uses generic event types from types package
 */

import type {
  ToolApprovalOptions,
  ToolApprovalResult,
  ToolApprovalCoordinatorParams,
  ToolApprovalRequest,
  Tool,
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
 * Extended parameters for tool approval coordinator
 */
export interface ExtendedToolApprovalCoordinatorParams extends ToolApprovalCoordinatorParams {
  /** Tool definition (for risk level and metadata) */
  tool?: Tool;
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
   * 1. Check auto-approval rules (if enabled)
   * 2. Check if tool requires approval
   * 3. Request user approval (if required)
   * 4. Return approval result
   *
   * @param params Approval parameters
   * @returns Approval result
   */
  async processToolApproval(
    params: ExtendedToolApprovalCoordinatorParams,
  ): Promise<ToolApprovalResult> {
    const { toolCall, options, contextId, nodeId, toolDescription, approvalHandler, tool } = params;

    // 1. Check auto-approval (if enabled and tool provided)
    if (options?.autoApprovalEnabled && tool) {
      const decision = this.checkAutoApproval(tool, toolCall, options);

      if (decision.decision === "approve") {
        logger.debug("Tool auto-approved", { toolId: tool.id, toolName: tool.name });
        return {
          approved: true,
          toolCallId: toolCall.id,
        };
      }

      if (decision.decision === "deny") {
        logger.debug("Tool auto-denied", { toolId: tool.id, reason: decision.reason });
        return {
          approved: false,
          toolCallId: toolCall.id,
          rejectionReason: decision.reason,
        };
      }

      if (decision.decision === "timeout") {
        logger.debug("Tool approval timeout with auto-response", { toolId: tool.id });
        return {
          approved: true,
          toolCallId: toolCall.id,
          userInstruction: String(decision.autoResponse),
        };
      }
    }

    // 2. Legacy check: if tool is in auto-approved list
    const toolName = toolCall.function?.name || "";
    if (!this.requiresApproval(toolName, options)) {
      return {
        approved: true,
        toolCallId: toolCall.id,
      };
    }

    // 3. Request user approval
    return this.requestUserApproval(params);
  }

  /**
   * Check auto-approval for a tool
   *
   * @param tool Tool definition
   * @param toolCall Tool call
   * @param options Approval options
   * @returns Auto-approval decision
   */
  private checkAutoApproval(
    tool: Tool,
    toolCall: { id: string; function?: { name?: string; arguments?: string } },
    options: ToolApprovalOptions,
  ): AutoApprovalDecision {
    // Extract parameters from tool call
    let parameters: Record<string, unknown> = {};
    try {
      parameters = JSON.parse(toolCall.function?.arguments ?? "{}");
    } catch {
      // Ignore parse errors
    }

    // Extract context from parameters
    const context = extractContextFromParameters(tool.id, parameters);

    // Check auto-approval
    return checkAutoApproval({
      options,
      tool,
      context,
    });
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
   * Check if tool requires approval (legacy method)
   *
   * @param toolName Tool name
   * @param options Approval options
   * @returns Whether approval is required
   */
  requiresApproval(toolName: string, options?: ToolApprovalOptions): boolean {
    // If no options, no approval required
    if (!options) {
      return false;
    }

    // If auto-approval is enabled, use new logic
    if (options.autoApprovalEnabled) {
      // Auto-approval enabled but no tool provided - require approval
      return true;
    }

    // Legacy: Check if tool is in auto-approved list
    const autoApproved = options.autoApprovedTools || [];
    return !autoApproved.includes(toolName);
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
          threadId: request.contextId || "",
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
          threadId: request.contextId || "",
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
}
