/**
 * Message Array Operation Utility Class
 *
 * Provides purely functional methods for manipulating message arrays. All methods return a new array without modifying the original one.
 *
 * Design Principles:
 * - Pure Functionality: No side effects; the input array is not altered, and a new array is returned.
 * - Immutability: Operations are based on indices, ensuring the original array remains unchanged.
 * - Combinability: Supports chaining and combining of methods.
 * - Type Safety: Complete TypeScript type support is provided.
 * - Single Responsibility: Each method performs only one specific task.
 */

import type { LLMMessage } from "@wf-agent/types";
import { MessageRole } from "@wf-agent/types";

/**
 * Truncation options
 */
export interface TruncateOptions {
  /** Retain the first N messages */
  keepFirst?: number;
  /** Retain the last N messages. */
  keepLast?: number;
  /** Delete the first N messages. */
  removeFirst?: number;
  /** Delete the last N messages. */
  removeLast?: number;
  /** Truncate by range */
  range?: { start: number; end: number };
  /** Filter and truncate by role. */
  role?: MessageRole;
}

/**
 * Content filtering options
 */
export interface ContentFilterOptions {
  /** Contains keywords */
  contains?: string[];
  /** Exclude keywords */
  excludes?: string[];
}

/**
 * Message snapshot
 */
export interface MessageSnapshot {
  /** Message array */
  messages: LLMMessage[];
  /** Snapshot timestamp */
  timestamp: number;
  /** Thread ID */
  threadId?: string;
  /** Workflow ID */
  workflowId?: string;
  /** Number of messages */
  messageCount: number;
}

/**
 * Message validation results
 */
export interface MessageValidationResult {
  /** Is it valid? */
  valid: boolean;
  /** Error message */
  errors: string[];
}

/**
 * Message grouping results
 */
export type MessageGroupByRole = Record<MessageRole, LLMMessage[]>;

/**
 * Message Array Operation Utility Class
 */
export class MessageArrayUtils {
  /**
   * Truncate the message array
   * @param messages The original message array
   * @param options Truncation options
   * @returns The new, truncated message array
   */
  static truncateMessages(messages: LLMMessage[], options: TruncateOptions): LLMMessage[] {
    let result = [...messages];

    // If a role is specified, filter first by that role.
    if (options.role) {
      result = result.filter(msg => msg.role === options.role);
    }

    // Retain the first N messages
    if (options.keepFirst !== undefined) {
      if (options.keepFirst === 0) {
        return [];
      }
      result = result.slice(0, options.keepFirst);
    }

    // Retain the last N messages.
    if (options.keepLast !== undefined) {
      if (options.keepLast === 0) {
        return [];
      }
      result = result.slice(-options.keepLast);
    }

    // Delete the first N messages.
    if (options.removeFirst !== undefined && options.removeFirst > 0) {
      result = result.slice(options.removeFirst);
    }

    // Delete the last N messages.
    if (options.removeLast !== undefined && options.removeLast > 0) {
      result = result.slice(0, -options.removeLast);
    }

    // Truncate by range
    if (options.range) {
      result = result.slice(options.range.start, options.range.end);
    }

    return result;
  }

  /**
   * Insert messages at the specified position
   * @param messages Array of original messages
   * @param position Position to insert the messages (-1 indicates the end)
   * @param newMessages Array of messages to be inserted
   * @returns New array of messages after insertion
   */
  static insertMessages(
    messages: LLMMessage[],
    position: number,
    newMessages: LLMMessage[],
  ): LLMMessage[] {
    if (newMessages.length === 0) {
      return [...messages];
    }

    // "1" indicates that the insertion should be made at the end.
    if (position === -1) {
      return [...messages, ...newMessages];
    }

    // Handling negative indices
    let insertIndex = position;
    if (insertIndex < 0) {
      insertIndex = messages.length + insertIndex + 1;
    }

    // Boundary checks
    if (insertIndex < 0) {
      insertIndex = 0;
    } else if (insertIndex > messages.length) {
      insertIndex = messages.length;
    }

    const result = [...messages];
    result.splice(insertIndex, 0, ...newMessages);
    return result;
  }

  /**
   * Replace the message at the specified index
   * @param messages The original array of messages
   * @param index The index at which the message should be replaced
   * @param newMessage The new message to replace the existing one
   * @returns The new array of messages after the replacement
   * @throws Error An exception is thrown if the index is out of bounds
   */
  static replaceMessage(
    messages: LLMMessage[],
    index: number,
    newMessage: LLMMessage,
  ): LLMMessage[] {
    // Handling negative indices
    let actualIndex = index;
    if (index < 0) {
      actualIndex = messages.length + index;
    }

    // Verify the validity of the index.
    if (actualIndex < 0 || actualIndex >= messages.length) {
      throw new Error(`Index ${index} is out of bounds. Array length: ${messages.length}`);
    }

    const result = [...messages];
    result[actualIndex] = newMessage;
    return result;
  }

  /**
   * Clear the message array
   * @param messages The original message array
   * @param keepSystemMessage Whether to retain system messages
   * @returns The new message array after clearing
   */
  static clearMessages(messages: LLMMessage[], keepSystemMessage: boolean = true): LLMMessage[] {
    if (!keepSystemMessage) {
      return [];
    }

    // Retain only system messages.
    return messages.filter(msg => msg.role === "system");
  }

  /**
   * Filter messages by role
   * @param messages The original array of messages
   * @param roles The array of roles to retain
   * @returns The filtered new array of messages
   */
  static filterMessagesByRole(messages: LLMMessage[], roles: MessageRole[]): LLMMessage[] {
    return messages.filter(msg => roles.includes(msg.role));
  }

  /**
   * Filter messages by content keywords
   * @param messages: The original array of messages
   * @param options: The filtering options
   * @returns: The new array of filtered messages
   */
  static filterMessagesByContent(
    messages: LLMMessage[],
    options: ContentFilterOptions,
  ): LLMMessage[] {
    return messages.filter(msg => {
      const content = typeof msg.content === "string" ? msg.content : JSON.stringify(msg.content);

      // Filter by content keywords (including)
      if (options.contains && options.contains.length > 0) {
        if (!options.contains.some((keyword: string) => content.includes(keyword))) {
          return false;
        }
      }

      // Filter by content keywords (exclude).
      if (options.excludes && options.excludes.length > 0) {
        if (options.excludes.some((keyword: string) => content.includes(keyword))) {
          return false;
        }
      }

      return true;
    });
  }

  /**
   * Merge multiple message arrays
   * @param arrays The message arrays to be merged
   * @returns The merged new message array
   */
  static mergeMessageArrays(...arrays: LLMMessage[][]): LLMMessage[] {
    return arrays.flat();
  }

  /**
   * Deduplicate the message array
   * @param messages The original array of messages
   * @param keyFn A function used to generate unique keys
   * @returns A new array of messages after deduplication
   */
  static deduplicateMessages(
    messages: LLMMessage[],
    keyFn?: (msg: LLMMessage) => string,
  ): LLMMessage[] {
    if (keyFn) {
      const seen = new Set<string>();
      return messages.filter(msg => {
        const key = keyFn(msg);
        if (seen.has(key)) {
          return false;
        }
        seen.add(key);
        return true;
      });
    }

    // By default, duplicates are removed based on content and role.
    const seen = new Set<string>();
    return messages.filter(msg => {
      const content = typeof msg.content === "string" ? msg.content : JSON.stringify(msg.content);
      const key = `${msg.role}:${content}`;
      if (seen.has(key)) {
        return false;
      }
      seen.add(key);
      return true;
    });
  }

  /**
   * Extract messages within a specified range
   * @param messages: The original array of messages
   * @param start: The starting index
   * @param end: The ending index (exclusive)
   * @returns: The new array of extracted messages
   */
  static extractMessagesByRange(messages: LLMMessage[], start: number, end: number): LLMMessage[] {
    return messages.slice(start, end);
  }

  /**
   * Group messages by role
   * @param messages The original array of messages
   * @returns Objects with messages grouped by role
   */
  static splitMessagesByRole(messages: LLMMessage[]): MessageGroupByRole {
    const result: MessageGroupByRole = {
      system: [],
      user: [],
      assistant: [],
      tool: [],
    };

    for (const msg of messages) {
      if (result[msg.role]) {
        result[msg.role].push(msg);
      }
    }

    return result;
  }

  /**
   * Verify the validity of the message array
   * @param messages The array of messages to be verified
   * @returns The verification result
   */
  static validateMessageArray(messages: LLMMessage[]): MessageValidationResult {
    const errors: string[] = [];

    if (!Array.isArray(messages)) {
      errors.push("Messages must be an array");
      return { valid: false, errors };
    }

    const validRoles: MessageRole[] = ["system", "user", "assistant", "tool"];

    for (let i = 0; i < messages.length; i++) {
      const msg = messages[i];

      if (!msg) {
        errors.push(`Message at index ${i} is null or undefined`);
        continue;
      }

      // Verify the role
      if (!msg.role || !validRoles.includes(msg.role)) {
        errors.push(`Message at index ${i} has invalid role: ${msg.role}`);
      }

      // Verify the content.
      if (msg.content === undefined || msg.content === null) {
        errors.push(`Message at index ${i} has missing content`);
      }
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Deep copy of the message array
   * @param messages: The original message array
   * @returns: The copied new message array
   */
  static cloneMessages(messages: LLMMessage[]): LLMMessage[] {
    return messages.map(msg => ({
      ...msg,
      content:
        typeof msg.content === "object" && msg.content !== null
          ? JSON.parse(JSON.stringify(msg.content))
          : msg.content,
    }));
  }

  /**
   * Create a message snapshot (including metadata)
   * @param messages Array of original messages
   * @param metadata Metadata for the snapshot
   * @returns Message snapshot
   */
  static createMessageSnapshot(
    messages: LLMMessage[],
    metadata?: {
      timestamp?: number;
      threadId?: string;
      workflowId?: string;
    },
  ): MessageSnapshot {
    return {
      messages: this.cloneMessages(messages),
      timestamp: metadata?.timestamp || Date.now(),
      threadId: metadata?.threadId,
      workflowId: metadata?.workflowId,
      messageCount: messages.length,
    };
  }

  /**
   * Restore the message array from the snapshot
   * @param snapshot: The message snapshot
   * @returns: The message array
   */
  static restoreFromSnapshot(snapshot: MessageSnapshot): LLMMessage[] {
    return this.cloneMessages(snapshot.messages);
  }

  /**
   * Get the last N messages
   * @param messages: The original array of messages
   * @param count: The number of messages
   * @returns: The last N messages
   */
  static getRecentMessages(messages: LLMMessage[], count: number): LLMMessage[] {
    if (count === 0) {
      return [];
    }
    return messages.slice(-count);
  }

  /**
   * Get the last N messages for the specified role
   * @param messages Array of raw messages
   * @param role The role of the messages
   * @param count The number of messages
   * @returns The last N messages for the specified role
   */
  static getRecentMessagesByRole(
    messages: LLMMessage[],
    role: MessageRole,
    count: number,
  ): LLMMessage[] {
    const filtered = this.filterMessagesByRole(messages, [role]);
    return filtered.slice(-count);
  }

  /**
   * Search for messages
   * @param messages Array of original messages
   * @param query Search keyword
   * @returns Array of matching messages
   */
  static searchMessages(messages: LLMMessage[], query: string): LLMMessage[] {
    const lowerQuery = query.toLowerCase();
    return messages.filter(msg => {
      const content = typeof msg.content === "string" ? msg.content : JSON.stringify(msg.content);
      return content.toLowerCase().includes(lowerQuery);
    });
  }
}
