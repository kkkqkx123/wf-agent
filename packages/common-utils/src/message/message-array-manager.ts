/**
 * Message Array Manager
 * Implement the core logic for managing message arrays, supporting batch processing and rollback features.
 */

import { now } from "../utils/timestamp-utils.js";
import type {
  Message,
  MessageArrayState,
  MessageOperationConfig,
  MessageOperationResult,
  MessageArrayStats,
  BatchSnapshot,
  AppendMessageOperation,
  InsertMessageOperation,
  ReplaceMessageOperation,
  TruncateMessageOperation,
  ClearMessageOperation,
  FilterMessageOperation,
  RollbackMessageOperation,
  MessageMarkMap,
} from "@wf-agent/types";

/**
 * Message Array Manager Class
 * Supports a variety of message operations and batch management
 */
export class MessageArrayManager {
  private state: MessageArrayState;

  constructor(initialMessages: Message[] = []) {
    this.state = {
      messages: initialMessages,
      batchSnapshots: [],
      currentBatchIndex: 0,
      totalMessageCount: initialMessages.length,
    };
  }

  /**
   * Execute message operation
   * @param operation: Operation configuration
   * @returns: Operation result
   */
  execute(operation: MessageOperationConfig): MessageOperationResult {
    switch (operation.operation) {
      case "APPEND":
        return this.executeAppend(operation as AppendMessageOperation);
      case "INSERT":
        return this.executeInsert(operation as InsertMessageOperation);
      case "REPLACE":
        return this.executeReplace(operation as ReplaceMessageOperation);
      case "TRUNCATE":
        return this.executeTruncate(operation as TruncateMessageOperation);
      case "CLEAR":
        return this.executeClear(operation as ClearMessageOperation);
      case "FILTER":
        return this.executeFilter(operation as FilterMessageOperation);
      case "ROLLBACK":
        return this.executeRollback(operation as RollbackMessageOperation);
      default:
        throw new Error(
          `Unsupported operation type: ${(operation as unknown as { operation: string }).operation}`,
        );
    }
  }

  /**
   * Get the current status of the message array
   * @returns The status of the message array
   */
  getState(): MessageArrayState {
    return { ...this.state };
  }

  /**
   * Get the messages of the current batch
   * @returns Array of messages for the current batch
   */
  getCurrentMessages(): Message[] {
    return [...this.state.messages];
  }

  /**
   * Get statistical information
   * @returns Statistical information
   */
  getStats(): MessageArrayStats {
    return {
      totalMessages: this.state.messages.length,
      currentBatchMessages: this.state.messages.length,
      totalBatches: this.state.currentBatchIndex + 1,
      currentBatchIndex: this.state.currentBatchIndex,
    };
  }

  /**
   * Roll back to a specified batch
   * @param batchIndex: Batch index
   * @returns: Operation result
   */
  rollback(batchIndex: number): MessageOperationResult {
    const operation: RollbackMessageOperation = {
      operation: "ROLLBACK",
      targetBatchIndex: batchIndex,
    };
    return this.executeRollback(operation);
  }

  /**
   * Get batch snapshot
   * @param batchIndex: Batch index
   * @returns: Batch snapshot
   */
  getBatchSnapshot(batchIndex: number): BatchSnapshot | null {
    return this.state.batchSnapshots[batchIndex] || null;
  }

  /**
   * Perform the APPEND operation (low overhead, does not create a new batch).
   */
  private executeAppend(operation: AppendMessageOperation): MessageOperationResult {
    // Append messages directly to the current batch.
    const newMessages = [...this.state.messages, ...operation.messages];

    // Do not create new batches, do not create snapshots.
    const newState: MessageArrayState = {
      messages: newMessages,
      batchSnapshots: this.state.batchSnapshots,
      currentBatchIndex: this.state.currentBatchIndex,
      totalMessageCount: newMessages.length,
    };

    this.state = newState;

    return {
      messages: newMessages,
      markMap: this.createMarkMap(newMessages),
      affectedBatchIndex: this.state.currentBatchIndex,
      stats: this.calculateStats(newState),
    };
  }

  /**
   * Perform the INSERT operation (which is resource-intensive and creates a new batch).
   */
  private executeInsert(operation: InsertMessageOperation): MessageOperationResult {
    // Verify the insertion position
    if (operation.position < 0 || operation.position > this.state.messages.length) {
      throw new Error(
        `Invalid insert position: ${operation.position}. Must be between 0 and ${this.state.messages.length}`,
      );
    }

    // Create a snapshot (deep copy) of the current batch.
    const snapshot: BatchSnapshot = {
      batchIndex: this.state.currentBatchIndex,
      timestamp: now(),
      messages: JSON.parse(JSON.stringify(this.state.messages)), // Deep Copy
      messageCount: this.state.messages.length,
      description: `Before INSERT at position ${operation.position}`,
    };

    // Perform the insertion operation
    const newMessages = [...this.state.messages];
    newMessages.splice(operation.position, 0, ...operation.messages);

    // Create a new batch
    const newBatchIndex = this.state.currentBatchIndex + 1;
    const newState: MessageArrayState = {
      messages: newMessages,
      batchSnapshots: [...this.state.batchSnapshots, snapshot],
      currentBatchIndex: newBatchIndex,
      totalMessageCount: newMessages.length,
    };

    this.state = newState;

    return {
      messages: newMessages,
      markMap: this.createMarkMap(newMessages),
      affectedBatchIndex: newBatchIndex,
      stats: this.calculateStats(newState),
    };
  }

  /**
   * Perform the REPLACE operation (which is resource-intensive and creates a new batch).
   */
  private executeReplace(operation: ReplaceMessageOperation): MessageOperationResult {
    // Verify replacement index
    if (operation.index < 0 || operation.index >= this.state.messages.length) {
      throw new Error(
        `Invalid replace index: ${operation.index}. Must be between 0 and ${this.state.messages.length - 1}`,
      );
    }

    // Create a snapshot (deep copy) of the current batch.
    const snapshot: BatchSnapshot = {
      batchIndex: this.state.currentBatchIndex,
      timestamp: now(),
      messages: JSON.parse(JSON.stringify(this.state.messages)), // Deep Copy
      messageCount: this.state.messages.length,
      description: `Before REPLACE at index ${operation.index}`,
    };

    // Perform the replacement operation.
    const newMessages = [...this.state.messages];
    newMessages[operation.index] = operation.message;

    // Create a new batch
    const newBatchIndex = this.state.currentBatchIndex + 1;
    const newState: MessageArrayState = {
      messages: newMessages,
      batchSnapshots: [...this.state.batchSnapshots, snapshot],
      currentBatchIndex: newBatchIndex,
      totalMessageCount: newMessages.length,
    };

    this.state = newState;

    return {
      messages: newMessages,
      markMap: this.createMarkMap(newMessages),
      affectedBatchIndex: newBatchIndex,
      stats: this.calculateStats(newState),
    };
  }

  /**
   * Perform the TRUNCATE operation (high overhead, creates a new batch)
   */
  private executeTruncate(operation: TruncateMessageOperation): MessageOperationResult {
    // Create a snapshot (deep copy) of the current batch.
    const snapshot: BatchSnapshot = {
      batchIndex: this.state.currentBatchIndex,
      timestamp: now(),
      messages: JSON.parse(JSON.stringify(this.state.messages)), // Deep copy
      messageCount: this.state.messages.length,
      description: "Before TRUNCATE",
    };

    // Perform the truncation operation.
    let newMessages = [...this.state.messages];

    // First, filter by role (if a role is specified).
    if (operation.role) {
      newMessages = newMessages.filter(msg => msg.role === operation.role);
    }

    // Apply truncation strategy
    const strategy = operation.strategy;
    switch (strategy.type) {
      case "KEEP_FIRST":
        newMessages = newMessages.slice(0, strategy.count);
        break;
      case "KEEP_LAST":
        newMessages = newMessages.slice(-strategy.count);
        break;
      case "REMOVE_FIRST":
        newMessages = newMessages.slice(strategy.count);
        break;
      case "REMOVE_LAST":
        newMessages = newMessages.slice(0, -strategy.count);
        break;
      case "RANGE":
        newMessages = newMessages.slice(strategy.start, strategy.end);
        break;
      default:
        throw new Error(`Unknown strategy type: ${(strategy as { type: string }).type}`);
    }

    // Create a new batch
    const newBatchIndex = this.state.currentBatchIndex + 1;
    const newState: MessageArrayState = {
      messages: newMessages,
      batchSnapshots: [...this.state.batchSnapshots, snapshot],
      currentBatchIndex: newBatchIndex,
      totalMessageCount: newMessages.length,
    };

    this.state = newState;

    return {
      messages: newMessages,
      markMap: this.createMarkMap(newMessages),
      affectedBatchIndex: newBatchIndex,
      stats: this.calculateStats(newState),
    };
  }

  /**
   * Perform the CLEAR operation (low overhead, creates a new batch, the snapshot is empty).
   */
  private executeClear(_operation: ClearMessageOperation): MessageOperationResult {
    // Create an empty snapshot (with no additional copying overhead).
    const snapshot: BatchSnapshot = {
      batchIndex: this.state.currentBatchIndex,
      timestamp: now(),
      messages: [], // Empty array, no copying overhead
      messageCount: 0,
      description: "Before CLEAR",
    };

    // Perform a clear operation (clear everything completely; if you need to retain specific messages, use the FILTER operation).
    const newMessages: Message[] = [];

    // Create a new batch
    const newBatchIndex = this.state.currentBatchIndex + 1;
    const newState: MessageArrayState = {
      messages: newMessages,
      batchSnapshots: [...this.state.batchSnapshots, snapshot],
      currentBatchIndex: newBatchIndex,
      totalMessageCount: newMessages.length,
    };

    this.state = newState;

    return {
      messages: newMessages,
      markMap: this.createMarkMap(newMessages),
      affectedBatchIndex: newBatchIndex,
      stats: this.calculateStats(newState),
    };
  }

  /**
   * Perform the FILTER operation (which is resource-intensive and creates a new batch).
   */
  private executeFilter(operation: FilterMessageOperation): MessageOperationResult {
    // Create a snapshot (deep copy) of the current batch.
    const snapshot: BatchSnapshot = {
      batchIndex: this.state.currentBatchIndex,
      timestamp: now(),
      messages: JSON.parse(JSON.stringify(this.state.messages)), // Deep Copy
      messageCount: this.state.messages.length,
      description: "Before FILTER",
    };

    // Perform the filtering operation
    let newMessages = [...this.state.messages];

    // Filter by role
    if (operation.roles && operation.roles.length > 0) {
      newMessages = newMessages.filter(msg => operation.roles!.includes(msg.role));
    }

    // Filter by content keywords (including)
    if (operation.contentContains && operation.contentContains.length > 0) {
      newMessages = newMessages.filter(msg => {
        const content = this.extractTextContent(msg.content);
        return operation.contentContains!.some(keyword => content.includes(keyword));
      });
    }

    // Exclude by content keywords (do not include)
    if (operation.contentExcludes && operation.contentExcludes.length > 0) {
      newMessages = newMessages.filter(msg => {
        const content = this.extractTextContent(msg.content);
        return !operation.contentExcludes!.some(keyword => content.includes(keyword));
      });
    }

    // Create a new batch
    const newBatchIndex = this.state.currentBatchIndex + 1;
    const newState: MessageArrayState = {
      messages: newMessages,
      batchSnapshots: [...this.state.batchSnapshots, snapshot],
      currentBatchIndex: newBatchIndex,
      totalMessageCount: newMessages.length,
    };

    this.state = newState;

    return {
      messages: newMessages,
      markMap: this.createMarkMap(newMessages),
      affectedBatchIndex: newBatchIndex,
      stats: this.calculateStats(newState),
    };
  }

  /**
   * Perform the ROLLBACK operation (without creating a new batch).
   */
  private executeRollback(operation: RollbackMessageOperation): MessageOperationResult {
    // Verify batch index
    if (
      operation.targetBatchIndex < 0 ||
      operation.targetBatchIndex > this.state.currentBatchIndex
    ) {
      throw new Error(
        `Invalid batch index: ${operation.targetBatchIndex}. Must be between 0 and ${this.state.currentBatchIndex}`,
      );
    }

    // Get the target snapshot
    const targetSnapshot = this.state.batchSnapshots[operation.targetBatchIndex];

    if (!targetSnapshot) {
      // Revert to batch 0 (initial state).
      const newState: MessageArrayState = {
        messages: [],
        batchSnapshots: [],
        currentBatchIndex: 0,
        totalMessageCount: 0,
      };

      this.state = newState;

      return {
        messages: [],
        markMap: this.createMarkMap([]),
        affectedBatchIndex: 0,
        stats: this.calculateStats(newState),
      };
    }

    // Restore to the target batch status.
    const newState: MessageArrayState = {
      messages: JSON.parse(JSON.stringify(targetSnapshot.messages)), // Deep copy restoration
      batchSnapshots: this.state.batchSnapshots.slice(0, operation.targetBatchIndex),
      currentBatchIndex: operation.targetBatchIndex,
      totalMessageCount: targetSnapshot.messageCount,
    };

    this.state = newState;

    return {
      messages: newState.messages,
      markMap: this.createMarkMap(newState.messages),
      affectedBatchIndex: operation.targetBatchIndex,
      stats: this.calculateStats(newState),
    };
  }

  /**
   * Calculate statistical information
   */
  private calculateStats(state: MessageArrayState): {
    originalMessageCount: number;
    visibleMessageCount: number;
    invisibleMessageCount: number;
  } {
    return {
      originalMessageCount: state.totalMessageCount,
      visibleMessageCount: state.messages.length,
      invisibleMessageCount: state.totalMessageCount - state.messages.length,
    };
  }

  /**
   * Creating Message Tag Mappings
   */
  private createMarkMap(messages: Message[]): MessageMarkMap {
    return {
      originalIndices: messages.map((_, index) => index),
      batchBoundaries: [0],
      boundaryToBatch: [0],
      currentBatch: this.state.currentBatchIndex,
    };
  }

  /**
   * Extracting the text content of a message
   */
  private extractTextContent(content: Message["content"]): string {
    if (typeof content === "string") {
      return content;
    }

    // Handling the contents of an array type
    return content
      .map((item: { type: string; text?: string }) => {
        if (item.type === "text") {
          return item.text || "";
        }
        return "";
      })
      .join(" ");
  }
}
