/**
 * Unit tests for OpenAI Response Formatter
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { OpenAIResponseFormatter } from "../openai-response.js";
import type { LLMRequest, LLMMessage } from "@wf-agent/types";
import type { FormatterConfig } from "../types.js";

vi.mock("./tool-converter.js", () => ({
  convertToolsToOpenAIFormat: vi.fn((tools: any[]) =>
    tools.map((t: any) => ({ type: "function", function: { name: t.name } })),
  ),
}));

vi.mock("../../../utils/index.js", () => ({
  sdkLogger: { warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

describe("OpenAIResponseFormatter", () => {
  let formatter: OpenAIResponseFormatter;
  let mockConfig: FormatterConfig;

  beforeEach(() => {
    formatter = new OpenAIResponseFormatter();
    mockConfig = {
      profile: {
        provider: "openai_response",
        model: "gpt-4o",
        apiKey: "test-key",
        parameters: {},
      },
      stream: false,
      timeout: 30000,
    } as unknown as FormatterConfig;
  });

  describe("getSupportedProvider", () => {
    it("should return OPENAI_RESPONSE", () => {
      expect(formatter.getSupportedProvider()).toBe("OPENAI_RESPONSE");
    });
  });

  describe("buildNativeRequest", () => {
    it("should return HTTP request with /responses endpoint", () => {
      const request: LLMRequest = {
        messages: [{ role: "user", content: "Hello" }],
      };
      const result = formatter.buildRequest(request, mockConfig);
      expect(result.httpRequest.url).toContain("/responses");
      expect(result.httpRequest.method).toBe("POST");
      expect(result.httpRequest.headers!["Content-Type"]).toBe("application/json");
    });

    it("should include Authorization header when apiKey is set", () => {
      const request: LLMRequest = { messages: [{ role: "user", content: "Hi" }] };
      const result = formatter.buildRequest(request, mockConfig);
      expect(result.httpRequest!.headers).toHaveProperty("Authorization");
    });

    it("should include tools in body when provided", () => {
      const request: LLMRequest = {
        messages: [{ role: "user", content: "Use a tool" }],
        tools: [
          {
            id: "tool-1",
            description: "A test tool",
            parameters: { type: "object", properties: {}, required: [] },
          },
        ],
      };
      const result = formatter.buildRequest(request, mockConfig);
      const body = result.httpRequest.body as Record<string, unknown>;
      expect(body["tools"]).toBeDefined();
    });

    it("should use `input` field instead of `messages` in request body", () => {
      const request: LLMRequest = {
        messages: [{ role: "user", content: "Hello" }],
      };
      const result = formatter.buildRequest(request, mockConfig);
      const body = result.httpRequest.body as Record<string, unknown>;
      expect(body["input"]).toBeDefined();
      expect(body["messages"]).toBeUndefined();
    });
  });

  describe("parseNativeResponse", () => {
    it("should parse a valid response", () => {
      const data = {
        id: "resp-123",
        model: "gpt-4o",
        output: [
          {
            content: [{ text: "Hello there!" }],
          },
        ],
        usage: { input_tokens: 10, output_tokens: 20 },
        status: "completed",
      };
      const result = formatter.parseResponse(data, mockConfig);
      expect(result.id).toBe("resp-123");
      expect(result.content).toBe("Hello there!");
      expect(result.message.content).toBe("Hello there!");
      expect(result.usage?.promptTokens).toBe(10);
      expect(result.usage?.completionTokens).toBe(20);
      expect(result.finishReason).toBe("completed");
    });

    it("should parse response with tool calls", () => {
      const data = {
        id: "resp-456",
        model: "gpt-4o",
        output: [
          {
            content: [{ text: "" }],
            tool_calls: [
              {
                id: "call-1",
                type: "function",
                function: { name: "get_weather", arguments: '{"city":"London"}' },
              },
            ],
          },
        ],
        status: "completed",
      };
      const result = formatter.parseResponse(data, mockConfig);
      expect(result.toolCalls).toBeDefined();
      expect(result.toolCalls).toHaveLength(1);
      expect(result.toolCalls![0]!.function.name).toBe("get_weather");
    });

    it("should handle empty output array", () => {
      const data = { id: "resp-789", model: "gpt-4o", output: [], status: "completed" };
      const result = formatter.parseResponse(data, mockConfig);
      expect(result.content).toBe("");
    });
  });

  describe("parseStreamChunk", () => {
    it("should parse streaming chunk with text delta", () => {
      const data = {
        type: "response.output_text.delta",
        output: [{ content: [{ text: "Hello" }] }],
        status: "in_progress",
      };
      const result = formatter.parseStreamChunk(data);
      expect(result.valid).toBe(true);
      expect(result.chunk.delta).toBe("Hello");
      expect(result.chunk.done).toBe(false);
    });

    it("should mark as done when status is completed", () => {
      const data = {
        type: "response.completed",
        output: [],
        status: "completed",
      };
      const result = formatter.parseStreamChunk(data);
      expect(result.valid).toBe(true);
      expect(result.chunk.done).toBe(true);
    });

    it("should extract tool calls delta from streaming chunk", () => {
      const data = {
        type: "response.output_item.done",
        output: [
          {
            tool_calls: [
              {
                id: "call-1",
                type: "function",
                function: { name: "get_weather", arguments: '{"city":"Paris"}' },
              },
            ],
          },
        ],
        status: "in_progress",
      };
      const result = formatter.parseStreamChunk(data);
      expect(result.valid).toBe(true);
      expect(result.chunk.toolCallsDelta).toBeDefined();
      expect(result.chunk.toolCallsDelta).toHaveLength(1);
    });
  });

  describe("convertMessages", () => {
    it("should convert messages to OpenAI format", () => {
      const messages: LLMMessage[] = [
        { role: "user", content: "Hello" },
        { role: "assistant", content: "Hi there!" },
      ];
      const result = formatter.convertMessages(messages);
      expect(result).toHaveLength(2);
      expect(result[0]).toHaveProperty("role", "user");
      expect(result[1]).toHaveProperty("role", "assistant");
    });

    it("should convert tool calls in messages", () => {
      const messages: LLMMessage[] = [
        {
          role: "assistant",
          content: "",
          toolCalls: [
            { id: "call-1", type: "function", function: { name: "get_weather", arguments: "{}" } },
          ],
        },
      ];
      const result = formatter.convertMessages(messages);
      expect((result[0] as any).tool_calls).toBeDefined();
      expect((result[0] as any).tool_calls).toHaveLength(1);
    });

    it("should add tool_call_id for tool role messages", () => {
      const messages: LLMMessage[] = [
        { role: "tool", content: '{"temp": 20}', toolCallId: "call-1" },
      ];
      const result = formatter.convertMessages(messages);
      expect((result[0] as any).tool_call_id).toBe("call-1");
    });
  });

  describe("parseToolCalls", () => {
    it("should parse tool calls with function structure", () => {
      const toolCalls = [
        { id: "call-1", function: { name: "get_weather", arguments: '{"city":"Tokyo"}' } },
      ];
      const result = formatter.parseToolCalls(toolCalls);
      expect(result).toHaveLength(1);
      expect(result[0]!.function.name).toBe("get_weather");
    });

    it("should parse tool calls with flat name/arguments structure", () => {
      const toolCalls = [{ id: "call-2", name: "search", arguments: '{"q":"test"}' }];
      const result = formatter.parseToolCalls(toolCalls);
      expect(result).toHaveLength(1);
      expect(result[0]!.function.name).toBe("search");
    });

    it("should handle missing arguments gracefully", () => {
      const toolCalls = [{ id: "call-3", name: "noop" }];
      const result = formatter.parseToolCalls(toolCalls);
      expect(result[0]!.function.arguments).toBe("{}");
    });
  });
});
