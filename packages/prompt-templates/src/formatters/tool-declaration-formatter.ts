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
