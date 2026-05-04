/**
 * MessageHistory - Message History Manager
 * Manages the message history of the Agent Loop
 *
 * Core Responsibilities:
 * 1. Manages the message history of the Agent Loop.
 * 2. Provides functionality for querying and clearing the message history.
 * 3. Supports state snapshots and recovery (for use as checkpoints).
 *
 * Design Principles:
 * - Instance Isolation: Each AgentLoopEntity maintains its own independent message manager instance.
 * - Implements the LifecycleCapable interface to support snapshot and recovery features.
 */

import type { LLMMessage } from "@wf-agent/types";
import type { StateManager } from "../../core/types/state-manager.js";
import { RuntimeValidationError } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "MessageHistory" });

/**
 * Message History Status API
 */
export interface MessageHistoryState {
  messages: LLMMessage[];
}

/**
 * MessageHistory - The message history manager class
 */
export class MessageHistory implements StateManager<MessageHistoryState> {
  private messages: LLMMessage[] = [];

  constructor(private agentLoopId: string) {
    logger.debug("MessageHistory created", { agentLoopId });
  }

  /**
   * Get the Agent Loop ID
   * @returns Agent Loop ID
   */
  getAgentLoopId(): string {
    return this.agentLoopId;
  }

  /**
   * Add a message
   * @param message LLM message
   */
  addMessage(message: LLMMessage): void {
    // Validate input
    if (!message) {
      throw new RuntimeValidationError("Message cannot be null or undefined", {
        operation: "addMessage",
        field: "message",
      });
    }

    if (!message.role) {
      throw new RuntimeValidationError("Message role is required", {
        operation: "addMessage",
        field: "message.role",
      });
    }

    this.messages.push(message);
  }

  /**
   * Get all messages
   * @returns A copy of the array of messages
   */
  getMessages(): LLMMessage[] {
    return [...this.messages];
  }

  /**
   * Paginated retrieval of messages
   * @param options Paging parameters
   */
  getMessagesPaged(options: { offset?: number; limit?: number } = {}): {
    total: number;
    messages: LLMMessage[];
  } {
    const total = this.messages.length;
    const limit = options.limit ?? 100;
    const offset = options.offset ?? 0;
    const messages = this.messages.slice(offset, offset + limit).map(msg => ({ ...msg }));
    return { total, messages };
  }

  /**
   * Get the latest messages
   * @param count Number of messages
   * @returns Array of the latest messages
   */
  getRecentMessages(count: number): LLMMessage[] {
    return this.messages.slice(-count).map(msg => ({ ...msg }));
  }

  /**
   * Search for messages (basic implementation)
   * @param filter Filter
   */
  findMessages(filter: { role?: string; contentContains?: string }): number[] {
    const results: number[] = [];
    this.messages.forEach((msg, index) => {
      let match = true;
      if (filter.role && msg.role !== filter.role) match = false;
      if (
        filter.contentContains &&
        typeof msg.content === "string" &&
        !msg.content.includes(filter.contentContains)
      )
        match = false;
      if (match) results.push(index);
    });
    return results;
  }

  /**
   * Set message history
   * @param messages List of messages
   */
  setMessages(messages: LLMMessage[]): void {
    // Validate input
    if (!messages || !Array.isArray(messages)) {
      throw new RuntimeValidationError("Messages must be an array", {
        operation: "setMessages",
        field: "messages",
      });
    }

    // Validate each message
    messages.forEach((msg, index) => {
      if (!msg) {
        throw new RuntimeValidationError(`Message at index ${index} is null or undefined`, {
          operation: "setMessages",
          field: `messages[${index}]`,
        });
      }
      if (!msg.role) {
        throw new RuntimeValidationError(`Message at index ${index} is missing role`, {
          operation: "setMessages",
          field: `messages[${index}].role`,
        });
      }
    });

    logger.debug("Setting message history", {
      agentLoopId: this.agentLoopId,
      messageCount: messages.length,
    });
    this.messages = [...messages];
  }

  /**
   * Clear message history
   */
  clearMessages(): void {
    logger.info("Clearing message history", {
      agentLoopId: this.agentLoopId,
      previousMessageCount: this.messages.length,
    });
    this.messages = [];
  }

  /**
   * Get statistical information
   * @returns Statistical information
   */
  getStats(): {
    totalMessages: number;
    roleDistribution: Record<string, number>;
    totalTokenUsage: {
      promptTokens: number;
      completionTokens: number;
      totalTokens: number;
    };
  } {
    const distribution: Record<string, number> = {};
    const usage = { promptTokens: 0, completionTokens: 0, totalTokens: 0 };

    this.messages.forEach(msg => {
      distribution[msg.role] = (distribution[msg.role] || 0) + 1;
      const msgUsage = msg.metadata?.["usage"] as Record<string, number> | undefined;
      if (msgUsage) {
        usage.promptTokens += msgUsage["promptTokens"] || 0;
        usage.completionTokens += msgUsage["completionTokens"] || 0;
        usage.totalTokens += msgUsage["totalTokens"] || 0;
      }
    });

    return {
      totalMessages: this.messages.length,
      roleDistribution: distribution,
      totalTokenUsage: usage,
    };
  }

  /**
   * Standardization History: Handling Unresponsive Tool Calls
   * Refer to Lim-Code to ensure the tool call sequence is complete.
   */
  normalizeHistory(): void {
    const originalCount = this.messages.length;

    logger.debug("Normalizing message history", {
      agentLoopId: this.agentLoopId,
      originalMessageCount: originalCount,
    });

    const respondedToolCallIds = new Set<string>();

    // 1. Collect all the responded IDs.
    this.messages.forEach(msg => {
      if (msg.role === "tool" && msg.toolCallId) {
        respondedToolCallIds.add(msg.toolCallId);
      }
    });

    // 2. Check for any unresponsive assistant messages.
    const normalizedMessages: LLMMessage[] = [];
    let addedErrorMessages = 0;
    this.messages.forEach(msg => {
      normalizedMessages.push(msg);

      if (msg.role === "assistant" && msg.toolCalls) {
        msg.toolCalls.forEach(call => {
          if (!respondedToolCallIds.has(call.id)) {
            // Complete a response for failure/rejection.
            normalizedMessages.push({
              role: "tool",
              toolCallId: call.id,
              content: `Error: Tool call ${call.id} was not responded to before session end/reset.`,
              timestamp: Date.now(),
              metadata: { normalized: true },
            });
            respondedToolCallIds.add(call.id);
            addedErrorMessages++;
          }
        });
      }
    });

    this.messages = normalizedMessages;

    if (addedErrorMessages > 0) {
      logger.warn("Added error messages for unresponded tool calls during normalization", {
        agentLoopId: this.agentLoopId,
        addedErrorCount: addedErrorMessages,
        originalMessageCount: originalCount,
        normalizedMessageCount: this.messages.length,
      });
    } else {
      logger.debug("Message history normalized without changes", {
        agentLoopId: this.agentLoopId,
        messageCount: this.messages.length,
      });
    }
  }

  /**
   * Create a status snapshot
   * @returns A snapshot of the message history status
   */
  createSnapshot(): MessageHistoryState {
    return {
      messages: this.messages.map(msg => ({ ...msg })),
    };
  }

  /**
   * Restore from snapshot state
   * @param snapshot: A snapshot of the message history state
   */
  restoreFromSnapshot(snapshot: MessageHistoryState): void {
    this.messages = snapshot.messages.map(msg => ({ ...msg }));
  }

  /**
   * Clean up resources
   * Clear the message history
   */
  cleanup(): void {
    const messageCount = this.messages.length;
    logger.debug("Cleaning up MessageHistory", {
      agentLoopId: this.agentLoopId,
      messageCount,
    });
    this.messages = [];
  }

  /**
   * Get the number of messages managed
   * @returns Count of messages
   */
  size(): number {
    return this.messages.length;
  }

  /**
   * Check if the message history is empty (no messages)
   * @returns true if no messages exist
   */
  isEmpty(): boolean {
    return this.messages.length === 0;
  }

  /**
   * Reset to initial state
   * Clears all messages
   */
  reset(): void {
    this.cleanup();
  }
}
