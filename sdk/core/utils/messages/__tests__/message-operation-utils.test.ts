/**
 * Message Operation Utils Unit Tests
 */

import { describe, it, expect } from "vitest";
import type {
  LLMMessage,
  MessageMarkMap,
  AppendMessageOperation,
  TruncateMessageOperation,
  InsertMessageOperation,
  ReplaceMessageOperation,
  ClearMessageOperation,
  FilterMessageOperation,
  RollbackMessageOperation,
} from "@wf-agent/types";
import { executeOperation } from "../message-operation-utils.js";

// Helper function to create mock messages
function createMockMessages(count: number, role: string = "user"): LLMMessage[] {
  return Array.from({ length: count }, (_, i) => ({
    role: role as any,
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

describe("executeOperation - APPEND", () => {
  it("should append messages to the end", async () => {
    const messages = createMockMessages(3);
    const markMap = createMockMarkMap(0, [0], [0, 1, 2]);
    
    const operation: AppendMessageOperation = {
      operation: "APPEND",
      messages: createMockMessages(2, "assistant"),
    };

    const result = await executeOperation({ messages, markMap }, operation);

    expect(result.messages).toHaveLength(5);
    expect(result.messages[3]?.role).toBe("assistant");
    expect(result.markMap.originalIndices).toHaveLength(5);
  });

  it("should not affect visibility", async () => {
    const messages = createMockMessages(3);
    const markMap = createMockMarkMap(0, [0], [0, 1, 2]);
    
    const operation: AppendMessageOperation = {
      operation: "APPEND",
      messages: createMockMessages(2),
    };

    const result = await executeOperation({ messages, markMap }, operation);

    // All messages should still be visible
    expect(result.stats.visibleMessageCount).toBe(5);
    expect(result.stats.invisibleMessageCount).toBe(0);
  });
});

describe("executeOperation - TRUNCATE", () => {
  it("should truncate messages with keepLast strategy", async () => {
    const messages = createMockMessages(5);
    const markMap = createMockMarkMap(0, [0], [0, 1, 2, 3, 4]);
    
    const operation: TruncateMessageOperation = {
      operation: "TRUNCATE",
      strategy: { type: "KEEP_LAST", count: 3 },
    };

    const result = await executeOperation({ messages, markMap }, operation);

    expect(result.messages).toHaveLength(3);
    expect(result.messages[0]?.content).toBe("Message 3");
  });

  it("should create new batch if specified", async () => {
    const messages = createMockMessages(5);
    const markMap = createMockMarkMap(0, [0], [0, 1, 2, 3, 4]);
    
    const operation: TruncateMessageOperation = {
      operation: "TRUNCATE",
      strategy: { type: "KEEP_FIRST", count: 2 },
      createNewBatch: true,
    };

    const result = await executeOperation({ messages, markMap }, operation);

    expect(result.markMap.currentBatch).toBe(1);
  });

  it("should handle visibleOnly mode", async () => {
    const messages = createMockMessages(10);
    const markMap = createMockMarkMap(0, [5], Array.from({ length: 10 }, (_, i) => i));
    
    const operation: TruncateMessageOperation = {
      operation: "TRUNCATE",
      strategy: { type: "KEEP_LAST", count: 2 },
    };

    const result = await executeOperation(
      { messages, markMap, options: { visibleOnly: true } },
      operation,
    );

    // Should keep invisible messages (0-4) and last 2 visible messages (8-9)
    expect(result.messages.length).toBeGreaterThan(2);
  });
});

describe("executeOperation - INSERT", () => {
  it("should insert messages at specified position", async () => {
    const messages = createMockMessages(3);
    const markMap = createMockMarkMap(0, [0], [0, 1, 2]);
    
    const operation: InsertMessageOperation = {
      operation: "INSERT",
      position: 1,
      messages: createMockMessages(2, "assistant"),
    };

    const result = await executeOperation({ messages, markMap }, operation);

    expect(result.messages).toHaveLength(5);
    expect(result.messages[1]?.role).toBe("assistant");
  });

  it("should append when position is -1", async () => {
    const messages = createMockMessages(3);
    const markMap = createMockMarkMap(0, [0], [0, 1, 2]);
    
    const operation: InsertMessageOperation = {
      operation: "INSERT",
      position: -1,
      messages: createMockMessages(2),
    };

    const result = await executeOperation({ messages, markMap }, operation);

    expect(result.messages).toHaveLength(5);
    expect(result.messages[3]?.content).toBe("Message 1");
  });
});

describe("executeOperation - REPLACE", () => {
  it("should replace message at specified index", async () => {
    const messages = createMockMessages(3);
    const markMap = createMockMarkMap(0, [0], [0, 1, 2]);
    
    const operation: ReplaceMessageOperation = {
      operation: "REPLACE",
      index: 1,
      message: { role: "assistant", content: "Replaced" },
    };

    const result = await executeOperation({ messages, markMap }, operation);

    expect(result.messages).toHaveLength(3);
    expect(result.messages[1]?.content).toBe("Replaced");
  });

  it("should throw error for out of bounds index", async () => {
    const messages = createMockMessages(3);
    const markMap = createMockMarkMap(0, [0], [0, 1, 2]);
    
    const operation: ReplaceMessageOperation = {
      operation: "REPLACE",
      index: 10,
      message: { role: "assistant", content: "Replaced" },
    };

    await expect(executeOperation({ messages, markMap }, operation)).rejects.toThrow();
  });
});

describe("executeOperation - CLEAR", () => {
  it("should clear all messages", async () => {
    const messages = createMockMessages(5);
    const markMap = createMockMarkMap(0, [0], [0, 1, 2, 3, 4]);
    
    const operation: ClearMessageOperation = {
      operation: "CLEAR",
    };

    const result = await executeOperation({ messages, markMap }, operation);

    expect(result.messages).toHaveLength(0);
  });

  it("should clear only visible messages in visibleOnly mode", async () => {
    const messages = createMockMessages(10);
    const markMap = createMockMarkMap(0, [5], Array.from({ length: 10 }, (_, i) => i));
    
    const operation: ClearMessageOperation = {
      operation: "CLEAR",
    };

    const result = await executeOperation(
      { messages, markMap, options: { visibleOnly: true } },
      operation,
    );

    // Should keep invisible messages (0-4)
    expect(result.messages.length).toBe(5);
  });
});

describe("executeOperation - FILTER", () => {
  it("should filter by role", async () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "User 1" },
      { role: "assistant", content: "Assistant 1" },
      { role: "user", content: "User 2" },
      { role: "assistant", content: "Assistant 2" },
    ];
    const markMap = createMockMarkMap(0, [0], [0, 1, 2, 3]);
    
    const operation: FilterMessageOperation = {
      operation: "FILTER",
      roles: ["user"],
    };

    const result = await executeOperation({ messages, markMap }, operation);

    expect(result.messages).toHaveLength(2);
    expect(result.messages.every(m => m.role === "user")).toBe(true);
  });

  it("should filter by content keywords", async () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "Hello world" },
      { role: "user", content: "Goodbye world" },
      { role: "user", content: "Hello there" },
    ];
    const markMap = createMockMarkMap(0, [0], [0, 1, 2]);
    
    const operation: FilterMessageOperation = {
      operation: "FILTER",
      contentContains: ["Hello"],
    };

    const result = await executeOperation({ messages, markMap }, operation);

    expect(result.messages).toHaveLength(2);
  });

  it("should handle visibleOnly mode correctly", async () => {
    const messages: LLMMessage[] = [
      { role: "system", content: "System" },
      { role: "user", content: "User 1" },
      { role: "assistant", content: "Assistant 1" },
      { role: "user", content: "User 2" },
      { role: "assistant", content: "Assistant 2" },
    ];
    const markMap = createMockMarkMap(0, [2], [0, 1, 2, 3, 4]);
    
    const operation: FilterMessageOperation = {
      operation: "FILTER",
      roles: ["user"],
    };

    const result = await executeOperation(
      { messages, markMap, options: { visibleOnly: true } },
      operation,
    );

    // Should keep invisible messages (0-1) and filtered visible user messages
    expect(result.messages.length).toBeGreaterThanOrEqual(2);
  });
});

describe("executeOperation - ROLLBACK", () => {
  it("should rollback to specified batch", async () => {
    const messages = createMockMessages(10);
    const markMap = createMockMarkMap(2, [0, 5, 10], Array.from({ length: 10 }, (_, i) => i));
    
    const operation: RollbackMessageOperation = {
      operation: "ROLLBACK",
      targetBatchIndex: 1,
    };

    const result = await executeOperation({ messages, markMap }, operation);

    expect(result.markMap.currentBatch).toBe(1);
    // Should only have messages from batch 1 onwards
    expect(result.messages.length).toBe(5);
  });
});

describe("executeOperation - Callback", () => {
  it("should call callback after operation", async () => {
    const messages = createMockMessages(3);
    const markMap = createMockMarkMap(0, [0], [0, 1, 2]);
    
    const operation: AppendMessageOperation = {
      operation: "APPEND",
      messages: createMockMessages(2),
    };

    let callbackCalled = false;
    const callback = async () => {
      callbackCalled = true;
    };

    await executeOperation({ messages, markMap }, operation, callback);

    expect(callbackCalled).toBe(true);
  });
});

describe("executeOperation - Error handling", () => {
  it("should throw error for unsupported operation type", async () => {
    const messages = createMockMessages(3);
    const markMap = createMockMarkMap(0, [0], [0, 1, 2]);
    
    const operation = {
      operation: "INVALID_OPERATION",
    } as any;

    await expect(executeOperation({ messages, markMap }, operation)).rejects.toThrow();
  });
});

describe("executeOperation - Statistics", () => {
  it("should calculate correct statistics", async () => {
    const messages = createMockMessages(10);
    const markMap = createMockMarkMap(0, [5], Array.from({ length: 10 }, (_, i) => i));
    
    const operation: AppendMessageOperation = {
      operation: "APPEND",
      messages: createMockMessages(2),
    };

    const result = await executeOperation({ messages, markMap }, operation);

    expect(result.stats.originalMessageCount).toBe(12);
    expect(result.stats.visibleMessageCount).toBe(7); // 5 original visible + 2 appended
    expect(result.stats.invisibleMessageCount).toBe(5);
  });
});
