import { describe, it, expect } from 'vitest';
import { ToolDeclarationFormatter } from '../../src/formatters/tool-declaration-formatter';
import type { ToolSchema, LLMToolCall, LLMMessage } from '@wf-agent/types';

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
            enum: ['add', 'subtract', 'multiply', 'divide'],
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

    it('should exclude parameters when disabled', () => {
      const result = ToolDeclarationFormatter.formatTools(mockTools, {
        format: 'xml',
        includeDescription: true,
        includeParameters: false,
      });

      expect(result).not.toContain('<parameters>');
    });

    it('should escape special XML characters', () => {
      const toolsWithSpecialChars: ToolSchema[] = [
        {
          id: 'test-tool',
          description: 'Test with <special> & "characters"',
          parameters: {
            type: 'object',
            properties: {},
            required: [],
          },
        },
      ];

      const result = ToolDeclarationFormatter.formatTools(toolsWithSpecialChars, {
        format: 'xml',
        includeDescription: true,
        includeParameters: false,
      });

      expect(result).toContain('&lt;special&gt;');
      expect(result).toContain('&amp;');
      expect(result).toContain('&quot;');
    });

    it('should return empty string for empty tools array', () => {
      const result = ToolDeclarationFormatter.formatTools([], {
        format: 'xml',
      });

      expect(result).toBe('');
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

    it('should use custom markers when provided', () => {
      const result = ToolDeclarationFormatter.formatTools(mockTools, {
        format: 'json',
        markers: {
          start: '[TOOL]',
          end: '[/TOOL]',
        },
      });

      expect(result).toContain('[TOOL]');
      expect(result).toContain('[/TOOL]');
      expect(result).not.toContain('<<<TOOL_CALL>>>');
    });

    it('should exclude description when disabled', () => {
      const result = ToolDeclarationFormatter.formatTools(mockTools, {
        format: 'json',
        includeDescription: false,
        includeParameters: true,
      });

      const jsonMatch = result.match(/<<<TOOL_CALL>>>([\s\S]*?)<<<END_TOOL_CALL>>>/);
      const parsed = JSON.parse(jsonMatch![1]);
      expect(parsed.description).toBeUndefined();
    });
  });

  describe('formatToolCalls', () => {
    const mockToolCalls: LLMToolCall[] = [
      {
        id: 'call_123',
        type: 'function' as const,
        function: {
          name: 'calculator',
          arguments: '{"a":10,"b":5,"operation":"add"}',
        },
      },
    ];

    it('should format tool calls as XML', () => {
      const result = ToolDeclarationFormatter.formatToolCalls(mockToolCalls, {
        format: 'xml',
      });

      expect(result).toContain('<tool_use>');
      expect(result).toContain('<tool_name>calculator</tool_name>');
      expect(result).toContain('<a>10</a>');
      expect(result).toContain('<b>5</b>');
      expect(result).toContain('<operation>add</operation>');
    });

    it('should format tool calls as JSON', () => {
      const result = ToolDeclarationFormatter.formatToolCalls(mockToolCalls, {
        format: 'json',
      });

      expect(result).toContain('<<<TOOL_CALL>>>');
      expect(result).toContain('"tool": "calculator"');
      expect(result).toContain('"a": 10');
    });

    it('should handle multiple tool calls', () => {
      const multipleToolCalls: LLMToolCall[] = [
        {
          id: 'call_1',
          type: 'function' as const,
          function: {
            name: 'tool1',
            arguments: '{"param":"value1"}',
          },
        },
        {
          id: 'call_2',
          type: 'function' as const,
          function: {
            name: 'tool2',
            arguments: '{"param":"value2"}',
          },
        },
      ];

      const result = ToolDeclarationFormatter.formatToolCalls(multipleToolCalls, {
        format: 'xml',
      });

      expect(result).toContain('<tool_name>tool1</tool_name>');
      expect(result).toContain('<tool_name>tool2</tool_name>');
    });

    it('should return empty string for empty tool calls array', () => {
      const result = ToolDeclarationFormatter.formatToolCalls([], {
        format: 'xml',
      });

      expect(result).toBe('');
    });
  });

  describe('formatToolResult', () => {
    const mockToolMessage: LLMMessage = {
      role: 'tool',
      content: 'Result: 15',
      toolCallId: 'call_123',
    };

    it('should format tool result as XML', () => {
      const result = ToolDeclarationFormatter.formatToolResult(mockToolMessage, {
        format: 'xml',
      });

      expect(result).toContain('<tool_use_result tool="call_123">');
      expect(result).toContain('Result: 15');
      expect(result).toContain('</tool_use_result>');
    });

    it('should format tool result as JSON', () => {
      const result = ToolDeclarationFormatter.formatToolResult(mockToolMessage, {
        format: 'json',
      });

      expect(result).toContain('<<<TOOL_CALL>>>');
      expect(result).toContain('"tool_result": "call_123"');
      expect(result).toContain('"content": "Result: 15"');
    });

    it('should handle non-string content', () => {
      const messageWithObjectContent: LLMMessage = {
        role: 'tool',
        content: JSON.stringify({ result: 15, status: 'success' }),
        toolCallId: 'call_456',
      };

      const result = ToolDeclarationFormatter.formatToolResult(messageWithObjectContent, {
        format: 'json',
      });

      expect(result).toContain('"tool_result": "call_456"');
      // Content is stringified, so we look for the escaped version
      expect(result).toContain('\\"result\\":15');
    });
  });
});
