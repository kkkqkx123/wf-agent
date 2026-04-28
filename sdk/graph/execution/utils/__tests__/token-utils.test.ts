/**
 * Unit Tests for TokenUtils
 * Testing the Token utility functions
 */

import { describe, it, expect } from "vitest";
import {
  estimateTokens,
  getTokenUsage,
  isTokenLimitExceeded,
} from "../../../../core/utils/token/token-utils.js";
import type { LLMMessage, LLMUsage } from "@wf-agent/types";

describe("estimateTokens", () => {
  describe("Base Message Count", () => {
    it("The number of tokens for simple messages should be correctly estimated", () => {
      const messages: LLMMessage[] = [{ role: "user", content: "Hello" }];

      const tokens = estimateTokens(messages);

      // "Hello" ≈ 1-2 tokens (Latin estimation), Metadata overhead = 4 tokens ≈ 5-6 tokens
      expect(tokens).toBeGreaterThanOrEqual(5);
    });

    it("The number of tokens for multiple messages should be correctly estimated", () => {
      const messages: LLMMessage[] = [
        { role: "user", content: "Hello" },
        { role: "assistant", content: "Hi there" },
      ];

      const tokens = estimateTokens(messages);

      // Each message has metadata overhead of at least 4 tokens.
      expect(tokens).toBeGreaterThanOrEqual(10);
    });

    it("An empty message array returns 0", () => {
      const messages: LLMMessage[] = [];

      const tokens = estimateTokens(messages);

      expect(tokens).toBe(0);
    });
  });

  describe("content type handling", () => {
    it("The string content should be handled correctly.", () => {
      const messages: LLMMessage[] = [
        { role: "user", content: "This is a test message with some words" },
      ];

      const tokens = estimateTokens(messages);

      expect(tokens).toBeGreaterThan(4); // At least the content + metadata
    });

    it("Arrays of content should be handled correctly", () => {
      const messages: LLMMessage[] = [
        {
          role: "user",
          content: [
            { type: "text", text: "Hello" },
            { type: "text", text: "World" },
            { type: "text", text: "Test" },
          ],
        },
      ];

      const tokens = estimateTokens(messages);

      expect(tokens).toBeGreaterThan(4);
    });

    it("Empty content should be handled correctly", () => {
      const messages: LLMMessage[] = [{ role: "user", content: "" }];

      const tokens = estimateTokens(messages);

      // Empty string, 0 tokens + 4 metadata = 4
      expect(tokens).toBeGreaterThanOrEqual(4);
    });
  });

  describe("thinking Content counting", () => {
    it("Thinking content should be counted", () => {
      const messages: LLMMessage[] = [
        {
          role: "assistant",
          content: "Answer",
          thinking: "Let me think about this step by step...",
        },
      ];

      const tokens = estimateTokens(messages);

      // The content within 'thinking' should be counted.
      expect(tokens).toBeGreaterThan(4);
    });

    it("Messages without thinking should be handled correctly", () => {
      const messages: LLMMessage[] = [{ role: "assistant", content: "Answer" }];

      const tokens = estimateTokens(messages);

      expect(tokens).toBeGreaterThan(0);
    });
  });

  describe("toolCalls Count", () => {
    it("The toolCalls structure should be counted", () => {
      const messages: LLMMessage[] = [
        {
          role: "assistant",
          content: "Let me call a tool",
          toolCalls: [
            {
              id: "call-1",
              type: "function",
              function: {
                name: "testFunction",
                arguments: '{"arg": "value"}',
              },
            },
          ],
        },
      ];

      const tokens = estimateTokens(messages);

      // The `toolCalls` should have an increased number of tokens.
      expect(tokens).toBeGreaterThan(4);
    });

    it("Multiple toolCalls should be handled correctly", () => {
      const messages: LLMMessage[] = [
        {
          role: "assistant",
          content: "Let me call tools",
          toolCalls: [
            {
              id: "call-1",
              type: "function",
              function: { name: "func1", arguments: "{}" },
            },
            {
              id: "call-2",
              type: "function",
              function: { name: "func2", arguments: "{}" },
            },
          ],
        },
      ];

      const tokens = estimateTokens(messages);

      expect(tokens).toBeGreaterThan(4);
    });

    it("Messages without toolCalls should be handled correctly", () => {
      const messages: LLMMessage[] = [{ role: "assistant", content: "No tool calls" }];

      const tokens = estimateTokens(messages);

      expect(tokens).toBeGreaterThan(0);
    });
  });

  describe("metadata overhead", () => {
    it("There should be a metadata overhead of about 4 tokens per message", () => {
      const messages: LLMMessage[] = [
        { role: "user", content: "" },
        { role: "assistant", content: "" },
        { role: "user", content: "" },
      ];

      const tokens = estimateTokens(messages);

      // 3 messages, with at least 4 tokens of metadata per message = 12 tokens
      expect(tokens).toBeGreaterThanOrEqual(12);
    });
  });

  describe("CJK content estimation", () => {
    it("CJK characters should be estimated correctly", () => {
      const messages: LLMMessage[] = [{ role: "user", content: "你好世界" }];

      const tokens = estimateTokens(messages);

      // 4 CJK characters ≈ 4 tokens + 4 metadata ≈ 8 tokens
      expect(tokens).toBeGreaterThanOrEqual(8);
    });

    it("Mixed CJK and Latin content should be estimated correctly", () => {
      const messages: LLMMessage[] = [{ role: "user", content: "Hello. 世界." }];

      const tokens = estimateTokens(messages);

      // 5 Latin ≈ 1-2 tokens + 2 CJK ≈ 2 tokens + 4 metadata ≈ 7-8 tokens
      expect(tokens).toBeGreaterThanOrEqual(7);
    });
  });
});

describe("getTokenUsage", () => {
  it("Prioritize API statistics when providing usage", () => {
    const usage: LLMUsage = {
      promptTokens: 10,
      completionTokens: 5,
      totalTokens: 15,
    };
    const messages: LLMMessage[] = [{ role: "user", content: "Test" }];

    const tokens = getTokenUsage(usage, messages);

    expect(tokens).toBe(15);
  });

  it("When there is no usage, the local estimate is used", () => {
    const messages: LLMMessage[] = [{ role: "user", content: "Test message" }];

    const tokens = getTokenUsage(null, messages);

    expect(tokens).toBeGreaterThan(0);
  });

  it("When usage is undefined, the local estimate is used.", () => {
    const messages: LLMMessage[] = [{ role: "user", content: "Test message" }];

    const tokens = getTokenUsage(undefined as any, messages);

    expect(tokens).toBeGreaterThan(0);
  });

  it("Local estimates should be consistent with estimateTokens results", () => {
    const messages: LLMMessage[] = [{ role: "user", content: "Test message for comparison" }];

    const usageTokens = getTokenUsage(null, messages);
    const estimatedTokens = estimateTokens(messages);

    expect(usageTokens).toBe(estimatedTokens);
  });
});

describe("isTokenLimitExceeded", () => {
  it("Returns true when tokensUsed exceeds tokenLimit.", () => {
    expect(isTokenLimitExceeded(100, 50)).toBe(true);
    expect(isTokenLimitExceeded(51, 50)).toBe(true);
  });

  it("Returns false when tokensUsed equals tokenLimit.", () => {
    expect(isTokenLimitExceeded(50, 50)).toBe(false);
  });

  it("Returns false if tokensUsed is less than tokenLimit.", () => {
    expect(isTokenLimitExceeded(49, 50)).toBe(false);
    expect(isTokenLimitExceeded(0, 50)).toBe(false);
  });

  it("Correct handling of boundary values", () => {
    expect(isTokenLimitExceeded(0, 0)).toBe(false);
    expect(isTokenLimitExceeded(1, 0)).toBe(true);
  });

  it("Handling large values correctly", () => {
    expect(isTokenLimitExceeded(1000000, 500000)).toBe(true);
    expect(isTokenLimitExceeded(500000, 1000000)).toBe(false);
  });
});

describe("integration test", () => {
  it("The complete token estimation process", () => {
    const messages: LLMMessage[] = [
      { role: "system", content: "You are a helpful assistant" },
      { role: "user", content: "Hello, how are you?" },
      {
        role: "assistant",
        content: "I am doing well, thank you!",
        thinking: "The user is greeting me",
      },
    ];

    const tokens = estimateTokens(messages);

    // Verify that the total number of tokens is reasonable.
    expect(tokens).toBeGreaterThan(0);
    expect(tokens).toBeLessThan(1000); // reasonable upper limit
  });

  it("Token limit checking combined with estimation", () => {
    const messages: LLMMessage[] = [{ role: "user", content: "A very long message ".repeat(100) }];

    const tokens = getTokenUsage(null, messages);
    const limit = 500;

    // Long messages may exceed the limit.
    const isExceeded = isTokenLimitExceeded(tokens, limit);

    // Verify logical consistency.
    if (tokens > limit) {
      expect(isExceeded).toBe(true);
    } else {
      expect(isExceeded).toBe(false);
    }
  });
});
