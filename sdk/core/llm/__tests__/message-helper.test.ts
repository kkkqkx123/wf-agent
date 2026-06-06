/**
 * Message Helper Unit Tests
 * Tests for message extraction and filtering utilities
 */

import { describe, it, expect } from "vitest";
import {
  extractSystemMessage,
  filterSystemMessages,
  extractAndFilterSystemMessages,
  isEmptyMessages,
  getLastMessage,
  getLastUserMessage,
  getLastAssistantMessage,
  countMessagesByRole,
} from "../message-helper.js";
import type { LLMMessage } from "@wf-agent/types";

function msg(overrides: Partial<LLMMessage>): LLMMessage {
  return {
    role: "user",
    content: "test",
    ...overrides,
  };
}

describe("message-helper", () => {
  describe("extractSystemMessage", () => {
    it("should extract the first system message", () => {
      const messages = [
        msg({ role: "system", content: "You are helpful" }),
        msg({ role: "user", content: "Hello" }),
      ];
      expect(extractSystemMessage(messages)).toEqual({
        role: "system",
        content: "You are helpful",
      });
    });

    it("should return null if no system message", () => {
      const messages = [
        msg({ role: "user", content: "Hello" }),
        msg({ role: "assistant", content: "Hi" }),
      ];
      expect(extractSystemMessage(messages)).toBeNull();
    });

    it("should return null for empty array", () => {
      expect(extractSystemMessage([])).toBeNull();
    });

    it("should return null for null/undefined input", () => {
      expect(extractSystemMessage(null as any)).toBeNull();
      expect(extractSystemMessage(undefined as any)).toBeNull();
    });

    it("should return the first system message when multiple exist", () => {
      const messages = [
        msg({ role: "system", content: "First" }),
        msg({ role: "system", content: "Second" }),
      ];
      const result = extractSystemMessage(messages);
      expect(result?.content).toBe("First");
    });
  });

  describe("filterSystemMessages", () => {
    it("should remove all system messages", () => {
      const messages = [
        msg({ role: "system", content: "System" }),
        msg({ role: "user", content: "Hello" }),
        msg({ role: "assistant", content: "Hi" }),
      ];
      const result = filterSystemMessages(messages);
      expect(result).toHaveLength(2);
      expect(result.every(m => m.role !== "system")).toBe(true);
    });

    it("should return empty array for empty input", () => {
      expect(filterSystemMessages([])).toEqual([]);
    });

    it("should return empty array for null/undefined input", () => {
      expect(filterSystemMessages(null as any)).toEqual([]);
      expect(filterSystemMessages(undefined as any)).toEqual([]);
    });

    it("should return same array if no system messages", () => {
      const messages = [
        msg({ role: "user", content: "Hello" }),
        msg({ role: "assistant", content: "Hi" }),
      ];
      const result = filterSystemMessages(messages);
      expect(result).toHaveLength(2);
    });
  });

  describe("extractAndFilterSystemMessages", () => {
    it("should extract and filter system messages", () => {
      const messages = [
        msg({ role: "system", content: "System" }),
        msg({ role: "user", content: "Hello" }),
      ];
      const { systemMessage, filteredMessages } = extractAndFilterSystemMessages(messages);
      expect(systemMessage?.content).toBe("System");
      expect(filteredMessages).toHaveLength(1);
      expect(filteredMessages[0].role).toBe("user");
    });

    it("should return null for systemMessage when none present", () => {
      const messages = [msg({ role: "user", content: "Hello" })];
      const { systemMessage, filteredMessages } = extractAndFilterSystemMessages(messages);
      expect(systemMessage).toBeNull();
      expect(filteredMessages).toHaveLength(1);
    });
  });

  describe("isEmptyMessages", () => {
    it("should return true for empty array", () => {
      expect(isEmptyMessages([])).toBe(true);
    });

    it("should return true for undefined", () => {
      expect(isEmptyMessages(undefined)).toBe(true);
    });

    it("should return false for non-empty array", () => {
      expect(isEmptyMessages([msg({ content: "Hello" })])).toBe(false);
    });
  });

  describe("getLastMessage", () => {
    it("should return the last message", () => {
      const messages = [
        msg({ role: "user", content: "Hello" }),
        msg({ role: "assistant", content: "Hi" }),
      ];
      expect(getLastMessage(messages)?.content).toBe("Hi");
    });

    it("should return null for empty array", () => {
      expect(getLastMessage([])).toBeNull();
    });

    it("should return null for null/undefined", () => {
      expect(getLastMessage(null as any)).toBeNull();
      expect(getLastMessage(undefined as any)).toBeNull();
    });
  });

  describe("getLastUserMessage", () => {
    it("should return the last user message", () => {
      const messages = [
        msg({ role: "user", content: "First" }),
        msg({ role: "assistant", content: "Response" }),
        msg({ role: "user", content: "Second" }),
      ];
      expect(getLastUserMessage(messages)?.content).toBe("Second");
    });

    it("should return null if no user messages", () => {
      const messages = [
        msg({ role: "assistant", content: "Hi" }),
        msg({ role: "tool", content: "result" }),
      ];
      expect(getLastUserMessage(messages)).toBeNull();
    });

    it("should return null for empty array", () => {
      expect(getLastUserMessage([])).toBeNull();
    });
  });

  describe("getLastAssistantMessage", () => {
    it("should return the last assistant message", () => {
      const messages = [
        msg({ role: "user", content: "Hello" }),
        msg({ role: "assistant", content: "First" }),
        msg({ role: "user", content: "Again" }),
        msg({ role: "assistant", content: "Second" }),
      ];
      expect(getLastAssistantMessage(messages)?.content).toBe("Second");
    });

    it("should return null if no assistant messages", () => {
      const messages = [
        msg({ role: "user", content: "Hi" }),
        msg({ role: "tool", content: "result" }),
      ];
      expect(getLastAssistantMessage(messages)).toBeNull();
    });

    it("should return null for empty array", () => {
      expect(getLastAssistantMessage([])).toBeNull();
    });
  });

  describe("countMessagesByRole", () => {
    it("should count messages by role", () => {
      const messages = [
        msg({ role: "system", content: "S" }),
        msg({ role: "user", content: "U1" }),
        msg({ role: "user", content: "U2" }),
        msg({ role: "assistant", content: "A1" }),
        msg({ role: "tool", content: "T1" }),
        msg({ role: "assistant", content: "A2" }),
      ];
      const counts = countMessagesByRole(messages);
      expect(counts).toEqual({
        system: 1,
        user: 2,
        assistant: 2,
        tool: 1,
      });
    });

    it("should return zero counts for empty array", () => {
      const counts = countMessagesByRole([]);
      expect(counts).toEqual({
        system: 0,
        user: 0,
        assistant: 0,
        tool: 0,
      });
    });

    it("should return zero counts for null/undefined", () => {
      const nullCounts = countMessagesByRole(null as any);
      expect(nullCounts).toEqual({
        system: 0,
        user: 0,
        assistant: 0,
        tool: 0,
      });

      const undefinedCounts = countMessagesByRole(undefined as any);
      expect(undefinedCounts).toEqual({
        system: 0,
        user: 0,
        assistant: 0,
        tool: 0,
      });
    });

    it("should ignore messages with unknown roles", () => {
      const messages = [
        msg({ role: "custom_role" as any, content: "test" }),
      ];
      const counts = countMessagesByRole(messages);
      expect(counts).toEqual({
        system: 0,
        user: 0,
        assistant: 0,
        tool: 0,
      });
    });
  });
});