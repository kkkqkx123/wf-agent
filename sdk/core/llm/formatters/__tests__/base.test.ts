/**
 * BaseFormatter unit tests
 */

import { describe, it, expect, beforeEach } from "vitest";
import { BaseFormatter } from "../base.js";
import type { LLMRequest, LLMMessage, LLMToolCall, ToolSchema } from "@wf-agent/types";
import type { FormatterConfig, ParseStreamChunkResult } from "../types.js";

// Create a test-oriented Formatter subclass
class TestFormatter extends BaseFormatter {
  getSupportedProvider(): string {
    return "TEST";
  }

  buildRequest(request: LLMRequest, config: FormatterConfig) {
    return {
      httpRequest: {
        url: "/test",
        method: "POST" as const,
        headers: {},
        body: {},
      },
    };
  }

  parseResponse(data: any, config: FormatterConfig) {
    return {
      id: "test-id",
      model: "test-model",
      content: data.content || "",
      message: { role: "assistant" as const, content: data.content || "" },
      finishReason: "stop",
      duration: 0,
    };
  }

  parseStreamChunk(data: any, config: FormatterConfig): ParseStreamChunkResult {
    return {
      chunk: {
        delta: data.delta || "",
        done: data.done || false,
      },
      valid: true,
    };
  }

  convertTools(tools: ToolSchema[]): any {
    return tools;
  }

  convertMessages(messages: LLMMessage[]): any {
    return messages;
  }

  parseToolCalls(data: any): LLMToolCall[] {
    return data || [];
  }
}

describe("BaseFormatter", () => {
  let formatter: TestFormatter;
  let mockConfig: FormatterConfig;

  beforeEach(() => {
    formatter = new TestFormatter();
    mockConfig = {
      profile: {
        id: "test-profile-id",
        name: "test-profile",
        provider: "TEST" as any,
        model: "test-model",
        apiKey: "test-api-key",
        parameters: {},
      },
    };
  });

  describe("parseStreamLine", () => {
    it("Blank lines should be skipped", () => {
      const result = formatter.parseStreamLine("", mockConfig);
      expect(result.valid).toBe(false);
      expect(result.chunk.done).toBe(false);
    });

    it("The [DONE] flag should be recognized", () => {
      const result = formatter.parseStreamLine("data: [DONE]", mockConfig);
      expect(result.valid).toBe(false);
      expect(result.chunk.done).toBe(true);
    });

    it("Lines without the data: prefix should be skipped.", () => {
      const result = formatter.parseStreamLine("invalid line", mockConfig);
      expect(result.valid).toBe(false);
    });

    it("Valid SSE lines should be parsed", () => {
      const result = formatter.parseStreamLine('data: {"delta": "test"}', mockConfig);
      expect(result.valid).toBe(true);
      expect(result.chunk.delta).toBe("test");
    });

    it("Invalid JSON should be skipped", () => {
      const result = formatter.parseStreamLine("data: {invalid json}", mockConfig);
      expect(result.valid).toBe(false);
    });
  });

  describe("validateConfig", () => {
    it("Valid configurations should be verified", () => {
      expect(formatter.validateConfig(mockConfig)).toBe(true);
    });

    it("Configurations without profile should be rejected", () => {
      const invalidConfig = {} as FormatterConfig;
      expect(formatter.validateConfig(invalidConfig)).toBe(false);
    });

    it("Configurations without a model should be rejected", () => {
      const invalidConfig = {
        profile: { provider: "TEST" as any, apiKey: "test" },
      } as FormatterConfig;
      expect(formatter.validateConfig(invalidConfig)).toBe(false);
    });
  });

  describe("extractSystemMessage", () => {
    it("System messages should be extracted", () => {
      const messages: LLMMessage[] = [
        { role: "system", content: "System prompt" },
        { role: "user", content: "User message" },
      ];

      const result = formatter["extractSystemMessage"](messages);
      expect(result.systemMessage).toEqual({
        role: "system",
        content: "System prompt",
      });
      expect(result.filteredMessages).toHaveLength(1);
      expect(result.filteredMessages[0]?.role).toBe("user");
    });

    it("The last system message should be used", () => {
      const messages: LLMMessage[] = [
        { role: "system", content: "First system" },
        { role: "system", content: "Second system" },
        { role: "user", content: "User message" },
      ];

      const result = formatter["extractSystemMessage"](messages);
      expect(result.systemMessage?.content).toBe("Second system");
    });

    it("Should handle the absence of system messages", () => {
      const messages: LLMMessage[] = [{ role: "user", content: "User message" }];

      const result = formatter["extractSystemMessage"](messages);
      expect(result.systemMessage).toBeNull();
      expect(result.filteredMessages).toHaveLength(1);
    });
  });

  describe("findLastUserMessageGroupIndex", () => {
    it("The index of the last set of user messages should be found", () => {
      const messages: LLMMessage[] = [
        { role: "user", content: "User 1" },
        { role: "assistant", content: "Assistant 1" },
        { role: "user", content: "User 2" },
        { role: "user", content: "User 3" },
      ];

      const index = formatter["findLastUserMessageGroupIndex"](messages);
      expect(index).toBe(2);
    });

    it("Single user messages should be handled", () => {
      const messages: LLMMessage[] = [
        { role: "user", content: "User 1" },
        { role: "assistant", content: "Assistant 1" },
      ];

      const index = formatter["findLastUserMessageGroupIndex"](messages);
      expect(index).toBe(0);
    });

    it("Should return -1 if the user message is not found", () => {
      const messages: LLMMessage[] = [{ role: "assistant", content: "Assistant 1" }];

      const index = formatter["findLastUserMessageGroupIndex"](messages);
      expect(index).toBe(-1);
    });
  });

  describe("cleanInternalFields", () => {
    it("Internal fields should be cleaned up", () => {
      const messages: LLMMessage[] = [
        {
          role: "user",
          content: "Test",
          // @ts-expect-error Test the internal fields
          _internal: "should be removed",
        },
      ];

      const cleaned = formatter["cleanInternalFields"](messages);
      expect(cleaned[0]).toEqual({
        role: "user",
        content: "Test",
      });
    });

    it("Fields related to tool calls should be preserved", () => {
      const messages: LLMMessage[] = [
        {
          role: "assistant",
          content: "",
          toolCalls: [
            {
              id: "call_1",
              type: "function",
              function: {
                name: "test",
                arguments: "{}",
              },
            },
          ],
        },
        {
          role: "tool",
          content: "result",
          toolCallId: "call_1",
        },
      ];

      const cleaned = formatter["cleanInternalFields"](messages);
      expect(cleaned[0]?.toolCalls).toBeDefined();
      expect(cleaned[1]?.toolCallId).toBe("call_1");
    });
  });

  describe("mergeParameters", () => {
    it("The two parameter objects should be merged", () => {
      const profileParams = { temperature: 0.7, max_tokens: 1000 };
      const requestParams = { temperature: 0.8, top_p: 0.9 };

      const merged = formatter["mergeParameters"](profileParams, requestParams);
      expect(merged).toEqual({
        temperature: 0.8,
        max_tokens: 1000,
        top_p: 0.9,
      });
    });

    it("Empty parameters should be handled", () => {
      const merged = formatter["mergeParameters"]({}, {});
      expect(merged).toEqual({});
    });
  });

  describe("deepMerge", () => {
    it("Nested objects should be deeply merged", () => {
      const target = {
        a: 1,
        b: { c: 2, d: 3 },
      };
      const source = {
        b: { d: 4, e: 5 },
        f: 6,
      };

      const result = formatter["deepMerge"](target, source);
      expect(result).toEqual({
        a: 1,
        b: { c: 2, d: 4, e: 5 },
        f: 6,
      });
    });

    it("Arrays should be merged", () => {
      const target = { items: [1, 2] };
      const source = { items: [3, 4] };

      const result = formatter["deepMerge"](target, source);
      expect(result.items).toEqual([1, 2, 3, 4]);
    });

    it("Should override values of non-object types", () => {
      const target = { a: 1, b: "old" };
      const source = { b: "new", c: null };

      const result = formatter["deepMerge"](target, source);
      expect(result).toEqual({
        a: 1,
        b: "new",
        c: undefined,
      });
    });

    it("Should handle null or undefined sources", () => {
      const target = { a: 1 };
      expect(formatter["deepMerge"](target, null)).toEqual(target);
      expect(formatter["deepMerge"](target, undefined)).toEqual(target);
    });

    it("The array source should be handled", () => {
      const target = { a: 1 };
      const source = [1, 2, 3];

      const result = formatter["deepMerge"](target, source);
      expect(result).toEqual([1, 2, 3]);
    });
  });

  describe("buildAuthHeader", () => {
    it("The Bearer authentication header should be constructed", () => {
      const config = { ...mockConfig, authType: "bearer" as const };
      const header = formatter["buildAuthHeader"]("test-key", config, "x-api-key");

      expect(header).toEqual({
        Authorization: "Bearer test-key",
      });
    });

    it("Native authentication headers should be built", () => {
      const config = { ...mockConfig, authType: "native" as const };
      const header = formatter["buildAuthHeader"]("test-key", config, "x-api-key");

      expect(header).toEqual({
        "x-api-key": "test-key",
      });
    });

    it("Should return empty objects when there is no API key", () => {
      const header = formatter["buildAuthHeader"](undefined, mockConfig, "x-api-key");
      expect(header).toEqual({});
    });

    it("Native authentication should be used by default", () => {
      const config = { ...mockConfig };
      const header = formatter["buildAuthHeader"]("test-key", config, "x-api-key");

      expect(header).toEqual({
        "x-api-key": "test-key",
      });
    });
  });

  describe("buildCustomHeaders", () => {
    it("A simplified version of the custom request header should be merged", () => {
      const config = {
        ...mockConfig,
        customHeaders: {
          "X-Custom-1": "value1",
          "X-Custom-2": "value2",
        },
      };

      const headers = formatter["buildCustomHeaders"](config);
      expect(headers).toEqual({
        "X-Custom-1": "value1",
        "X-Custom-2": "value2",
      });
    });

    it("The full version of the custom request header should be merged", () => {
      const config = {
        ...mockConfig,
        customHeadersList: [
          { key: "X-Custom-1", value: "value1", enabled: true },
          { key: "X-Custom-2", value: "value2", enabled: true },
          { key: "X-Custom-3", value: "value3", enabled: false },
        ],
      };

      const headers = formatter["buildCustomHeaders"](config);
      expect(headers).toEqual({
        "X-Custom-1": "value1",
        "X-Custom-2": "value2",
      });
    });

    it("Custom request headers in both formats should be handled", () => {
      const config = {
        ...mockConfig,
        customHeaders: { "X-Custom-1": "value1" },
        customHeadersList: [{ key: "X-Custom-2", value: "value2", enabled: true }],
      };

      const headers = formatter["buildCustomHeaders"](config);
      expect(headers).toEqual({
        "X-Custom-1": "value1",
        "X-Custom-2": "value2",
      });
    });

    it("Request headers with empty key names should be skipped", () => {
      const config = {
        ...mockConfig,
        customHeadersList: [
          { key: "  ", value: "value", enabled: true },
          { key: "X-Custom", value: "value", enabled: true },
        ],
      };

      const headers = formatter["buildCustomHeaders"](config);
      expect(headers).toEqual({
        "X-Custom": "value",
      });
    });

    it("Request headers with no value should be handled", () => {
      const config = {
        ...mockConfig,
        customHeadersList: [{ key: "X-Custom", value: "", enabled: true }],
      };

      const headers = formatter["buildCustomHeaders"](config);
      expect(headers).toEqual({
        "X-Custom": "",
      });
    });
  });

  describe("applyCustomBody", () => {
    it("A simplified version of the custom request body should be merged", () => {
      const baseBody = { model: "test", messages: [] };
      const config = {
        ...mockConfig,
        customBody: { temperature: 0.8, top_p: 0.9 },
      };

      const result = formatter["applyCustomBody"](baseBody, config);
      expect(result).toEqual({
        model: "test",
        messages: [],
        temperature: 0.8,
        top_p: 0.9,
      });
    });

    it("Should merge the custom request body of the simple schema", () => {
      const baseBody = { model: "test", messages: [] };
      const config = {
        ...mockConfig,
        customBodyConfig: {
          mode: "simple" as const,
          items: [
            { key: "temperature", value: "0.8", enabled: true },
            { key: "top_p", value: "0.9", enabled: true },
            { key: "disabled", value: "1.0", enabled: false },
          ],
        },
      };

      const result = formatter["applyCustomBody"](baseBody, config);
      expect(result).toEqual({
        model: "test",
        messages: [],
        temperature: 0.8,
        top_p: 0.9,
      });
    });

    it("Should parse JSON values in simple mode", () => {
      const baseBody = { model: "test" };
      const config = {
        ...mockConfig,
        customBodyConfig: {
          mode: "simple" as const,
          items: [{ key: "extra_body", value: '{"key": "value"}', enabled: true }],
        },
      };

      const result = formatter["applyCustomBody"](baseBody, config);
      expect(result).toEqual({
        model: "test",
        extra_body: { key: "value" },
      });
    });

    it("Nested paths in simple patterns should be handled", () => {
      const baseBody = { model: "test" };
      const config = {
        ...mockConfig,
        customBodyConfig: {
          mode: "simple" as const,
          items: [{ key: "extra_body.google", value: "value", enabled: true }],
        },
      };

      const result = formatter["applyCustomBody"](baseBody, config);
      expect(result).toEqual({
        model: "test",
        extra_body: { google: "value" },
      });
    });

    it("Custom request bodies that should be merged with the advanced schema", () => {
      const baseBody = { model: "test", messages: [] };
      const config = {
        ...mockConfig,
        customBodyConfig: {
          mode: "advanced" as const,
          json: '{"temperature": 0.8, "top_p": 0.9}',
        },
      };

      const result = formatter["applyCustomBody"](baseBody, config);
      expect(result).toEqual({
        model: "test",
        messages: [],
        temperature: 0.8,
        top_p: 0.9,
      });
    });

    it("Invalid JSON should be skipped", () => {
      const baseBody = { model: "test" };
      const config = {
        ...mockConfig,
        customBodyConfig: {
          mode: "advanced" as const,
          json: "{invalid json}",
        },
      };

      const result = formatter["applyCustomBody"](baseBody, config);
      expect(result).toEqual({ model: "test" });
    });

    it("Custom request bodies should be skipped when customBodyEnabled is false", () => {
      const baseBody = { model: "test" };
      const config = {
        ...mockConfig,
        customBodyEnabled: false,
        customBody: { temperature: 0.8 },
      };

      const result = formatter["applyCustomBody"](baseBody, config);
      expect(result).toEqual({ model: "test" });
    });
  });

  describe("buildQueryString", () => {
    it("The query parameter string should be constructed", () => {
      const config = {
        ...mockConfig,
        queryParams: { key1: "value1", key2: "value2" },
      };

      const queryString = formatter["buildQueryString"](config);
      expect(queryString).toBe("?key1=value1&key2=value2");
    });

    it("Should handle numbers and booleans", () => {
      const config = {
        ...mockConfig,
        queryParams: { num: 123, bool: true },
      };

      const queryString = formatter["buildQueryString"](config);
      expect(queryString).toBe("?num=123&bool=true");
    });

    it("Should return the empty string when there are no query parameters", () => {
      const queryString = formatter["buildQueryString"](mockConfig);
      expect(queryString).toBe("");
    });
  });

  describe("buildStreamOptions", () => {
    it("Streaming options should be built", () => {
      const config = {
        ...mockConfig,
        streamOptions: { includeUsage: true },
      };

      const options = formatter["buildStreamOptions"](config);
      expect(options).toEqual({ include_usage: true });
    });

    it("should return undefined if there is no streaming option", () => {
      const options = formatter["buildStreamOptions"](mockConfig);
      expect(options).toBeUndefined();
    });

    it("undefined should be returned when includeUsage is false", () => {
      const config = {
        ...mockConfig,
        streamOptions: { includeUsage: false },
      };

      const options = formatter["buildStreamOptions"](config);
      expect(options).toBeUndefined();
    });
  });

  describe("Tool call parsing methods", () => {
    describe("parseXMLToolCalls", () => {
      it("Tool calls that should parse the XML format", () => {
        const xmlText = `
          <tool_use>
            <tool_name>test_tool</tool_name>
            <parameters>
              <param1>value1</param1>
              <param2>value2</param2>
            </parameters>
          </tool_use>
        `;

        const toolCalls = formatter.parseXMLToolCalls(xmlText);
        expect(toolCalls).toHaveLength(1);
        expect(toolCalls[0]?.function.name).toBe("test_tool");
      });

      it("The nested XML parameters should be parsed.", () => {
        const xmlText = `
          <tool_use>
            <tool_name>test_tool</tool_name>
            <parameters>
              <nested>
                <key>value</key>
              </nested>
            </parameters>
          </tool_use>
        `;

        const toolCalls = formatter.parseXMLToolCalls(xmlText);
        const args = JSON.parse(toolCalls[0]!.function.arguments);
        expect(args).toEqual({ nested: { key: "value" } });
      });

      it("The XML array should be parsed.", () => {
        const xmlText = `
          <tool_use>
            <tool_name>test_tool</tool_name>
            <parameters>
              <items>
                <item>1</item>
                <item>2</item>
                <item>3</item>
              </items>
            </parameters>
          </tool_use>
        `;

        const toolCalls = formatter.parseXMLToolCalls(xmlText);
        const args = JSON.parse(toolCalls[0]!.function.arguments);
        expect(args).toEqual({ items: [1, 2, 3] });
      });

      it("Multiple tool calls should be parsed.", () => {
        const xmlText = `
          <tool_use>
            <tool_name>tool1</tool_name>
            <parameters>
              <param>value1</param>
            </parameters>
          </tool_use>
          <tool_use>
            <tool_name>tool2</tool_name>
            <parameters>
              <param>value2</param>
            </parameters>
          </tool_use>
        `;

        const toolCalls = formatter.parseXMLToolCalls(xmlText);
        expect(toolCalls).toHaveLength(2);
      });
    });

    describe("parseJSONToolCalls", () => {
      it("Tools that should parse JSON format calls", () => {
        const text = `
          <<<TOOL_CALL>>>
          {"tool": "test_tool", "parameters": {"param1": "value1"}}
          <<<END_TOOL_CALL>>>
        `;

        const toolCalls = formatter.parseJSONToolCalls(text);
        expect(toolCalls).toHaveLength(1);
        expect(toolCalls[0]?.function.name).toBe("test_tool");
      });

      it("A tool that should parse JSON calls with custom markers.", () => {
        const text = `
          <<<CUSTOM_START>>>
          {"tool": "test_tool", "parameters": {}}
          <<<CUSTOM_END>>>
        `;

        const toolCalls = formatter.parseJSONToolCalls(text, {
          toolCallStartMarker: "<<<CUSTOM_START>>>",
          toolCallEndMarker: "<<<CUSTOM_END>>>",
        });
        expect(toolCalls).toHaveLength(1);
      });

      it("Multiple JSON tool calls should be parsed.", () => {
        const text = `
          <<<TOOL_CALL>>>
          {"tool": "tool1", "parameters": {}}
          <<<END_TOOL_CALL>>>
          <<<TOOL_CALL>>>
          {"tool": "tool2", "parameters": {}}
          <<<END_TOOL_CALL>>>
        `;

        const toolCalls = formatter.parseJSONToolCalls(text);
        expect(toolCalls).toHaveLength(2);
      });
    });

    describe("parseToolCallsFromText", () => {
      it("XML format should be automatically detected and parsed.", () => {
        const text = `
          <tool_use>
            <tool_name>test_tool</tool_name>
            <parameters>
              <param>value</param>
            </parameters>
          </tool_use>
        `;

        const toolCalls = formatter.parseToolCallsFromText(text);
        expect(toolCalls).toHaveLength(1);
        expect(toolCalls[0]?.function.name).toBe("test_tool");
      });

      it("JSON format should be automatically detected and parsed.", () => {
        const text = `
          <<<TOOL_CALL>>>
          {"tool": "test_tool", "parameters": {}}
          <<<END_TOOL_CALL>>>
        `;

        const toolCalls = formatter.parseToolCallsFromText(text);
        expect(toolCalls).toHaveLength(1);
        expect(toolCalls[0]?.function.name).toBe("test_tool");
      });

      it("An empty array should be returned when the format cannot be recognized.", () => {
        const text = "No tool calls here";
        const toolCalls = formatter.parseToolCallsFromText(text);
        expect(toolCalls).toHaveLength(0);
      });

      it("XML format should be tried as a priority.", () => {
        const text = `
          <tool_use>
            <tool_name>xml_tool</tool_name>
            <parameters></parameters>
          </tool_use>
          <<<TOOL_CALL>>>
          {"tool": "json_tool", "parameters": {}}
          <<<END_TOOL_CALL>>>
        `;

        const toolCalls = formatter.parseToolCallsFromText(text);
        expect(toolCalls).toHaveLength(1);
        expect(toolCalls[0]?.function.name).toBe("xml_tool");
      });
    });
  });
});
