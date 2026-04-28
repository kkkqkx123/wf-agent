/**
 * ToolCallParser Unit Tests
 * Comprehensive tests for enhanced parser including raw JSON, partial parsing, and auto-detection
 */

import { describe, it, expect } from "vitest";
import { ToolCallParser, type ToolCallParseOptions } from "../tool-call-parser.js";

describe("ToolCallParser", () => {
  describe("parseXMLToolCalls", () => {
    it("should parse simple XML tool calls", () => {
      const xmlText = `
        <tool_use>
          <tool_name>test_tool</tool_name>
          <parameters>
            <param1>value1</param1>
            <param2>value2</param2>
          </parameters>
        </tool_use>
      `;

      const toolCalls = ToolCallParser.parseXMLToolCalls(xmlText);
      expect(toolCalls).toHaveLength(1);
      expect(toolCalls[0]?.type).toBe("function");
      expect(toolCalls[0]?.function.name).toBe("test_tool");
      expect(JSON.parse(toolCalls[0]!.function.arguments)).toEqual({
        param1: "value1",
        param2: "value2",
      });
    });

    it("should parse nested XML parameters", () => {
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

      const toolCalls = ToolCallParser.parseXMLToolCalls(xmlText);
      const args = JSON.parse(toolCalls[0]!.function.arguments);
      expect(args).toEqual({
        nested: { key: "value" },
      });
    });

    it("should parse XML arrays with primitive values", () => {
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

      const toolCalls = ToolCallParser.parseXMLToolCalls(xmlText);
      const args = JSON.parse(toolCalls[0]!.function.arguments);
      expect(args).toEqual({ items: [1, 2, 3] });
    });

    it("should parse XML arrays with nested objects", () => {
      const xmlText = `
        <tool_use>
          <tool_name>test_tool</tool_name>
          <parameters>
            <users>
              <item>
                <name>Alice</name>
                <age>30</age>
              </item>
              <item>
                <name>Bob</name>
                <age>25</age>
              </item>
            </users>
          </parameters>
        </tool_use>
      `;

      const toolCalls = ToolCallParser.parseXMLToolCalls(xmlText);
      const args = JSON.parse(toolCalls[0]!.function.arguments);
      expect(args).toEqual({
        users: [
          { name: "Alice", age: 30 },
          { name: "Bob", age: 25 },
        ],
      });
    });

    it("should parse XML value types correctly (int, float, bool, null)", () => {
      const xmlText = `
        <tool_use>
          <tool_name>test_tool</tool_name>
          <parameters>
            <integer>42</integer>
            <float>3.14</float>
            <boolean_true>true</boolean_true>
            <boolean_false>false</boolean_false>
            <null_value>null</null_value>
            <string>hello</string>
          </parameters>
        </tool_use>
      `;

      const toolCalls = ToolCallParser.parseXMLToolCalls(xmlText);
      const args = JSON.parse(toolCalls[0]!.function.arguments);
      expect(args).toEqual({
        integer: 42,
        float: 3.14,
        boolean_true: true,
        boolean_false: false,
        null_value: null,
        string: "hello",
      });
    });

    it("should parse multiple XML tool calls", () => {
      const xmlText = `
        <tool_use>
          <tool_name>tool1</tool_name>
        </tool_use>
        <tool_use>
          <tool_name>tool2</tool_name>
        </tool_use>
      `;

      const toolCalls = ToolCallParser.parseXMLToolCalls(xmlText);
      expect(toolCalls).toHaveLength(2);
      expect(toolCalls[0]?.function.name).toBe("tool1");
      expect(toolCalls[1]?.function.name).toBe("tool2");
    });

    it("should handle XML tool calls without parameters", () => {
      const xmlText = `
        <tool_use>
          <tool_name>test_tool</tool_name>
        </tool_use>
      `;

      const toolCalls = ToolCallParser.parseXMLToolCalls(xmlText);
      const args = JSON.parse(toolCalls[0]!.function.arguments);
      expect(args).toEqual({});
    });

    it("should skip invalid or empty XML tool calls", () => {
      const xmlText = `
        <tool_use>
          <tool_name>valid</tool_name>
        </tool_use>
        <tool_use>
          <tool_name></tool_name>
        </tool_use>
      `;

      const toolCalls = ToolCallParser.parseXMLToolCalls(xmlText);
      expect(toolCalls).toHaveLength(1);
    });
  });

  describe("parseJSONToolCalls (wrapped markers)", () => {
    it("should parse standard wrapped JSON tool calls", () => {
      const text = `
        <<<TOOL_CALL>>>
        {"tool": "test_tool", "parameters": {"param1": "value1"}}
        <<<END_TOOL_CALL>>>
      `;

      const toolCalls = ToolCallParser.parseJSONToolCalls(text);
      expect(toolCalls).toHaveLength(1);
      expect(toolCalls[0]?.function.name).toBe("test_tool");
      expect(JSON.parse(toolCalls[0]!.function.arguments)).toEqual({ param1: "value1" });
    });

    it("should parse OpenAI-style { name, arguments }", () => {
      const text = `
        <<<TOOL_CALL>>>
        {"name": "test_tool", "arguments": "{\\"key\\": \\"val\\"}"}
        <<<END_TOOL_CALL>>>
      `;

      const toolCalls = ToolCallParser.parseJSONToolCalls(text);
      expect(toolCalls[0]?.function.arguments).toBe('{"key": "val"}');
    });

    it("should parse full native OpenAI tool call structure", () => {
      const text = `
        <<<TOOL_CALL>>>
        {
          "id": "call_manual",
          "type": "function",
          "function": {
            "name": "test_tool",
            "arguments": "{}"
          }
        }
        <<<END_TOOL_CALL>>>
      `;

      const toolCalls = ToolCallParser.parseJSONToolCalls(text);
      expect(toolCalls[0]?.id).toBe("call_manual");
      expect(toolCalls[0]?.function.name).toBe("test_tool");
    });

    it("should stringify object arguments automatically", () => {
      const text = `
        <<<TOOL_CALL>>>
        {"name": "test_tool", "arguments": {"a": 1}}
        <<<END_TOOL_CALL>>>
      `;

      const toolCalls = ToolCallParser.parseJSONToolCalls(text);
      expect(toolCalls[0]?.function.arguments).toBe('{"a":1}');
    });

    it("should support custom markers", () => {
      const text = `
        [[START]]
        {"tool": "test"}
        [[END]]
      `;

      const options: ToolCallParseOptions = {
        toolCallStartMarker: "[[START]]",
        toolCallEndMarker: "[[END]]",
      };

      const toolCalls = ToolCallParser.parseJSONToolCalls(text, options);
      expect(toolCalls).toHaveLength(1);
    });

    it("should parse multiple wrapped JSON tool calls", () => {
      const text = `
        <<<TOOL_CALL>>>{"tool":"t1"}<<<END_TOOL_CALL>>>
        <<<TOOL_CALL>>>{"tool":"t2"}<<<END_TOOL_CALL>>>
      `;

      const toolCalls = ToolCallParser.parseJSONToolCalls(text);
      expect(toolCalls).toHaveLength(2);
    });

    it("should skip invalid JSON and empty blocks", () => {
      const text1 = "<<<TOOL_CALL>>>{invalid}{<<<END_TOOL_CALL>>>";
      const text2 = "<<<TOOL_CALL>>>    <<<END_TOOL_CALL>>>";

      expect(ToolCallParser.parseJSONToolCalls(text1)).toHaveLength(0);
      expect(ToolCallParser.parseJSONToolCalls(text2)).toHaveLength(0);
    });
  });

  describe("parseRawJsonToolCalls (no markers)", () => {
    it("should parse raw single JSON tool call { tool, parameters }", () => {
      const text = `{"tool": "raw_tool", "parameters": {"x": 1}}`;
      const calls = ToolCallParser.parseRawJsonToolCalls(text);
      expect(calls[0]?.function.name).toBe("raw_tool");
    });

    it("should parse raw JSON array of tool calls", () => {
      const text = `[{"tool":"a"},{"tool":"b"}]`;
      const calls = ToolCallParser.parseRawJsonToolCalls(text);
      expect(calls).toHaveLength(2);
    });

    it("should parse OpenAI raw format { function: { name, arguments } }", () => {
      const text = `{"function":{"name":"openai_raw","arguments":"{}"}}`;
      const calls = ToolCallParser.parseRawJsonToolCalls(text);
      expect(calls[0]?.function.name).toBe("openai_raw");
    });

    it("should strip markdown code blocks like ```json", () => {
      const text = '```json\n{"tool":"test"}\n```';
      const calls = ToolCallParser.parseRawJsonToolCalls(text);
      expect(calls).toHaveLength(1);
    });
  });

  describe("parsePartial (streaming chunk support)", () => {
    it("should return empty for incomplete content", () => {
      const calls = ToolCallParser.parsePartial(`{"tool": "test`);
      expect(calls).toHaveLength(0);
    });

    it("should parse complete content in partial mode", () => {
      const calls = ToolCallParser.parsePartial(`{"tool": "test", "parameters": {}}`);
      expect(calls).toHaveLength(1);
    });
  });

  describe("parseFromText (auto-detect)", () => {
    it("should auto-detect XML and parse first", () => {
      const text = `<tool_use><tool_name>xml_test</tool_name></tool_use>`;
      const calls = ToolCallParser.parseFromText(text);
      expect(calls[0]?.function.name).toBe("xml_test");
    });

    it("should auto-detect wrapped JSON", () => {
      const text = `<<<TOOL_CALL>>>{"tool":"json_test"}<<<END_TOOL_CALL>>>`;
      const calls = ToolCallParser.parseFromText(text);
      expect(calls[0]?.function.name).toBe("json_test");
    });

    it("should auto-detect raw JSON", () => {
      const text = `{"tool":"raw_auto"}`;
      const calls = ToolCallParser.parseFromText(text);
      expect(calls[0]?.function.name).toBe("raw_auto");
    });

    it("should respect preferredFormats order", () => {
      const text = `
        {"tool":"raw_first"}
        <<<TOOL_CALL>>>{"tool":"json_second"}<<<END_TOOL_CALL>>>
      `;

      const calls = ToolCallParser.parseFromText(text, {
        preferredFormats: ["raw", "json", "xml"],
      });

      expect(calls[0]?.function.name).toBe("raw_first");
    });

    it("should return empty array for unrecognized text", () => {
      const calls = ToolCallParser.parseFromText("just plain text");
      expect(calls).toEqual([]);
    });

    it("should handle null/undefined/empty input gracefully", () => {
      expect(ToolCallParser.parseFromText("")).toEqual([]);
      // @ts-expect-error testing invalid input
      expect(ToolCallParser.parseFromText(null)).toEqual([]);
      // @ts-expect-error testing invalid input
      expect(ToolCallParser.parseFromText(undefined)).toEqual([]);
    });
  });

  describe("Helper methods", () => {
    it("hasXMLToolCalls should detect <tool_use>", () => {
      expect(ToolCallParser.hasXMLToolCalls("<tool_use>")).toBe(true);
      expect(ToolCallParser.hasXMLToolCalls("nothing")).toBe(false);
    });

    it("hasJSONToolCalls should detect markers", () => {
      expect(ToolCallParser.hasJSONToolCalls("<<<TOOL_CALL>>>")).toBe(true);
      expect(ToolCallParser.hasJSONToolCalls("text")).toBe(false);
    });

    it("hasRawJsonToolCalls should detect JSON braces", () => {
      expect(ToolCallParser.hasRawJsonToolCalls("{...}")).toBe(true);
      expect(ToolCallParser.hasRawJsonToolCalls("[...]")).toBe(true);
      expect(ToolCallParser.hasRawJsonToolCalls("text")).toBe(false);
    });

    it("escapeRegExp should escape all special characters", () => {
      const escaped = ToolCallParser.escapeRegExp(".*+?^${}()|[]\\");
      expect(escaped).toBe("\\.\\*\\+\\?\\^\\$\\{\\}\\(\\)\\|\\[\\]\\\\");
    });

    it("generateToolCallId should produce valid format", () => {
      const id = ToolCallParser["generateToolCallId"]();
      expect(id).toMatch(/^call_\d+_[a-z0-9]{8}$/);
    });
  });
});
