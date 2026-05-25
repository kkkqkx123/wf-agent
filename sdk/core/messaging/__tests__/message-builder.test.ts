/**
 * Message Builder Tests
 */
import { describe, it, expect } from 'vitest';
import { MessageBuilder } from '../message-builder.js';

describe('MessageBuilder', () => {
  describe('buildUserMessage', () => {
    it('should build a user message with content', () => {
      const message = MessageBuilder.buildUserMessage('Hello');
      expect(message).toEqual({
        role: 'user',
        content: 'Hello',
      });
    });

    it('should build a user message with empty content', () => {
      const message = MessageBuilder.buildUserMessage('');
      expect(message).toEqual({
        role: 'user',
        content: '',
      });
    });

    it('should build a user message with special characters', () => {
      const content = 'Special chars: \n\t\r\b\f';
      const message = MessageBuilder.buildUserMessage(content);
      expect(message.role).toBe('user');
      expect(message.content).toBe(content);
    });
  });

  describe('buildAssistantMessage', () => {
    it('should build an assistant message with content only', () => {
      const message = MessageBuilder.buildAssistantMessage('Hi there');
      expect(message).toEqual({
        role: 'assistant',
        content: 'Hi there',
      });
      expect(message.toolCalls).toBeUndefined();
      expect(message.thinking).toBeUndefined();
    });

    it('should build an assistant message with tool calls', () => {
      const toolCalls = [
        { id: 'call_1', name: 'get_weather', arguments: { city: 'Beijing' } },
      ];
      const message = MessageBuilder.buildAssistantMessage('Let me check', toolCalls);
      expect(message.role).toBe('assistant');
      expect(message.content).toBe('Let me check');
      expect(message.toolCalls).toEqual(toolCalls);
      expect(message.thinking).toBeUndefined();
    });

    it('should build an assistant message with thinking', () => {
      const thinking = 'I need to calculate...';
      const message = MessageBuilder.buildAssistantMessage('The answer is 42', undefined, thinking);
      expect(message.role).toBe('assistant');
      expect(message.content).toBe('The answer is 42');
      expect(message.thinking).toBe(thinking);
      expect(message.toolCalls).toBeUndefined();
    });

    it('should build an assistant message with both tool calls and thinking', () => {
      const toolCalls = [
        { id: 'call_2', name: 'search', arguments: { query: 'test' } },
      ];
      const thinking = 'Searching...';
      const message = MessageBuilder.buildAssistantMessage('Searching now', toolCalls, thinking);
      expect(message.role).toBe('assistant');
      expect(message.content).toBe('Searching now');
      expect(message.toolCalls).toEqual(toolCalls);
      expect(message.thinking).toBe(thinking);
    });

    it('should not set toolCalls property when empty array is passed', () => {
      const message = MessageBuilder.buildAssistantMessage('No tools', []);
      expect(message.toolCalls).toBeUndefined();
    });
  });

  describe('buildToolResultMessage', () => {
    it('should build a tool result message', () => {
      const message = MessageBuilder.buildToolResultMessage('call_1', 'Result data');
      expect(message).toEqual({
        role: 'tool',
        toolCallId: 'call_1',
        content: 'Result data',
      });
    });

    it('should build a tool result message with empty content', () => {
      const message = MessageBuilder.buildToolResultMessage('call_2', '');
      expect(message).toEqual({
        role: 'tool',
        toolCallId: 'call_2',
        content: '',
      });
    });
  });

  describe('buildSystemMessage', () => {
    it('should build a system message', () => {
      const message = MessageBuilder.buildSystemMessage('System instruction');
      expect(message).toEqual({
        role: 'system',
        content: 'System instruction',
      });
    });

    it('should build a system message with empty content', () => {
      const message = MessageBuilder.buildSystemMessage('');
      expect(message).toEqual({
        role: 'system',
        content: '',
      });
    });
  });

  describe('buildToolDescriptionMessage', () => {
    it('should build a tool description message', () => {
      const message = MessageBuilder.buildToolDescriptionMessage('Tool description text');
      expect(message).not.toBeNull();
      expect(message).toEqual({
        role: 'system',
        content: 'Tool description text',
      });
    });

    it('should return null for empty description', () => {
      const message = MessageBuilder.buildToolDescriptionMessage('');
      expect(message).toBeNull();
    });

    it('should return null for whitespace-only description', () => {
      const message = MessageBuilder.buildToolDescriptionMessage('   ');
      expect(message).not.toBeNull();
      expect(message!.content).toBe('   ');
    });
  });
});
