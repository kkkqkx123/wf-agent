/**
 * MessageResourceAPI - Workflow Message Resource Management API
 * Provides APIs for managing messages in workflow executions.
 * Extends the shared BaseMessageResourceAPI with workflow-specific implementation.
 */

import {
  BaseMessageResourceAPI,
  type BaseMessageFilter,
  type ParsedMessageId,
  type GetAllMessagesResult,
} from "../../shared/resources/message-base.js";
import type { WorkflowExecutionRegistry } from "../../../workflow/registry/workflow-execution-registry.js";
import type { LLMMessage } from "@wf-agent/types";
import { WorkflowExecutionNotFoundError } from "@wf-agent/types";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";

/**
 * Workflow Message Filter
 */
export interface MessageFilter extends BaseMessageFilter {
  /** Filter by role */
  role?: string;
  /** Filter by content (fuzzy matching) */
  content?: string;
  /** Filter by execution ID */
  executionId?: string;
  /** Filter by time range */
  timeRange?: {
    start?: number;
    end?: number;
  };
}

/**
 * Message Statistics
 */
export interface MessageStats {
  /** Total number of messages */
  total: number;
  /** Distribution by role */
  byRole: Record<string, number>;
  /** Distribution by type */
  byType: Record<string, number>;
  /** Token usage statistics */
  totalTokenUsage?: {
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
  };
}

/**
 * MessageResourceAPI - Workflow Message Resource Management API
 */
export class MessageResourceAPI extends BaseMessageResourceAPI<MessageFilter> {
  private registry: WorkflowExecutionRegistry;

  constructor(deps: APIDependencyManager) {
    super();
    this.registry = deps.getWorkflowExecutionRegistry();
  }

  // ============================================================================
  // Implement BaseMessageResourceAPI abstract methods
  // ============================================================================

  /**
   * Get messages for a specific workflow execution
   */
  protected getEntityMessages(executionId: string): LLMMessage[] {
    const executionEntity = this.registry.get(executionId);
    if (!executionEntity) {
      throw new WorkflowExecutionNotFoundError(
        `Workflow execution not found: ${executionId}`,
        executionId,
      );
    }

    const stateCoordinator = this.registry.getStateCoordinator(executionId);
    if (!stateCoordinator) {
      throw new WorkflowExecutionNotFoundError(
        `State coordinator not found for execution: ${executionId}`,
        executionId,
      );
    }

    return stateCoordinator.getMessages() || [];
  }

  /**
   * Get all messages across all workflow executions
   */
  protected getAllEntityMessages(): GetAllMessagesResult {
    const executionEntities = this.registry.getAll();
    const allMessages: LLMMessage[] = [];
    const entityCounts: Record<string, number> = {};

    for (const executionEntity of executionEntities) {
      const stateCoordinator = this.registry.getStateCoordinator(executionEntity.id);
      if (!stateCoordinator) {
        continue;
      }
      const messages = stateCoordinator.getMessages() || [];
      entityCounts[executionEntity.id] = messages.length;
      allMessages.push(...messages);
    }

    return { messages: allMessages, entityCounts };
  }

  /**
   * Parse composite message ID (format: executionId:index)
   */
  protected parseMessageId(id: string): ParsedMessageId | null {
    const [executionId, indexStr] = id.split(":");
    if (!executionId || !indexStr) {
      return null;
    }

    const index = parseInt(indexStr, 10);
    if (isNaN(index) || index < 0) {
      return null;
    }

    return { entityId: executionId, index };
  }

  // ============================================================================
  // Workflow-specific message methods
  // ============================================================================

  /**
   * Get workflow execution message list
   * @param executionId Execution ID
   * @param limit Limit on the number of messages to return
   * @param offset Offset for starting the message retrieval
   * @param orderBy Sorting method
   * @returns Array of messages
   */
  async getWorkflowExecutionMessages(
    executionId: string,
    limit?: number,
    offset?: number,
    orderBy: "asc" | "desc" = "asc",
  ): Promise<LLMMessage[]> {
    return this.getEntityMessagesWithOptions(executionId, limit, offset, orderBy);
  }

  /**
   * Get the last N messages
   * @param executionId Execution ID
   * @param count Number of messages
   * @returns Array of messages
   */
  async getRecentMessages(executionId: string, count: number): Promise<LLMMessage[]> {
    return this.getRecentEntityMessages(executionId, count);
  }

  /**
   * Search for messages
   * @param executionId Execution ID
   * @param query Search keyword
   * @returns Array of matching messages
   */
  async searchMessages(executionId: string, query: string): Promise<LLMMessage[]> {
    return this.searchEntityMessages(executionId, query);
  }

  /**
   * Get message statistics information
   * @param executionId Execution ID
   * @returns Statistical information with token usage
   */
  async getMessageStats(executionId: string): Promise<MessageStats> {
    const messages = this.getEntityMessages(executionId);
    const stateCoordinator = this.registry.getStateCoordinator(executionId);

    const tokenUsage = (stateCoordinator as unknown as Record<string, unknown>)["getTokenUsage"] as
      | (() => { promptTokens: number; completionTokens: number; totalTokens: number })
      | undefined;

    const byRole: Record<string, number> = {};
    const byType: Record<string, number> = {};

    messages.forEach((msg) => {
      byRole[msg.role] = (byRole[msg.role] || 0) + 1;
      const msgType = (msg as unknown as Record<string, unknown>)["type"] as string | undefined;
      if (msgType) {
        byType[msgType] = (byType[msgType] || 0) + 1;
      }
    });

    return {
      total: messages.length,
      byRole,
      byType,
      totalTokenUsage: typeof tokenUsage === "function" ? tokenUsage() : undefined,
    };
  }

  /**
   * Get message statistics for all executions
   * @returns Global statistics information
   */
  async getGlobalMessageStats(): Promise<{
    total: number;
    byExecution: Record<string, number>;
    byRole: Record<string, number>;
  }> {
    const stats = await this.getGlobalMessagesStats();
    return {
      total: stats.total,
      byExecution: stats.byEntity,
      byRole: stats.byRole,
    };
  }

  /**
   * Get the message conversation history
   * @param executionId Execution ID
   * @param maxMessages Maximum number of messages
   * @returns Array of conversation history
   */
  async getConversationHistory(executionId: string, maxMessages?: number): Promise<LLMMessage[]> {
    return this.getEntityConversationHistory(executionId, maxMessages);
  }

  /**
   * Get the underlying WorkflowExecutionRegistry instance
   * @returns WorkflowExecutionRegistry instance
   */
  getRegistry(): WorkflowExecutionRegistry {
    return this.registry;
  }

  /**
   * Normalize the message history for a workflow execution
   * Consolidates consecutive messages from the same role.
   * @param executionId Execution ID
   */
  async normalizeHistory(executionId: string): Promise<void> {
    const stateCoordinator = this.registry.getStateCoordinator(executionId);
    if (stateCoordinator) {
      stateCoordinator.normalizeHistory();
    }
  }

  /**
   * Get the number of messages for a workflow execution
   * @param executionId Execution ID
   * @returns Number of messages
   */
  async getMessageCount(executionId: string): Promise<number> {
    const messages = this.getEntityMessages(executionId);
    return messages.length;
  }

  /**
   * Get message stats with token usage information
   * @param executionId Execution ID
   * @returns Enhanced message statistics
   */
  async getEnhancedMessageStats(executionId: string): Promise<MessageStats> {
    const messages = this.getEntityMessages(executionId);
    const stateCoordinator = this.registry.getStateCoordinator(executionId);

    const byRole: Record<string, number> = {};
    const byType: Record<string, number> = {};

    messages.forEach((msg) => {
      byRole[msg.role] = (byRole[msg.role] || 0) + 1;
      const msgType = (msg as unknown as Record<string, unknown>)["type"] as string | undefined;
      if (msgType) {
        byType[msgType] = (byType[msgType] || 0) + 1;
      }
    });

    // Extract token usage if available from state coordinator
    let totalTokenUsage: { promptTokens: number; completionTokens: number; totalTokens: number } | undefined;
    const tokenUsage = (stateCoordinator as unknown as Record<string, unknown>)["getTokenUsage"] as (() => { promptTokens: number; completionTokens: number; totalTokens: number }) | undefined;
    if (typeof tokenUsage === "function") {
      totalTokenUsage = tokenUsage();
    }

    return {
      total: messages.length,
      byRole,
      byType,
      totalTokenUsage,
    };
  }
}