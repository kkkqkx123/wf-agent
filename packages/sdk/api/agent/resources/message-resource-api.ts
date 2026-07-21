/**
 * AgentLoopMessageResourceAPI - Agent Loop Message Resource Management API
 * Provides APIs for managing messages in agent loop executions.
 * Extends the shared BaseMessageResourceAPI with agent-specific implementation.
 */

import {
  BaseMessageResourceAPI,
  type BaseMessageFilter,
  type ParsedMessageId,
  type GetAllMessagesResult,
} from "../../shared/resources/message-base.js";
import type { LLMMessage, ID } from "@wf-agent/types";
import type { AgentLoopRegistry } from "../../../agent/registry/agent-loop-registry.js";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";

/**
 * Agent Loop Message Filter
 */
export interface AgentLoopMessageFilter extends BaseMessageFilter {
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
 * Agent Loop Message Stats
 */
export interface AgentLoopMessageStats {
  /** Total number of messages */
  total: number;
  /** Distribution by role */
  byRole: Record<string, number>;
  /** Distribution by type */
  byType: Record<string, number>;
  /** Token usage statistics */
  totalTokenUsage: {
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
  };
}

/**
 * AgentLoopMessageResourceAPI - Agent Loop Message Resource Management API
 */
export class AgentLoopMessageResourceAPI extends BaseMessageResourceAPI<AgentLoopMessageFilter> {
  private registry: AgentLoopRegistry;

  constructor(deps: APIDependencyManager) {
    super();
    this.registry = deps.getAgentLoopRegistry();
  }

  // ============================================================================
  // Implement BaseMessageResourceAPI abstract methods
  // ============================================================================

  /**
   * Get messages for a specific agent loop
   */
  protected getEntityMessages(agentLoopId: string): LLMMessage[] {
    const stateCoordinator = this.registry.getStateCoordinator(agentLoopId);
    return stateCoordinator?.getMessages() ?? [];
  }

  /**
   * Get all messages across all agent loops
   */
  protected getAllEntityMessages(): GetAllMessagesResult {
    const entities = this.registry.getAll();
    const allMessages: LLMMessage[] = [];
    const entityCounts: Record<string, number> = {};

    for (const entity of entities) {
      const stateCoordinator = this.registry.getStateCoordinator(entity.id);
      const messages = stateCoordinator?.getMessages() ?? [];
      entityCounts[entity.id] = messages.length;
      allMessages.push(...messages);
    }

    return { messages: allMessages, entityCounts };
  }

  /**
   * Parse composite message ID (format: agentLoopId:messageIndex)
   */
  protected parseMessageId(id: string): ParsedMessageId | null {
    const [agentLoopId, indexStr] = id.split(":");
    if (!agentLoopId || !indexStr) {
      return null;
    }

    const index = parseInt(indexStr, 10);
    if (isNaN(index) || index < 0) {
      return null;
    }

    return { entityId: agentLoopId, index };
  }

  // ============================================================================
  // Agent-specific message methods
  // ============================================================================

  /**
   * Get the list of messages in the Agent Loop
   * @param agentLoopId Agent Loop ID
   * @param limit Limit the number of returns
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
    return this.getEntityMessagesWithOptions(agentLoopId, limit, offset, orderBy);
  }

  /**
   * Get the last N messages
   * @param agentLoopId Agent Loop ID
   * @param count Number of messages
   * @returns array of messages
   */
  async getRecentMessages(agentLoopId: ID, count: number): Promise<LLMMessage[]> {
    return this.getRecentEntityMessages(agentLoopId, count);
  }

  /**
   * Search Message
   * @param agentLoopId Agent Loop ID
   * @param query Search keywords
   * @returns Array of matching messages
   */
  async searchMessages(agentLoopId: ID, query: string): Promise<LLMMessage[]> {
    return this.searchEntityMessages(agentLoopId, query);
  }

  /**
   * Get message statistics
   * @param agentLoopId Agent Loop ID
   * @returns Statistics
   */
  async getMessageStats(agentLoopId: ID): Promise<AgentLoopMessageStats> {
    const entity = await this.registry.get(agentLoopId);
    if (!entity) {
      return {
        total: 0,
        byRole: {},
        byType: {},
        totalTokenUsage: { promptTokens: 0, completionTokens: 0, totalTokens: 0 },
      };
    }

    const stateCoordinator = this.registry.getStateCoordinator(agentLoopId);
    const messages = stateCoordinator?.getMessages() ?? [];
    const tokenUsage = stateCoordinator?.getTokenUsage() ?? {
      promptTokens: 0,
      completionTokens: 0,
      totalTokens: 0,
    };

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
      totalTokenUsage: tokenUsage,
    };
  }

  /**
   * Normalized message history
   * @param agentLoopId Agent Loop ID
   */
  async normalizeHistory(agentLoopId: ID): Promise<void> {
    const stateCoordinator = this.registry.getStateCoordinator(agentLoopId);
    if (stateCoordinator) {
      stateCoordinator.normalizeHistory();
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
    const stats = await this.getGlobalMessagesStats();
    return {
      total: stats.total,
      byAgentLoop: stats.byEntity,
      byRole: stats.byRole,
    };
  }

  /**
   * Get the message dialog history
   * @param agentLoopId Agent Loop ID
   * @param maxMessages Maximum number of messages
   * @returns Conversation History Array
   */
  async getConversationHistory(agentLoopId: ID, maxMessages?: number): Promise<LLMMessage[]> {
    return this.getEntityConversationHistory(agentLoopId, maxMessages);
  }

  /**
   * Get the number of messages
   * @param agentLoopId Agent Loop ID
   * @returns Number of messages
   */
  async getMessageCount(agentLoopId: ID): Promise<number> {
    const messages = await this.getEntityMessages(agentLoopId);
    return messages.length;
  }

  /**
   * Get the underlying AgentLoopRegistry instance
   * @returns AgentLoopRegistry instance
   */
  getRegistry(): AgentLoopRegistry {
    return this.registry;
  }
}