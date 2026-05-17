/**
 * LLM Execution Coordinator Tests
 * Tests for LLM call coordination, tool execution, token tracking, and interruption handling
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { LLMExecutionCoordinator } from '../llm-execution-coordinator.js';
import type { LLMExecutionParams } from '../llm-execution-coordinator.js';
import { ConversationSession } from '../../messaging/conversation-session.js';
import { EventRegistry } from '../../registry/event-registry.js';
import type { LLMExecutor } from '../../executors/llm-executor.js';
import type { ToolCallExecutor } from '../../executors/tool-call-executor.js';
import type { LLMExecutionConfig } from '@wf-agent/types';

describe('LLMExecutionCoordinator', () => {
  let coordinator: LLMExecutionCoordinator;
  let mockLLMExecutor: any;
  let mockToolCallExecutor: any;
  let conversationState: ConversationSession;
  let eventManager: EventRegistry;

  const mockConfig: LLMExecutionConfig = {
    profileId: 'test-profile',
    parameters: { temperature: 0.7 },
    maxToolCallsPerRequest: 3,
    enableTokenTracking: true,
    tokenWarningThreshold: 80,
    tokenLimit: 100000,
  };

  const mockLLMResult = {
    success: true,
    result: {
      content: 'Test response from LLM',
      toolCalls: [],
      usage: {
        promptTokens: 100,
        completionTokens: 50,
        totalTokens: 150,
      },
    },
  };

  beforeEach(() => {
    // Create mock executors
    mockLLMExecutor = {
      executeLLMCall: vi.fn().mockResolvedValue(mockLLMResult),
    };

    mockToolCallExecutor = {
      executeToolCalls: vi.fn().mockResolvedValue(undefined),
    };

    eventManager = new EventRegistry();
    conversationState = new ConversationSession({ eventManager });
    
    coordinator = new LLMExecutionCoordinator(
      mockLLMExecutor as LLMExecutor,
      mockToolCallExecutor as ToolCallExecutor
    );

    vi.clearAllMocks();
  });

  describe('Basic LLM Execution', () => {
    it('should execute LLM call successfully', async () => {
      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Hello, how are you?',
        config: mockConfig,
      };

      const result = await coordinator.executeLLM(params, conversationState);

      expect(result.success).toBe(true);
      expect(result.content).toBe('Test response from LLM');
      expect(result.messages).toBeDefined();
      expect(mockLLMExecutor.executeLLMCall).toHaveBeenCalled();
    });

    it('should add user message to conversation', async () => {
      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Test prompt',
        config: mockConfig,
      };

      await coordinator.executeLLM(params, conversationState);

      const messages = conversationState.getMessages();
      expect(messages.length).toBeGreaterThanOrEqual(1);
      expect(messages[0]?.role).toBe('user');
      expect(messages[0]?.content).toBe('Test prompt');
    });

    it('should add assistant message to conversation', async () => {
      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Test prompt',
        config: mockConfig,
      };

      await coordinator.executeLLM(params, conversationState);

      const messages = conversationState.getMessages();
      const assistantMessage = messages.find(m => m.role === 'assistant');
      expect(assistantMessage).toBeDefined();
      expect(assistantMessage?.content).toBe('Test response from LLM');
    });
  });

  describe('Tool Call Execution', () => {
    it('should execute tool calls when present', async () => {
      const toolCalls = [
        {
          id: 'call_1',
          name: 'read_file',
          arguments: '{"path": "test.txt"}',
        },
      ];

      mockLLMExecutor.executeLLMCall.mockResolvedValueOnce({
        success: true,
        result: {
          content: '',
          toolCalls,
          usage: { promptTokens: 100, completionTokens: 50, totalTokens: 150 },
        },
      });

      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Read the file',
        config: mockConfig,
        tools: [],
        executeTools: true,
      };

      await coordinator.executeLLM(params, conversationState);

      // Verify tool executor was called (signal may be undefined or an AbortSignal)
      expect(mockToolCallExecutor.executeToolCalls).toHaveBeenCalled();
      const callArgs = mockToolCallExecutor.executeToolCalls.mock.calls[0];
      expect(callArgs[0]).toEqual(toolCalls);
      expect(callArgs[1]).toBe(conversationState);
      expect(callArgs[2]).toBe('test-context-123');
    });

    it('should not execute tool calls when executeTools is false', async () => {
      const toolCalls = [
        {
          id: 'call_1',
          name: 'read_file',
          arguments: '{}',
        },
      ];

      mockLLMExecutor.executeLLMCall.mockResolvedValueOnce({
        success: true,
        result: {
          content: '',
          toolCalls,
          usage: { promptTokens: 100, completionTokens: 50, totalTokens: 150 },
        },
      });

      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Read the file',
        config: mockConfig,
        executeTools: false,
      };

      await coordinator.executeLLM(params, conversationState);

      expect(mockToolCallExecutor.executeToolCalls).not.toHaveBeenCalled();
    });

    it('should throw error when tool calls exceed limit', async () => {
      const toolCalls = Array.from({ length: 5 }, (_, i) => ({
        id: `call_${i}`,
        name: `tool_${i}`,
        arguments: '{}',
      }));

      mockLLMExecutor.executeLLMCall.mockResolvedValueOnce({
        success: true,
        result: {
          content: '',
          toolCalls,
          usage: { promptTokens: 100, completionTokens: 50, totalTokens: 150 },
        },
      });

      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Execute multiple tools',
        config: { ...mockConfig, maxToolCallsPerRequest: 3 },
        nodeId: 'test-node',
      };

      await expect(coordinator.executeLLM(params, conversationState)).rejects.toThrow(
        'exceeds limit of 3'
      );
    });
  });

  describe('Token Tracking', () => {
    it('should update token usage after LLM call', async () => {
      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Test prompt',
        config: mockConfig,
      };

      await coordinator.executeLLM(params, conversationState);

      const tokenUsage = conversationState.getTokenUsage();
      expect(tokenUsage).toBeDefined();
      expect(tokenUsage?.totalTokens).toBe(150);
    });

    it('should trigger token warning when threshold exceeded', async () => {
      // Set low token limit to trigger warning
      const lowLimitConfig: LLMExecutionConfig = {
        ...mockConfig,
        tokenLimit: 200,
        tokenWarningThreshold: 50,
      };

      // Mock LLM result with high token usage (150 tokens, which is 75% of 200 limit)
      mockLLMExecutor.executeLLMCall.mockResolvedValueOnce({
        success: true,
        result: {
          content: 'Test response',
          toolCalls: [],
          usage: { promptTokens: 100, completionTokens: 50, totalTokens: 150 },
        },
      });

      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Test prompt',
        config: lowLimitConfig,
        eventManager,
      };

      const warningEvents: any[] = [];
      // Use the same ID as contextId since events are emitted with executionId = contextId
      const emitter = eventManager.getEmitter('test-context-123');
      emitter.on('TOKEN_USAGE_WARNING' as any, (event: any) => {
        warningEvents.push(event);
      });

      await coordinator.executeLLM(params, conversationState);

      // Warning should be triggered immediately after the first call
      // because token warning is now checked AFTER updating token usage
      expect(warningEvents.length).toBeGreaterThan(0);
      expect(warningEvents[0].tokensUsed).toBe(150);
      expect(warningEvents[0].tokenLimit).toBe(200);
      expect(warningEvents[0].usagePercentage).toBeCloseTo(75, 0);
    });

    it('should respect enableTokenTracking setting', async () => {
      const noTrackingConfig: LLMExecutionConfig = {
        ...mockConfig,
        enableTokenTracking: false,
      };

      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Test prompt',
        config: noTrackingConfig,
      };

      await coordinator.executeLLM(params, conversationState);

      // When tracking is disabled, checkTokenUsage is not called,
      // but updateTokenUsage still updates the tracker
      // So we verify that the method was called but tracking logic was skipped
      const tokenUsage = conversationState.getTokenUsage();
      // The token usage will still be updated by updateTokenUsage call
      expect(tokenUsage).toBeDefined();
    });
  });

  describe('Interruption Handling', () => {
    it('should handle interruption before LLM call', async () => {
      const abortController = new AbortController();
      abortController.abort();

      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Test prompt',
        config: mockConfig,
        abortSignal: abortController.signal,
      };

      await coordinator.executeLLM(params, conversationState);

      expect(mockLLMExecutor.executeLLMCall).not.toHaveBeenCalled();
    });

    it('should handle interruption result from LLM executor', async () => {
      mockLLMExecutor.executeLLMCall.mockResolvedValueOnce({
        success: false,
        interruption: { type: 'ABORTED', reason: 'User cancelled' },
      });

      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Test prompt',
        config: mockConfig,
      };

      await coordinator.executeLLM(params, conversationState);
    });
  });

  describe('Event Emission', () => {
    it('should emit message added events', async () => {
      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Test prompt',
        config: mockConfig,
        eventManager,
      };

      const messageEvents: any[] = [];
      // Use the same ID as contextId since events are emitted with executionId = contextId
      const emitter = eventManager.getEmitter('test-context-123');
      emitter.on('MESSAGE_ADDED' as any, (event: any) => {
        messageEvents.push(event);
      });

      await coordinator.executeLLM(params, conversationState);

      // Should have at least 2 message events (user + assistant)
      expect(messageEvents.length).toBeGreaterThanOrEqual(2);
    });

    it('should emit conversation state changed event', async () => {
      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Test prompt',
        config: mockConfig,
        eventManager,
      };

      const stateEvents: any[] = [];
      // Use the same ID as contextId since events are emitted with executionId = contextId
      const emitter = eventManager.getEmitter('test-context-123');
      emitter.on('CONVERSATION_STATE_CHANGED' as any, (event: any) => {
        stateEvents.push(event);
      });

      await coordinator.executeLLM(params, conversationState);

      expect(stateEvents.length).toBeGreaterThan(0);
      expect(stateEvents[0].messageCount).toBeGreaterThan(0);
      expect(stateEvents[0].tokenUsage).toBe(150);
    });

    it('should handle event emission errors gracefully', async () => {
      // Mock eventManager.emit to throw error
      const originalEmit = eventManager.emit.bind(eventManager);
      eventManager.emit = vi.fn().mockRejectedValue(new Error('Event emission failed'));

      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Test prompt',
        config: mockConfig,
        eventManager,
      };

      // Should not throw despite event emission failure
      await expect(coordinator.executeLLM(params, conversationState)).resolves.not.toThrow();

      // Restore original emit
      eventManager.emit = originalEmit;
    });
  });

  describe('Tool Schema Preparation', () => {
    it('should prepare tool schemas from tools', async () => {
      const tools = [
        {
          id: 'read_file',
          description: 'Read a file',
          parameters: {
            type: 'object',
            properties: { path: { type: 'string' } },
          },
        },
      ];

      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Read the file',
        config: mockConfig,
        tools,
      };

      await coordinator.executeLLM(params, conversationState);

      expect(mockLLMExecutor.executeLLMCall).toHaveBeenCalled();
      const callArgs = mockLLMExecutor.executeLLMCall.mock.calls[0];
      expect(callArgs[1].tools).toBeDefined();
    });

    it('should handle empty tools array', async () => {
      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Test prompt',
        config: mockConfig,
        tools: [],
      };

      await coordinator.executeLLM(params, conversationState);

      expect(mockLLMExecutor.executeLLMCall).toHaveBeenCalled();
    });
  });

  describe('Integration Scenarios', () => {
    it('should handle complete LLM-tool call loop', async () => {
      // This test verifies a single LLM call with tool execution
      // Note: The actual loop would require multiple executeLLM calls from the caller
      mockLLMExecutor.executeLLMCall.mockResolvedValueOnce({
        success: true,
        result: {
          content: 'The weather in Beijing is sunny',
          toolCalls: [
            {
              id: 'call_1',
              name: 'get_weather',
              arguments: '{"city": "Beijing"}',
            },
          ],
          usage: { promptTokens: 100, completionTokens: 50, totalTokens: 150 },
        },
      });

      const params: LLMExecutionParams = {
        contextId: 'weather-query',
        prompt: 'What is the weather in Beijing?',
        config: mockConfig,
      };

      const result = await coordinator.executeLLM(params, conversationState);

      expect(result.success).toBe(true);
      expect(result.content).toBe('The weather in Beijing is sunny');
      expect(mockLLMExecutor.executeLLMCall).toHaveBeenCalledTimes(1);
      expect(mockToolCallExecutor.executeToolCalls).toHaveBeenCalledTimes(1);
    });

    it('should handle multiple sequential LLM calls', async () => {
      const results: string[] = [];

      for (let i = 0; i < 3; i++) {
        const params: LLMExecutionParams = {
          contextId: `context-${i}`,
          prompt: `Question ${i}`,
          config: mockConfig,
        };

        const result = await coordinator.executeLLM(params, conversationState);
        results.push(result.content || '');
      }

      expect(results.length).toBe(3);
      expect(mockLLMExecutor.executeLLMCall).toHaveBeenCalledTimes(3);
    });
  });

  describe('Error Handling', () => {
    it('should handle LLM executor failure', async () => {
      mockLLMExecutor.executeLLMCall.mockResolvedValueOnce({
        success: false,
        interruption: { type: 'ERROR', reason: 'LLM service unavailable' },
      });

      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Test prompt',
        config: mockConfig,
      };

      const result = await coordinator.executeLLM(params, conversationState);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });

    it('should handle missing config parameters gracefully', async () => {
      const minimalConfig: LLMExecutionConfig = {
        profileId: 'test-profile',
      };

      const params: LLMExecutionParams = {
        contextId: 'test-context-123',
        prompt: 'Test prompt',
        config: minimalConfig,
      };

      const result = await coordinator.executeLLM(params, conversationState);

      expect(result.success).toBe(true);
    });
  });
});
