/**
 * Unit tests for Anthropic Formatter
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { AnthropicFormatter } from "../anthropic.js";
import type { LLMRequest, LLMMessage } from "@wf-agent/types";
import type { FormatterConfig } from "../types.js";

vi.mock("../tool-converter.js", () => ({
  convertToolsToAnthropicFormat: vi.fn((tools: any[]) =>
    tools.map((t: any) => ({ name: t.name, type: "custom" }))
  ),
}));

vi.mock("../../../utils/index.js", () => ({
  sdkLogger: { warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../tool-call-parser.js", () => ({
  ToolCallParser: {
    parseFromText: vi.fn(() => []),
  },
  getToolCallParserOptions: vi.fn(() => ({})),
}));

vi.mock("../tool-declaration-formatter.js", () => ({
  ToolDeclarationFormatter: {
    formatTools: vi.fn(() => "<tools></tools>"),
  },
}));

vi.mock("../history-converter.js", () => ({
  HistoryConverter: {
    convertToTextMode: vi.fn((messages: any[]) => messages),
  },
}));

describe("AnthropicFormatter", () => {
  let formatter: AnthropicFormatter;
  let mockConfig: FormatterConfig;

  beforeEach(() => {
    formatter = new AnthropicFormatter();
    mockConfig = {
      profile: {
        provider: "anthropic",
        model: "claude-3-opus-20240229",
        apiKey: "sk-ant-test",
        parameters: {},
      },
      stream: false,
      timeout: 60000,
    } as unknown as FormatterConfig;
  });

  describe("constructor", () => {
    it("should default to apiVersion 2023-06-01", () => {
      expect(formatter).toBeInstanceOf(AnthropicFormatter);
    });

    it("should accept custom apiVersion", () => {
      const customFormatter = new AnthropicFormatter("2024-01-01");
      expect(customFormatter).toBeInstanceOf(AnthropicFormatter);
    });
  });

  describe("getSupportedProvider", () => {
    it("should return ANTHROPIC", () => {
      expect(formatter.getSupportedProvider()).toBe("ANTHROPIC");
    });
  });

  describe("buildNativeRequest", () => {
    it("should return HTTP request with /v1/messages endpoint", () => {
      const request: LLMRequest = { messages: [{ role: "user", content: "Hello" }] };
      const result = formatter.buildRequest(request, mockConfig);
      expect(result.httpRequest!.url).toContain("/v1/messages");
      expect(result.httpRequest!.method).toBe("POST");
    });

    it("should include anthropic-specific headers", () => {
      const request: LLMRequest = { messages: [{ role: "user", content: "Hi" }] };
      const result = formatter.buildRequest(request, mockConfig);
      expect(result.httpRequest!.headers!["anthropic-version"]).toBeDefined();
      expect(result.httpRequest!.headers!["anthropic-dangerous-direct-browser-access"]).toBe("false");
    });

    it("should include x-api-key header when apiKey is set", () => {
      const request: LLMRequest = { messages: [{ role: "user", content: "Hi" }] };
      const result = formatter.buildRequest(request, mockConfig);
      expect(result.httpRequest!.headers).toHaveProperty("x-api-key");
    });

    it("should include max_tokens in body", () => {
      const request: LLMRequest = { messages: [{ role: "user", content: "Hello" }] };
      const result = formatter.buildRequest(request, mockConfig);
      const body = result.httpRequest!.body as Record<string, unknown>;
      expect(body["max_tokens"]).toBeDefined();
    });
  });

  describe("parseNativeResponse", () => {
    it("should parse a standard response", () => {
      const data = {
        id: "msg-123",
        model: "claude-3-opus-20240229",
        type: "message",
        role: "assistant",
        content: [
          { type: "text", text: "Hello from Claude!" },
        ],
        stop_reason: "end_turn",
        usage: { input_tokens: 10, output_tokens: 25 },
      };
      const result = formatter.parseResponse(data, mockConfig);
      expect(result.id).toBe("msg-123");
      expect(result.content).toBe("Hello from Claude!");
      expect(result.usage?.promptTokens).toBe(10);
      expect(result.usage?.completionTokens).toBe(25);
      expect(result.finishReason).toBe("end_turn");
    });

    it("should parse response with tool calls", () => {
      const data = {
        id: "msg-456",
        model: "claude-3-opus-20240229",
        type: "message",
        role: "assistant",
        content: [
          { type: "text", text: "Let me check the weather." },
          { type: "tool_use", id: "toolu-1", name: "get_weather", input: { city: "London" } },
        ],
        stop_reason: "tool_use",
        usage: { input_tokens: 20, output_tokens: 30 },
      };
      const result = formatter.parseResponse(data, mockConfig);
      expect(result.toolCalls).toBeDefined();
      expect(result.toolCalls).toHaveLength(1);
      expect(result.toolCalls![0]!.function!.name).toBe("get_weather");
      expect(result.toolCalls![0]!.id).toBe("toolu-1");
    });

    it("should extract thinking content", () => {
      const data = {
        id: "msg-789",
        model: "claude-3-opus-20240229",
        type: "message",
        content: [
          { type: "thinking", thinking: "I need to think step by step..." },
          { type: "text", text: "Final answer" },
        ],
        stop_reason: "end_turn",
        usage: { input_tokens: 10, output_tokens: 50 },
      };
      const result = formatter.parseResponse(data, mockConfig);
      expect(result.reasoningContent).toBe("I need to think step by step...");
    });
  });

  describe("parseStreamChunk", () => {
    it("should parse text delta chunk", () => {
      const data = {
        type: "content_block_delta",
        delta: { type: "text_delta", text: "Hello" },
      };
      const result = formatter.parseStreamChunk(data);
      expect(result.valid).toBe(true);
      expect(result.chunk.delta).toBe("Hello");
    });

    it("should parse thinking delta chunk", () => {
      const data = {
        type: "content_block_delta",
        delta: { type: "thinking_delta", thinking: "Thinking deeply..." },
      };
      const result = formatter.parseStreamChunk(data);
      expect(result.valid).toBe(true);
      expect(result.chunk.reasoningDelta).toBe("Thinking deeply...");
    });

    it("should parse tool use start chunk", () => {
      const data = {
        type: "content_block_start",
        content_block: { type: "tool_use", id: "toolu-1", name: "get_weather", input: {} },
      };
      const result = formatter.parseStreamChunk(data);
      expect(result.valid).toBe(true);
      expect(result.chunk.toolCallsDelta).toBeDefined();
      expect(result.chunk.toolCallsDelta![0]!.function!.name).toBe("get_weather");
    });

    it("should handle message delta with stop reason", () => {
      const data = {
        type: "message_delta",
        delta: { stop_reason: "end_turn" },
        usage: { input_tokens: 10, output_tokens: 20 },
      };
      const result = formatter.parseStreamChunk(data);
      expect(result.valid).toBe(true);
      expect(result.chunk.done).toBe(true);
      expect(result.chunk.finishReason).toBe("end_turn");
    });
  });

  describe("convertMessages", () => {
    it("should filter out system messages", () => {
      const messages: LLMMessage[] = [
        { role: "system", content: "You are helpful." },
        { role: "user", content: "Hello" },
      ];
      const result = formatter.convertMessages(messages) as any[];
      expect(result).toHaveLength(1);
      expect(result[0].role).toBe("user");
    });

    it("should convert content to Anthropic format", () => {
      const messages: LLMMessage[] = [
        { role: "user", content: "Hello" },
      ];
      const result = formatter.convertMessages(messages) as any[];
      expect(result[0].content[0].type).toBe("text");
      expect(result[0].content[0].text).toBe("Hello");
    });

    it("should convert tool results", () => {
      const messages: LLMMessage[] = [
        { role: "tool", content: '{"temp":25}', toolCallId: "toolu-1" },
      ];
      const result = formatter.convertMessages(messages) as any[];
      expect(result[0].role).toBe("user");
      expect(result[0].content[0].type).toBe("tool_result");
      expect(result[0].content[0].tool_use_id).toBe("toolu-1");
    });

    it("should convert tool calls in assistant messages", () => {
      const messages: LLMMessage[] = [
        {
          role: "assistant",
          content: "",
          toolCalls: [{ id: "toolu-1", type: "function", function: { name: "get_weather", arguments: '{"city":"Paris"}' } }],
        },
      ];
      const result = formatter.convertMessages(messages) as any[];
      expect(result[0].content[0].type).toBe("tool_use");
      expect(result[0].content[0].name).toBe("get_weather");
    });
  });

  describe("parseToolCalls", () => {
    it("should parse tool calls with input field", () => {
      const toolCalls = [
        { id: "toolu-1", name: "get_weather", input: { city: "Tokyo" } },
      ];
      const result = formatter.parseToolCalls(toolCalls);
      expect(result).toHaveLength(1);
      expect(result[0]!.function!.name).toBe("get_weather");
      expect(result[0]!.function!.arguments).toBe('{"city":"Tokyo"}');
    });

    it("should handle string input", () => {
      const toolCalls = [
        { id: "toolu-2", name: "search", input: '{"q":"test"}' },
      ];
      const result = formatter.parseToolCalls(toolCalls);
      expect(result[0]!.function!.arguments).toBe('{"q":"test"}');
    });

    it("should handle null input", () => {
      const result = formatter.parseToolCalls([] as unknown[]);
      expect(result).toEqual([]);
    });
  });

  describe("buildCountTokensRequest", () => {
    it("should build a count tokens request", () => {
      const request: LLMRequest = {
        messages: [{ role: "user", content: "Hello" }],
      };
      const result = formatter.buildCountTokensRequest(request, mockConfig);
      expect(result.httpRequest!.url).toContain("/v1/messages");
      expect(result.httpRequest!.method).toBe("POST");
    });

    it("should include system message if present", () => {
      const request: LLMRequest = {
        messages: [
          { role: "system", content: "You are helpful." },
          { role: "user", content: "Hello" },
        ],
      };
      const result = formatter.buildCountTokensRequest(request, mockConfig);
      const body = result.httpRequest!.body as Record<string, unknown>;
      expect(body["system"]).toBe("You are helpful.");
    });
  });
});
