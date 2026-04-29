# Tool Call Format Implementation - Code Examples

This document provides concrete code examples for implementing the tool call format features.

---

## Phase 2: ToolDeclarationFormatter Implementation

### File: `packages/common-utils/src/tool/declaration-formatter.ts`

```typescript
/**
 * Tool Declaration Formatter
 * 
 * Converts tool schemas and tool calls to XML/JSON format strings
 * for prompt injection and message history conversion.
 */

import type { ToolSchema, LLMToolCall, LLMMessage } from '@wf-agent/types';
import type { ToolCallFormatMarkers, ToolCallXmlTags } from '@wf-agent/types';
import { DEFAULT_JSON_MARKERS, DEFAULT_XML_TAGS } from '@wf-agent/types';

export interface ToolDeclarationOptions {
  /** Output format */
  format: 'xml' | 'json';
  /** Include parameter details */
  includeParameters?: boolean;
  /** Include description */
  includeDescription?: boolean;
  /** Custom XML tags (for XML format) */
  xmlTags?: ToolCallXmlTags;
  /** Custom markers (for JSON format) */
  markers?: ToolCallFormatMarkers;
}

export class ToolDeclarationFormatter {
  /**
   * Convert tool schemas to declaration string
   * Used for injecting into system prompt
   */
  static formatTools(tools: ToolSchema[], options: ToolDeclarationOptions): string {
    if (!tools || tools.length === 0) {
      return '';
    }

    if (options.format === 'xml') {
      return this.formatToolsXML(tools, options);
    } else {
      return this.formatToolsJSON(tools, options);
    }
  }

  /**
   * Format tools as XML
   */
  private static formatToolsXML(tools: ToolSchema[], options: ToolDeclarationOptions): string {
    const tags = options.xmlTags || DEFAULT_XML_TAGS;
    const parts: string[] = [`<${tags.toolUse.replace('_use', 's')}>`];

    for (const tool of tools) {
      parts.push(this.formatToolXML(tool, tags, options));
    }

    parts.push(`</${tags.toolUse.replace('_use', 's')}>`);
    return parts.join('\n');
  }

  /**
   * Format single tool as XML
   */
  private static formatToolXML(
    tool: ToolSchema,
    tags: ToolCallXmlTags,
    options: ToolDeclarationOptions
  ): string {
    const lines: string[] = [`  <tool name="${tool.id}">`];

    if (options.includeDescription !== false && tool.description) {
      lines.push(`    <description>${this.escapeXml(tool.description)}</description>`);
    }

    if (options.includeParameters !== false && tool.parameters?.properties) {
      lines.push('    <parameters>');
      const params = this.formatParametersXML(tool.parameters, tags);
      lines.push(params);
      lines.push('    </parameters>');
    }

    lines.push('  </tool>');
    return lines.join('\n');
  }

  /**
   * Format parameters as XML
   */
  private static formatParametersXML(
    parameters: { properties: Record<string, any>; required?: string[] },
    tags: ToolCallXmlTags
  ): string {
    const lines: string[] = [];
    const required = parameters.required || [];

    for (const [name, schema] of Object.entries(parameters.properties)) {
      const isRequired = required.includes(name);
      const reqStr = isRequired ? 'required' : 'optional';
      const type = schema.type || 'string';
      const desc = schema.description || '';

      lines.push(`      - ${name} (${reqStr}) [${type}]: ${desc}`);
    }

    return lines.join('\n');
  }

  /**
   * Format tools as JSON
   */
  private static formatToolsJSON(tools: ToolSchema[], options: ToolDeclarationOptions): string {
    const markers = options.markers || DEFAULT_JSON_MARKERS;
    const parts: string[] = [];

    for (const tool of tools) {
      const toolObj: any = {
        name: tool.id,
      };

      if (options.includeDescription !== false && tool.description) {
        toolObj.description = tool.description;
      }

      if (options.includeParameters !== false && tool.parameters?.properties) {
        toolObj.parameters = this.formatParametersJSON(tool.parameters);
      }

      parts.push(`${markers.start}\n${JSON.stringify(toolObj, null, 2)}\n${markers.end}`);
    }

    return parts.join('\n\n');
  }

  /**
   * Format parameters as JSON
   */
  private static formatParametersJSON(parameters: {
    properties: Record<string, any>;
    required?: string[];
  }): Record<string, any> {
    const result: Record<string, any> = {};
    const required = parameters.required || [];

    for (const [name, schema] of Object.entries(parameters.properties)) {
      result[name] = {
        type: schema.type || 'string',
        required: required.includes(name),
        description: schema.description || '',
      };
    }

    return result;
  }

  /**
   * Convert array of tool calls to text format
   */
  static formatToolCalls(toolCalls: LLMToolCall[], options: ToolDeclarationOptions): string {
    if (!toolCalls || toolCalls.length === 0) {
      return '';
    }

    if (options.format === 'xml') {
      return toolCalls.map(tc => this.formatToolCallXML(tc, options)).join('\n\n');
    } else {
      return toolCalls.map(tc => this.formatToolCallJSON(tc, options)).join('\n\n');
    }
  }

  /**
   * Format single tool call as XML
   */
  private static formatToolCallXML(toolCall: LLMToolCall, options: ToolDeclarationOptions): string {
    const tags = options.xmlTags || DEFAULT_XML_TAGS;
    const args = JSON.parse(toolCall.function.arguments || '{}');

    const lines: string[] = [
      `<${tags.toolUse}>`,
      `  <${tags.toolName}>${toolCall.function.name}</${tags.toolName}>`,
      `  <${tags.parameters}>`,
    ];

    for (const [key, value] of Object.entries(args)) {
      lines.push(`    <${key}>${this.escapeXml(String(value))}</${key}>`);
    }

    lines.push(`  </${tags.parameters}>`);
    lines.push(`</${tags.toolUse}>`);

    return lines.join('\n');
  }

  /**
   * Format single tool call as JSON
   */
  private static formatToolCallJSON(toolCall: LLMToolCall, options: ToolDeclarationOptions): string {
    const markers = options.markers || DEFAULT_JSON_MARKERS;
    const args = JSON.parse(toolCall.function.arguments || '{}');

    const obj = {
      tool: toolCall.function.name,
      parameters: args,
    };

    return `${markers.start}\n${JSON.stringify(obj, null, 2)}\n${markers.end}`;
  }

  /**
   * Format tool result as text
   */
  static formatToolResult(message: LLMMessage, options: ToolDeclarationOptions): string {
    if (options.format === 'xml') {
      return this.formatToolResultXML(message, options);
    } else {
      return this.formatToolResultJSON(message, options);
    }
  }

  /**
   * Format tool result as XML
   */
  private static formatToolResultXML(message: LLMMessage, options: ToolDeclarationOptions): string {
    const tags = options.xmlTags || DEFAULT_XML_TAGS;
    const content = typeof message.content === 'string' 
      ? message.content 
      : JSON.stringify(message.content);

    return `<${tags.toolUse}_result tool="${message.toolCallId || 'unknown'}">\n${content}\n</${tags.toolUse}_result>`;
  }

  /**
   * Format tool result as JSON
   */
  private static formatToolResultJSON(message: LLMMessage, options: ToolDeclarationOptions): string {
    const markers = options.markers || DEFAULT_JSON_MARKERS;
    const content = typeof message.content === 'string' 
      ? message.content 
      : JSON.stringify(message.content);

    const obj = {
      tool_result: message.toolCallId || 'unknown',
      content,
    };

    return `${markers.start}\n${JSON.stringify(obj, null, 2)}\n${markers.end}`;
  }

  /**
   * Escape special XML characters
   */
  private static escapeXml(str: string): string {
    return str
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&apos;');
  }
}
```

---

## Phase 4: HistoryConverter Implementation

### File: `sdk/core/messages/history-converter.ts`

```typescript
/**
 * History Converter
 * 
 * Converts message history between native function-calling format
 * and text-based formats (XML/JSON).
 */

import type { LLMMessage, ToolCallFormat, ToolCallFormatConfig } from '@wf-agent/types';
import { ToolDeclarationFormatter } from '@wf-agent/common-utils';
import { DEFAULT_XML_TAGS, DEFAULT_JSON_MARKERS } from '@wf-agent/types';

export interface HistoryConversionOptions {
  /** Target format */
  targetFormat: ToolCallFormat;
  /** Custom XML tags */
  xmlTags?: ToolCallFormatConfig['xmlTags'];
  /** Custom markers */
  markers?: ToolCallFormatConfig['markers'];
}

export class HistoryConverter {
  /**
   * Convert entire message history to text-based format
   */
  static convertToTextMode(
    messages: LLMMessage[],
    format: ToolCallFormat,
    options?: Partial<HistoryConversionOptions>
  ): LLMMessage[] {
    // No conversion needed for native mode
    if (format === 'function_call') {
      return messages;
    }

    return messages.map(msg => {
      if (msg.role === 'assistant' && msg.toolCalls?.length) {
        return this.convertAssistantMessage(msg, format, options);
      }
      if (msg.role === 'tool' && msg.toolCallId) {
        return this.convertToolResultMessage(msg, format, options);
      }
      return msg;
    });
  }

  /**
   * Convert assistant message with tool calls to text format
   */
  static convertAssistantMessage(
    message: LLMMessage,
    format: ToolCallFormat,
    options?: Partial<HistoryConversionOptions>
  ): LLMMessage {
    if (!message.toolCalls || message.toolCalls.length === 0) {
      return message;
    }

    const xmlTags = options?.xmlTags || DEFAULT_XML_TAGS;
    const markers = options?.markers || DEFAULT_JSON_MARKERS;

    const toolCallText = ToolDeclarationFormatter.formatToolCalls(
      message.toolCalls,
      {
        format: format === 'json_wrapped' || format === 'json_raw' ? 'json' : 'xml',
        xmlTags,
        markers,
      }
    );

    // Combine existing content with tool calls
    const combinedContent = message.content
      ? `${message.content}\n\n${toolCallText}`
      : toolCallText;

    // Return message without toolCalls field (they're now in content)
    return {
      role: 'assistant',
      content: combinedContent,
    };
  }

  /**
   * Convert tool result message to text format
   */
  static convertToolResultMessage(
    message: LLMMessage,
    format: ToolCallFormat,
    options?: Partial<HistoryConversionOptions>
  ): LLMMessage {
    const xmlTags = options?.xmlTags || DEFAULT_XML_TAGS;
    const markers = options?.markers || DEFAULT_JSON_MARKERS;

    const resultText = ToolDeclarationFormatter.formatToolResult(
      message,
      {
        format: format === 'json_wrapped' || format === 'json_raw' ? 'json' : 'xml',
        xmlTags,
        markers,
      }
    );

    // Tool results become user messages in text mode
    return {
      role: 'user',
      content: resultText,
    };
  }

  /**
   * Check if message needs conversion
   */
  static needsConversion(message: LLMMessage, targetFormat: ToolCallFormat): boolean {
    if (targetFormat === 'function_call') {
      return false;
    }

    // Assistant messages with toolCalls need conversion
    if (message.role === 'assistant' && message.toolCalls?.length) {
      return true;
    }

    // Tool messages need conversion
    if (message.role === 'tool' && message.toolCallId) {
      return true;
    }

    return false;
  }

  /**
   * Convert back from text mode to native mode (for debugging/testing)
   */
  static convertFromTextMode(
    messages: LLMMessage[],
    sourceFormat: ToolCallFormat
  ): LLMMessage[] {
    // This is complex and may require parsing
    // For now, we'll skip this implementation
    // In practice, you'd use ToolCallParser to extract tool calls from content
    throw new Error('convertFromTextMode not yet implemented');
  }
}
```

---

## Phase 3: Formatter Enhancement Examples

### Example: OpenAIChatFormatter Text Mode

**File**: `sdk/core/llm/formatters/openai-chat.ts`

```typescript
export class OpenAIChatFormatter extends BaseFormatter {

  buildRequest(request: LLMRequest, config: FormatterConfig): BuildRequestResult {
    // Use mode-aware building
    if (this.isTextBasedToolMode(config)) {
      return this.buildTextModeRequest(request, config);
    }
    return this.buildNativeRequest(request, config);
  }

  parseResponse(data: unknown, config: FormatterConfig): LLMResult {
    // Use mode-aware parsing
    if (this.isTextBasedToolMode(config)) {
      return this.parseTextModeResponse(data, config);
    }
    return this.parseNativeResponse(data, config);
  }

  protected buildNativeRequest(
    request: LLMRequest,
    config: FormatterConfig
  ): BuildRequestResult {
    // Current implementation unchanged
    const body = this.buildRequestBody(request, config);
    
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      ...config.profile.headers,
    };

    if (config.profile.apiKey) {
      Object.assign(headers, this.buildAuthHeader(config.profile.apiKey, config, "Authorization"));
    }

    Object.assign(headers, this.buildCustomHeaders(config));
    const finalBody = this.applyCustomBody(body, config);
    const queryString = this.buildQueryString(config);

    return {
      httpRequest: {
        url: `/chat/completions${queryString}`,
        method: "POST",
        headers,
        body: finalBody,
        timeout: config.timeout,
      },
      transformedBody: finalBody,
    };
  }

  protected buildTextModeRequest(
    request: LLMRequest,
    config: FormatterConfig
  ): BuildRequestResult {
    const format = this.getToolCallFormat(config);

    // 1. Generate tool declarations
    const toolDeclarations = ToolDeclarationFormatter.formatTools(
      request.tools || [],
      {
        format: format === 'json_wrapped' || format === 'json_raw' ? 'json' : 'xml',
        xmlTags: config.toolCallFormat?.xmlTags,
        markers: config.toolCallFormat?.markers,
        includeDescription: config.toolCallFormat?.includeDescription,
      }
    );

    // 2. Convert history to text mode
    const convertedMessages = HistoryConverter.convertToTextMode(
      request.messages,
      format,
      {
        xmlTags: config.toolCallFormat?.xmlTags,
        markers: config.toolCallFormat?.markers,
      }
    );

    // 3. Inject tool declarations into system message
    const messagesWithTools = this.injectToolDeclarations(
      convertedMessages,
      toolDeclarations,
      format
    );

    // 4. Build request WITHOUT tools field
    const parameters = this.mergeParameters(config.profile.parameters, request.parameters);
    const body: Record<string, unknown> = {
      model: config.profile.model,
      messages: this.convertMessages(messagesWithTools),
      stream: config.stream || false,
    };

    const { stream: _stream, ...otherParams } = parameters;
    Object.assign(body, otherParams);

    // NO tools field!

    if (config.stream && config.streamOptions?.includeUsage) {
      body["stream_options"] = this.buildStreamOptions(config);
    }

    // Build headers (same as native)
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      ...config.profile.headers,
    };

    if (config.profile.apiKey) {
      Object.assign(headers, this.buildAuthHeader(config.profile.apiKey, config, "Authorization"));
    }

    Object.assign(headers, this.buildCustomHeaders(config));
    const finalBody = this.applyCustomBody(body, config);
    const queryString = this.buildQueryString(config);

    return {
      httpRequest: {
        url: `/chat/completions${queryString}`,
        method: "POST",
        headers,
        body: finalBody,
        timeout: config.timeout,
      },
      transformedBody: finalBody,
    };
  }

  protected parseNativeResponse(data: unknown, config: FormatterConfig): LLMResult {
    // Current implementation unchanged
    const dataRecord = data as Record<string, unknown>;
    const choices = dataRecord["choices"] as Array<Record<string, unknown>> | undefined;
    const choice = choices?.[0];
    const message = choice?.["message"] as Record<string, unknown> | undefined;

    const reasoningContent = message?.["reasoning_content"] as string | undefined;
    const usage = dataRecord["usage"] as Record<string, unknown> | undefined;
    const completionTokensDetails = usage?.["completion_tokens_details"] as Record<string, number> | undefined;
    const reasoningTokens = completionTokensDetails?.["reasoning_tokens"] || 0;

    return {
      id: dataRecord["id"] as string,
      model: dataRecord["model"] as string,
      content: (message?.["content"] as string) || "",
      message: this.parseMessage(message),
      toolCalls: message?.["tool_calls"]
        ? this.parseToolCalls(message["tool_calls"] as unknown[])
        : undefined,
      usage: usage
        ? {
            promptTokens: usage["prompt_tokens"] as number,
            completionTokens: usage["completion_tokens"] as number,
            totalTokens: usage["total_tokens"] as number,
            reasoningTokens: reasoningTokens > 0 ? reasoningTokens : undefined,
          }
        : undefined,
      finishReason: choice?.["finish_reason"] as string,
      duration: 0,
      metadata: {
        created: dataRecord["created"] as number,
        systemFingerprint: dataRecord["system_fingerprint"] as string,
      },
      reasoningContent,
      reasoningTokens: reasoningTokens > 0 ? reasoningTokens : undefined,
    };
  }

  protected parseTextModeResponse(data: unknown, config: FormatterConfig): LLMResult {
    const dataRecord = data as Record<string, unknown>;
    const choices = dataRecord["choices"] as Array<Record<string, unknown>> | undefined;
    const choice = choices?.[0];
    const message = choice?.["message"] as Record<string, unknown> | undefined;

    const content = (message?.["content"] as string) || "";
    const format = this.getToolCallFormat(config);

    // Parse tool calls from content using ToolCallParser
    const toolCalls = ToolCallParser.parseFromText(
      content,
      getToolCallParserOptions(config.toolCallFormat)
    );

    const reasoningContent = message?.["reasoning_content"] as string | undefined;
    const usage = dataRecord["usage"] as Record<string, unknown> | undefined;

    return {
      id: dataRecord["id"] as string,
      model: dataRecord["model"] as string,
      content,
      message: {
        role: "assistant",
        content,
        toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      },
      toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      usage: usage
        ? {
            promptTokens: (usage["prompt_tokens"] as number) || 0,
            completionTokens: (usage["completion_tokens"] as number) || 0,
            totalTokens: (usage["total_tokens"] as number) || 0,
          }
        : undefined,
      finishReason: choice?.["finish_reason"] as string,
      duration: 0,
      metadata: {
        created: dataRecord["created"] as number,
        systemFingerprint: dataRecord["system_fingerprint"] as string,
      },
      reasoningContent,
    };
  }

  private injectToolDeclarations(
    messages: LLMMessage[],
    toolDeclarations: string,
    format: ToolCallFormat
  ): LLMMessage[] {
    const instructions = this.getToolUsageInstructions(format);
    const fullInjection = `${instructions}\n\n${toolDeclarations}`;

    const systemMsgIndex = messages.findIndex(m => m.role === 'system');

    if (systemMsgIndex >= 0) {
      const updated = [...messages];
      updated[systemMsgIndex] = {
        ...updated[systemMsgIndex],
        content: `${updated[systemMsgIndex].content}\n\n${fullInjection}`,
      };
      return updated;
    } else {
      return [
        {
          role: 'system',
          content: fullInjection,
        },
        ...messages,
      ];
    }
  }

  private getToolUsageInstructions(format: ToolCallFormat): string {
    if (format === 'xml') {
      return `## Tool Usage Instructions

When you need to use a tool, format your response as follows:

<tool_use>
  <tool_name>tool_name_here</tool_name>
  <parameters>
    <param1>value1</param1>
    <param2>value2</param2>
  </parameters>
</tool_use>

You can use multiple tools in one response by including multiple <tool_use> blocks.`;
    } else if (format === 'json_wrapped') {
      return `## Tool Usage Instructions

When you need to use a tool, format your response as follows:

<<<TOOL_CALL>>>
{
  "tool": "tool_name_here",
  "parameters": {
    "param1": "value1",
    "param2": "value2"
  }
}
<<<END_TOOL_CALL>>>

You can use multiple tools in one response by including multiple blocks.`;
    }

    return '';
  }
}
```

---

## Test Examples

### Unit Test: ToolDeclarationFormatter

**File**: `packages/common-utils/__tests__/tool/declaration-formatter.test.ts`

```typescript
import { describe, it, expect } from 'vitest';
import { ToolDeclarationFormatter } from '../../src/tool/declaration-formatter';
import type { ToolSchema } from '@wf-agent/types';

describe('ToolDeclarationFormatter', () => {
  const mockTools: ToolSchema[] = [
    {
      id: 'calculator',
      description: 'Performs arithmetic calculations',
      parameters: {
        type: 'object',
        properties: {
          a: { type: 'number', description: 'First operand' },
          b: { type: 'number', description: 'Second operand' },
          operation: { 
            type: 'string', 
            description: 'Operation: add, subtract, multiply, divide',
            enum: ['add', 'subtract', 'multiply', 'divide']
          },
        },
        required: ['a', 'b', 'operation'],
      },
    },
  ];

  describe('formatTools - XML', () => {
    it('should generate valid XML format', () => {
      const result = ToolDeclarationFormatter.formatTools(mockTools, {
        format: 'xml',
        includeDescription: true,
        includeParameters: true,
      });

      expect(result).toContain('<tools>');
      expect(result).toContain('<tool name="calculator">');
      expect(result).toContain('<description>Performs arithmetic calculations</description>');
      expect(result).toContain('<parameters>');
      expect(result).toContain('- a (required) [number]: First operand');
    });

    it('should exclude description when disabled', () => {
      const result = ToolDeclarationFormatter.formatTools(mockTools, {
        format: 'xml',
        includeDescription: false,
        includeParameters: true,
      });

      expect(result).not.toContain('<description>');
    });
  });

  describe('formatTools - JSON', () => {
    it('should generate valid JSON format with markers', () => {
      const result = ToolDeclarationFormatter.formatTools(mockTools, {
        format: 'json',
        includeDescription: true,
        includeParameters: true,
      });

      expect(result).toContain('<<<TOOL_CALL>>>');
      expect(result).toContain('<<<END_TOOL_CALL>>>');
      
      const jsonMatch = result.match(/<<<TOOL_CALL>>>([\s\S]*?)<<<END_TOOL_CALL>>>/);
      expect(jsonMatch).toBeTruthy();
      
      const parsed = JSON.parse(jsonMatch![1]);
      expect(parsed.name).toBe('calculator');
      expect(parsed.description).toBe('Performs arithmetic calculations');
    });
  });

  describe('formatToolCalls', () => {
    it('should format tool calls as XML', () => {
      const toolCalls = [
        {
          id: 'call_123',
          type: 'function' as const,
          function: {
            name: 'calculator',
            arguments: '{"a":10,"b":5,"operation":"add"}',
          },
        },
      ];

      const result = ToolDeclarationFormatter.formatToolCalls(toolCalls, {
        format: 'xml',
      });

      expect(result).toContain('<tool_use>');
      expect(result).toContain('<tool_name>calculator</tool_name>');
      expect(result).toContain('<a>10</a>');
      expect(result).toContain('<b>5</b>');
      expect(result).toContain('<operation>add</operation>');
    });

    it('should format tool calls as JSON', () => {
      const toolCalls = [
        {
          id: 'call_123',
          type: 'function' as const,
          function: {
            name: 'calculator',
            arguments: '{"a":10,"b":5,"operation":"add"}',
          },
        },
      ];

      const result = ToolDeclarationFormatter.formatToolCalls(toolCalls, {
        format: 'json',
      });

      expect(result).toContain('<<<TOOL_CALL>>>');
      expect(result).toContain('"tool": "calculator"');
      expect(result).toContain('"a": 10');
    });
  });
});
```

### Integration Test: OpenAI XML Mode

**File**: `sdk/core/llm/formatters/__tests__/openai-chat-text-mode.test.ts`

```typescript
import { describe, it, expect, beforeEach } from 'vitest';
import { OpenAIChatFormatter } from '../openai-chat';
import type { LLMRequest, LLMProfile, ToolSchema } from '@wf-agent/types';
import type { FormatterConfig } from '../types';

describe('OpenAIChatFormatter - Text Mode', () => {
  let formatter: OpenAIChatFormatter;
  let config: FormatterConfig;

  const mockTools: ToolSchema[] = [
    {
      id: 'calculator',
      description: 'Performs calculations',
      parameters: {
        type: 'object',
        properties: {
          a: { type: 'number', description: 'First number' },
          b: { type: 'number', description: 'Second number' },
        },
        required: ['a', 'b'],
      },
    },
  ];

  beforeEach(() => {
    formatter = new OpenAIChatFormatter();

    const profile: LLMProfile = {
      id: 'test-profile',
      name: 'Test Profile',
      provider: 'OPENAI_CHAT',
      model: 'gpt-4',
      apiKey: 'test-key',
      parameters: { temperature: 0.7 },
      toolCallFormat: {
        format: 'xml',
        includeDescription: true,
        descriptionStyle: 'compact',
      },
    };

    config = {
      profile,
      stream: false,
      tools: mockTools,
    };
  });

  it('should build request without tools field in XML mode', () => {
    const request: LLMRequest = {
      messages: [
        { role: 'user', content: 'Calculate 10 + 5' },
      ],
      tools: mockTools,
      parameters: {},
    };

    const result = formatter.buildRequest(request, config);

    expect(result.httpRequest.body).toBeDefined();
    const body = result.httpRequest.body as Record<string, unknown>;
    
    // Should NOT have tools field
    expect(body.tools).toBeUndefined();
    
    // Should have messages with tool declarations in system message
    expect(body.messages).toBeDefined();
    const messages = body.messages as any[];
    const systemMsg = messages.find(m => m.role === 'system');
    expect(systemMsg).toBeDefined();
    expect(systemMsg.content).toContain('<tools>');
    expect(systemMsg.content).toContain('<tool name="calculator">');
  });

  it('should parse XML tool calls from response content', () => {
    const responseData = {
      id: 'chatcmpl-123',
      model: 'gpt-4',
      choices: [
        {
          message: {
            role: 'assistant',
            content: `Let me calculate that.

<tool_use>
  <tool_name>calculator</tool_name>
  <parameters>
    <a>10</a>
    <b>5</b>
  </parameters>
</tool_use>`,
          },
          finish_reason: 'stop',
        },
      ],
      usage: {
        prompt_tokens: 50,
        completion_tokens: 100,
        total_tokens: 150,
      },
    };

    const result = formatter.parseResponse(responseData, config);

    expect(result.content).toContain('Let me calculate that');
    expect(result.toolCalls).toBeDefined();
    expect(result.toolCalls!.length).toBe(1);
    expect(result.toolCalls![0].function.name).toBe('calculator');
    
    const args = JSON.parse(result.toolCalls![0].function.arguments);
    expect(args.a).toBe(10);
    expect(args.b).toBe(5);
  });
});
```

---

## Usage Examples

### Example 1: Configure Profile with XML Mode

```typescript
import type { LLMProfile } from '@wf-agent/types';

const xmlProfile: LLMProfile = {
  id: 'openai-xml-tools',
  name: 'OpenAI with XML Tools',
  provider: 'OPENAI_CHAT',
  model: 'gpt-4-turbo',
  apiKey: process.env.OPENAI_API_KEY!,
  parameters: {
    temperature: 0.7,
    max_tokens: 4096,
  },
  toolCallFormat: {
    format: 'xml',
    includeDescription: true,
    descriptionStyle: 'compact',
    includeExamples: true,
    includeRules: true,
  },
};
```

### Example 2: Using in Execution

```typescript
import { createLLMClient } from '@wf-agent/sdk';

const client = createLLMClient(xmlProfile);

const result = await client.generate({
  messages: [
    { role: 'user', content: 'What is 10 + 5?' },
  ],
  tools: [calculatorTool],
});

// Result will contain parsed tool calls from XML format
if (result.toolCalls) {
  console.log('Tool calls:', result.toolCalls);
}
```

### Example 3: Streaming with XML Mode

```typescript
const stream = client.stream({
  messages: [
    { role: 'user', content: 'Calculate multiple things' },
  ],
  tools: [calculatorTool, weatherTool],
});

for await (const chunk of stream) {
  if (chunk.delta) {
    process.stdout.write(chunk.delta);
  }
  
  if (chunk.toolCallsDelta) {
    console.log('New tool calls:', chunk.toolCallsDelta);
  }
  
  if (chunk.done) {
    console.log('Stream complete');
  }
}
```

---

*Last updated: 2026-04-29*
