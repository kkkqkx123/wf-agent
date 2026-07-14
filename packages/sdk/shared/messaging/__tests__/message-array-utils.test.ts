/**
 * MessageArrayUtils Unit Tests
 */

import { describe, it, expect } from "vitest";
import type { LLMMessage } from "@wf-agent/types";
import { MessageArrayUtils } from "../message-array-utils.js";

// Helper function to create mock messages
function createMockMessages(count: number, role: string = "user"): LLMMessage[] {
  return Array.from({ length: count }, (_, i) => ({
    role: role as any,
    content: `Message ${i + 1}`,
  }));
}

describe("truncateMessages", () => {
  it("should keep first N messages", () => {
    const messages = createMockMessages(5);
    const result = MessageArrayUtils.truncateMessages(messages, { keepFirst: 3 });
    expect(result).toHaveLength(3);
    expect(result[0]?.content).toBe("Message 1");
  });

  it("should keep last N messages", () => {
    const messages = createMockMessages(5);
    const result = MessageArrayUtils.truncateMessages(messages, { keepLast: 2 });
    expect(result).toHaveLength(2);
    expect(result[0]?.content).toBe("Message 4");
  });

  it("should remove first N messages", () => {
    const messages = createMockMessages(5);
    const result = MessageArrayUtils.truncateMessages(messages, { removeFirst: 2 });
    expect(result).toHaveLength(3);
    expect(result[0]?.content).toBe("Message 3");
  });

  it("should remove last N messages", () => {
    const messages = createMockMessages(5);
    const result = MessageArrayUtils.truncateMessages(messages, { removeLast: 2 });
    expect(result).toHaveLength(3);
    expect(result[0]?.content).toBe("Message 1");
  });

  it("should truncate by range", () => {
    const messages = createMockMessages(5);
    const result = MessageArrayUtils.truncateMessages(messages, { range: { start: 1, end: 4 } });
    expect(result).toHaveLength(3);
    expect(result[0]?.content).toBe("Message 2");
  });

  it("should filter by role before truncation", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "User 1" },
      { role: "assistant", content: "Assistant 1" },
      { role: "user", content: "User 2" },
      { role: "assistant", content: "Assistant 2" },
    ];
    const result = MessageArrayUtils.truncateMessages(messages, { keepLast: 1, role: "user" });
    expect(result).toHaveLength(1);
    expect(result[0]?.role).toBe("user");
  });

  it("should return empty array when keepFirst is 0", () => {
    const messages = createMockMessages(5);
    const result = MessageArrayUtils.truncateMessages(messages, { keepFirst: 0 });
    expect(result).toHaveLength(0);
  });
});

describe("insertMessages", () => {
  it("should insert messages at specified position", () => {
    const messages = createMockMessages(3);
    const newMessages = createMockMessages(2, "assistant");
    const result = MessageArrayUtils.insertMessages(messages, 1, newMessages);
    expect(result).toHaveLength(5);
    expect(result[1]?.role).toBe("assistant");
  });

  it("should append messages when position is -1", () => {
    const messages = createMockMessages(3);
    const newMessages = createMockMessages(2, "assistant");
    const result = MessageArrayUtils.insertMessages(messages, -1, newMessages);
    expect(result).toHaveLength(5);
    expect(result[3]?.role).toBe("assistant");
  });

  it("should handle negative positions", () => {
    const messages = createMockMessages(5);
    const newMessages = createMockMessages(1, "assistant");
    const result = MessageArrayUtils.insertMessages(messages, -2, newMessages);
    expect(result).toHaveLength(6);
    expect(result[4]?.role).toBe("assistant");
  });

  it("should handle position beyond array length", () => {
    const messages = createMockMessages(3);
    const newMessages = createMockMessages(2, "assistant");
    const result = MessageArrayUtils.insertMessages(messages, 10, newMessages);
    expect(result).toHaveLength(5);
    expect(result[3]?.role).toBe("assistant");
  });

  it("should return copy when newMessages is empty", () => {
    const messages = createMockMessages(3);
    const result = MessageArrayUtils.insertMessages(messages, 1, []);
    expect(result).toHaveLength(3);
    expect(result).not.toBe(messages);
  });
});

describe("replaceMessage", () => {
  it("should replace message at specified index", () => {
    const messages = createMockMessages(3);
    const newMessage: LLMMessage = { role: "assistant", content: "New message" };
    const result = MessageArrayUtils.replaceMessage(messages, 1, newMessage);
    expect(result).toHaveLength(3);
    expect(result[1]?.content).toBe("New message");
  });

  it("should handle negative indices", () => {
    const messages = createMockMessages(3);
    const newMessage: LLMMessage = { role: "assistant", content: "New message" };
    const result = MessageArrayUtils.replaceMessage(messages, -1, newMessage);
    expect(result[2]?.content).toBe("New message");
  });

  it("should throw error for out of bounds index", () => {
    const messages = createMockMessages(3);
    const newMessage: LLMMessage = { role: "assistant", content: "New message" };
    expect(() => MessageArrayUtils.replaceMessage(messages, 5, newMessage)).toThrow();
  });
});

describe("clearMessages", () => {
  it("should clear all messages when keepSystemMessage is false", () => {
    const messages = createMockMessages(3);
    const result = MessageArrayUtils.clearMessages(messages, false);
    expect(result).toHaveLength(0);
  });

  it("should keep system messages when keepSystemMessage is true", () => {
    const messages: LLMMessage[] = [
      { role: "system", content: "System" },
      { role: "user", content: "User 1" },
      { role: "assistant", content: "Assistant 1" },
    ];
    const result = MessageArrayUtils.clearMessages(messages, true);
    expect(result).toHaveLength(1);
    expect(result[0]?.role).toBe("system");
  });
});

describe("filterMessagesByRole", () => {
  it("should filter messages by role", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "User 1" },
      { role: "assistant", content: "Assistant 1" },
      { role: "user", content: "User 2" },
    ];
    const result = MessageArrayUtils.filterMessagesByRole(messages, ["user"]);
    expect(result).toHaveLength(2);
    expect(result.every(m => m.role === "user")).toBe(true);
  });

  it("should filter by multiple roles", () => {
    const messages: LLMMessage[] = [
      { role: "system", content: "System" },
      { role: "user", content: "User 1" },
      { role: "assistant", content: "Assistant 1" },
    ];
    const result = MessageArrayUtils.filterMessagesByRole(messages, ["user", "assistant"]);
    expect(result).toHaveLength(2);
  });
});

describe("filterMessagesByContent", () => {
  it("should filter messages containing keywords", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "Hello world" },
      { role: "user", content: "Goodbye world" },
      { role: "user", content: "Hello there" },
    ];
    const result = MessageArrayUtils.filterMessagesByContent(messages, { contains: ["Hello"] });
    expect(result).toHaveLength(2);
  });

  it("should filter messages excluding keywords", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "Hello world" },
      { role: "user", content: "Goodbye world" },
      { role: "user", content: "Hello there" },
    ];
    const result = MessageArrayUtils.filterMessagesByContent(messages, { excludes: ["world"] });
    expect(result).toHaveLength(1);
    expect(result[0]?.content).toBe("Hello there");
  });

  it("should combine contains and excludes", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "Hello world" },
      { role: "user", content: "Goodbye world" },
      { role: "user", content: "Hello there" },
    ];
    const result = MessageArrayUtils.filterMessagesByContent(messages, {
      contains: ["Hello"],
      excludes: ["world"],
    });
    expect(result).toHaveLength(1);
    expect(result[0]?.content).toBe("Hello there");
  });
});

describe("mergeMessageArrays", () => {
  it("should merge multiple arrays", () => {
    const arr1 = createMockMessages(2, "user");
    const arr2 = createMockMessages(2, "assistant");
    const result = MessageArrayUtils.mergeMessageArrays(arr1, arr2);
    expect(result).toHaveLength(4);
  });

  it("should handle empty arrays", () => {
    const arr1 = createMockMessages(2);
    const result = MessageArrayUtils.mergeMessageArrays(arr1, []);
    expect(result).toHaveLength(2);
  });
});

describe("deduplicateMessages", () => {
  it("should deduplicate by default key (role:content)", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "Hello" },
      { role: "user", content: "Hello" },
      { role: "user", content: "World" },
    ];
    const result = MessageArrayUtils.deduplicateMessages(messages);
    expect(result).toHaveLength(2);
  });

  it("should deduplicate using custom key function", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "Hello 1" },
      { role: "user", content: "Hello 2" },
      { role: "assistant", content: "Hello 3" },
    ];
    const result = MessageArrayUtils.deduplicateMessages(messages, msg => msg.role);
    expect(result).toHaveLength(2);
  });
});

describe("extractMessagesByRange", () => {
  it("should extract messages in range", () => {
    const messages = createMockMessages(5);
    const result = MessageArrayUtils.extractMessagesByRange(messages, 1, 4);
    expect(result).toHaveLength(3);
    expect(result[0]?.content).toBe("Message 2");
  });
});

describe("splitMessagesByRole", () => {
  it("should split messages by role", () => {
    const messages: LLMMessage[] = [
      { role: "system", content: "System" },
      { role: "user", content: "User 1" },
      { role: "assistant", content: "Assistant 1" },
      { role: "user", content: "User 2" },
    ];
    const result = MessageArrayUtils.splitMessagesByRole(messages);
    expect(result.system).toHaveLength(1);
    expect(result.user).toHaveLength(2);
    expect(result.assistant).toHaveLength(1);
    expect(result.tool).toHaveLength(0);
  });
});

describe("validateMessageArray", () => {
  it("should validate correct messages", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "Hello" },
      { role: "assistant", content: "Hi" },
    ];
    const result = MessageArrayUtils.validateMessageArray(messages);
    expect(result.valid).toBe(true);
    expect(result.errors).toHaveLength(0);
  });

  it("should detect invalid role", () => {
    const messages: any[] = [{ role: "invalid", content: "Hello" }];
    const result = MessageArrayUtils.validateMessageArray(messages);
    expect(result.valid).toBe(false);
    expect(result.errors.length).toBeGreaterThan(0);
  });

  it("should detect missing content", () => {
    const messages: any[] = [{ role: "user" }];
    const result = MessageArrayUtils.validateMessageArray(messages);
    expect(result.valid).toBe(false);
  });

  it("should detect null message", () => {
    const messages: any[] = [null];
    const result = MessageArrayUtils.validateMessageArray(messages);
    expect(result.valid).toBe(false);
  });
});

describe("cloneMessages", () => {
  it("should deep clone messages", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "Hello" },
      { role: "assistant", content: "Hi" },
    ];
    const cloned = MessageArrayUtils.cloneMessages(messages);
    expect(cloned).toEqual(messages);
    expect(cloned).not.toBe(messages);
    expect(cloned[1]).not.toBe(messages[1]);
  });
});

describe("createMessageSnapshot", () => {
  it("should create snapshot with metadata", () => {
    const messages = createMockMessages(3);
    const snapshot = MessageArrayUtils.createMessageSnapshot(messages, {
      executionId: "exec-123",
      workflowId: "wf-456",
    });
    expect(snapshot.messages).toHaveLength(3);
    expect(snapshot.executionId).toBe("exec-123");
    expect(snapshot.workflowId).toBe("wf-456");
    expect(snapshot.messageCount).toBe(3);
    expect(snapshot.timestamp).toBeDefined();
  });

  it("should use current timestamp if not provided", () => {
    const messages = createMockMessages(2);
    const snapshot = MessageArrayUtils.createMessageSnapshot(messages);
    expect(snapshot.timestamp).toBeGreaterThan(0);
  });
});

describe("restoreFromSnapshot", () => {
  it("should restore messages from snapshot", () => {
    const messages = createMockMessages(3);
    const snapshot = MessageArrayUtils.createMessageSnapshot(messages);
    const restored = MessageArrayUtils.restoreFromSnapshot(snapshot);
    expect(restored).toEqual(messages);
    expect(restored).not.toBe(messages);
  });
});

describe("getRecentMessages", () => {
  it("should get last N messages", () => {
    const messages = createMockMessages(5);
    const result = MessageArrayUtils.getRecentMessages(messages, 2);
    expect(result).toHaveLength(2);
    expect(result[0]?.content).toBe("Message 4");
  });

  it("should return empty array when count is 0", () => {
    const messages = createMockMessages(3);
    const result = MessageArrayUtils.getRecentMessages(messages, 0);
    expect(result).toHaveLength(0);
  });

  it("should return all messages when count exceeds length", () => {
    const messages = createMockMessages(3);
    const result = MessageArrayUtils.getRecentMessages(messages, 10);
    expect(result).toHaveLength(3);
  });
});

describe("getRecentMessagesByRole", () => {
  it("should get last N messages for specified role", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "User 1" },
      { role: "assistant", content: "Assistant 1" },
      { role: "user", content: "User 2" },
      { role: "assistant", content: "Assistant 2" },
      { role: "user", content: "User 3" },
    ];
    const result = MessageArrayUtils.getRecentMessagesByRole(messages, "user", 2);
    expect(result).toHaveLength(2);
    expect(result[0]?.content).toBe("User 2");
    expect(result[1]?.content).toBe("User 3");
  });
});

describe("searchMessages", () => {
  it("should search messages by keyword", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "Hello world" },
      { role: "user", content: "Goodbye world" },
      { role: "user", content: "Hello there" },
    ];
    const result = MessageArrayUtils.searchMessages(messages, "Hello");
    expect(result).toHaveLength(2);
  });

  it("should be case insensitive", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "Hello World" },
      { role: "user", content: "HELLO THERE" },
    ];
    const result = MessageArrayUtils.searchMessages(messages, "hello");
    expect(result).toHaveLength(2);
  });
});