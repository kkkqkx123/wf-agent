/**
 * 可见范围计算器
 * 负责计算当前批次的消息可见范围，提供索引转换功能
 *
 * 核心概念：
 * - 可见消息：当前批次边界之后的消息，会被发送给LLM
 * - 不可见消息：当前批次边界之前的消息，仅存储但不发送给LLM
 * - 批次边界：通过 startNewBatch() 设置，控制消息可见性
 *
 * 所有函数都是纯函数，不持有任何状态
 */

import type { LLMMessage } from "@wf-agent/types";
import type { MessageMarkMap } from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";

/**
 * Get the boundary index of the current batch
 * @param markMap Message marker mapping
 * @returns Boundary index of the current batch
 * @throws ExecutionError Throws an exception if the current batch does not exist
 */
export function getCurrentBoundary(markMap: MessageMarkMap): number {
  // Verify the validity of the tag mapping.
  if (!markMap || !markMap.batchBoundaries || markMap.batchBoundaries.length === 0) {
    throw new ExecutionError(
      "Invalid MessageMarkMap: batchBoundaries is empty or undefined",
      undefined,
      undefined,
      { markMap },
    );
  }

  // Verify the validity of the current batch index.
  if (markMap.currentBatch < 0 || markMap.currentBatch >= markMap.batchBoundaries.length) {
    throw new ExecutionError(
      `Invalid currentBatch index: ${markMap.currentBatch}. Valid range: 0 to ${markMap.batchBoundaries.length - 1}`,
      undefined,
      undefined,
      {
        currentBatch: markMap.currentBatch,
        batchBoundariesLength: markMap.batchBoundaries.length,
        availableBatches: markMap.batchBoundaries.map((b, i) => ({ batch: i, boundary: b })),
      },
    );
  }

  const boundary = markMap.batchBoundaries[markMap.currentBatch];
  if (boundary === undefined) {
    throw new ExecutionError(
      `Boundary at batch ${markMap.currentBatch} is undefined`,
      undefined,
      undefined,
      {
        currentBatch: markMap.currentBatch,
        batchBoundaries: markMap.batchBoundaries,
      },
    );
  }
  return boundary;
}

/**
 * Get the original indices of visible messages
 * @param markMap: Message marker mapping
 * @returns: Array of original indices of visible messages
 */
export function getVisibleOriginalIndices(markMap: MessageMarkMap): number[] {
  const boundary = getCurrentBoundary(markMap);
  return markMap.originalIndices.filter(index => index >= boundary);
}

/**
 * Convert the visible index to the original index
 * @param visibleIndex: The index of the visible message
 * @param markMap: The message marker mapping
 * @returns: The original index of the message
 * @throws ExecutionError: Throws an exception when the visible index is out of bounds
 */
export function visibleIndexToOriginal(visibleIndex: number, markMap: MessageMarkMap): number {
  const visibleIndices = getVisibleOriginalIndices(markMap);
  if (visibleIndex < 0 || visibleIndex >= visibleIndices.length) {
    throw new ExecutionError(
      `Visible index ${visibleIndex} out of bounds. Visible messages count: ${visibleIndices.length}`,
      undefined,
      undefined,
      {
        visibleIndex,
        visibleMessageCount: visibleIndices.length,
        currentBatch: markMap.currentBatch,
        boundary: markMap.batchBoundaries[markMap.currentBatch],
      },
    );
  }
  const originalIndex = visibleIndices[visibleIndex];
  if (originalIndex === undefined) {
    throw new ExecutionError(
      `Original index at visible position ${visibleIndex} is undefined`,
      undefined,
      undefined,
      {
        visibleIndex,
        visibleIndices,
        markMap,
      },
    );
  }
  return originalIndex;
}

/**
 * Convert the original index to a visible index
 * @param originalIndex: The original message index
 * @param markMap: The message marker mapping
 * @returns: The visible message index; returns null if the message is not visible
 */
export function originalIndexToVisible(
  originalIndex: number,
  markMap: MessageMarkMap,
): number | null {
  const boundary = getCurrentBoundary(markMap);
  if (originalIndex < boundary) {
    return null; // Compressed, invisible
  }

  const visibleIndices = getVisibleOriginalIndices(markMap);
  const visibleIndex = visibleIndices.indexOf(originalIndex);
  return visibleIndex === -1 ? null : visibleIndex;
}

/**
 * Get visible messages
 * @param messages: The complete array of messages
 * @param markMap: The message marker mapping
 * @returns: The array of visible messages
 */
export function getVisibleMessages(messages: LLMMessage[], markMap: MessageMarkMap): LLMMessage[] {
  const visibleIndices = getVisibleOriginalIndices(markMap);
  return visibleIndices
    .map(index => messages[index])
    .filter((msg): msg is LLMMessage => msg !== undefined);
}

/**
 * Get invisible messages (messages before the batch boundary)
 * @param messages: The complete array of messages
 * @param markMap: The message marker mapping
 * @returns: The array of invisible messages
 */
export function getInvisibleMessages(
  messages: LLMMessage[],
  markMap: MessageMarkMap,
): LLMMessage[] {
  const boundary = getCurrentBoundary(markMap);
  const invisibleIndices = markMap.originalIndices.filter(index => index < boundary);
  return invisibleIndices
    .map(index => messages[index])
    .filter((msg): msg is LLMMessage => msg !== undefined);
}

/**
 * Check if the message is visible (after the current batch boundary)
 * @param originalIndex The original message index
 * @param markMap The message marker map
 * @returns Returns true if the message is visible, otherwise returns false
 */
export function isMessageVisible(originalIndex: number, markMap: MessageMarkMap): boolean {
  return originalIndexToVisible(originalIndex, markMap) !== null;
}

/**
 * Get the number of visible messages
 * @param markMap: Message marker mapping
 * @returns: Number of visible messages
 */
export function getVisibleMessageCount(markMap: MessageMarkMap): number {
  return getVisibleOriginalIndices(markMap).length;
}

/**
 * Get the number of invisible messages (the number of messages before the batch boundary)
 * @param markMap Message marker mapping
 * @returns Number of invisible messages
 */
export function getInvisibleMessageCount(markMap: MessageMarkMap): number {
  const boundary = getCurrentBoundary(markMap);
  return markMap.originalIndices.filter(index => index < boundary).length;
}
