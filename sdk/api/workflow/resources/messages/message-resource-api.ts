/**
 * MessageResourceAPI - Message Resource Management API
 *  Inherits from ReadonlyResourceAPI, providing read-only operations
 */

import { ReadonlyResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import type { WorkflowExecutionRegistry } from "../../../../workflow/stores/workflow-execution-registry.js";
import type { LLMMessage } from "@wf-agent/types";
import { WorkflowExecutionNotFoundError } from "@wf-agent/types";
import { getContainer } from "../../../../core/di/index.js";
import * as Identifiers from "../../../../core/di/service-identifiers.js";

/**
 * Message Filter
 */
export interface MessageFilter {
  /** Execution ID */
  executionId?: string;
  /** Role Filtering */
  role?: string;
  /** Content Keywords */
  content?: string;
  /** Time range start */
  startTimeFrom?: number;
  /** Time range has ended. */
  startTimeTo?: number;
}

/**
 * Message statistics information
 */
export interface MessageStats {
  /** Total amount */
  total: number;
  /** Count by role */
  byRole: Record<string, number>;
  /** Count by type */
  byType: Record<string, number>;
}

/**
 * MessageResourceAPI - Message Resource Management API
 */
export class MessageResourceAPI extends ReadonlyResourceAPI<LLMMessage, string, MessageFilter> {
  private registry: WorkflowExecutionRegistry;

  constructor() {
    super();
    const container = getContainer();
    this.registry = container.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry;
  }

  // ============================================================================
  // Implement the abstract method
  // ============================================================================

  /**
   * Get a single message
   * @param id: Message ID
   * @returns: Message object; returns null if the message does not exist
   */
  protected async getResource(id: string): Promise<LLMMessage | null> {
    // Messages are usually obtained through workflow execution entities, and here it is necessary to iterate through all workflow executions.
    const executionEntities = this.registry.getAll();
    for (const executionEntity of executionEntities) {
      const messages = executionEntity.getMessages() || [];
      const message = messages.find(
        (m: LLMMessage, index: number) => `${executionEntity.id}-${index}` === id,
      );
      if (message) {
        return message;
      }
    }
    return null;
  }

  /**
   * Get all messages
   * @returns Array of messages
   */
  protected async getAllResources(): Promise<LLMMessage[]> {
    const executionEntities = this.registry.getAll();
    const allMessages: LLMMessage[] = [];

    for (const executionEntity of executionEntities) {
      const messages = executionEntity.getMessages() || [];
      allMessages.push(...messages);
    }

    return allMessages;
  }

  /**
   * Apply filter criteria
   */
  protected override applyFilter(messages: LLMMessage[], filter: MessageFilter): LLMMessage[] {
    // Since LLMMessage does not have a execution ID or timestamp, it is only possible to filter by role and content.
    return messages.filter(message => {
      if (filter.role && message.role !== filter.role) {
        return false;
      }
      if (filter.content) {
        const content =
          typeof message.content === "string" ? message.content : JSON.stringify(message.content);
        if (!content.toLowerCase().includes(filter.content.toLowerCase())) {
          return false;
        }
      }
      return true;
    });
  }

  // ============================================================================
  // Message-specific method
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
    const executionEntity = this.registry.get(executionId);
    if (!executionEntity) {
      throw new WorkflowExecutionNotFoundError(`Workflow execution not found: ${executionId}`, executionId);
    }

    let messages = executionEntity.getMessages() || [];

    // Apply sorting
    if (orderBy === "desc") {
      messages.reverse();
    }

    // Implement pagination
    if (offset !== undefined || limit !== undefined) {
      const start = offset || 0;
      const end = limit !== undefined ? start + limit : undefined;
      messages = messages.slice(start, end);
    }

    return messages;
  }

  /**
   * Get the last N messages
   * @param executionId Execution ID
   * @param count Number of messages
   * @returns Array of messages
   */
  async getRecentMessages(executionId: string, count: number): Promise<LLMMessage[]> {
    const executionEntity = this.registry.get(executionId);
    if (!executionEntity) {
      throw new WorkflowExecutionNotFoundError(`Workflow execution not found: ${executionId}`, executionId);
    }

    const messages = executionEntity.getMessages() || [];
    return messages.slice(-count);
  }

  /**
   * Search for messages
   * @param executionId Execution ID
   * @param query Search keyword
   * @returns Array of matching messages
   */
  async searchMessages(executionId: string, query: string): Promise<LLMMessage[]> {
    const executionEntity = this.registry.get(executionId);
    if (!executionEntity) {
      throw new WorkflowExecutionNotFoundError(`Workflow execution not found: ${executionId}`, executionId);
    }

    const messages = executionEntity.getMessages() || [];
    return messages.filter((message: LLMMessage) => {
      const content =
        typeof message.content === "string" ? message.content : JSON.stringify(message.content);
      return content.toLowerCase().includes(query.toLowerCase());
    });
  }

  /**
   * Get message statistics information
   * @param executionId: Execution ID
   * @returns: Statistical information
   */
  async getMessageStats(executionId: string): Promise<MessageStats> {
    const executionEntity = this.registry.get(executionId);
    if (!executionEntity) {
      throw new WorkflowExecutionNotFoundError(`Workflow execution not found: ${executionId}`, executionId);
    }

    const messages = executionEntity.getMessages() || [];

    const stats: MessageStats = {
      total: messages.length,
      byRole: {},
      byType: {},
    };

    for (const message of messages) {
      // Count by role
      stats.byRole[message.role] = (stats.byRole[message.role] || 0) + 1;

      // Count by type
      const type = typeof message.content === "string" ? "text" : "object";
      stats.byType[type] = (stats.byType[type] || 0) + 1;
    }

    return stats;
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
    const executionEntities = this.registry.getAll();
    const stats = {
      total: 0,
      byExecution: {} as Record<string, number>,
      byRole: {} as Record<string, number>,
    };

    for (const executionEntity of executionEntities) {
      const messages = executionEntity.getMessages() || [];
      const executionId = executionEntity.id;

      stats.byExecution[executionId] = messages.length;
      stats.total += messages.length;

      for (const message of messages) {
        stats.byRole[message.role] = (stats.byRole[message.role] || 0) + 1;
      }
    }

    return stats;
  }

  /**
   * Get the message conversation history
   * @param executionId Execution ID
   * @param maxMessages Maximum number of messages
   * @returns Array of conversation history
   */
  async getConversationHistory(executionId: string, maxMessages?: number): Promise<LLMMessage[]> {
    const messages = await this.getWorkflowExecutionMessages(executionId);

    if (maxMessages && messages.length > maxMessages) {
      return messages.slice(-maxMessages);
    }

    return messages;
  }

  /**
   * Get the underlying WorkflowExecutionRegistry instance
   * @returns WorkflowExecutionRegistry instance
   */
  getRegistry(): WorkflowExecutionRegistry {
    return this.registry;
  }
}
