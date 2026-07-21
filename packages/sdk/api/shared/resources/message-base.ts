/**
 * BaseMessageResourceAPI - Shared base class for message resource management
 *
 * Provides a unified base implementation for message CRUD and query operations.
 * Both Agent and Workflow versions extend this class, providing only entity-specific
 * type annotations and method naming.
 *
 * Entity-specific subclasses must implement:
 * - getEntityMessages(entityId): LLMMessage[] - Get messages for an entity
 * - getAllEntityMessages(): GetAllMessagesResult - Get all messages across all entities
 * - parseMessageId(id): ParseResult - Parse the composite ID format
 */

import { QueryableResourceAPI } from "./generic-resource-api.js";
import type { LLMMessage } from "@wf-agent/types";

/**
 * Base message filter - entity-specific filters extend this
 */
export interface BaseMessageFilter {
  /** Filter by role */
  role?: string;
  /** Filter by content (fuzzy match) */
  content?: string;
}

/**
 * Result of parsing a composite message ID
 */
export interface ParsedMessageId {
  /** Entity ID (execution ID or agent loop ID) */
  entityId: string;
  /** Index within the entity's message array */
  index: number;
}

/**
 * Result of getting all messages across all entities
 */
export interface GetAllMessagesResult {
  /** All messages */
  messages: LLMMessage[];
  /** Map of entity ID to message count */
  entityCounts: Record<string, number>;
}

/**
 * BaseMessageResourceAPI - Shared base class for message resource management
 *
 * Handles common message operations:
 * - getResource, getAllResources, applyFilter
 * - getMessages, getRecentMessages, searchMessages
 * - getMessageStats, getGlobalMessageStats, getConversationHistory
 */
export abstract class BaseMessageResourceAPI<
  TFilter extends BaseMessageFilter = BaseMessageFilter,
> extends QueryableResourceAPI<LLMMessage, string, TFilter> {
  /**
   * Get messages for a specific entity
   * @param entityId Entity ID (execution ID or agent loop ID)
   * @returns Array of messages
   */
  protected abstract getEntityMessages(entityId: string): LLMMessage[] | Promise<LLMMessage[]>;

  /**
   * Get all messages across all entities
   * @returns Messages and entity counts
   */
  protected abstract getAllEntityMessages(): GetAllMessagesResult | Promise<GetAllMessagesResult>;

  /**
   * Parse a composite message ID into entity ID and index
   * @param id Composite message ID
   * @returns Parsed components or null if invalid
   */
  protected abstract parseMessageId(id: string): ParsedMessageId | null;

  /**
   * Get a single message by composite ID
   * @param id Composite message ID (format depends on subclass)
   * @returns Message or null
   */
  protected async getResource(id: string): Promise<LLMMessage | null> {
    const parsed = this.parseMessageId(id);
    if (!parsed) {
      return null;
    }

    const messages = await this.getEntityMessages(parsed.entityId);
    if (parsed.index < 0 || parsed.index >= messages.length) {
      return null;
    }

    return messages[parsed.index] ?? null;
  }

  /**
   * Get all messages across all entities
   * @returns Array of all messages
   */
  protected async getAllResources(): Promise<LLMMessage[]> {
    const result = await this.getAllEntityMessages();
    return result.messages;
  }

  /**
   * Apply filter criteria to messages
   */
  protected override applyFilter(messages: LLMMessage[], filter: TFilter): LLMMessage[] {
    return messages.filter((message) => {
      if (filter.role && message.role !== filter.role) {
        return false;
      }
      if (filter.content) {
        const content =
          typeof message.content === "string"
            ? message.content
            : JSON.stringify(message.content);
        if (!content.toLowerCase().includes(filter.content.toLowerCase())) {
          return false;
        }
      }
      return true;
    });
  }

  /**
   * Get message list for an entity with pagination and sorting
   * @param entityId Entity ID
   * @param limit Limit on the number of messages to return
   * @param offset Offset for starting the message retrieval
   * @param orderBy Sorting method
   * @returns Array of messages
   */
  protected async getEntityMessagesWithOptions(
    entityId: string,
    limit?: number,
    offset?: number,
    orderBy: "asc" | "desc" = "asc",
  ): Promise<LLMMessage[]> {
    let messages = await this.getEntityMessages(entityId);

    // Apply sorting
    if (orderBy === "desc") {
      messages = [...messages].reverse();
    }

    // Apply pagination
    if (offset !== undefined || limit !== undefined) {
      const start = offset || 0;
      const end = limit !== undefined ? start + limit : undefined;
      messages = messages.slice(start, end);
    }

    return messages;
  }

  /**
   * Get the last N messages for an entity
   * @param entityId Entity ID
   * @param count Number of messages
   * @returns Array of messages
   */
  protected async getRecentEntityMessages(entityId: string, count: number): Promise<LLMMessage[]> {
    const messages = await this.getEntityMessages(entityId);
    return messages.slice(-count);
  }

  /**
   * Search messages by content
   * @param entityId Entity ID
   * @param query Search keyword
   * @returns Array of matching messages
   */
  protected async searchEntityMessages(entityId: string, query: string): Promise<LLMMessage[]> {
    const messages = await this.getEntityMessages(entityId);
    return messages.filter((message) => {
      const content =
        typeof message.content === "string"
          ? message.content
          : JSON.stringify(message.content);
      return content.toLowerCase().includes(query.toLowerCase());
    });
  }

  /**
   * Get message statistics for an entity
   * @param entityId Entity ID
   * @returns Statistics
   */
  protected async getEntityMessageStats(
    entityId: string,
  ): Promise<{
    total: number;
    byRole: Record<string, number>;
    byType: Record<string, number>;
  }> {
    const messages = await this.getEntityMessages(entityId);

    const byRole: Record<string, number> = {};
    const byType: Record<string, number> = {};

    for (const message of messages) {
      byRole[message.role] = (byRole[message.role] || 0) + 1;
      const type = typeof message.content === "string" ? "text" : "object";
      byType[type] = (byType[type] || 0) + 1;
    }

    return {
      total: messages.length,
      byRole,
      byType,
    };
  }

  /**
   * Get global message statistics across all entities
   * @returns Global statistics
   */
  protected async getGlobalMessagesStats(): Promise<{
    total: number;
    byEntity: Record<string, number>;
    byRole: Record<string, number>;
  }> {
    const result = await this.getAllEntityMessages();

    const byRole: Record<string, number> = {};
    for (const message of result.messages) {
      byRole[message.role] = (byRole[message.role] || 0) + 1;
    }

    return {
      total: result.messages.length,
      byEntity: result.entityCounts,
      byRole,
    };
  }

  /**
   * Get conversation history for an entity
   * @param entityId Entity ID
   * @param maxMessages Maximum number of messages
   * @returns Conversation history array
   */
  protected async getEntityConversationHistory(
    entityId: string,
    maxMessages?: number,
  ): Promise<LLMMessage[]> {
    const messages = await this.getEntityMessagesWithOptions(entityId);

    if (maxMessages && messages.length > maxMessages) {
      return messages.slice(-maxMessages);
    }

    return messages;
  }
}