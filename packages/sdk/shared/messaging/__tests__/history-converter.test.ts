/**
 * History Converter Tests
 */
import { describe, it, expect } from "vitest";
import { HistoryConverter } from "../history-converter.js";
import type { LLMMessage } from "@wf-agent/types";

describe("HistoryConverter", () => {
  describe("convertToTextMode", () => {
    it("should return messages unchanged for function_call format", () => {
      const messages: LLMMessage[] = [
        { role: "user", content: "Hello" },
        {
          role: "assistant",
          content: "Hi",
          toolCalls: [{ id: "c1", type: "function", function: { name: "test", arguments: "{}" } }],
        },
      ];
      const result = HistoryConverter.convertToTextMode(messages, "native");
      expect(result).toBe(messages);
    });

    it("should convert assistant messages with tool calls in xml format", () => {
      const messages: LLMMessage[] = [
        { role: "user", content: "Hello" },
        {
          role: "assistant",
          content: "Let me check",
          toolCalls: [
            {
              id: "c1",
              type: "function",
              function: { name: "get_weather", arguments: '{"city":"Beijing"}' },
            },
          ],
        },
      ];
      const result = HistoryConverter.convertToTextMode(messages, "xml");
      expect(result).toHaveLength(2);
      const converted = result[1]!;
      expect(converted.role).toBe("assistant");
      expect(converted.toolCalls).toBeUndefined();
      expect(converted.content).toContain("get_weather");
    });

    it("should convert tool result messages in xml format", () => {
      const messages: LLMMessage[] = [{ role: "tool", toolCallId: "c1", content: "Sunny 25°C" }];
      const result = HistoryConverter.convertToTextMode(messages, "xml");
      expect(result).toHaveLength(1);
      expect(result[0]!.role).toBe("user");
      expect(result[0]!.content).toContain("c1");
    });

    it("should pass through messages without tool calls unchanged", () => {
      const messages: LLMMessage[] = [
        { role: "system", content: "System prompt" },
        { role: "user", content: "Hello" },
        { role: "assistant", content: "Hi" },
      ];
      const result = HistoryConverter.convertToTextMode(messages, "xml");
      expect(result).toHaveLength(3);
      expect(result[0]!.role).toBe("system");
      expect(result[1]!.role).toBe("user");
      expect(result[2]!.role).toBe("assistant");
    });

    it("should handle empty array", () => {
      const result = HistoryConverter.convertToTextMode([], "xml");
      expect(result).toEqual([]);
    });
  });

  describe("convertAssistantMessage", () => {
    it("should return message unchanged if no tool calls", () => {
      const message: LLMMessage = { role: "assistant", content: "Hello" };
      const result = HistoryConverter.convertAssistantMessage(message, "xml");
      expect(result).toBe(message);
    });

    it("should convert to xml format", () => {
      const message: LLMMessage = {
        role: "assistant",
        content: "Checking",
        toolCalls: [
          {
            id: "c1",
            type: "function",
            function: { name: "test_tool", arguments: '{"key":"value"}' },
          },
        ],
      };
      const result = HistoryConverter.convertAssistantMessage(message, "xml");
      expect(result.role).toBe("assistant");
      expect(result.toolCalls).toBeUndefined();
      expect(result.content).toContain("<tool_use>");
      expect(result.content).toContain("test_tool");
      expect(result.content).toContain("Checking");
    });

    it("should convert to json_wrapped format", () => {
      const message: LLMMessage = {
        role: "assistant",
        content: "Checking",
        toolCalls: [
          {
            id: "c1",
            type: "function",
            function: { name: "test_tool", arguments: '{"key":"value"}' },
          },
        ],
      };
      const result = HistoryConverter.convertAssistantMessage(message, "json_wrapped");
      expect(result.role).toBe("assistant");
      expect(result.toolCalls).toBeUndefined();
      expect(result.content).toContain("<<<TOOL_CALL>>>");
    });

    it("should convert to json_raw format", () => {
      const message: LLMMessage = {
        role: "assistant",
        content: "Checking",
        toolCalls: [
          {
            id: "c1",
            type: "function",
            function: { name: "test_tool", arguments: '{"key":"value"}' },
          },
        ],
      };
      const result = HistoryConverter.convertAssistantMessage(message, "json_raw");
      expect(result.role).toBe("assistant");
      expect(result.toolCalls).toBeUndefined();
      expect(result.content).toContain("test_tool");
    });
  });

  describe("convertToolResultMessage", () => {
    it("should convert to user message in xml format", () => {
      const message: LLMMessage = {
        role: "tool",
        toolCallId: "c1",
        content: "Result data",
      };
      const result = HistoryConverter.convertToolResultMessage(message, "xml");
      expect(result.role).toBe("user");
      expect(result.toolCallId).toBeUndefined();
      expect(result.content).toContain("c1");
      expect(result.content).toContain("Result data");
    });

    it("should convert to json_wrapped format", () => {
      const message: LLMMessage = {
        role: "tool",
        toolCallId: "c1",
        content: "Result data",
      };
      const result = HistoryConverter.convertToolResultMessage(message, "json_wrapped");
      expect(result.role).toBe("user");
      expect(result.content).toContain("<<<TOOL_CALL>>>");
    });
  });

  describe("needsConversion", () => {
    it("should return false for function_call format", () => {
      const message: LLMMessage = {
        role: "assistant",
        content: "Hi",
        toolCalls: [{ id: "c1", type: "function", function: { name: "test", arguments: "{}" } }],
      };
      expect(HistoryConverter.needsConversion(message, "native")).toBe(false);
    });

    it("should return true for assistant messages with tool calls in xml format", () => {
      const message: LLMMessage = {
        role: "assistant",
        content: "Hi",
        toolCalls: [{ id: "c1", type: "function", function: { name: "test", arguments: "{}" } }],
      };
      expect(HistoryConverter.needsConversion(message, "xml")).toBe(true);
    });

    it("should return true for tool messages in xml format", () => {
      const message: LLMMessage = {
        role: "tool",
        toolCallId: "c1",
        content: "Result",
      };
      expect(HistoryConverter.needsConversion(message, "xml")).toBe(true);
    });

    it("should return false for assistant messages without tool calls", () => {
      const message: LLMMessage = { role: "assistant", content: "Hi" };
      expect(HistoryConverter.needsConversion(message, "xml")).toBe(false);
    });

    it("should return false for user messages", () => {
      const message: LLMMessage = { role: "user", content: "Hi" };
      expect(HistoryConverter.needsConversion(message, "xml")).toBe(false);
    });

    it("should return false for tool messages without toolCallId", () => {
      const message: LLMMessage = { role: "tool", content: "Result" } as any;
      expect(HistoryConverter.needsConversion(message, "xml")).toBe(false);
    });
  });
});
