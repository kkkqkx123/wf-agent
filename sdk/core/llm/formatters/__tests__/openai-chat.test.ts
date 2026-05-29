/**
 * Unit tests for OpenAI Chat Formatter
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { OpenAIChatFormatter } from "../openai-chat.js";
import type { LLMRequest, LLMMessage } from "@wf-agent/types";
import type { FormatterConfig } from "../types.js";

vi.mock("../tool-converter.js", () => ({
  convertToolsToOpenAIFormat: vi.fn((tools: any[]) =>
    tools.map((t: any) => ({ type: "function", function: { name: t.name } }))
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

vi.mock("@wf-agent/prompt-templates", () => ({
  TOOLS_XML_LIST_TEMPLATE: "<tools_xml>",
  TOOLS_JSON_LIST_TEMPLATE: "<tools_json>",
  TOOLS_RAW_LIST_TEMPLATE: "<tools_raw>",
  TOOLS_RAW_COMPACT_LIST_TEMPLATE: "<tools_raw_compact>",
  TOOL_XML_FORMAT_TEMPLATE: "<tool_xml>",
  TOOL_JSON_FORMAT_TEMPLATE: "<tool_json>",
  TOOL_RAW_FORMAT_TEMPLATE: "<tool_raw>",
  TOOL_RAW_COMPACT_TEMPLATE: "<tool_raw_compact>",
  TOOL_XML_PARAMETER_LINE_TEMPLATE: "<param_xml>",
  TOOL_JSON_PARAMETER_LINE_TEMPLATE: "<param_json>",
  TOOL_RAW_PARAMETER_LINE_TEMPLATE: "<param_raw>",
}));

describe("OpenAIChatFormatter", () => {
  let formatter: OpenAIChatFormatter;
  let mockConfig: FormatterConfig;

  beforeEach(() => {
    formatter = new OpenAIChatFormatter();
    mockConfig = {
      profile: {
        provider: "openai_chat",
        model: "gpt-4",
        apiKey: "test-key",
        parameters: {},
      },
      stream: false,
      timeout: 30000,
    } as unknown as FormatterConfig;
  });

  describe("getSupportedProvider", () => {
    it("should return OPENAI_CHAT", () => {
      expect(formatter.getSupportedProvider()).toBe("OPENAI_CHAT");
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
      expect(result.httpRequest!.headers).toHaveProperty("Authorization");
    });

    it("should include tools in body when provided", () => {
      const request: LLMRequest = {
        messages: [{ role: "user", content: "Use a tool" }],
        tools: [{ id: "tool-1", description: "A test tool", parameters: { type: "object", properties: {}, required: [] } }],
      };
      const result = formatter.buildRequest(request, mockConfig);
      const body = result.httpRequest.body as Record<string, unknown>;
      expect(body["tools"]).toBeDefined();
    });
  });

  describe("parseNativeResponse", () => {
    it("should parse a standard response", () => {
      const data = {
        id: "chatcmpl-123",
        model: "gpt-4",
        choices: [
          {
            message: { content: "Hello there!" },
            finish_reason: "stop",
          },
        ],
        usage: { prompt_tokens: 10, completion_tokens: 20, total_tokens: 30 },
        created: 1700000000,
        system_fingerprint: "fp_abc",
      };
      const result = formatter.parseResponse(data, mockConfig);
      expect(result.id).toBe("chatcmpl-123");
      expect(result.content).toBe("Hello there!");
      expect(result.usage?.promptTokens).toBe(10);
      expect(result.usage?.completionTokens).toBe(20);
      expect(result.finishReason).toBe("stop");
    });

    it("should parse response with tool calls", () => {
      const data = {
        id: "chatcmpl-456",
        model: "gpt-4",
        choices: [
          {
            message: {
              content: "",
              tool_calls: [
                { id: "call-1", type: "function", function: { name: "get_weather", arguments: '{"city":"London"}' } },
              ],
            },
            finish_reason: "tool_calls",
          },
        ],
        usage: { prompt_tokens: 20, completion_tokens: 10, total_tokens: 30 },
        created: 1700000001,
        system_fingerprint: "fp_def",
      };
      const result = formatter.parseResponse(data, mockConfig);
      expect(result.toolCalls).toBeDefined();
      expect(result.toolCalls).toHaveLength(1);
      expect(result.toolCalls![0]!.function.name).toBe("get_weather");
      expect(result.finishReason).toBe("tool_calls");
    });

    it("should extract reasoning content", () => {
      const data = {
        id: "chatcmpl-789",
        model: "deepseek-r1",
        choices: [
          {
            message: {
              content: "Final answer",
              reasoning_content: "Step by step thinking...",
            },
            finish_reason: "stop",
          },
        ],
        usage: {
          prompt_tokens: 10,
          completion_tokens: 50,
          total_tokens: 60,
          completion_tokens_details: { reasoning_tokens: 30 },
        },
        created: 1700000002,
        system_fingerprint: "fp_ghi",
      };
      const result = formatter.parseResponse(data, mockConfig);
      expect(result.reasoningContent).toBe("Step by step thinking...");
      expect(result.reasoningTokens).toBe(30);
    });
  });

  describe("parseStreamChunk", () => {
    it("should parse streaming chunk with content delta", () => {
      const data = {
        choices: [{ delta: { content: "Hello" }, finish_reason: null }],
      };
      const result = formatter.parseStreamChunk(data);
      expect(result.valid).toBe(true);
      expect(result.chunk.delta).toBe("Hello");
      expect(result.chunk.done).toBe(false);
    });

    it("should handle streaming completion", () => {
      const data = {
        choices: [{ delta: {}, finish_reason: "stop" }],
        usage: { prompt_tokens: 10, completion_tokens: 20, total_tokens: 30 },
      };
      const result = formatter.parseStreamChunk(data);
      expect(result.valid).toBe(true);
      expect(result.chunk.done).toBe(true);
      expect(result.chunk.usage).toBeDefined();
    });

    it("should extract reasoning delta from streaming chunk", () => {
      const data = {
        choices: [{ delta: { reasoning_content: "Thinking..." } }],
      };
      const result = formatter.parseStreamChunk(data);
      expect(result.chunk.reasoningDelta).toBe("Thinking...");
    });

    it("should return invalid for empty choices", () => {
      const data = { choices: [] };
      const result = formatter.parseStreamChunk(data);
      expect(result.valid).toBe(false);
    });
  });

  describe("convertMessages", () => {
    it("should convert simple messages", () => {
      const messages: LLMMessage[] = [
        { role: "system", content: "You are helpful." },
        { role: "user", content: "Hello" },
      ];
      const result = formatter.convertMessages(messages);
      expect(result).toHaveLength(2);
    });

    it("should convert tool calls in assistant messages", () => {
      const messages: LLMMessage[] = [
        {
          role: "assistant",
          content: "",
          toolCalls: [{ id: "call-1", type: "function", function: { name: "test", arguments: "{}" } }],
        },
      ];
      const result = formatter.convertMessages(messages) as any[];
      expect(result[0].tool_calls).toHaveLength(1);
    });

    it("should add tool_call_id for tool role messages", () => {
      const messages: LLMMessage[] = [
        { role: "tool", content: '{"result": "ok"}', toolCallId: "call-1" },
      ];
      const result = formatter.convertMessages(messages) as any[];
      expect(result[0].tool_call_id).toBe("call-1");
    });
  });

  describe("parseToolCalls", () => {
    it("should parse tool calls correctly", () => {
      const toolCalls = [
        { id: "call-1", type: "function", function: { name: "get_weather", arguments: '{"city":"Paris"}' } },
      ];
      const result = formatter.parseToolCalls(toolCalls);
      expect(result).toHaveLength(1);
      expect(result![0]!.function.name).toBe("get_weather");
    });

    it("should handle non-array input", () => {
      const result = formatter.parseToolCalls(null);
      expect(result).toEqual([]);
    });
  });
});
