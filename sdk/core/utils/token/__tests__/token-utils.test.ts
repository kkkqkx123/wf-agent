/**
 * Unit Tests for Token Utilities
 */

import { describe, it, expect } from "vitest";
import type { LLMMessage, LLMUsage } from "@wf-agent/types";
import {
  estimateTokens,
  getTokenUsage,
  isTokenLimitExceeded,
} from "../token-utils.js";

describe("estimateTokens", () => {
  it("should return 0 for empty messages array", () => {
    expect(estimateTokens([])).toBe(0);
  });

  it("should count tokens for string content", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "Hello" },
    ];
    const tokens = estimateTokens(messages);
    // "Hello" + 4 overhead per message
    expect(tokens).toBeGreaterThan(0);
  });

  it("should count tokens for array content", () => {
    const messages: LLMMessage[] = [
      {
        role: "user",
        content: [{ type: "text", text: "Hello" }],
      },
    ];
    const tokens = estimateTokens(messages);
    expect(tokens).toBeGreaterThan(0);
  });

  it("should include thinking tokens", () => {
    const messages: LLMMessage[] = [
      {
        role: "assistant",
        content: "Short answer",
        thinking: "Let me think about this carefully and provide a thorough response",
      },
    ];
    const withoutThinking = estimateTokens([
      { role: "assistant", content: "Short answer" },
    ]);
    const withThinking = estimateTokens(messages);
    expect(withThinking).toBeGreaterThan(withoutThinking);
  });

  it("should include tool calls tokens", () => {
    const messages: LLMMessage[] = [
      {
        role: "assistant",
        content: "Let me search",
        toolCalls: [
          {
            id: "call-1",
            type: "function",
            function: { name: "search", arguments: '{"q":"test"}' },
          },
        ],
      },
    ];
    const tokens = estimateTokens(messages);
    expect(tokens).toBeGreaterThan(0);
  });

  it("should add 4 overhead tokens per message", () => {
    const singleMessage = estimateTokens([{ role: "user", content: "" }]);
    const twoMessages = estimateTokens([
      { role: "user", content: "" },
      { role: "assistant", content: "" },
    ]);
    // Single message: 0 (empty content) + 4 = 4
    // Two messages: 0 + 0 + 8 = 8
    expect(singleMessage).toBe(4);
    expect(twoMessages).toBe(8);
  });

  it("should handle complex message content", () => {
    const messages: LLMMessage[] = [
      {
        role: "user",
        content: [
          { type: "text", text: "Describe this image" },
          { type: "image", source: { type: "base64", data: "abc" } },
        ],
      },
    ];
    const tokens = estimateTokens(messages);
    expect(tokens).toBeGreaterThan(0);
  });
});

describe("getTokenUsage", () => {
  it("should prioritize API usage when available", () => {
    const usage: LLMUsage = {
      promptTokens: 10,
      completionTokens: 20,
      totalTokens: 30,
    };
    const messages: LLMMessage[] = [
      { role: "user", content: "very long message that would be estimated as more than 30 tokens" },
    ];
    const result = getTokenUsage(usage, messages);
    expect(result).toBe(30);
  });

  it("should fall back to local estimation when usage is null", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "Hello world" },
    ];
    const result = getTokenUsage(null, messages);
    // Should use estimateTokens which counts content + overhead
    expect(result).toBeGreaterThan(0);
  });

  it("should work with undefined usage", () => {
    const messages: LLMMessage[] = [
      { role: "user", content: "test" },
    ];
    const result = getTokenUsage(undefined as unknown as null, messages);
    expect(result).toBeGreaterThan(0);
  });

  it("should return 0 for empty messages with null usage", () => {
    const result = getTokenUsage(null, []);
    expect(result).toBe(0);
  });
});

describe("isTokenLimitExceeded", () => {
  it("should return true when tokens used exceed limit", () => {
    expect(isTokenLimitExceeded(100, 50)).toBe(true);
  });

  it("should return false when tokens used are within limit", () => {
    expect(isTokenLimitExceeded(50, 100)).toBe(false);
  });

  it("should return false when tokens used equal limit", () => {
    // isTokenLimitExceeded uses > (strictly greater than)
    expect(isTokenLimitExceeded(100, 100)).toBe(false);
  });

  it("should handle zero token limit", () => {
    expect(isTokenLimitExceeded(0, 0)).toBe(false);
    expect(isTokenLimitExceeded(1, 0)).toBe(true);
  });
});