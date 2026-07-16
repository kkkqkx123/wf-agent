/**
 * CrossBoundaryConverter Unit Tests
 *
 * Tests for cross-boundary protocol conversion covering:
 * - Same format (no conversion needed)
 * - Native to XML conversion
 * - XML to native conversion
 * - JSON wrapped to native conversion
 * - Tool call conversion
 * - Edge cases (empty messages, missing options)
 */
import { describe, it, expect, vi } from "vitest";
import { CrossBoundaryConverter } from "../cross-boundary-converter.js";
import type { LLMMessage, ToolCallFormatConfig } from "@wf-agent/types";

// Mock the HistoryConverter to avoid testing its internal logic
// CrossBoundaryConverter delegates to HistoryConverter for actual conversion
vi.mock("../history-converter.js", () => ({
  HistoryConverter: {
    toNative: vi.fn((messages: LLMMessage[]) => messages),
    fromNative: vi.fn((messages: LLMMessage[]) => messages),
  },
}));

describe("CrossBoundaryConverter", () => {
  const nativeConfig: ToolCallFormatConfig = {
    format: "native",
  };

  const xmlConfig: ToolCallFormatConfig = {
    format: "xml",
    xmlTags: {
      toolCall: "tool_use",
      toolName: "tool_name",
      toolArgs: "parameters",
      toolResult: "tool_result",
      toolCallId: "tool_call_id",
      toolOutput: "tool_output",
    },
  };

  const jsonWrappedConfig: ToolCallFormatConfig = {
    format: "json_wrapped",
    markers: { start: "<<<TOOL_CALL>>>", end: "<<<END_TOOL_CALL>>>" },
  };

  const sampleMessages: LLMMessage[] = [
    { role: "user", content: "Hello" },
    { role: "assistant", content: "I can help with that" },
    { role: "user", content: "Please do it" },
  ];

  // ===========================================================================
  // Scenario A: Same format — no conversion needed
  // ===========================================================================
  describe("same format (no conversion)", () => {
    it("should return messages unchanged when source and target formats are the same", () => {
      const result = CrossBoundaryConverter.convert(sampleMessages, nativeConfig, nativeConfig);
      expect(result).toBe(sampleMessages);
    });

    it("should return messages unchanged for same non-native format", () => {
      const result = CrossBoundaryConverter.convert(sampleMessages, xmlConfig, xmlConfig);
      expect(result).toBe(sampleMessages);
    });
  });

  // ===========================================================================
  // Scenario B: Native to XML conversion
  // ===========================================================================
  describe("native to xml conversion", () => {
    it("should convert native messages to xml format", () => {
      const result = CrossBoundaryConverter.convert(sampleMessages, nativeConfig, xmlConfig);
      expect(result).toBeDefined();
      expect(Array.isArray(result)).toBe(true);
    });

    it("should pass through conversion options", () => {
      const options = { stripToolCallText: true, restoreToolRole: true };
      const result = CrossBoundaryConverter.convert(sampleMessages, nativeConfig, xmlConfig, options);
      expect(result).toBeDefined();
    });

    it("should handle empty messages array", () => {
      const result = CrossBoundaryConverter.convert([], nativeConfig, xmlConfig);
      expect(result).toEqual([]);
    });
  });

  // ===========================================================================
  // Scenario C: XML to native conversion
  // ===========================================================================
  describe("xml to native conversion", () => {
    it("should convert xml messages to native format", () => {
      const result = CrossBoundaryConverter.convert(sampleMessages, xmlConfig, nativeConfig);
      expect(result).toBeDefined();
      expect(Array.isArray(result)).toBe(true);
    });

    it("should handle empty messages", () => {
      const result = CrossBoundaryConverter.convert([], xmlConfig, nativeConfig);
      expect(result).toEqual([]);
    });
  });

  // ===========================================================================
  // Scenario D: JSON wrapped to native conversion
  // ===========================================================================
  describe("json_wrapped to native conversion", () => {
    it("should convert json_wrapped messages to native format", () => {
      const result = CrossBoundaryConverter.convert(sampleMessages, jsonWrappedConfig, nativeConfig);
      expect(result).toBeDefined();
      expect(Array.isArray(result)).toBe(true);
    });

    it("should handle conversion with markers", () => {
      const result = CrossBoundaryConverter.convert(sampleMessages, jsonWrappedConfig, xmlConfig);
      expect(result).toBeDefined();
      expect(Array.isArray(result)).toBe(true);
    });
  });

  // ===========================================================================
  // Scenario E: Tool calls conversion
  // ===========================================================================
  describe("convertToolCalls", () => {
    const sampleToolCalls = [
      { id: "call-1", function: { name: "get_weather", arguments: '{"city":"London"}' } },
      { id: "call-2", function: { name: "search_web", arguments: '{"query":"news"}' } },
    ];

    it("should return tool calls unchanged when formats match", () => {
      const result = CrossBoundaryConverter.convertToolCalls(sampleToolCalls, nativeConfig, nativeConfig);
      expect(result).toBe(sampleToolCalls);
    });

    it("should return tool calls unchanged for different formats (structured data handled at message level)", () => {
      const result = CrossBoundaryConverter.convertToolCalls(sampleToolCalls, nativeConfig, xmlConfig);
      expect(result).toBe(sampleToolCalls);
    });

    it("should handle empty tool calls array", () => {
      const result = CrossBoundaryConverter.convertToolCalls([], nativeConfig, xmlConfig);
      expect(result).toEqual([]);
    });
  });
});