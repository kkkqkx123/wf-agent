/**
 * Unit tests for Gemini Native Formatter
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { GeminiNativeFormatter } from "../gemini-native.js";
import type { LLMRequest, LLMMessage } from "@wf-agent/types";
import type { FormatterConfig } from "../types.js";

vi.mock("../tool-converter.js", () => ({
  convertToolsToGeminiFormat: vi.fn((tools: any[]) =>
    tools.map((t: any) => ({ name: t.name }))
  ),
}));

vi.mock("../../../utils/index.js", () => ({
  sdkLogger: { warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("@wf-agent/common-utils", () => ({
  generateId: vi.fn(() => "mock-id-456"),
}));

describe("GeminiNativeFormatter", () => {
  let formatter: GeminiNativeFormatter;
  let mockConfig: FormatterConfig;

  beforeEach(() => {
    formatter = new GeminiNativeFormatter();
    mockConfig = {
      profile: {
        provider: "gemini_native",
        model: "gemini-2.0-flash",
        apiKey: "test-key",
        parameters: {},
      },
      stream: false,
      timeout: 30000,
    } as unknown as FormatterConfig;
  });

  describe("getSupportedProvider", () => {
    it("should return GEMINI_NATIVE", () => {
      expect(formatter.getSupportedProvider()).toBe("GEMINI_NATIVE");
    });
  });

  describe("buildNativeRequest", () => {
    it("should build request with generateContent endpoint", () => {
      const request: LLMRequest = { messages: [{ role: "user", content: "Hello" }] };
      const result = formatter.buildRequest(request, mockConfig);
      expect(result.httpRequest.url).toContain(":generateContent");
      expect(result.httpRequest.method).toBe("POST");
    });

    it("should include apiKey in query params", () => {
      const request: LLMRequest = { messages: [{ role: "user", content: "Hi" }] };
      const result = formatter.buildRequest(request, mockConfig);
      expect(result.httpRequest.query).toBeDefined();
      expect((result.httpRequest.query as any).key).toBe("test-key");
    });

    it("should include alt=sse in query params for streaming", () => {
      const streamConfig: FormatterConfig = { ...mockConfig, stream: true };
      const request: LLMRequest = { messages: [{ role: "user", content: "Stream" }] };
      const result = formatter.buildRequest(request, streamConfig);
      expect((result.httpRequest.query as any).alt).toBe("sse");
    });

    it("should build request with streamGenerateContent endpoint for streaming", () => {
      const streamConfig: FormatterConfig = { ...mockConfig, stream: true };
      const request: LLMRequest = { messages: [{ role: "user", content: "Stream" }] };
      const result = formatter.buildRequest(request, streamConfig);
      expect(result.httpRequest.url).toContain(":streamGenerateContent");
    });
  });

  describe("parseNativeResponse", () => {
    it("should parse a standard response", () => {
      const data = {
        candidates: [
          {
            content: {
              parts: [{ text: "Hello from Gemini Native!" }],
            },
            finishReason: "STOP",
            safetyRatings: [],
          },
        ],
        usageMetadata: { promptTokenCount: 5, candidatesTokenCount: 10, totalTokenCount: 15 },
        id: "gemini-native-1",
      };
      const result = formatter.parseResponse(data, mockConfig);
      expect(result.content).toBe("Hello from Gemini Native!");
      expect(result.usage?.promptTokens).toBe(5);
      expect(result.usage?.completionTokens).toBe(10);
      expect(result.finishReason).toBe("STOP");
    });

    it("should throw error when no candidates", () => {
      const data = { candidates: [] };
      expect(() => formatter.parseResponse(data, mockConfig)).toThrow("No candidate in response");
    });

    it("should parse response with function calls", () => {
      const data = {
        candidates: [
          {
            content: {
              parts: [
                { functionCall: { name: "get_weather", args: { city: "Paris" } } },
              ],
            },
            finishReason: "STOP",
          },
        ],
      };
      const result = formatter.parseResponse(data, mockConfig);
      expect(result.toolCalls).toBeDefined();
      expect(result.toolCalls).toHaveLength(1);
      expect(result.toolCalls![0]!.function.name).toBe("get_weather");
    });

    it("should extract thinking content", () => {
      const data = {
        candidates: [
          {
            content: {
              parts: [
                { text: "answer", thought: true },
                { text: "Final answer" },
              ],
            },
            finishReason: "STOP",
          },
        ],
        usageMetadata: { thoughtsTokenCount: 50 },
      };
      const result = formatter.parseResponse(data, mockConfig);
      expect(result.reasoningContent).toBeDefined();
      expect(result.reasoningTokens).toBe(50);
    });
  });

  describe("parseStreamLine", () => {
    it("should parse raw JSON line (Gemini Native returns JSON directly)", () => {
      const line = '{"candidates":[{"content":{"parts":[{"text":"Hello"}]}}]}';
      const result = formatter.parseStreamLine(line, mockConfig);
      expect(result.valid).toBe(true);
      expect(result.chunk.delta).toBe("Hello");
    });

    it("should ignore invalid JSON lines", () => {
      const line = "event: test";
      const result = formatter.parseStreamLine(line, mockConfig);
      expect(result.valid).toBe(false);
    });

    it("should skip empty lines", () => {
      const line = "";
      const result = formatter.parseStreamLine(line, mockConfig);
      expect(result.valid).toBe(false);
    });
  });

  describe("parseStreamChunk", () => {
    it("should parse streaming chunk", () => {
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
      expect(result).toHaveLength(2);
      expect(result[0].role).toBe("user");
      expect(result[0].parts[0].text).toBe("Hello");
      expect(result[1].role).toBe("model");
    });

    it("should convert tool results", () => {
      const messages: LLMMessage[] = [
        { role: "tool", content: "result", toolCallId: "func-1" },
      ];
      const result = formatter.convertMessages(messages) as any[];
      expect(result[0].role).toBe("user");
      expect(result[0].parts[0].functionResponse.name).toBe("func-1");
    });

    it("should convert tool calls in assistant messages", () => {
      const messages: LLMMessage[] = [
        {
          role: "assistant",
          content: "",
          toolCalls: [{ id: "call-1", type: "function", function: { name: "test", arguments: '{"x":1}' } }],
        },
      ];
      const result = formatter.convertMessages(messages) as any[];
      expect(result[0].parts[0].functionCall).toBeDefined();
      expect(result[0].parts[0].functionCall.name).toBe("test");
    });
  });

  describe("parseToolCalls", () => {
    it("should parse function-call-style tool calls", () => {
      const toolCalls = [
        { functionCall: { name: "search", args: { q: "test" } } },
      ];
      const result = formatter.parseToolCalls(toolCalls);
      expect(result[0]!.function.name).toBe("search");
    });

    it("should handle empty input", () => {
      const result = formatter.parseToolCalls([]);
      expect(result).toEqual([]);
    });
  });
});
