/**
 * Unified Message Operation Utility Functions (Batch-Aware)
 * Provides a unified interface for message operations with visibility scope and batch management support
 *
 * Core Concepts:
 * - Visible Messages: Messages after the current batch boundary, which will be sent to the LLM
 * - Invisible Messages: Messages before the current batch boundary, stored but not sent to the LLM
 * - Message Operations: truncate, insert, replace, clear, filter, etc.
 * - Batch Management: Controls message visibility via startNewBatch() and rollbackToBatch()
 *
 * All functions are pure functions and do not hold any state
 */

import type {
  MessageOperationContext,
  MessageOperationConfig,
  MessageOperationResult,
  TruncateMessageOperation,
  InsertMessageOperation,
  ReplaceMessageOperation,
  ClearMessageOperation,
  FilterMessageOperation,
  BatchManagementOperation,
  LLMMessage,
  MessageMarkMap,
} from "@wf-agent/types";
import {
  getCurrentBoundary,
  getVisibleOriginalIndices,
  visibleIndexToOriginal,
} from "./visible-range-calculator.js";
import { startNewBatch, rollbackToBatch } from "./batch-management-utils.js";
import { MessageArrayUtils } from "./message-array-utils.js";

/**
 * Message Operation Callback Function Type
 * Used to execute custom logic after a message operation (such as refreshing the tool's visibility declaration)
 */
export type MessageOperationCallback = (result: MessageOperationResult) => void | Promise<void>;

/**
 * Perform message operation
 * @param context: Message operation context
 * @param operation: Operation configuration
 * @param callback: Callback function after the operation (optional)
 * @returns: Operation result
 */
export async function executeOperation(
  context: MessageOperationContext,
  operation: MessageOperationConfig,
  callback?: MessageOperationCallback,
): Promise<MessageOperationResult> {
  const { messages, markMap, options = {} } = context;
  const visibleOnly = options.visibleOnly ?? true;

  let result: MessageOperationResult;

  switch (operation.operation) {
    case "TRUNCATE":
      result = executeTruncateOperation(
        messages,
        markMap,
        operation as TruncateMessageOperation,
        visibleOnly,
      );
      break;
    case "INSERT":
      result = executeInsertOperation(
        messages,
        markMap,
        operation as InsertMessageOperation,
        visibleOnly,
      );
      break;
    case "REPLACE":
      result = executeReplaceOperation(
        messages,
        markMap,
        operation as ReplaceMessageOperation,
        visibleOnly,
      );
      break;
    case "CLEAR":
      result = executeClearOperation(
        messages,
        markMap,
        operation as ClearMessageOperation,
        visibleOnly,
      );
      break;
    case "FILTER":
      result = executeFilterOperation(
        messages,
        markMap,
        operation as FilterMessageOperation,
        visibleOnly,
      );
      break;
    case "BATCH_MANAGEMENT":
      result = executeBatchManagementOperation(
        messages,
        markMap,
        operation as BatchManagementOperation,
      );
      break;
    default:
      throw new Error(
        `Unsupported operation type: ${(operation as { operation?: string }).operation}`,
      );
  }

  // Callback after the operation is performed (for example, to refresh the tool's visibility settings).
  if (callback) {
    await callback(result);
  }

  return result;
}

/**
 * Perform the truncation operation.
 */
function executeTruncateOperation(
  messages: LLMMessage[],
  markMap: MessageMarkMap,
  operation: TruncateMessageOperation,
  visibleOnly: boolean,
): MessageOperationResult {
  let workingMessages: LLMMessage[];
  let workingMarkMap: MessageMarkMap;

  // If only visible messages are to be manipulated, first retrieve the visible messages.
  if (visibleOnly) {
    const boundary = getCurrentBoundary(markMap);
    const visibleIndices = getVisibleOriginalIndices(markMap);
    const invisibleIndices = markMap.originalIndices.filter(idx => idx < boundary);
    const visibleMessages = visibleIndices
      .map(idx => messages[idx])
      .filter((msg): msg is LLMMessage => msg !== undefined);

    // Perform the truncation operation.
    const truncateOptions = convertStrategyToOptions(operation.strategy);
    const truncatedMessages = MessageArrayUtils.truncateMessages(visibleMessages, {
      ...truncateOptions,
      role: operation.role,
    });

    // Calculate the original indices that need to be retained.
    const keptVisibleIndices = getKeptVisibleIndices(visibleMessages, truncatedMessages);
    const keptVisibleOriginalIndices = keptVisibleIndices
      .map(idx => visibleIndices[idx])
      .filter((idx): idx is number => idx !== undefined);

    // Merge the invisible indexes with the retained visible indexes, maintaining the original order.
    const keptOriginalIndices = markMap.originalIndices.filter(
      idx => invisibleIndices.includes(idx) || keptVisibleOriginalIndices.includes(idx),
    );

    // Update the label mapping to retain only the necessary messages.
    workingMarkMap = updateMarkMapForKeptIndices(markMap, keptOriginalIndices);

    // Reconstruct the complete message array (maintaining the original order)
    workingMessages = rebuildMessagesArray(messages, keptOriginalIndices);
  } else {
    // Manipulate the complete array of messages.
    const truncateOptions = convertStrategyToOptions(operation.strategy);
    workingMessages = MessageArrayUtils.truncateMessages(messages, {
      ...truncateOptions,
      role: operation.role,
    });

    // Reconstruct the token mapping
    workingMarkMap = rebuildMarkMap(workingMessages);
  }

  // If it is necessary to create a new batch
  if (operation.createNewBatch) {
    const boundaryIndex = workingMessages.length;
    workingMarkMap = startNewBatch(workingMarkMap, boundaryIndex);
  }

  return {
    messages: workingMessages,
    markMap: workingMarkMap,
    affectedBatchIndex: workingMarkMap.currentBatch,
    stats: calculateStats(workingMessages, workingMarkMap),
  };
}

/**
 * Perform the insertion operation.
 */
function executeInsertOperation(
  messages: LLMMessage[],
  markMap: MessageMarkMap,
  operation: InsertMessageOperation,
  visibleOnly: boolean,
): MessageOperationResult {
  const workingMessages = [...messages];
  let workingMarkMap = { ...markMap };

  if (visibleOnly && operation.position !== -1) {
    // Translate "Convert visible positions to original positions" to English: "Convert visible positions to original positions"
    const visibleIndices = getVisibleOriginalIndices(markMap);
    if (operation.position >= 0 && operation.position <= visibleIndices.length) {
      let insertAtOriginalIndex: number;
      if (operation.position === visibleIndices.length) {
        insertAtOriginalIndex =
          visibleIndices.length > 0 ? (visibleIndices[visibleIndices.length - 1] ?? 0) + 1 : 0;
      } else {
        const idx = visibleIndices[operation.position];
        if (idx === undefined) {
          throw new Error(`Visible index at position ${operation.position} is undefined`);
        }
        insertAtOriginalIndex = idx;
      }

      // Insert the message in its original position.
      workingMessages.splice(insertAtOriginalIndex, 0, ...operation.messages);

      // Record the affected indices (calculated for potential future use)
      Array.from({ length: operation.messages.length }, (_, i) => insertAtOriginalIndex + i);

      // Update the tag mapping.
      workingMarkMap = updateMarkMapAfterInsert(
        markMap,
        insertAtOriginalIndex,
        operation.messages.length,
      );
    }
  } else {
    // Insert or manipulate the entire array at the end.
    const insertPosition = operation.position === -1 ? workingMessages.length : operation.position;
    workingMessages.splice(insertPosition, 0, ...operation.messages);

    workingMarkMap = updateMarkMapAfterInsert(markMap, insertPosition, operation.messages.length);
  }

  // If it is necessary to create a new batch
  if (operation.createNewBatch) {
    const boundaryIndex = workingMessages.length;
    workingMarkMap = startNewBatch(workingMarkMap, boundaryIndex);
  }

  return {
    messages: workingMessages,
    markMap: workingMarkMap,
    affectedBatchIndex: workingMarkMap.currentBatch,
    stats: calculateStats(workingMessages, workingMarkMap),
  };
}

/**
 * Perform the replacement operation.
 */
function executeReplaceOperation(
  messages: LLMMessage[],
  markMap: MessageMarkMap,
  operation: ReplaceMessageOperation,
  visibleOnly: boolean,
): MessageOperationResult {
  const workingMessages = [...messages];
  let workingMarkMap = { ...markMap };

  if (visibleOnly) {
    // Convert visible indices to original indices.
    const originalIndex = visibleIndexToOriginal(operation.index, markMap);
    workingMessages[originalIndex] = operation.message;
  } else {
    // Direct replacement
    if (operation.index < 0 || operation.index >= workingMessages.length) {
      throw new Error(`Index ${operation.index} is out of bounds`);
    }
    workingMessages[operation.index] = operation.message;
  }

  // If a new batch needs to be created
  if (operation.createNewBatch) {
    const boundaryIndex = workingMessages.length;
    workingMarkMap = startNewBatch(workingMarkMap, boundaryIndex);
  }

  return {
    messages: workingMessages,
    markMap: workingMarkMap,
    affectedBatchIndex: workingMarkMap.currentBatch,
    stats: calculateStats(workingMessages, workingMarkMap),
  };
}

/**
 * Perform the clearing operation.
 */
function executeClearOperation(
  messages: LLMMessage[],
  markMap: MessageMarkMap,
  operation: ClearMessageOperation,
  visibleOnly: boolean,
): MessageOperationResult {
  let workingMessages: LLMMessage[];
  let workingMarkMap: MessageMarkMap;

  // The CLEAR operation completely clears all messages. If you need to retain specific messages, please use the FILTER operation.
  const keepSystemMessage = false;

  if (visibleOnly) {
    // Clear only the visible messages and retain the invisible messages.
    const boundary = getCurrentBoundary(markMap);
    const invisibleIndices = markMap.originalIndices.filter(idx => idx < boundary);

    // Retain the index of invisible messages
    const keptIndices = invisibleIndices;

    // Rebuild the message array (retaining only invisible messages)
    workingMessages = rebuildMessagesArray(messages, keptIndices);
    workingMarkMap = updateMarkMapForKeptIndices(markMap, keptIndices);
  } else {
    // Clear all messages
    workingMessages = MessageArrayUtils.clearMessages(messages, keepSystemMessage);
    workingMarkMap = rebuildMarkMap(workingMessages);
  }

  // If it is necessary to create a new batch
  if (operation.createNewBatch) {
    const boundaryIndex = workingMessages.length;
    workingMarkMap = startNewBatch(workingMarkMap, boundaryIndex);
  }

  return {
    messages: workingMessages,
    markMap: workingMarkMap,
    affectedBatchIndex: workingMarkMap.currentBatch,
    stats: calculateStats(workingMessages, workingMarkMap),
  };
}

/**
 * Perform the filtering operation.
 */
function executeFilterOperation(
  messages: LLMMessage[],
  markMap: MessageMarkMap,
  operation: FilterMessageOperation,
  visibleOnly: boolean,
): MessageOperationResult {
  let workingMessages: LLMMessage[];
  let workingMarkMap: MessageMarkMap;

  if (visibleOnly) {
    // Filter only visible messages and retain invisible messages.
    const boundary = getCurrentBoundary(markMap);
    const visibleIndices = getVisibleOriginalIndices(markMap);
    const invisibleIndices = markMap.originalIndices.filter(idx => idx < boundary);
    const visibleMessages = visibleIndices
      .map(idx => messages[idx])
      .filter((msg): msg is LLMMessage => msg !== undefined);

    let filteredMessages = visibleMessages;

    // Filter by role
    if (operation.roles && operation.roles.length > 0) {
      filteredMessages = MessageArrayUtils.filterMessagesByRole(filteredMessages, operation.roles);
    }

    // Filter by content keywords
    if (operation.contentContains || operation.contentExcludes) {
      filteredMessages = MessageArrayUtils.filterMessagesByContent(filteredMessages, {
        contains: operation.contentContains,
        excludes: operation.contentExcludes,
      });
    }

    // Merge the invisible indexes with the filtered visible indexes.
    const keptVisibleIndices = filteredMessages
      .map(msg => messages.indexOf(msg))
      .filter((idx): idx is number => idx !== -1);
    const keptOriginalIndices = markMap.originalIndices.filter(
      idx => invisibleIndices.includes(idx) || keptVisibleIndices.includes(idx),
    );

    // Reconstruct the message array
    workingMessages = rebuildMessagesArray(messages, keptOriginalIndices);
    workingMarkMap = updateMarkMapForKeptIndices(markMap, keptOriginalIndices);
  } else {
    // Filter all messages
    workingMessages = messages;

    // Filter by role
    if (operation.roles && operation.roles.length > 0) {
      workingMessages = MessageArrayUtils.filterMessagesByRole(workingMessages, operation.roles);
    }

    // Filter by content keywords
    if (operation.contentContains || operation.contentExcludes) {
      workingMessages = MessageArrayUtils.filterMessagesByContent(workingMessages, {
        contains: operation.contentContains,
        excludes: operation.contentExcludes,
      });
    }

    workingMarkMap = rebuildMarkMap(workingMessages);
  }

  // If it is necessary to create a new batch
  if (operation.createNewBatch) {
    const boundaryIndex = workingMessages.length;
    workingMarkMap = startNewBatch(workingMarkMap, boundaryIndex);
  }

  return {
    messages: workingMessages,
    markMap: workingMarkMap,
    affectedBatchIndex: workingMarkMap.currentBatch,
    stats: calculateStats(workingMessages, workingMarkMap),
  };
}

/**
 * Perform batch management operations.
 */
function executeBatchManagementOperation(
  messages: LLMMessage[],
  markMap: MessageMarkMap,
  operation: BatchManagementOperation,
): MessageOperationResult {
  let workingMarkMap = { ...markMap };

  switch (operation.batchOperation) {
    case "START_NEW_BATCH":
      if (operation.boundaryIndex === undefined) {
        throw new Error("boundaryIndex is required for START_NEW_BATCH operation");
      }
      workingMarkMap = startNewBatch(workingMarkMap, operation.boundaryIndex);
      break;

    case "ROLLBACK_TO_BATCH":
      if (operation.targetBatch === undefined) {
        throw new Error("targetBatch is required for ROLLBACK_TO_BATCH operation");
      }
      workingMarkMap = rollbackToBatch(workingMarkMap, operation.targetBatch);
      break;

    default:
      throw new Error(`Unsupported batch operation: ${operation.batchOperation}`);
  }

  return {
    messages,
    markMap: workingMarkMap,
    affectedBatchIndex: workingMarkMap.currentBatch,
    stats: calculateStats(messages, workingMarkMap),
  };
}

/**
 * Get the retained visible index
 */
function getKeptVisibleIndices(
  originalMessages: LLMMessage[],
  keptMessages: LLMMessage[],
): number[] {
  const keptSet = new Set(keptMessages);
  return originalMessages
    .map((msg, idx) => (keptSet.has(msg) ? idx : -1))
    .filter(idx => idx !== -1);
}

/**
 * Update the token mapping to reflect the retained indices.
 */
function updateMarkMapForKeptIndices(
  markMap: MessageMarkMap,
  keptIndices: number[],
): MessageMarkMap {
  const newMarkMap: MessageMarkMap = {
    ...markMap,
    originalIndices: [...keptIndices],
    batchBoundaries: [...markMap.batchBoundaries],
    boundaryToBatch: [...markMap.boundaryToBatch],
  };

  return newMarkMap;
}

/**
 * Update the tag mapping to reflect the insertion operation.
 */
function updateMarkMapAfterInsert(
  markMap: MessageMarkMap,
  insertIndex: number,
  insertCount: number,
): MessageMarkMap {
  const newMarkMap: MessageMarkMap = {
    ...markMap,
    originalIndices: [...markMap.originalIndices],
    batchBoundaries: [...markMap.batchBoundaries],
    boundaryToBatch: [...markMap.boundaryToBatch],
  };

  // Update the original index
  newMarkMap.originalIndices = newMarkMap.originalIndices.map(idx =>
    idx >= insertIndex ? idx + insertCount : idx,
  );
  newMarkMap.originalIndices.push(
    ...Array.from({ length: insertCount }, (_, i) => insertIndex + i),
  );

  // Update batch boundaries
  newMarkMap.batchBoundaries = newMarkMap.batchBoundaries.map(boundary =>
    boundary >= insertIndex ? boundary + insertCount : boundary,
  );

  return newMarkMap;
}

/**
 * Rebuild the message array
 */
function rebuildMessagesArray(originalMessages: LLMMessage[], keptIndices: number[]): LLMMessage[] {
  return keptIndices
    .map(idx => originalMessages[idx])
    .filter((msg): msg is LLMMessage => msg !== undefined);
}

/**
 * Rebuild the token mapping
 */
function rebuildMarkMap(messages: LLMMessage[]): MessageMarkMap {
  const originalIndices = messages.map((_, idx) => idx);

  return {
    originalIndices,
    batchBoundaries: [0],
    boundaryToBatch: [0],
    currentBatch: 0,
  };
}

/**
 * Calculate operation statistics.
 */
function calculateStats(
  messages: LLMMessage[],
  markMap: MessageMarkMap,
): {
  originalMessageCount: number;
  visibleMessageCount: number;
  invisibleMessageCount: number;
} {
  const totalMessages = messages.length;
  const visibleMessageCount = getVisibleOriginalIndices(markMap).length;
  const invisibleMessageCount = totalMessages - visibleMessageCount;

  return {
    originalMessageCount: totalMessages,
    visibleMessageCount,
    invisibleMessageCount,
  };
}

/**
 * Convert the truncation strategy into options.
 */
function convertStrategyToOptions(
  strategy: TruncateMessageOperation["strategy"],
): Record<string, number | { start: number; end: number }> {
  switch (strategy.type) {
    case "KEEP_FIRST":
      return { keepFirst: strategy.count };
    case "KEEP_LAST":
      return { keepLast: strategy.count };
    case "REMOVE_FIRST":
      return { removeFirst: strategy.count };
    case "REMOVE_LAST":
      return { removeLast: strategy.count };
    case "RANGE":
      return { range: { start: strategy.start, end: strategy.end } };
    default:
      throw new Error(`Unsupported strategy type: ${(strategy as { type?: string }).type}`);
  }
}
