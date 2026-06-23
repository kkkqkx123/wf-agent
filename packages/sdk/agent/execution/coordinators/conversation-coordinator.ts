/**
 * ConversationCoordinator - Conversation Coordinator
 *
 * Responsibilities:
 * - Provide stateless conversation management coordination logic
 * - Encapsulates conversation history normalization and statistics logic
 * - Coordinates message flow between multiple Agent Loops (future extension).
 *
 * Design Principles:
 * - Stateless: all state is held by the AgentLoopEntity or its manager.
 * - Coordination: acts as a high-level interface to coordinate with underlying components to accomplish complex tasks.
 */

import { LLMMessage } from "@wf-agent/types";
import { AgentLoopRegistry } from "../../stores/agent-loop-registry.js";
import { ConversationSession } from "../../../shared/messaging/conversation-session.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ConversationCoordinator" });

export class ConversationCoordinator {
  constructor(private registry: AgentLoopRegistry) {}

  /**
   * Get the dialog manager
   * @param agentLoopId Agent Loop ID
   */
  async getConversationManager(agentLoopId: string): Promise<ConversationSession | undefined> {
    logger.debug("Getting conversation manager", { agentLoopId });

    const loop = await this.registry.get(agentLoopId);
    if (!loop) {
      logger.warn("Agent Loop not found when getting conversation manager", { agentLoopId });
      return undefined;
    }

    const stateCoordinator = this.registry.getStateCoordinator(agentLoopId);
    return stateCoordinator?.getConversationManager();
  }

  /**
   * Normalize and get history
   * @param agentLoopId Agent Loop ID
   */
  async getNormalizedHistory(agentLoopId: string): Promise<LLMMessage[]> {
    logger.debug("Getting normalized conversation history", { agentLoopId });

    const manager = await this.getConversationManager(agentLoopId);
    if (!manager) {
      logger.warn("Conversation manager not found", { agentLoopId });
      return [];
    }

    // ConversationSession doesn't have normalizeHistory, just get messages
    const messages = manager.getMessages();

    logger.debug("Normalized conversation history retrieved", {
      agentLoopId,
      messageCount: messages.length,
    });

    return messages;
  }

  /**
   * Get conversation statistics
   * @param agentLoopId Agent Loop ID
   */
  async getConversationStats(agentLoopId: string) {
    logger.debug("Getting conversation statistics", { agentLoopId });

    const manager = await this.getConversationManager(agentLoopId);
    if (!manager) {
      logger.warn("Conversation manager not found when getting stats", { agentLoopId });
      return undefined;
    }

    const messages = manager.getMessages();
    const tokenUsage = manager.getTokenUsage();

    // Build stats from ConversationSession data
    const roleDistribution: Record<string, number> = {};
    messages.forEach(msg => {
      roleDistribution[msg.role] = (roleDistribution[msg.role] || 0) + 1;
    });

    const stats = {
      totalMessages: messages.length,
      roleDistribution,
      totalTokenUsage: tokenUsage,
    };

    logger.debug("Conversation statistics retrieved", {
      agentLoopId,
      totalMessages: stats.totalMessages,
      roleDistribution: stats.roleDistribution,
    });

    return stats;
  }
}
