/**
 * Visible Range Calculator Unit Tests
 */

import { describe, it, expect } from "vitest";
import type { LLMMessage, MessageMarkMap } from "@wf-agent/types";
import {
  getCurrentBoundary,
  getVisibleOriginalIndices,
  visibleIndexToOriginal,
  originalIndexToVisible,
  getVisibleMessages,
  getInvisibleMessages,
  isMessageVisible,
  getVisibleMessageCount,
  getInvisibleMessageCount,
} from "../visible-range-calculator.js";

// Helper function to create mock messages
function createMockMessages(count: number): LLMMessage[] {
  return Array.from({ length: count }, (_, i) => ({
    role: "user",
    content: `Message ${i + 1}`,
  }));
}

// Helper function to create mock MessageMarkMap
function createMockMarkMap(
  currentBatch: number = 0,
  batchBoundaries: number[] = [0],
  originalIndices?: number[],
): MessageMarkMap {
  const indices = originalIndices ?? Array.from({ length: 10 }, (_, i) => i);
  return {
    originalIndices: indices,
    batchBoundaries,
    boundaryToBatch: batchBoundaries.map((_, i) => i),
    currentBatch,
  };
}

describe("getCurrentBoundary", () => {
  it("should return the correct boundary for current batch", () => {
    const markMap = createMockMarkMap(0, [0, 5, 10]);
    expect(getCurrentBoundary(markMap)).toBe(0);
  });

  it("should return boundary for non-zero batch", () => {
    const markMap = createMockMarkMap(1, [0, 5, 10]);
    expect(getCurrentBoundary(markMap)).toBe(5);
  });

  it("should throw error for invalid markMap", () => {
    const invalidMarkMap = {
      originalIndices: [],
      batchBoundaries: [],
      boundaryToBatch: [],
      currentBatch: 0,
    };
    expect(() => getCurrentBoundary(invalidMarkMap as any)).toThrow();
  });

  it("should throw error for invalid currentBatch index", () => {
    const markMap = createMockMarkMap(5, [0, 5, 10]);
    expect(() => getCurrentBoundary(markMap)).toThrow();
  });
});

describe("getVisibleOriginalIndices", () => {
  it("should return all indices when boundary is 0", () => {
    const markMap = createMockMarkMap(0, [0], [0, 1, 2, 3, 4]);
    const result = getVisibleOriginalIndices(markMap);
    expect(result).toEqual([0, 1, 2, 3, 4]);
  });

  it("should return indices after boundary", () => {
    const markMap = createMockMarkMap(0, [2], [0, 1, 2, 3, 4]);
    const result = getVisibleOriginalIndices(markMap);
    expect(result).toEqual([2, 3, 4]);
  });

  it("should return empty array when boundary is beyond all indices", () => {
    const markMap = createMockMarkMap(0, [10], [0, 1, 2, 3, 4]);
    const result = getVisibleOriginalIndices(markMap);
    expect(result).toEqual([]);
  });
});

describe("visibleIndexToOriginal", () => {
  it("should convert visible index to original index", () => {
    const markMap = createMockMarkMap(0, [2], [0, 1, 2, 3, 4]);
    // Visible indices are [2, 3, 4], so visible index 0 -> original index 2
    expect(visibleIndexToOriginal(0, markMap)).toBe(2);
    expect(visibleIndexToOriginal(1, markMap)).toBe(3);
    expect(visibleIndexToOriginal(2, markMap)).toBe(4);
  });

  it("should throw error for out of bounds visible index", () => {
    const markMap = createMockMarkMap(0, [2], [0, 1, 2, 3, 4]);
    expect(() => visibleIndexToOriginal(3, markMap)).toThrow();
  });

  it("should throw error for negative visible index", () => {
    const markMap = createMockMarkMap(0, [0], [0, 1, 2]);
    expect(() => visibleIndexToOriginal(-1, markMap)).toThrow();
  });
});

describe("originalIndexToVisible", () => {
  it("should convert original index to visible index", () => {
    const markMap = createMockMarkMap(0, [2], [0, 1, 2, 3, 4]);
    // Original indices [2, 3, 4] are visible
    expect(originalIndexToVisible(2, markMap)).toBe(0);
    expect(originalIndexToVisible(3, markMap)).toBe(1);
    expect(originalIndexToVisible(4, markMap)).toBe(2);
  });

  it("should return null for invisible original index", () => {
    const markMap = createMockMarkMap(0, [2], [0, 1, 2, 3, 4]);
    // Original indices [0, 1] are invisible
    expect(originalIndexToVisible(0, markMap)).toBeNull();
    expect(originalIndexToVisible(1, markMap)).toBeNull();
  });

  it("should return null for out of bounds original index", () => {
    const markMap = createMockMarkMap(0, [0], [0, 1, 2]);
    expect(originalIndexToVisible(5, markMap)).toBeNull();
  });
});

describe("getVisibleMessages", () => {
  it("should return visible messages", () => {
    const messages = createMockMessages(5);
    const markMap = createMockMarkMap(0, [2], [0, 1, 2, 3, 4]);
    const result = getVisibleMessages(messages, markMap);
    expect(result).toHaveLength(3);
    expect(result[0]?.content).toBe("Message 3");
    expect(result[1]?.content).toBe("Message 4");
    expect(result[2]?.content).toBe("Message 5");
  });

  it("should return all messages when boundary is 0", () => {
    const messages = createMockMessages(3);
    const markMap = createMockMarkMap(0, [0], [0, 1, 2]);
    const result = getVisibleMessages(messages, markMap);
    expect(result).toHaveLength(3);
  });

  it("should return empty array when boundary is beyond all messages", () => {
    const messages = createMockMessages(3);
    const markMap = createMockMarkMap(0, [10], [0, 1, 2]);
    const result = getVisibleMessages(messages, markMap);
    expect(result).toHaveLength(0);
  });
});

describe("getInvisibleMessages", () => {
  it("should return invisible messages", () => {
    const messages = createMockMessages(5);
    const markMap = createMockMarkMap(0, [2], [0, 1, 2, 3, 4]);
    const result = getInvisibleMessages(messages, markMap);
    expect(result).toHaveLength(2);
    expect(result[0]?.content).toBe("Message 1");
    expect(result[1]?.content).toBe("Message 2");
  });

  it("should return empty array when boundary is 0", () => {
    const messages = createMockMessages(3);
    const markMap = createMockMarkMap(0, [0], [0, 1, 2]);
    const result = getInvisibleMessages(messages, markMap);
    expect(result).toHaveLength(0);
  });

  it("should return all messages when boundary is beyond all messages", () => {
    const messages = createMockMessages(3);
    const markMap = createMockMarkMap(0, [10], [0, 1, 2]);
    const result = getInvisibleMessages(messages, markMap);
    expect(result).toHaveLength(3);
  });
});

describe("isMessageVisible", () => {
  it("should return true for visible messages", () => {
    const markMap = createMockMarkMap(0, [2], [0, 1, 2, 3, 4]);
    expect(isMessageVisible(2, markMap)).toBe(true);
    expect(isMessageVisible(3, markMap)).toBe(true);
    expect(isMessageVisible(4, markMap)).toBe(true);
  });

  it("should return false for invisible messages", () => {
    const markMap = createMockMarkMap(0, [2], [0, 1, 2, 3, 4]);
    expect(isMessageVisible(0, markMap)).toBe(false);
    expect(isMessageVisible(1, markMap)).toBe(false);
  });
});

describe("getVisibleMessageCount", () => {
  it("should return correct visible message count", () => {
    const markMap = createMockMarkMap(0, [2], [0, 1, 2, 3, 4]);
    expect(getVisibleMessageCount(markMap)).toBe(3);
  });

  it("should return 0 when no messages are visible", () => {
    const markMap = createMockMarkMap(0, [10], [0, 1, 2]);
    expect(getVisibleMessageCount(markMap)).toBe(0);
  });

  it("should return total count when all messages are visible", () => {
    const markMap = createMockMarkMap(0, [0], [0, 1, 2, 3, 4]);
    expect(getVisibleMessageCount(markMap)).toBe(5);
  });
});

describe("getInvisibleMessageCount", () => {
  it("should return correct invisible message count", () => {
    const markMap = createMockMarkMap(0, [2], [0, 1, 2, 3, 4]);
    expect(getInvisibleMessageCount(markMap)).toBe(2);
  });

  it("should return 0 when all messages are visible", () => {
    const markMap = createMockMarkMap(0, [0], [0, 1, 2, 3, 4]);
    expect(getInvisibleMessageCount(markMap)).toBe(0);
  });

  it("should return total count when no messages are visible", () => {
    const markMap = createMockMarkMap(0, [10], [0, 1, 2, 3, 4]);
    expect(getInvisibleMessageCount(markMap)).toBe(5);
  });
});

describe("Integration tests", () => {
  it("should handle complex scenarios correctly", () => {
    const messages = createMockMessages(10);
    const markMap = createMockMarkMap(
      1,
      [0, 5, 10],
      Array.from({ length: 10 }, (_, i) => i),
    );

    // Current batch is 1, boundary is 5
    expect(getCurrentBoundary(markMap)).toBe(5);
    expect(getVisibleMessageCount(markMap)).toBe(5);
    expect(getInvisibleMessageCount(markMap)).toBe(5);

    const visibleMessages = getVisibleMessages(messages, markMap);
    expect(visibleMessages).toHaveLength(5);
    expect(visibleMessages[0]?.content).toBe("Message 6");

    const invisibleMessages = getInvisibleMessages(messages, markMap);
    expect(invisibleMessages).toHaveLength(5);
    expect(invisibleMessages[0]?.content).toBe("Message 1");
  });
});
