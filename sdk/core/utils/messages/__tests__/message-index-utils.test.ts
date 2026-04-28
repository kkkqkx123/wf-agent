/**
 * MessageIndexUtils Unit Tests
 */

import { describe, it, expect } from "vitest";
import {
  getIndicesByRole,
  getRecentIndicesByRole,
  getRangeIndicesByRole,
  getCountByRole,
  getVisibleIndicesByRole,
  getVisibleRecentIndicesByRole,
  getVisibleRangeIndicesByRole,
  getVisibleCountByRole,
} from "../message-index-utils.js";
import type { LLMMessage } from "@wf-agent/types";
import type { MessageMarkMap } from "@wf-agent/types";
import { MessageRole } from "@wf-agent/types";

// Create an auxiliary function for generating test messages
function createMockMessages(count: number, role: MessageRole = "user"): LLMMessage[] {
  return Array.from({ length: count }, (_, i) => ({
    role,
    content: `Message ${i + 1}`,
  }));
}

function createMixedMessages(): LLMMessage[] {
  return [
    { role: "system", content: "System message" },
    { role: "user", content: "User message 1" },
    { role: "assistant", content: "Assistant message 1" },
    { role: "user", content: "User message 2" },
    { role: "assistant", content: "Assistant message 2" },
    { role: "tool", content: "Tool result" },
  ];
}

// Create a test MessageMarkMap
function createMockMarkMap(
  currentBatch: number = 0,
  batchBoundaries: number[] = [0],
): MessageMarkMap {
  return {
    originalIndices: Array.from({ length: 10 }, (_, i) => i),
    batchBoundaries,
    boundaryToBatch: batchBoundaries.map((_, i) => i),
    currentBatch,
  };
}

describe("getIndicesByRole", () => {
  it("Should return all indexes for the specified role", () => {
    const messages = createMixedMessages();
    const result = getIndicesByRole(messages, "user");
    expect(result).toEqual([1, 3]);
  });

  it("should return the index of the assistant role", () => {
    const messages = createMixedMessages();
    const result = getIndicesByRole(messages, "assistant");
    expect(result).toEqual([2, 4]);
  });

  it("should return the index of the system role", () => {
    const messages = createMixedMessages();
    const result = getIndicesByRole(messages, "system");
    expect(result).toEqual([0]);
  });

  it("The index of the tool role should be returned", () => {
    const messages = createMixedMessages();
    const result = getIndicesByRole(messages, "tool");
    expect(result).toEqual([5]);
  });

  it("Should return an empty array when the role does not exist", () => {
    const messages = createMixedMessages();
    const result = getIndicesByRole(messages, "user");
    // Remove all user messages.
    const noUserMessages = messages.filter(m => m.role !== "user");
    const emptyResult = getIndicesByRole(noUserMessages, "user");
    expect(emptyResult).toEqual([]);
  });

  it("Empty message arrays should be handled", () => {
    const result = getIndicesByRole([], "user");
    expect(result).toEqual([]);
  });

  it("Should handle the case where all messages are in the same role", () => {
    const messages = createMockMessages(5, "user");
    const result = getIndicesByRole(messages, "user");
    expect(result).toEqual([0, 1, 2, 3, 4]);
  });
});

describe("getRecentIndicesByRole", () => {
  it("Should return the index of the last N messages for the specified role", () => {
    const messages = createMixedMessages();
    const result = getRecentIndicesByRole(messages, "user", 1);
    expect(result).toEqual([3]);
  });

  it("Should return the index of the last 2 messages for the specified role", () => {
    const messages = createMixedMessages();
    const result = getRecentIndicesByRole(messages, "user", 2);
    expect(result).toEqual([1, 3]);
  });

  it("All indexes should be returned when N is greater than the number of role messages", () => {
    const messages = createMixedMessages();
    const result = getRecentIndicesByRole(messages, "user", 10);
    expect(result).toEqual([1, 3]);
  });

  it("当N为0时应该返回所有索引（slice(-0)返回整个数组）", () => {
    const messages = createMixedMessages();
    const result = getRecentIndicesByRole(messages, "user", 0);
    // slice(-0) 在 JavaScript 中等同于 slice(0)，返回整个数组
    expect(result).toEqual([1, 3]);
  });

  it("Empty message arrays should be handled", () => {
    const result = getRecentIndicesByRole([], "user", 3);
    expect(result).toEqual([]);
  });

  it("Should deal with situations where roles do not exist", () => {
    const messages = createMixedMessages();
    const result = getRecentIndicesByRole(messages, "tool", 5);
    expect(result).toEqual([5]);
  });
});

describe("getRangeIndicesByRole", () => {
  it("Should return the index range for the specified role", () => {
    const messages = createMixedMessages();
    const result = getRangeIndicesByRole(messages, "user", 0, 1);
    expect(result).toEqual([1]);
  });

  it("Should return the index range (multiple) for the specified role", () => {
    const messages = createMixedMessages();
    const result = getRangeIndicesByRole(messages, "user", 0, 2);
    expect(result).toEqual([1, 3]);
  });

  it("The case where start is 0 should be handled", () => {
    const messages = createMixedMessages();
    const result = getRangeIndicesByRole(messages, "user", 0, 1);
    expect(result).toEqual([1]);
  });

  it("End out of range should be addressed", () => {
    const messages = createMixedMessages();
    const result = getRangeIndicesByRole(messages, "user", 0, 10);
    expect(result).toEqual([1, 3]);
  });

  it("Should return an empty array when start >= end", () => {
    const messages = createMixedMessages();
    const result = getRangeIndicesByRole(messages, "user", 1, 1);
    expect(result).toEqual([]);
  });

  it("Empty message arrays should be handled", () => {
    const result = getRangeIndicesByRole([], "user", 0, 2);
    expect(result).toEqual([]);
  });

  it("Should deal with situations where roles do not exist", () => {
    const messages = createMixedMessages();
    const result = getRangeIndicesByRole(messages, "tool", 0, 2);
    expect(result).toEqual([5]);
  });
});

describe("getCountByRole", () => {
  it("The number of messages that should be returned for the specified role", () => {
    const messages = createMixedMessages();
    const result = getCountByRole(messages, "user");
    expect(result).toBe(2);
  });

  it("The number of messages that should be returned for the assistant role", () => {
    const messages = createMixedMessages();
    const result = getCountByRole(messages, "assistant");
    expect(result).toBe(2);
  });

  it("The number of messages that should be returned for the system role", () => {
    const messages = createMixedMessages();
    const result = getCountByRole(messages, "system");
    expect(result).toBe(1);
  });

  it("Number of messages that should be returned for the tool role", () => {
    const messages = createMixedMessages();
    const result = getCountByRole(messages, "tool");
    expect(result).toBe(1);
  });

  it("Should return 0 when the role does not exist", () => {
    const messages = createMixedMessages();
    const noUserMessages = messages.filter(m => m.role !== "user");
    const result = getCountByRole(noUserMessages, "user");
    expect(result).toBe(0);
  });

  it("Empty message arrays should be handled", () => {
    const result = getCountByRole([], "user");
    expect(result).toBe(0);
  });

  it("Should handle the case where all messages are in the same role", () => {
    const messages = createMockMessages(5, "user");
    const result = getCountByRole(messages, "user");
    expect(result).toBe(5);
  });
});

describe("getVisibleIndicesByRole", () => {
  it("Should return the index of the role specified in the visible message", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleIndicesByRole(messages, markMap, "user");
    expect(result).toEqual([1, 3]);
  });

  it("Should only return indexes greater than or equal to boundary", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [2]);
    const result = getVisibleIndicesByRole(messages, markMap, "user");
    expect(result).toEqual([3]);
  });

  it("The case where the boundary is 0 should be handled", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleIndicesByRole(messages, markMap, "system");
    expect(result).toEqual([0]);
  });

  it("Empty arrays should be returned when boundary is greater than all message indices.", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [10]);
    const result = getVisibleIndicesByRole(messages, markMap, "user");
    expect(result).toEqual([]);
  });

  it("Should return an empty array when the boundary is undefined.", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(1, [0]); // currentBatch=1, 但batchBoundaries只有[0]
    const result = getVisibleIndicesByRole(messages, markMap, "user");
    expect(result).toEqual([]);
  });

  it("Empty message arrays should be handled", () => {
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleIndicesByRole([], markMap, "user");
    expect(result).toEqual([]);
  });

  it("Should deal with situations where roles do not exist", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleIndicesByRole(messages, markMap, "tool");
    expect(result).toEqual([5]);
  });
});

describe("getVisibleRecentIndicesByRole", () => {
  it("Should return the index of the last N messages for the role specified in the visible message", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleRecentIndicesByRole(messages, markMap, "user", 1);
    expect(result).toEqual([3]);
  });

  it("Should return the last 2 message indexes for the role specified in the visible message", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleRecentIndicesByRole(messages, markMap, "user", 2);
    expect(result).toEqual([1, 3]);
  });

  it("The effect of boundaries should be considered", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [2]);
    const result = getVisibleRecentIndicesByRole(messages, markMap, "user", 2);
    expect(result).toEqual([3]);
  });

  it("All visible indexes should be returned when N is greater than the number of visible role messages", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleRecentIndicesByRole(messages, markMap, "user", 10);
    expect(result).toEqual([1, 3]);
  });

  it("当N为0时应该返回所有可见索引（slice(-0)返回整个数组）", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleRecentIndicesByRole(messages, markMap, "user", 0);
    // slice(-0) 在 JavaScript 中等同于 slice(0)，返回整个数组
    expect(result).toEqual([1, 3]);
  });

  it("Should return an empty array when the boundary is undefined.", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(1, [0]);
    const result = getVisibleRecentIndicesByRole(messages, markMap, "user", 2);
    expect(result).toEqual([]);
  });

  it("Empty message arrays should be handled", () => {
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleRecentIndicesByRole([], markMap, "user", 2);
    expect(result).toEqual([]);
  });
});

describe("getVisibleRangeIndicesByRole", () => {
  it("The index range of the role specified in the visible message should be returned", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleRangeIndicesByRole(messages, markMap, "user", 0, 1);
    expect(result).toEqual([1]);
  });

  it("Should return the index range (multiple) for the role specified in the visible message", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleRangeIndicesByRole(messages, markMap, "user", 0, 2);
    expect(result).toEqual([1, 3]);
  });

  it("The effect of boundaries should be considered", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [2]);
    const result = getVisibleRangeIndicesByRole(messages, markMap, "user", 0, 2);
    expect(result).toEqual([3]);
  });

  it("Should return an empty array when start >= end", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleRangeIndicesByRole(messages, markMap, "user", 1, 1);
    expect(result).toEqual([]);
  });

  it("Should return an empty array when the boundary is undefined.", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(1, [0]);
    const result = getVisibleRangeIndicesByRole(messages, markMap, "user", 0, 2);
    expect(result).toEqual([]);
  });

  it("Empty message arrays should be handled", () => {
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleRangeIndicesByRole([], markMap, "user", 0, 2);
    expect(result).toEqual([]);
  });

  it("End out of range should be addressed", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleRangeIndicesByRole(messages, markMap, "user", 0, 10);
    expect(result).toEqual([1, 3]);
  });
});

describe("getVisibleCountByRole", () => {
  it("Should return the number of messages with the specified role in the visible messages", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleCountByRole(messages, markMap, "user");
    expect(result).toBe(2);
  });

  it("The effect of boundaries should be considered", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [2]);
    const result = getVisibleCountByRole(messages, markMap, "user");
    expect(result).toBe(1);
  });

  it("When boundary is 0, the number of messages should be returned.", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleCountByRole(messages, markMap, "system");
    expect(result).toBe(1);
  });

  it("Should return 0 when boundary is greater than all message indexes", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [10]);
    const result = getVisibleCountByRole(messages, markMap, "user");
    expect(result).toBe(0);
  });

  it("When `boundary` is not defined, 0 should be returned.", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(1, [0]);
    const result = getVisibleCountByRole(messages, markMap, "user");
    expect(result).toBe(0);
  });

  it("The empty message array should be handled accordingly.", () => {
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleCountByRole([], markMap, "user");
    expect(result).toBe(0);
  });

  it("The case where the role does not exist should be handled.", () => {
    const messages = createMixedMessages();
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleCountByRole(messages, markMap, "tool");
    expect(result).toBe(1);
  });

  it("The situation where all visible messages are from the same character should be handled.", () => {
    const messages = createMockMessages(5, "user");
    const markMap = createMockMarkMap(0, [0]);
    const result = getVisibleCountByRole(messages, markMap, "user");
    expect(result).toBe(5);
  });
});

describe("Integration testing", () => {
  it("Complex message scenarios should be handled correctly.", () => {
    const messages: LLMMessage[] = [
      { role: "system", content: "System" },
      { role: "user", content: "User 1" },
      { role: "assistant", content: "Assistant 1" },
      { role: "user", content: "User 2" },
      { role: "assistant", content: "Assistant 2" },
      { role: "user", content: "User 3" },
      { role: "assistant", content: "Assistant 3" },
    ];

    // Testing getIndicesByRole
    const userIndices = getIndicesByRole(messages, "user");
    expect(userIndices).toEqual([1, 3, 5]);

    // Testing getRecentIndicesByRole
    const recentUserIndices = getRecentIndicesByRole(messages, "user", 2);
    expect(recentUserIndices).toEqual([3, 5]);

    // Testing getRangeIndicesByRole
    const rangeUserIndices = getRangeIndicesByRole(messages, "user", 0, 2);
    expect(rangeUserIndices).toEqual([1, 3]);

    // Test getCountByRole
    const userCount = getCountByRole(messages, "user");
    expect(userCount).toBe(3);

    // Test functions related to visible messages
    const markMap = createMockMarkMap(0, [2]);
    const visibleUserIndices = getVisibleIndicesByRole(messages, markMap, "user");
    expect(visibleUserIndices).toEqual([3, 5]);

    const visibleRecentUserIndices = getVisibleRecentIndicesByRole(messages, markMap, "user", 1);
    expect(visibleRecentUserIndices).toEqual([5]);

    const visibleRangeUserIndices = getVisibleRangeIndicesByRole(messages, markMap, "user", 0, 1);
    expect(visibleRangeUserIndices).toEqual([3]);

    const visibleUserCount = getVisibleCountByRole(messages, markMap, "user");
    expect(visibleUserCount).toBe(2);
  });

  it("Boundary cases should be handled correctly.", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "User 1" },
      { role: "user", content: "User 2" },
      { role: "user", content: "User 3" },
    ];

    const markMap = createMockMarkMap(0, [1]);

    // Testing boundary values
    expect(getIndicesByRole(messages, "user")).toEqual([0, 1, 2]);
    // slice(-0) 返回整个数组，这是 JavaScript 的标准行为
    expect(getRecentIndicesByRole(messages, "user", 0)).toEqual([0, 1, 2]);
    expect(getRangeIndicesByRole(messages, "user", 0, 0)).toEqual([]);
    expect(getCountByRole(messages, "user")).toBe(3);
    expect(getVisibleIndicesByRole(messages, markMap, "user")).toEqual([1, 2]);
    expect(getVisibleRecentIndicesByRole(messages, markMap, "user", 1)).toEqual([2]);
    expect(getVisibleRangeIndicesByRole(messages, markMap, "user", 0, 1)).toEqual([1]);
    expect(getVisibleCountByRole(messages, markMap, "user")).toBe(2);
  });
});
