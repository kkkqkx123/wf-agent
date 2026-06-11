/**
 * Tests for MessageHistory
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { MessageHistory } from "../message-history.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type { LLMMessage } from "@wf-agent/types";

// Mock dependencies
vi.mock("../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

describe("MessageHistory", () => {
  let messageHistory: MessageHistory;

  const createMessage = (role: string, content: string, overrides: Partial<LLMMessage> = {}): LLMMessage => ({
    role: role as LLMMessage["role"],
    content,
    ...overrides,
  });

  beforeEach(() => {
    messageHistory = new MessageHistory("test-agent-1");
  });

  describe("constructor", () => {
    it("should create an empty message history", () => {
      expect(messageHistory.getAgentLoopId()).toBe("test-agent-1");
      expect(messageHistory.size()).toBe(0);
      expect(messageHistory.isEmpty()).toBe(true);
      expect(messageHistory.getMessages()).toEqual([]);
    });
  });

  describe("getAgentLoopId", () => {
    it("should return the agent loop ID", () => {
      expect(messageHistory.getAgentLoopId()).toBe("test-agent-1");
    });
  });

  describe("addMessage", () => {
    it("should add a message to the history", () => {
      const message = createMessage("user", "Hello");

      messageHistory.addMessage(message);

      expect(messageHistory.size()).toBe(1);
      expect(messageHistory.isEmpty()).toBe(false);
    });

    it("should throw RuntimeValidationError for null message", () => {
      expect(() => messageHistory.addMessage(null as unknown as LLMMessage)).toThrow(RuntimeValidationError);
    });

    it("should throw RuntimeValidationError for undefined message", () => {
      expect(() => messageHistory.addMessage(undefined as unknown as LLMMessage)).toThrow(RuntimeValidationError);
    });

    it("should throw RuntimeValidationError for message without role", () => {
      const message = { content: "test" } as LLMMessage;

      expect(() => messageHistory.addMessage(message)).toThrow(RuntimeValidationError);
    });

    it("should add multiple messages in order", () => {
      const msg1 = createMessage("user", "Hello");
      const msg2 = createMessage("assistant", "Hi there");
      const msg3 = createMessage("user", "How are you?");

      messageHistory.addMessage(msg1);
      messageHistory.addMessage(msg2);
      messageHistory.addMessage(msg3);

      expect(messageHistory.size()).toBe(3);
      const messages = messageHistory.getMessages();
      expect(messages[0].content).toBe("Hello");
      expect(messages[1].content).toBe("Hi there");
      expect(messages[2].content).toBe("How are you?");
    });
  });

  describe("getMessages", () => {
    it("should return a copy of messages", () => {
      const message = createMessage("user", "Hello");
      messageHistory.addMessage(message);

      const messages = messageHistory.getMessages();

      // Modifying returned array should not affect internal state
      messages.push(createMessage("assistant", "Hi"));
      expect(messageHistory.size()).toBe(1);
    });

    it("should return a shallow copy of the messages array", () => {
      const message = createMessage("user", "Hello");
      messageHistory.addMessage(message);

      const messages = messageHistory.getMessages();

      // Modifying returned array should not affect internal state
      messages.pop();
      expect(messageHistory.size()).toBe(1);
    });
  });

  describe("getMessagesPaged", () => {
    beforeEach(() => {
      for (let i = 0; i < 10; i++) {
        messageHistory.addMessage(createMessage("user", `Message ${i}`));
      }
    });

    it("should return all messages with default pagination", () => {
      const result = messageHistory.getMessagesPaged();

      expect(result.total).toBe(10);
      expect(result.messages).toHaveLength(10);
    });

    it("should respect limit parameter", () => {
      const result = messageHistory.getMessagesPaged({ limit: 3 });

      expect(result.total).toBe(10);
      expect(result.messages).toHaveLength(3);
    });

    it("should respect offset parameter", () => {
      const result = messageHistory.getMessagesPaged({ offset: 5, limit: 3 });

      expect(result.total).toBe(10);
      expect(result.messages).toHaveLength(3);
      expect(result.messages[0].content).toBe("Message 5");
    });

    it("should handle offset beyond total", () => {
      const result = messageHistory.getMessagesPaged({ offset: 20, limit: 5 });

      expect(result.total).toBe(10);
      expect(result.messages).toHaveLength(0);
    });

    it("should return cloned messages", () => {
      const result = messageHistory.getMessagesPaged({ limit: 1 });

      result.messages[0].content = "Modified";
      expect(messageHistory.getMessages()[0].content).toBe("Message 0");
    });

    it("should use default values when options are empty", () => {
      const result = messageHistory.getMessagesPaged({});

      expect(result.total).toBe(10);
      expect(result.messages).toHaveLength(10);
    });
  });

  describe("getRecentMessages", () => {
    beforeEach(() => {
      for (let i = 0; i < 5; i++) {
        messageHistory.addMessage(createMessage("user", `Message ${i}`));
      }
    });

    it("should return the most recent messages", () => {
      const recent = messageHistory.getRecentMessages(3);

      expect(recent).toHaveLength(3);
      expect(recent[0].content).toBe("Message 2");
      expect(recent[2].content).toBe("Message 4");
    });

    it("should return fewer messages when count exceeds total", () => {
      const recent = messageHistory.getRecentMessages(10);

      expect(recent).toHaveLength(5);
    });

    it("should return all messages when count is 0 (slice(-0) returns full array)", () => {
      const recent = messageHistory.getRecentMessages(0);

      expect(recent).toHaveLength(5);
    });

    it("should return cloned messages", () => {
      const recent = messageHistory.getRecentMessages(1);

      recent[0].content = "Modified";
      expect(messageHistory.getMessages()[4].content).toBe("Message 4");
    });
  });

  describe("findMessages", () => {
    beforeEach(() => {
      messageHistory.addMessage(createMessage("user", "Hello world"));
      messageHistory.addMessage(createMessage("assistant", "Hi there"));
      messageHistory.addMessage(createMessage("user", "How are you?"));
      messageHistory.addMessage(createMessage("assistant", "I'm fine, thanks!"));
    });

    it("should find messages by role", () => {
      const results = messageHistory.findMessages({ role: "user" });

      expect(results).toEqual([0, 2]);
    });

    it("should find messages by content substring", () => {
      const results = messageHistory.findMessages({ contentContains: "Hello" });

      expect(results).toEqual([0]);
    });

    it("should find messages by both role and content", () => {
      const results = messageHistory.findMessages({ role: "assistant", contentContains: "fine" });

      expect(results).toEqual([3]);
    });

    it("should return empty array when no matches", () => {
      const results = messageHistory.findMessages({ role: "system" });

      expect(results).toEqual([]);
    });
  });

  describe("setMessages", () => {
    it("should replace all messages", () => {
      messageHistory.addMessage(createMessage("user", "Original"));

      const newMessages = [
        createMessage("system", "System message"),
        createMessage("user", "New message"),
      ];

      messageHistory.setMessages(newMessages);

      expect(messageHistory.size()).toBe(2);
      expect(messageHistory.getMessages()[0].content).toBe("System message");
    });

    it("should throw RuntimeValidationError for non-array input", () => {
      expect(() => messageHistory.setMessages(null as unknown as LLMMessage[])).toThrow(RuntimeValidationError);
      expect(() => messageHistory.setMessages(undefined as unknown as LLMMessage[])).toThrow(RuntimeValidationError);
      expect(() => messageHistory.setMessages("not array" as unknown as LLMMessage[])).toThrow(RuntimeValidationError);
    });

    it("should throw RuntimeValidationError for array with null message", () => {
      expect(() => messageHistory.setMessages([null as unknown as LLMMessage])).toThrow(RuntimeValidationError);
    });

    it("should throw RuntimeValidationError for array with message without role", () => {
      expect(() => messageHistory.setMessages([{ content: "no role" } as LLMMessage])).toThrow(RuntimeValidationError);
    });

    it("should create a copy of the input array", () => {
      const messages = [createMessage("user", "Test")];
      messageHistory.setMessages(messages);

      messages.push(createMessage("assistant", "Extra"));
      expect(messageHistory.size()).toBe(1);
    });
  });

  describe("clearMessages", () => {
    it("should clear all messages", () => {
      messageHistory.addMessage(createMessage("user", "Hello"));
      messageHistory.addMessage(createMessage("assistant", "Hi"));

      messageHistory.clearMessages();

      expect(messageHistory.size()).toBe(0);
      expect(messageHistory.isEmpty()).toBe(true);
      expect(messageHistory.getMessages()).toEqual([]);
    });

    it("should handle clearing empty history", () => {
      messageHistory.clearMessages();

      expect(messageHistory.size()).toBe(0);
    });
  });

  describe("getStats", () => {
    it("should return empty stats for empty history", () => {
      const stats = messageHistory.getStats();

      expect(stats.totalMessages).toBe(0);
      expect(stats.roleDistribution).toEqual({});
      expect(stats.totalTokenUsage).toEqual({ promptTokens: 0, completionTokens: 0, totalTokens: 0 });
    });

    it("should calculate role distribution", () => {
      messageHistory.addMessage(createMessage("user", "Hello"));
      messageHistory.addMessage(createMessage("assistant", "Hi"));
      messageHistory.addMessage(createMessage("user", "How are you?"));

      const stats = messageHistory.getStats();

      expect(stats.totalMessages).toBe(3);
      expect(stats.roleDistribution).toEqual({ user: 2, assistant: 1 });
    });

    it("should calculate token usage from metadata", () => {
      messageHistory.addMessage(
        createMessage("assistant", "Response", {
          metadata: {
            usage: { promptTokens: 10, completionTokens: 20, totalTokens: 30 },
          },
        }),
      );
      messageHistory.addMessage(
        createMessage("assistant", "Response 2", {
          metadata: {
            usage: { promptTokens: 5, completionTokens: 10, totalTokens: 15 },
          },
        }),
      );

      const stats = messageHistory.getStats();

      expect(stats.totalTokenUsage).toEqual({
        promptTokens: 15,
        completionTokens: 30,
        totalTokens: 45,
      });
    });

    it("should handle messages without usage metadata", () => {
      messageHistory.addMessage(createMessage("user", "Hello"));

      const stats = messageHistory.getStats();

      expect(stats.totalTokenUsage).toEqual({ promptTokens: 0, completionTokens: 0, totalTokens: 0 });
    });
  });

  describe("normalizeHistory", () => {
    it("should not change history when tool calls are fully responded", () => {
      messageHistory.addMessage(
        createMessage("assistant", "", {
          toolCalls: [{ id: "call-1", type: "function", function: { name: "test_tool", arguments: "{}" } }],
        }),
      );
      messageHistory.addMessage(
        createMessage("tool", "Result", { toolCallId: "call-1" }),
      );

      messageHistory.normalizeHistory();

      expect(messageHistory.size()).toBe(2);
    });

    it("should add error messages for unresponded tool calls", () => {
      messageHistory.addMessage(createMessage("user", "Do something"));
      messageHistory.addMessage(
        createMessage("assistant", "", {
          toolCalls: [
            { id: "call-1", type: "function", function: { name: "tool_a", arguments: "{}" } },
            { id: "call-2", type: "function", function: { name: "tool_b", arguments: "{}" } },
          ],
        }),
      );
      messageHistory.addMessage(
        createMessage("tool", "Result A", { toolCallId: "call-1" }),
      );

      messageHistory.normalizeHistory();

      // Original 3 messages + 1 error for call-2
      expect(messageHistory.size()).toBe(4);
      const messages = messageHistory.getMessages();
      // Error for call-2 is inserted right after the assistant message (index 1), so it's at index 2
      const errorMessage = messages[2];
      expect(errorMessage.role).toBe("tool");
      expect(errorMessage.toolCallId).toBe("call-2");
      expect(errorMessage.content).toContain("not responded");
      expect(errorMessage.metadata).toEqual({ normalized: true });
    });

    it("should not change history with no tool calls", () => {
      messageHistory.addMessage(createMessage("user", "Hello"));
      messageHistory.addMessage(createMessage("assistant", "Hi there"));

      messageHistory.normalizeHistory();

      expect(messageHistory.size()).toBe(2);
    });

    it("should handle empty history", () => {
      messageHistory.normalizeHistory();

      expect(messageHistory.size()).toBe(0);
    });
  });

  describe("snapshot operations", () => {
    it("should create a snapshot of the state", () => {
      messageHistory.addMessage(createMessage("user", "Hello"));
      messageHistory.addMessage(createMessage("assistant", "Hi"));

      const snapshot = messageHistory.createSnapshot();

      expect(snapshot.messages).toHaveLength(2);
      expect(snapshot.messages[0].content).toBe("Hello");
      expect(snapshot.messages[1].content).toBe("Hi");
    });

    it("should return cloned messages in snapshot", () => {
      messageHistory.addMessage(createMessage("user", "Hello"));

      const snapshot = messageHistory.createSnapshot();

      snapshot.messages[0].content = "Modified";
      expect(messageHistory.getMessages()[0].content).toBe("Hello");
    });

    it("should restore from snapshot", () => {
      messageHistory.addMessage(createMessage("user", "Original"));

      const snapshot = messageHistory.createSnapshot();
      snapshot.messages.push(createMessage("assistant", "Restored message"));

      // Clear and restore with modified snapshot
      messageHistory.clearMessages();
      messageHistory.restoreFromSnapshot({ messages: snapshot.messages });

      expect(messageHistory.size()).toBe(2);
      expect(messageHistory.getMessages()[1].content).toBe("Restored message");
    });

    it("should create cloned messages when restoring from snapshot", () => {
      const originalMessages = [createMessage("user", "Hello")];
      messageHistory.restoreFromSnapshot({ messages: originalMessages });

      originalMessages[0].content = "Modified";

      expect(messageHistory.getMessages()[0].content).toBe("Hello");
    });

    it("should restore empty snapshot", () => {
      messageHistory.addMessage(createMessage("user", "Hello"));

      messageHistory.restoreFromSnapshot({ messages: [] });

      expect(messageHistory.size()).toBe(0);
    });
  });

  describe("cleanup", () => {
    it("should clear all messages", () => {
      messageHistory.addMessage(createMessage("user", "Hello"));
      messageHistory.addMessage(createMessage("assistant", "Hi"));

      messageHistory.cleanup();

      expect(messageHistory.size()).toBe(0);
      expect(messageHistory.isEmpty()).toBe(true);
    });

    it("should handle cleanup of empty history", () => {
      messageHistory.cleanup();

      expect(messageHistory.size()).toBe(0);
    });
  });

  describe("reset", () => {
    it("should reset to initial state", () => {
      messageHistory.addMessage(createMessage("user", "Hello"));
      messageHistory.addMessage(createMessage("assistant", "Hi"));

      messageHistory.reset();

      expect(messageHistory.size()).toBe(0);
      expect(messageHistory.isEmpty()).toBe(true);
      expect(messageHistory.getMessages()).toEqual([]);
    });

    it("should be idempotent", () => {
      messageHistory.reset();

      expect(messageHistory.size()).toBe(0);
    });
  });

  describe("size and isEmpty", () => {
    it("should return 0 and true for empty history", () => {
      expect(messageHistory.size()).toBe(0);
      expect(messageHistory.isEmpty()).toBe(true);
    });

    it("should return correct size after adding messages", () => {
      messageHistory.addMessage(createMessage("user", "Msg 1"));
      expect(messageHistory.size()).toBe(1);
      expect(messageHistory.isEmpty()).toBe(false);

      messageHistory.addMessage(createMessage("assistant", "Msg 2"));
      expect(messageHistory.size()).toBe(2);
    });

    it("should update size after clearing", () => {
      messageHistory.addMessage(createMessage("user", "Msg 1"));
      messageHistory.clearMessages();

      expect(messageHistory.size()).toBe(0);
      expect(messageHistory.isEmpty()).toBe(true);
    });
  });
});