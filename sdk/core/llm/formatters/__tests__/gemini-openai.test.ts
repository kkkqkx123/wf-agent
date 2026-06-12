/**
 * Unit tests for Gemini OpenAI Compatible Formatter
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { GeminiOpenAIFormatter } from "../gemini-openai.js";
import type { LLMRequest, LLMMessage } from "@wf-agent/types";
import type { FormatterConfig } from "../types.js";

vi.mock("../tool-converter.js", () => ({
  convertToolsToOpenAIFormat: vi.fn((tools: any[]) =>
    tools.map((t: any) => ({ type: "function", function: { name: t.name } })),
  ),
}));

vi.mock("../../../utils/index.js", () => ({
  sdkLogger: { warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("@wf-agent/common-utils", () => ({
  generateId: vi.fn(() => "mock-id-123"),
}));

describe("GeminiOpenAIFormatter", () => {
  let formatter: GeminiOpenAIFormatter;
  let mockConfig: FormatterConfig;

  beforeEach(() => {
    formatter = new GeminiOpenAIFormatter();
    mockConfig = {
      profile: {
        provider: "gemini_openai",
        model: "gemini-1.5-pro",
        apiKey: "test-key",
        parameters: {},
      },
      stream: false,
      timeout: 30000,
    } as unknown as FormatterConfig;
  });

  describe("getSupportedProvider", () => {
    it("should return GEMINI_OPENAI", () => {
      expect(formatter.getSupportedProvider()).toBe("GEMINI_OPENAI");
    });
  });

  describe("buildNativeRequest", () => {
    it("should return HTTP request with /chat/completions endpoint", () => {
      const request: LLMRequest = { messages: [{ role: "user", content: "Hello" }] };
      const result = formatter.buildRequest(request, mockConfig);
      expect(result.httpRequest.url).toContain("/chat/completions");
      expect(result.httpRequest.method).toBe("POST");
    });

    it("should include Authorization header when apiKey is set", () => {
      const request: LLMRequest = { messages: [{ role: "user", content: "Hi" }] };
      const result = formatter.buildRequest(request, mockConfig);
      expect(result.httpRequest.headers).toHaveProperty("Authorization");
    });
  });

  describe("parseNativeResponse", () => {
    it("should parse a valid response with text content", () => {
      const data = {
        id: "gemini-123",
        candidates: [
          {
            content: {
              parts: [{ text: "Hello from Gemini!" }],
            },
            finishReason: "STOP",
          },
        ],
        usageMetadata: { promptTokenCount: 10, candidatesTokenCount: 20, totalTokenCount: 30 },
      };
      const result = formatter.parseResponse(data, mockConfig);
      expect(result.id).toBe("gemini-123");
      expect(result.content).toBe("Hello from Gemini!");
      expect(result.usage?.promptTokens).toBe(10);
      expect(result.usage?.completionTokens).toBe(20);
    });

    it("should throw error when no candidates", () => {
      const data = { id: "gemini-456", candidates: [] };
      expect(() => formatter.parseResponse(data, mockConfig)).toThrow("No candidate in response");
    });

    it("should parse response with function calls", () => {
      const data = {
        candidates: [
          {
            content: {
              parts: [{ functionCall: { name: "get_weather", args: { city: "Tokyo" } } }],
            },
            finishReason: "STOP",
          },
        ],
      };
      const result = formatter.parseResponse(data, mockConfig);
      expect(result.toolCalls).toBeDefined();
      expect(result.toolCalls).toHaveLength(1);
      expect(result.toolCalls![0]!.function!.name).toBe("get_weather");
    });
  });

  describe("parseStreamChunk", () => {
    it("should parse streaming chunk with text", () => {
      const data = {
        candidates: [
          {
            content: { parts: [{ text: "Hello" }] },
            finishReason: "",
          },
        ],
      };
      const result = formatter.parseStreamChunk(data);
      expect(result.valid).toBe(true);
      expect(result.chunk.delta).toBe("Hello");
      expect(result.chunk.done).toBe(false);
    });

    it("should mark as done for terminal finish reasons", () => {
      const data = {
        candidates: [
          {
            content: { parts: [{ text: "Done" }] },
            finishReason: "STOP",
          },
        ],
      };
      const result = formatter.parseStreamChunk(data);
      expect(result.valid).toBe(true);
      expect(result.chunk.done).toBe(true);
    });

    it("should return valid=false for empty candidates", () => {
      const data = { candidates: [] };
      const result = formatter.parseStreamChunk(data);
      expect(result.valid).toBe(false);
    });
  });

  describe("convertMessages", () => {
    it("should convert messages to Gemini format", () => {
      const messages: LLMMessage[] = [
        { role: "system", content: "You are helpful." },
        { role: "user", content: "Hello" },
        { role: "assistant", content: "Hi" },
      ];
      const result = formatter.convertMessages(messages) as any[];
      // System messages are filtered (Gemini uses systemInstruction)
      expect(result).toHaveLength(2);
      expect(result[0].role).toBe("user");
      expect(result[1].role).toBe("model");
    });

    it("should convert tool results", () => {
      const messages: LLMMessage[] = [
        { role: "tool", content: "result data", toolCallId: "func-1" },
      ];
      const result = formatter.convertMessages(messages) as any[];
      expect(result[0].role).toBe("user");
      expect(result[0].content[0].functionResponse).toBeDefined();
      expect(result[0].content[0].functionResponse.name).toBe("func-1");
    });

    it("should convert tool calls in assistant messages", () => {
      const messages: LLMMessage[] = [
        {
          role: "assistant",
          content: "",
          toolCalls: [
            { id: "call-1", type: "function", function: { name: "test", arguments: '{"x":1}' } },
          ],
        },
      ];
      const result = formatter.convertMessages(messages) as any[];
      expect(result[0].content[0].functionCall).toBeDefined();
      expect(result[0].content[0].functionCall.name).toBe("test");
    });
  });

  describe("parseToolCalls", () => {
    it("should parse function-call-style tool calls", () => {
      const toolCalls = [{ functionCall: { name: "search", args: { q: "hello" } } }];
      const result = formatter.parseToolCalls(toolCalls);
      expect(result).toHaveLength(1);
      expect(result[0]!.function.name).toBe("search");
    });

    it("should parse flat-style tool calls", () => {
      const toolCalls = [{ id: "call-1", name: "get_info", args: '{"id":42}' }];
      const result = formatter.parseToolCalls(toolCalls);
      expect(result[0]!.function.name).toBe("get_info");
    });

    it("should handle empty input", () => {
      const result = formatter.parseToolCalls([]);
      expect(result).toEqual([]);
    });
  });
});
