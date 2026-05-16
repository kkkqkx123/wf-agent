/**
 * LLM Executor Unit Tests
 * Tests for the LLMExecutor class
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { LLMExecutor, type LLMExecutionRequestData } from '../llm-executor.js';
import type { LLMWrapper } from '../../llm/wrapper.js';
import type { LLMResult, LLMMessage } from '@wf-agent/types';
import { ok, err } from '@wf-agent/common-utils';
import { LLMError } from '@wf-agent/types';
import { createInterruptionAbortReason } from '../../utils/interruption/index.js';

// Mock LLMWrapper
const createMockLLMWrapper = () => {
  const mock = {
    generate: vi.fn(),
    generateStream: vi.fn(),
  };
  return mock as unknown as LLMWrapper;
};

describe('LLMExecutor', () => {
  let executor: LLMExecutor;
  let mockLLMWrapper: ReturnType<typeof createMockLLMWrapper>;

  beforeEach(() => {
    mockLLMWrapper = createMockLLMWrapper();
    executor = new LLMExecutor(mockLLMWrapper);
    vi.clearAllMocks();
  });

  describe('executeLLMCall - Non-streaming mode', () => {
    it('should execute LLM call successfully', async () => {
      const messages: LLMMessage[] = [
        { role: 'user', content: 'Hello' },
      ];
      const requestData: LLMExecutionRequestData = {
        prompt: 'test prompt',
        profileId: 'test-profile',
        parameters: {},
        stream: false,
      };

      const mockResult: LLMResult = {
        id: 'test-id',
        model: 'test-model',
        content: 'Test response',
        message: { role: 'assistant', content: 'Test response' },
        finishReason: 'stop',
        duration: 100,
        usage: { promptTokens: 5, completionTokens: 5, totalTokens: 10 },
        toolCalls: [],
      };

      (mockLLMWrapper.generate as any).mockResolvedValue(ok(mockResult));

      const result = await executor.executeLLMCall(messages, requestData);

      expect(result.content).toBe('Test response');
      expect(result.finishReason).toBe('stop');
      expect(mockLLMWrapper.generate).toHaveBeenCalledTimes(1);
    });

    it('should handle LLM error correctly', async () => {
      const messages: LLMMessage[] = [
        { role: 'user', content: 'Hello' },
      ];
      const requestData: LLMExecutionRequestData = {
        prompt: 'test prompt',
        profileId: 'test-profile',
        parameters: {},
        stream: false,
      };

      const mockError = new LLMError('LLM failed', 'test-provider');
      (mockLLMWrapper.generate as any).mockResolvedValue(err(mockError));

      // The executor throws ExecutionError for non-abort errors
      await expect(executor.executeLLMCall(messages, requestData)).rejects.toThrow('LLM call failed');
    });

    it('should handle tool calls in response', async () => {
      const messages: LLMMessage[] = [
        { role: 'user', content: 'Use a tool' },
      ];
      const requestData: LLMExecutionRequestData = {
        prompt: 'test prompt',
        profileId: 'test-profile',
        parameters: {},
        tools: [{ id: 'test-tool', description: 'A test tool', parameters: { type: 'object', properties: {}, required: [] } }],
        stream: false,
      };

      const mockResult: LLMResult = {
        id: 'test-id',
        model: 'test-model',
        content: '',
        message: { role: 'assistant', content: '' },
        finishReason: 'tool_calls',
        duration: 100,
        usage: { promptTokens: 10, completionTokens: 10, totalTokens: 20 },
        toolCalls: [
          {
            id: 'call_1',
            type: 'function',
            function: {
              name: 'test-tool',
              arguments: '{"param": "value"}',
            },
          },
        ],
      };

      (mockLLMWrapper.generate as any).mockResolvedValue(ok(mockResult));

      const result = await executor.executeLLMCall(messages, requestData);

      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.result.toolCalls).toBeDefined();
        expect(result.result.toolCalls?.length).toBe(1);
        expect(result.result.toolCalls?.[0]?.name).toBe('test-tool');
        expect(result.result.toolCalls?.[0]?.arguments).toBe('{"param": "value"}');
      }
    });
  });

  describe('executeLLMCall - Streaming mode', () => {
    it('should execute streaming LLM call successfully', async () => {
      const messages: LLMMessage[] = [
        { role: 'user', content: 'Hello' },
      ];
      const requestData: LLMExecutionRequestData = {
        prompt: 'test prompt',
        profileId: 'test-profile',
        parameters: {},
        stream: true,
      };

      const mockFinalResult: LLMResult = {
        id: 'test-id',
        model: 'test-model',
        content: 'Streaming response',
        message: { role: 'assistant', content: 'Streaming response' },
        finishReason: 'stop',
        duration: 150,
        usage: { promptTokens: 8, completionTokens: 7, totalTokens: 15 },
        toolCalls: [],
      };

      // Mock message stream
      const mockStream = {
        done: vi.fn().mockResolvedValue(undefined),
        getFinalResult: vi.fn().mockResolvedValue(mockFinalResult),
      };

      (mockLLMWrapper.generateStream as any).mockResolvedValue(ok(mockStream as any));

      const result = await executor.executeLLMCall(messages, requestData);

      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.result.content).toBe('Streaming response');
      }
      expect(mockLLMWrapper.generateStream).toHaveBeenCalledTimes(1);
      expect(mockStream.done).toHaveBeenCalledTimes(1);
      expect(mockStream.getFinalResult).toHaveBeenCalledTimes(1);
    });

    it('should handle streaming LLM error correctly', async () => {
      const messages: LLMMessage[] = [
        { role: 'user', content: 'Hello' },
      ];
      const requestData: LLMExecutionRequestData = {
        prompt: 'test prompt',
        profileId: 'test-profile',
        parameters: {},
        stream: true,
      };

      const mockError = new LLMError('Stream failed', 'test-provider');
      (mockLLMWrapper.generateStream as any).mockResolvedValue(err(mockError));

      // The executor throws ExecutionError for non-abort errors
      await expect(executor.executeLLMCall(messages, requestData)).rejects.toThrow('LLM call failed');
    });
  });

  describe('executeLLMCall - Abort handling', () => {
    it('should handle abort signal correctly', async () => {
      const messages: LLMMessage[] = [
        { role: 'user', content: 'Hello' },
      ];
      const requestData: LLMExecutionRequestData = {
        prompt: 'test prompt',
        profileId: 'test-profile',
        parameters: {},
        stream: false,
      };

      const abortController = new AbortController();
      abortController.abort();

      const mockError = new LLMError('Aborted', 'test-provider');
      (mockError as any).name = 'AbortError';
      (mockLLMWrapper.generate as any).mockResolvedValue(err(mockError));

      // For regular abort (not PAUSE/STOP), the executor throws ExecutionError
      await expect(
        executor.executeLLMCall(messages, requestData, {
          abortSignal: abortController.signal,
        })
      ).rejects.toThrow('LLM call failed');
    });
  });

  describe('executeLLMCall - Request data', () => {
    it('should pass correct parameters to LLM wrapper', async () => {
      const messages: LLMMessage[] = [
        { role: 'user', content: 'Test' },
      ];
      const requestData: LLMExecutionRequestData = {
        prompt: 'test prompt',
        profileId: 'test-profile',
        parameters: { temperature: 0.7 },
        tools: [{ id: 'tool1', description: 'Tool 1', parameters: { type: 'object', properties: {}, required: [] } }],
        stream: false,
      };

      const mockResult: LLMResult = {
        id: 'test-id',
        model: 'test-model',
        content: 'Response',
        message: { role: 'assistant', content: 'Response' },
        finishReason: 'stop',
        duration: 50,
        toolCalls: [],
      };

      (mockLLMWrapper.generate as any).mockResolvedValue(ok(mockResult));

      await executor.executeLLMCall(messages, requestData);

      expect(mockLLMWrapper.generate).toHaveBeenCalledWith({
        profileId: 'test-profile',
        messages,
        tools: requestData.tools,
        parameters: { temperature: 0.7 },
        stream: false,
        signal: undefined,
      });
    });

    it('should handle empty tools array', async () => {
      const messages: LLMMessage[] = [
        { role: 'user', content: 'Test' },
      ];
      const requestData: LLMExecutionRequestData = {
        prompt: 'test prompt',
        profileId: 'test-profile',
        parameters: {},
        tools: [],
        stream: false,
      };

      const mockResult: LLMResult = {
        id: 'test-id',
        model: 'test-model',
        content: 'Response',
        message: { role: 'assistant', content: 'Response' },
        finishReason: 'stop',
        duration: 50,
        toolCalls: [],
      };

      (mockLLMWrapper.generate as any).mockResolvedValue(ok(mockResult));

      const result = await executor.executeLLMCall(messages, requestData);

      expect(result.success).toBe(true);
      expect(mockLLMWrapper.generate).toHaveBeenCalledWith({
        profileId: 'test-profile',
        messages,
        tools: [],
        parameters: {},
        stream: false,
        signal: undefined,
      });
    });

    it('should handle dynamicTools configuration', async () => {
      const messages: LLMMessage[] = [
        { role: 'user', content: 'Test' },
      ];
      const requestData: LLMExecutionRequestData = {
        prompt: 'test prompt',
        profileId: 'test-profile',
        parameters: {},
        dynamicTools: {
          toolIds: ['dynamic-tool-1', 'dynamic-tool-2'],
          descriptionTemplate: 'Dynamic tool: {{name}}',
        },
        stream: false,
      };

      const mockResult: LLMResult = {
        id: 'test-id',
        model: 'test-model',
        content: 'Response',
        message: { role: 'assistant', content: 'Response' },
        finishReason: 'stop',
        duration: 50,
        toolCalls: [],
      };

      (mockLLMWrapper.generate as any).mockResolvedValue(ok(mockResult));

      const result = await executor.executeLLMCall(messages, requestData);

      expect(result.success).toBe(true);
      // dynamicTools is not passed to llmWrapper.generate, only tools field is used
      expect(mockLLMWrapper.generate).toHaveBeenCalledWith({
        profileId: 'test-profile',
        messages,
        tools: undefined,
        parameters: {},
        stream: false,
        signal: undefined,
      });
    });

    it('should handle maxToolCallsPerRequest configuration', async () => {
      const messages: LLMMessage[] = [
        { role: 'user', content: 'Test' },
      ];
      const requestData: LLMExecutionRequestData = {
        prompt: 'test prompt',
        profileId: 'test-profile',
        parameters: {},
        maxToolCallsPerRequest: 5,
        stream: false,
      };

      const mockResult: LLMResult = {
        id: 'test-id',
        model: 'test-model',
        content: 'Response',
        message: { role: 'assistant', content: 'Response' },
        finishReason: 'stop',
        duration: 50,
        toolCalls: [],
      };

      (mockLLMWrapper.generate as any).mockResolvedValue(ok(mockResult));

      const result = await executor.executeLLMCall(messages, requestData);

      expect(result.success).toBe(true);
      // maxToolCallsPerRequest is not passed to llmWrapper.generate
      expect(mockLLMWrapper.generate).toHaveBeenCalled();
    });
  });

  describe('executeLLMCall - Interruption handling', () => {
    it('should return interruption state for PAUSE', async () => {
      const messages: LLMMessage[] = [
        { role: 'user', content: 'Hello' },
      ];
      const requestData: LLMExecutionRequestData = {
        prompt: 'test prompt',
        profileId: 'test-profile',
        parameters: {},
        stream: false,
      };

      const abortController = new AbortController();
      // Use proper interruption abort reason
      const pauseReason = createInterruptionAbortReason('PAUSE', 'exec-1', 'node-1');
      abortController.abort(pauseReason);

      // Create a proper AbortError with the interruption reason as cause
      const mockError = createAbortError('Paused', abortController.signal);
      (mockLLMWrapper.generate as any).mockResolvedValue(err(mockError));

      const result = await executor.executeLLMCall(messages, requestData, {
        abortSignal: abortController.signal,
      });

      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.interruption.type).toBe('paused');
        if (result.interruption.type === 'paused') {
          expect(result.interruption.executionId).toBe('exec-1');
          expect(result.interruption.nodeId).toBe('node-1');
        }
      }
    });

    it('should return interruption state for STOP', async () => {
      const messages: LLMMessage[] = [
        { role: 'user', content: 'Hello' },
      ];
      const requestData: LLMExecutionRequestData = {
        prompt: 'test prompt',
        profileId: 'test-profile',
        parameters: {},
        stream: false,
      };

      const abortController = new AbortController();
      // Use proper interruption abort reason
      const stopReason = createInterruptionAbortReason('STOP', 'exec-1', 'node-1');
      abortController.abort(stopReason);

      // Create a proper AbortError with the interruption reason as cause
      const mockError = createAbortError('Stopped', abortController.signal);
      (mockLLMWrapper.generate as any).mockResolvedValue(err(mockError));

      const result = await executor.executeLLMCall(messages, requestData, {
        abortSignal: abortController.signal,
      });

      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.interruption.type).toBe('stopped');
        if (result.interruption.type === 'stopped') {
          expect(result.interruption.executionId).toBe('exec-1');
          expect(result.interruption.nodeId).toBe('node-1');
        }
      }
    });
  });
});
