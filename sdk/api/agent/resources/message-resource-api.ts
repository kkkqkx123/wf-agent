/**
 * AgentLoopMessageResourceAPI - Agent Loop Message Resource Management API
 * Inherits ReadonlyResourceAPI, provides read-only operations
 *
 * Responsibilities:
 * - Encapsulates MessageHistory, provides message history management functionality
 * - Supports message query, search, statistics and other functionalities
 */

import { ReadonlyResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { LLMMessage, ID } from "@wf-agent/types";
import type { AgentLoopRegistry } from "../../../agent/stores/agent-loop-registry.js";
import { getContainer } from "../../../core/di/index.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";

/**
 * message filter
 */
export interface AgentLoopMessageFilter {
  /** Agent Loop ID */
  agentLoopId?: ID;
  /** Role Filtering */
  role?: string;
  /** Content Keywords */
  content?: string;
}

/**
 * Message Statistics
 */
export interface AgentLoopMessageStats {
  /** aggregate */
  total: number;
  /** Statistics by Role */
  byRole: Record<string, number>;
  /** Statistics by type */
  byType: Record<string, number>;
}

/**
 * AgentLoopMessageResourceAPI - Agent Loop Message Resource Management API
 */
export class AgentLoopMessageResourceAPI extends ReadonlyResourceAPI<
  LLMMessage,
  string,
  AgentLoopMessageFilter
> {
  private registry: AgentLoopRegistry;

  constructor() {
    super();
    const container = getContainer();
    this.registry = container.get(Identifiers.AgentLoopRegistry) as AgentLoopRegistry;
  }

  // ============================================================================
  // Implementing Abstract Methods
  // ============================================================================

  /**
   * Get a single message
   * @param id Message ID (format: agentLoopId:messageIndex)
   * @returns the message object, or null if it doesn't exist.
   */
  protected async getResource(id: string): Promise<LLMMessage | null> {
    const [agentLoopId, indexStr] = id.split(":");
    if (!agentLoopId || !indexStr) {
      return null;
    }

    const entity = this.registry.get(agentLoopId);
    if (!entity) {
      return null;
    }

    const messages = entity.getMessages();
    const index = parseInt(indexStr, 10);
    if (isNaN(index) || index < 0 || index >= messages.length) {
      return null;
    }

    return messages[index]!;
  }

  /**
   * Get all messages
   * @returns the array of messages
   */
  protected async getAllResources(): Promise<LLMMessage[]> {
    const entities = this.registry.getAll();
    const allMessages: LLMMessage[] = [];

    for (const entity of entities) {
      const messages = entity.getMessages();
      allMessages.push(...messages);
    }

    return allMessages;
  }

  /**
   * Applying Filter Criteria
   */
  protected override applyFilter(
    messages: LLMMessage[],
    filter: AgentLoopMessageFilter,
  ): LLMMessage[] {
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
  // message-specific method
  // ============================================================================

  /**
   * Get the list of messages in the Agent Loop
   * @param agentLoopId Agent Loop ID
   * @param limit Limit the number of returns.
   * @param offset offset
   * @param orderBy Order by
   * @returns the message array
   */
  async getAgentLoopMessages(
    agentLoopId: ID,
    limit?: number,
    offset?: number,
    orderBy: "asc" | "desc" = "asc",
  ): Promise<LLMMessage[]> {
    const entity = this.registry.get(agentLoopId);
    if (!entity) {
      return [];
    }

    let messages = entity.getMessages();

    // Application Sorting
    if (orderBy === "desc") {
      messages = [...messages].reverse();
    }

    // application paging
    if (offset !== undefined || limit !== undefined) {
      const start = offset || 0;
      const end = limit !== undefined ? start + limit : undefined;
      messages = messages.slice(start, end);
    }

    return messages;
  }

  /**
   * Get the last N messages
   * @param agentLoopId Agent Loop ID
   * @param count Number of messages
   * @returns array of messages
   */
  async getRecentMessages(agentLoopId: ID, count: number): Promise<LLMMessage[]> {
    const entity = this.registry.get(agentLoopId);
    if (!entity) {
      return [];
    }

    return entity.getRecentMessages(count);
  }

  /**
   * Search Message
   * @param agentLoopId Agent Loop ID
   * @param query Search keywords
   * @returns Array of matching messages
   */
  async searchMessages(agentLoopId: ID, query: string): Promise<LLMMessage[]> {
    const entity = this.registry.get(agentLoopId);
    if (!entity) {
      return [];
    }

    const messages = entity.getMessages();
    return messages.filter(message => {
      const content =
        typeof message.content === "string" ? message.content : JSON.stringify(message.content);
      return content.toLowerCase().includes(query.toLowerCase());
    });
  }

  /**
   * Get message statistics
   * @param agentLoopId Agent Loop ID
   * @returns Statistics
   */
  async getMessageStats(agentLoopId: ID): Promise<{
    total: number;
    byRole: Record<string, number>;
    byType: Record<string, number>;
    totalTokenUsage: { promptTokens: number; completionTokens: number; totalTokens: number };
  }> {
    const entity = this.registry.get(agentLoopId);
    if (!entity) {
      return {
        total: 0,
        byRole: {},
        byType: {},
        totalTokenUsage: { promptTokens: 0, completionTokens: 0, totalTokens: 0 },
      };
    }

    const messages = entity.getMessages();
    const tokenUsage = entity.conversationManager.getTokenUsage();

    // Build stats from ConversationSession data
    const roleDistribution: Record<string, number> = {};
    const typeDistribution: Record<string, number> = {};
    messages.forEach(msg => {
      roleDistribution[msg.role] = (roleDistribution[msg.role] || 0) + 1;
      const msgType = (msg as unknown as Record<string, unknown>)["type"] as string | undefined;
      if (msgType) {
        typeDistribution[msgType] = (typeDistribution[msgType] || 0) + 1;
      }
    });

    return {
      total: messages.length,
      byRole: roleDistribution,
      byType: typeDistribution,
      totalTokenUsage: tokenUsage,
    };
  }

  /**
   * Normalized message history
   * @param agentLoopId Agent Loop ID
   */
  async normalizeHistory(agentLoopId: ID): Promise<void> {
    const entity = this.registry.get(agentLoopId);
    if (entity) {
      entity.normalizeHistory();
    }
  }

  /**
   * Get message statistics for all Agent Loops
   * @returns global statistics
   */
  async getGlobalMessageStats(): Promise<{
    total: number;
    byAgentLoop: Record<string, number>;
    byRole: Record<string, number>;
  }> {
    const entities = this.registry.getAll();
    const stats = {
      total: 0,
      byAgentLoop: {} as Record<string, number>,
      byRole: {} as Record<string, number>,
    };

    for (const entity of entities) {
      const messages = entity.getMessages();
      const agentLoopId = entity.id;

      stats.byAgentLoop[agentLoopId] = messages.length;
      stats.total += messages.length;

      for (const message of messages) {
        stats.byRole[message.role] = (stats.byRole[message.role] || 0) + 1;
      }
    }

    return stats;
  }

  /**
   * Get the message dialog history
   * @param agentLoopId Agent Loop ID
   * @param maxMessages Maximum number of messages
   * @returns Conversation History Array
   */
  async getConversationHistory(agentLoopId: ID, maxMessages?: number): Promise<LLMMessage[]> {
    const messages = await this.getAgentLoopMessages(agentLoopId);

    if (maxMessages && messages.length > maxMessages) {
      return messages.slice(-maxMessages);
    }

    return messages;
  }

  /**
   * Get the number of messages
   * @param agentLoopId Agent Loop ID
   * @returns Number of messages
   */
  async getMessageCount(agentLoopId: ID): Promise<number> {
    const entity = this.registry.get(agentLoopId);
    if (!entity) {
      return 0;
    }

    return entity.getMessages().length;
  }

  /**
   * Get the underlying AgentLoopRegistry instance
   * @returns AgentLoopRegistry instance
   */
  getRegistry(): AgentLoopRegistry {
    return this.registry;
  }
}
