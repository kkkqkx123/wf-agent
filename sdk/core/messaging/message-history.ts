/**
 * Message History Base Class
 * Provides basic message history management capabilities, including batch visibility control.
 *
 * Core Responsibilities:
 * 1. Operations for adding, deleting, and querying messages
 * 2. Batch visibility control (used for context compression, checkpoint recovery)
 * 3. Provides convenient methods for message construction
 * 4. Supports message querying and filtering
 *
 * Design Principles:
 * - No external dependencies: Does not rely on Graph or Agent-specific components
 * - Scalability: Can be extended by subclasses (e.g., ConversationSession)
 * - Immutability: All returned messages are copies
 * - Batch visibility: Supports control over message visibility for context compression
 */

import type { LLMMessage, LLMToolCall, MessageRole, MessageMarkMap } from "@wf-agent/types";
import type { CheckpointStorageCallback } from "@wf-agent/storage";
import { MessageArrayUtils } from "../utils/messages/message-array-utils.js";
import {
  startNewBatch,
  startNewBatchWithCheckpoint,
  rollbackToBatch as rollbackBatch,
  getBatchInfo,
  getAllBatchesInfo,
  getBatchesToRelease,
  updateBatchCheckpoint,
  getBatchCheckpointId,
  isBatchInMemory,
  rebuildIndicesAfterRelease,
} from "../utils/messages/batch-management-utils.js";
import {
  getVisibleMessages,
  getVisibleMessageCount,
  getInvisibleMessages,
  getInvisibleMessageCount,
  isMessageVisible,
  getCurrentBoundary,
} from "../utils/messages/visible-range-calculator.js";
import {
  getIndicesByRole,
  getCountByRole,
  getVisibleIndicesByRole,
  getVisibleRecentIndicesByRole,
  getVisibleCountByRole,
} from "../utils/messages/message-index-utils.js";

/**
 * Message History Configuration
 */
export interface MessageHistoryConfig {
  /** Initial message list */
  initialMessages?: LLMMessage[];
}

/**
 * Message History Status (for use in snapshots)
 */
export interface MessageHistoryState {
  /** Message List */
  messages: LLMMessage[];
  /** Message Tag Mapping */
  markMap: MessageMarkMap;
}

/**
 * Message History Base Class
 *
 * Provides basic message history management capabilities, which can be extended by both Agent and Graph.
 * It includes batch visibility control, supports context compression, and checkpoint recovery.
 */
export class MessageHistory {
  protected messages: LLMMessage[] = [];
  protected markMap: MessageMarkMap;

  /**
   * Constructor
   * @param config Configuration options
   */
  constructor(config: MessageHistoryConfig = {}) {
    if (config.initialMessages) {
      this.messages = MessageArrayUtils.cloneMessages(config.initialMessages);
    }

    // Initialize the token mapping
    this.markMap = {
      originalIndices: this.messages.map((_, index) => index),
      batchBoundaries: [0],
      boundaryToBatch: [0],
      currentBatch: 0,
    };
  }

  // ============================================================
  // Basic Operations
  // ============================================================

  /**
   * Add a single message
   * @param message The message object
   * @returns The length of the array of added messages
   */
  addMessage(message: LLMMessage): number {
    this.messages.push({ ...message });
    const newIndex = this.messages.length - 1;

    // Synchronize the update of tag mappings.
    this.markMap.originalIndices.push(newIndex);

    return this.messages.length;
  }

  /**
   * Batch add messages
   * @param messages: An array of messages
   * @returns: The length of the array of messages after addition
   */
  addMessages(...messages: LLMMessage[]): number {
    for (const message of messages) {
      this.addMessage(message);
    }
    return this.messages.length;
  }

  /**
   * Retrieve visible messages (messages after the batch boundary)
   * This is the message sent to the LLM
   * @returns A copy of the array of visible messages
   */
  getMessages(): LLMMessage[] {
    return getVisibleMessages(this.messages, this.markMap);
  }

  /**
   * Get all messages (including invisible messages)
   * @returns A copy of the array of all messages
   */
  getAllMessages(): LLMMessage[] {
    return MessageArrayUtils.cloneMessages(this.messages);
  }

  /**
   * Get the number of messages (only visible messages)
   * @returns Number of visible messages
   */
  getMessageCount(): number {
    return getVisibleMessageCount(this.markMap);
  }

  /**
   * Get the total number of messages (including invisible messages)
   * @returns Total number of messages
   */
  getTotalMessageCount(): number {
    return this.messages.length;
  }

  /**
   * Clear all messages
   */
  clear(): void {
    this.messages = [];
    this.markMap = {
      originalIndices: [],
      batchBoundaries: [0],
      boundaryToBatch: [0],
      currentBatch: 0,
    };
  }

  // ============================================================
  // Batch Visibility Control
  // ============================================================

  /**
   * Start a new batch
   * This is used to mark the beginning of a new conversation phase; previous messages will be marked as invisible.
   * @param boundaryIndex Boundary index (default is the current number of messages)
   * @returns New batch number
   */
  startNewBatch(boundaryIndex?: number): number {
    const index = boundaryIndex ?? this.messages.length;
    this.markMap = startNewBatch(this.markMap, index);
    return this.markMap.currentBatch;
  }

  /**
   * Roll back to a specified batch
   * Used to undo or revert to a previous state of the conversation
   * @param targetBatch The target batch number
   */
  rollbackToBatch(targetBatch: number): void {
    this.markMap = rollbackBatch(this.markMap, targetBatch);
  }

  /**
   * Get the current batch number
   * @returns The current batch number
   */
  getCurrentBatch(): number {
    return this.markMap.currentBatch;
  }

  /**
   * Get batch information
   * @param batchId: Batch ID
   * @returns: Batch information
   */
  getBatchInfo(batchId: number) {
    return getBatchInfo(this.markMap, batchId);
  }

  /**
   * Get all batch information
   * @returns All batch information
   */
  getAllBatchesInfo() {
    return getAllBatchesInfo(this.markMap);
  }

  /**
   * Get the current batch boundary index
   * @returns Boundary index
   */
  getCurrentBoundary(): number {
    return getCurrentBoundary(this.markMap);
  }

  /**
   * Get the tag map
   * @returns A copy of the tag map
   */
  getMarkMap(): MessageMarkMap {
    return {
      ...this.markMap,
      originalIndices: [...this.markMap.originalIndices],
      batchBoundaries: [...this.markMap.batchBoundaries],
      boundaryToBatch: [...this.markMap.boundaryToBatch],
    };
  }

  /**
   * Set the marker mapping
   * @param markMap Marker mapping
   */
  setMarkMap(markMap: MessageMarkMap): void {
    this.markMap = {
      ...markMap,
      originalIndices: [...markMap.originalIndices],
      batchBoundaries: [...markMap.batchBoundaries],
      boundaryToBatch: [...markMap.boundaryToBatch],
      batchToCheckpoint: markMap.batchToCheckpoint ? [...markMap.batchToCheckpoint] : undefined,
      memoryRange: markMap.memoryRange ? { ...markMap.memoryRange } : undefined,
    };
  }

  // ============================================================
  // Checkpoint Memory Optimization
  // ============================================================

  /**
   * Start a new batch with checkpoint support for memory optimization
   * This will save the previous batch's messages to checkpoint and release them from memory
   * @param checkpointStorage Checkpoint storage callback
   * @param boundaryIndex Boundary index (default is the current number of messages)
   * @param keepInMemory Number of recent batches to keep in memory (default: 2)
   * @returns New batch number
   */
  async startNewBatchWithCheckpoint(
    checkpointStorage: CheckpointStorageCallback,
    boundaryIndex?: number,
    keepInMemory: number = 2,
  ): Promise<number> {
    const index = boundaryIndex ?? this.messages.length;

    // Save current batch messages to checkpoint before starting new batch
    const currentBatch = this.markMap.currentBatch;
    const batchMessages = this.getBatchMessages(currentBatch);

    if (batchMessages.length > 0) {
      // Serialize and save to checkpoint
      const checkpointData = this.serializeBatchMessages(batchMessages);
      const checkpointId = `batch-${currentBatch}-${Date.now()}`;

      await checkpointStorage.save(checkpointId, checkpointData, {
        threadId: "message-history",
        workflowId: "batch-checkpoint",
        timestamp: Date.now(),
      });

      // Update markMap with checkpoint mapping
      this.markMap = startNewBatchWithCheckpoint(this.markMap, index, checkpointId);
    } else {
      this.markMap = startNewBatch(this.markMap, index);
    }

    // Release old batches from memory
    await this.releaseOldBatches(checkpointStorage, keepInMemory);

    return this.markMap.currentBatch;
  }

  /**
   * Get messages for a specific batch
   * @param batchId Batch ID
   * @returns Array of messages in the batch
   */
  protected getBatchMessages(batchId: number): LLMMessage[] {
    const boundaryIndex = this.markMap.batchBoundaries[batchId];
    const nextBoundaryIndex = this.markMap.batchBoundaries[batchId + 1] ?? this.messages.length;

    if (boundaryIndex === undefined) {
      return [];
    }

    const messages: LLMMessage[] = [];
    for (let i = boundaryIndex; i < nextBoundaryIndex && i < this.messages.length; i++) {
      const msg = this.messages[i];
      if (msg) {
        messages.push({ ...msg });
      }
    }
    return messages;
  }

  /**
   * Serialize batch messages for checkpoint storage
   * @param messages Array of messages
   * @returns Serialized data as Uint8Array
   */
  protected serializeBatchMessages(messages: LLMMessage[]): Uint8Array {
    const data = JSON.stringify(messages);
    return new TextEncoder().encode(data);
  }

  /**
   * Deserialize batch messages from checkpoint data
   * @param data Serialized data
   * @returns Array of messages
   */
  protected deserializeBatchMessages(data: Uint8Array): LLMMessage[] {
    const decoder = new TextDecoder();
    const jsonStr = decoder.decode(data);
    return JSON.parse(jsonStr) as LLMMessage[];
  }

  /**
   * Release old batches from memory
   * @param checkpointStorage Checkpoint storage callback
   * @param keepInMemory Number of recent batches to keep in memory
   */
  protected async releaseOldBatches(
    checkpointStorage: CheckpointStorageCallback,
    keepInMemory: number,
  ): Promise<void> {
    const batchesToRelease = getBatchesToRelease(this.markMap, keepInMemory);

    if (batchesToRelease.length === 0) {
      return;
    }

    // Get indices to remove
    const indicesToRemove = new Set<number>();
    for (const batchId of batchesToRelease) {
      const boundaryIndex = this.markMap.batchBoundaries[batchId];
      const nextBoundaryIndex = this.markMap.batchBoundaries[batchId + 1] ?? this.messages.length;

      if (boundaryIndex !== undefined) {
        for (let i = boundaryIndex; i < nextBoundaryIndex; i++) {
          indicesToRemove.add(i);
        }
      }
    }

    // Filter out released messages
    this.messages = this.messages.filter((_, index) => !indicesToRemove.has(index));

    // Rebuild indices
    this.markMap = rebuildIndicesAfterRelease(this.markMap, batchesToRelease);

    // Update memory range
    const currentBatch = this.markMap.currentBatch;
    this.markMap.memoryRange = {
      startBatch: Math.max(0, currentBatch - keepInMemory + 1),
      endBatch: currentBatch,
    };
  }

  /**
   * Load batch messages from checkpoint
   * @param batchId Batch ID
   * @param checkpointStorage Checkpoint storage callback
   * @returns Array of messages, or null if not found
   */
  async loadBatchFromCheckpoint(
    batchId: number,
    checkpointStorage: CheckpointStorageCallback,
  ): Promise<LLMMessage[] | null> {
    const checkpointId = getBatchCheckpointId(this.markMap, batchId);

    if (!checkpointId) {
      return null;
    }

    try {
      const data = await checkpointStorage.load(checkpointId);
      if (!data) {
        return null;
      }

      return this.deserializeBatchMessages(data);
    } catch {
      return null;
    }
  }

  /**
   * Check if a batch is loaded in memory
   * @param batchId Batch ID
   * @returns True if batch is in memory
   */
  isBatchInMemory(batchId: number): boolean {
    return isBatchInMemory(this.markMap, batchId);
  }

  /**
   * Update checkpoint ID for a batch
   * Used when re-saving or migrating checkpoints to a different storage
   * @param batchId Batch ID
   * @param checkpointId New checkpoint ID
   */
  updateBatchCheckpoint(batchId: number, checkpointId: string): void {
    this.markMap = updateBatchCheckpoint(this.markMap, batchId, checkpointId);
  }

  /**
   * Get all messages including those from checkpoints
   * @param checkpointStorage Checkpoint storage callback
   * @returns Complete array of all messages
   */
  async getAllMessagesWithRestore(
    checkpointStorage: CheckpointStorageCallback,
  ): Promise<LLMMessage[]> {
    const allMessages: LLMMessage[] = [];
    const batches = this.markMap.boundaryToBatch;

    for (const batchId of batches) {
      if (isBatchInMemory(this.markMap, batchId)) {
        // Batch is in memory, get from current messages
        const batchMessages = this.getBatchMessages(batchId);
        allMessages.push(...batchMessages);
      } else {
        // Batch is in checkpoint, load from storage
        const checkpointMessages = await this.loadBatchFromCheckpoint(batchId, checkpointStorage);
        if (checkpointMessages) {
          allMessages.push(...checkpointMessages);
        }
      }
    }

    return allMessages;
  }

  /**
   * Retrieve invisible messages (messages before the batch boundary)
   * @returns Array of invisible messages
   */
  getInvisibleMessages(): LLMMessage[] {
    return getInvisibleMessages(this.messages, this.markMap);
  }

  /**
   * Get the number of invisible messages
   * @returns The number of invisible messages
   */
  getInvisibleMessageCount(): number {
    return getInvisibleMessageCount(this.markMap);
  }

  /**
   * Check if the message is visible
   * @param originalIndex: The index of the original message
   * @returns: Whether the message is visible or not
   */
  isMessageVisible(originalIndex: number): boolean {
    return isMessageVisible(originalIndex, this.markMap);
  }

  // ============================================================
  // Convenient method - Building messages of a specific type
  // ============================================================

  /**
   * Add system messages
   * @param content Message content
   * @returns Length of the array of added messages
   */
  addSystemMessage(content: string): number {
    return this.addMessage({ role: "system", content });
  }

  /**
   * Add user messages
   * @param content Message content
   * @returns Length of the array of added messages
   */
  addUserMessage(content: string): number {
    return this.addMessage({ role: "user", content });
  }

  /**
   * Add assistant message
   * @param content Message content
   * @param toolCalls Tool calls (optional)
   * @param thinking Thinking content (optional)
   * @returns Length of the array of added messages
   */
  addAssistantMessage(content: string, toolCalls?: LLMToolCall[], thinking?: string): number {
    const message: LLMMessage = {
      role: "assistant",
      content,
    };

    if (toolCalls && toolCalls.length > 0) {
      message.toolCalls = toolCalls;
    }

    if (thinking) {
      message.thinking = thinking;
    }

    return this.addMessage(message);
  }

  /**
   * Add tool result message
   * @param toolCallId: Tool call ID
   * @param content: Result content
   * @returns: Length of the array of added messages
   */
  addToolResultMessage(toolCallId: string, content: string): number {
    return this.addMessage({
      role: "tool",
      toolCallId,
      content,
    });
  }

  // ============================================================
  // Query Method - Visible Messages
  // ============================================================

  /**
   * Retrieve the last N visible messages
   * @param n - Number of messages
   * @returns - Array of messages
   */
  getRecentMessages(n: number): LLMMessage[] {
    const visibleMessages = this.getMessages();
    if (n >= visibleMessages.length) {
      return visibleMessages;
    }
    return visibleMessages.slice(-n);
  }

  /**
   * Filter visible messages by role
   * @param roles An array of roles to retain
   * @returns An array of filtered messages
   */
  filterMessagesByRole(roles: MessageRole[]): LLMMessage[] {
    const visibleMessages = this.getMessages();
    return MessageArrayUtils.filterMessagesByRole(visibleMessages, roles);
  }

  /**
   * Get the last N visible messages for the specified role
   * @param role: The role of the messages
   * @param n: The number of messages
   * @returns: An array of messages
   */
  getRecentMessagesByRole(role: MessageRole, n: number): LLMMessage[] {
    const indices = getVisibleRecentIndicesByRole(this.messages, this.markMap, role, n);
    return indices.map(index => ({ ...this.messages[index]! }));
  }

  /**
   * Get all visible messages for the specified role
   * @param role: The role of the messages
   * @returns: An array of messages
   */
  getMessagesByRole(role: MessageRole): LLMMessage[] {
    const indices = getVisibleIndicesByRole(this.messages, this.markMap, role);
    return indices.map(index => ({ ...this.messages[index]! }));
  }

  /**
   * Get the number of visible messages for the specified role
   * @param role: The role of the messages
   * @returns: The number of messages
   */
  getMessageCountByRole(role: MessageRole): number {
    return getVisibleCountByRole(this.messages, this.markMap, role);
  }

  /**
   * Search for visible messages
   * @param query Search keyword
   * @returns Array of matching messages
   */
  searchMessages(query: string): LLMMessage[] {
    const visibleMessages = this.getMessages();
    return MessageArrayUtils.searchMessages(visibleMessages, query);
  }

  // ============================================================
  // Query method - All messages (including invisible ones)
  // ============================================================

  /**
   * Retrieve all messages for the specified role (including invisible messages)
   * @param role: The role for which the messages are to be retrieved
   * @returns: An array of messages
   */
  getAllMessagesByRole(role: MessageRole): LLMMessage[] {
    const indices = getIndicesByRole(this.messages, role);
    return indices.map(index => ({ ...this.messages[index]! }));
  }

  /**
   * Get the total number of messages for the specified role (including invisible messages)
   * @param role: The message role
   * @returns: The total number of messages
   */
  getTotalMessageCountByRole(role: MessageRole): number {
    return getCountByRole(this.messages, role);
  }

  // ============================================================
  // Operation method
  // ============================================================

  /**
   * Truncate the message array
   * @param options Truncation options
   */
  truncateMessages(options: Parameters<typeof MessageArrayUtils.truncateMessages>[1]): void {
    this.messages = MessageArrayUtils.truncateMessages(this.messages, options);
    // Resynchronize the tag mapping.
    this.syncMarkMapFromMessages();
  }

  /**
   * Insert messages at the specified position
   * @param position The position to insert the messages (-1 indicates the end)
   * @param newMessages The array of messages to be inserted
   */
  insertMessages(position: number, newMessages: LLMMessage[]): void {
    this.messages = MessageArrayUtils.insertMessages(this.messages, position, newMessages);
    // Resynchronize the tag mapping.
    this.syncMarkMapFromMessages();
  }

  /**
   * Replace the message at the specified index
   * @param index The index at which the message should be replaced
   * @param newMessage The new message to be inserted
   */
  replaceMessage(index: number, newMessage: LLMMessage): void {
    this.messages = MessageArrayUtils.replaceMessage(this.messages, index, newMessage);
  }

  /**
   * Clear messages (system messages can be optionally retained)
   * @param keepSystemMessage: Whether to retain system messages or not
   */
  clearMessages(keepSystemMessage: boolean = true): void {
    this.messages = MessageArrayUtils.clearMessages(this.messages, keepSystemMessage);
    // Resynchronize the tag mapping.
    this.syncMarkMapFromMessages();
  }

  /**
   * Deduplicate the message array
   * @param keyFn (optional) A function used to generate unique keys
   */
  deduplicateMessages(keyFn?: (msg: LLMMessage) => string): void {
    this.messages = MessageArrayUtils.deduplicateMessages(this.messages, keyFn);
    // Resynchronize the marker mapping.
    this.syncMarkMapFromMessages();
  }

  // ============================================================
  // Tool Method
  // ============================================================

  /**
   * Initialize the message list
   * @param initialMessages: The initial list of messages
   */
  initializeHistory(initialMessages: LLMMessage[] = []): void {
    this.messages = MessageArrayUtils.cloneMessages(initialMessages);
    this.syncMarkMapFromMessages();
  }

  /**
   * Synchronize the marker mapping with the message array.
   */
  protected syncMarkMapFromMessages(): void {
    this.markMap.originalIndices = this.messages.map((_, index) => index);
  }

  /**
   * Clone the current MessageHistory instance.
   * @returns The cloned MessageHistory instance
   */
  clone(): MessageHistory {
    const cloned = new MessageHistory();
    cloned.messages = MessageArrayUtils.cloneMessages(this.messages);
    cloned.markMap = this.getMarkMap();
    return cloned;
  }

  /**
   * Verify the validity of the message array
   * @returns Verification result
   */
  validate(): { valid: boolean; errors: string[] } {
    return MessageArrayUtils.validateMessageArray(this.messages);
  }

  /**
   * Create a status snapshot
   * @returns A snapshot of the message history status
   */
  createSnapshot(): MessageHistoryState {
    return {
      messages: this.getAllMessages(),
      markMap: this.getMarkMap(),
    };
  }

  /**
   * Restore from snapshot state
   * @param snapshot: A snapshot of the message history state
   */
  restoreFromSnapshot(snapshot: MessageHistoryState): void {
    this.messages = MessageArrayUtils.cloneMessages(snapshot.messages);
    this.markMap = {
      ...snapshot.markMap,
      originalIndices: [...snapshot.markMap.originalIndices],
      batchBoundaries: [...snapshot.markMap.batchBoundaries],
      boundaryToBatch: [...snapshot.markMap.boundaryToBatch],
    };
  }
}
