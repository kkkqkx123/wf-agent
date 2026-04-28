/**
 * Message Indexing Tool Functions
 * Provide type index querying and calculation capabilities
 *
 * Design Notes:
 * - Pure functions with no side effects
 */

import type { LLMMessage } from "@wf-agent/types";
import type { MessageMarkMap } from "@wf-agent/types";
import { MessageRole } from "@wf-agent/types";

/**
 * Get the message index for the specified role
 * @param messages Array of messages
 * @param role Role of the messages
 * @returns Array of message indices for the specified role
 */
export function getIndicesByRole(messages: LLMMessage[], role: MessageRole): number[] {
  return messages
    .map((msg, index) => (msg.role === role ? index : -1))
    .filter(index => index !== -1);
}

/**
 * Get the indices of the last N messages for the specified role
 * @param messages Array of messages
 * @param role Message role
 * @param n Number of messages
 * @returns Array of indices for the last N messages of the specified role
 */
export function getRecentIndicesByRole(
  messages: LLMMessage[],
  role: MessageRole,
  n: number,
): number[] {
  const indices = getIndicesByRole(messages, role);
  return indices.slice(-n);
}

/**
 * Get the index range for the specified role
 * @param messages Array of messages
 * @param role Role of the messages
 * @param start Starting position (in the array of types)
 * @param end Ending position (in the array of types)
 * @returns Index range for the specified role
 */
export function getRangeIndicesByRole(
  messages: LLMMessage[],
  role: MessageRole,
  start: number,
  end: number,
): number[] {
  const indices = getIndicesByRole(messages, role);
  return indices.slice(start, end);
}

/**
 * Get the number of messages for a specified role
 * @param messages Array of messages
 * @param role The role of the messages
 * @returns The number of messages for the specified role
 */
export function getCountByRole(messages: LLMMessage[], role: MessageRole): number {
  return messages.filter(msg => msg.role === role).length;
}

/**
 * Get the index of the specified role in the visible messages
 * @param messages Array of messages
 * @param markMap Map of message markers
 * @param role Role of the messages
 * @returns Array of indices of the specified role in the visible messages
 */
export function getVisibleIndicesByRole(
  messages: LLMMessage[],
  markMap: MessageMarkMap,
  role: MessageRole,
): number[] {
  const boundary = markMap.batchBoundaries[markMap.currentBatch];
  if (boundary === undefined) {
    return [];
  }
  return messages
    .map((msg, index) => {
      if (msg.role === role && index >= boundary) {
        return index;
      }
      return -1;
    })
    .filter(index => index !== -1);
}

/**
 * Get the indices of the last N messages of the specified role in the visible messages
 * @param messages Array of messages
 * @param markMap Message marker mapping
 * @param role Message role
 * @param n Number of messages
 * @returns Array of indices for the last N messages of the specified role in the visible messages
 */
export function getVisibleRecentIndicesByRole(
  messages: LLMMessage[],
  markMap: MessageMarkMap,
  role: MessageRole,
  n: number,
): number[] {
  const indices = getVisibleIndicesByRole(messages, markMap, role);
  return indices.slice(-n);
}

/**
 * Get the index range of the specified role in the visible messages
 * @param messages Array of messages
 * @param markMap Map of message markers
 * @param role Role of the messages
 * @param start Start position (in the type array)
 * @param end End position (in the type array)
 * @returns Index range of the specified role in the visible messages
 */
export function getVisibleRangeIndicesByRole(
  messages: LLMMessage[],
  markMap: MessageMarkMap,
  role: MessageRole,
  start: number,
  end: number,
): number[] {
  const indices = getVisibleIndicesByRole(messages, markMap, role);
  return indices.slice(start, end);
}

/**
 * Get the number of messages for the specified role in the visible messages
 * @param messages Array of messages
 * @param markMap Message marker mapping
 * @param role Role of the messages
 * @returns Number of messages for the specified role in the visible messages
 */
export function getVisibleCountByRole(
  messages: LLMMessage[],
  markMap: MessageMarkMap,
  role: MessageRole,
): number {
  const boundary = markMap.batchBoundaries[markMap.currentBatch];
  if (boundary === undefined) {
    return 0;
  }
  return messages.filter(msg => msg.role === role && messages.indexOf(msg) >= boundary).length;
}
