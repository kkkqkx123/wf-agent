/**
 * LLM Executor Unit Tests
 * Tests for the LLMExecutor class
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { LLMExecutor, type LLMExecutionRequestData } from "../llm-executor.js";
import type { LLMWrapper } from "../../llm/wrapper.js";
import type { LLMResult, LLMMessage } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { LLMError } from "@wf-agent/types";

// Mock LLMWrapper
const createMockLLMWrapper = () => {
  const mock = {
    generate: vi.fn(),
    generateStream: vi.fn(),
  };
  return mock as unknown as LLMWrapper;
};

describe("LLMExecutor", () => {
  let executor: LLMExecutor;
  let mockLLMWrapper: ReturnType<typeof createMockLLMWrapper>;

  beforeEach(() => {
    mockLLMWrapper = createMockLLMWrapper();
    executor = new LLMExecutor(mockLLMWrapper);
    vi.clearAllMocks();
  });

  describe("executeLLMCall - Non-streaming mode", () => {
    it("should execute LLM call successfully", async () => {
      const messages: LLMMessage[] = [{ role: "user", content: "Hello" }];
      const requestData: LLMExecutionRequestData = {
        prompt: "test prompt",
        profileId: "test-profile",
        parameters: {},
        stream: false,
      };

      const mockResult: LLMResult = {
        id: "test-id",
        model: "test-model",
        content: "Test response",
        message: { role: "assistant", content: "Test response" },
        finishReason: "stop",
        duration: 100,
        usage: { promptTokens: 5, completionTokens: 5, totalTokens: 10 },
        toolCalls: [],
      };

      (mockLLMWrapper.generate as any).mockResolvedValue(ok(mockResult));

      const result = await executor.executeLLMCall(messages, requestData);

      expect(result.content).toBe("Test response");
      expect(result.finishReason).toBe("stop");
      expect(mockLLMWrapper.generate).toHaveBeenCalledTimes(1);
    });

    it("should handle LLM error correctly", async () => {
      const messages: LLMMessage[] = [{ role: "user", content: "Hello" }];
      const requestData: LLMExecutionRequestData = {
        prompt: "test prompt",
        profileId: "test-profile",
        parameters: {},
        stream: false,
      };

      const mockError = new LLMError("LLM failed", "test-provider");
      (mockLLMWrapper.generate as any).mockResolvedValue(err(mockError));

      // The executor throws the original error for non-abort errors
      await expect(executor.executeLLMCall(messages, requestData)).rejects.toThrow("LLM failed");
    });

    it("should handle tool calls in response", async () => {
      const messages: LLMMessage[] = [{ role: "user", content: "Use a tool" }];
      const requestData: LLMExecutionRequestData = {
        prompt: "test prompt",
        profileId: "test-profile",
        parameters: {},
        tools: [
          {
            id: "test-tool",
            description: "A test tool",
            parameters: { type: "object", properties: {}, required: [] },
          },
        ],
        stream: false,
      };

      const mockResult: LLMResult = {
        id: "test-id",
        model: "test-model",
        content: "",
        message: { role: "assistant", content: "" },
        finishReason: "tool_calls",
        duration: 100,
        usage: { promptTokens: 10, completionTokens: 10, totalTokens: 20 },
        toolCalls: [
          {
            id: "call_1",
            type: "function",
            function: {
              name: "test-tool",
              arguments: '{"param": "value"}',
            },
          },
        ],
      };

      (mockLLMWrapper.generate as any).mockResolvedValue(ok(mockResult));

      const result = await executor.executeLLMCall(messages, requestData);

      expect(result.toolCalls).toBeDefined();
      expect(result.toolCalls?.length).toBe(1);
      expect(result.toolCalls?.[0]?.name).toBe("test-tool");
      expect(result.toolCalls?.[0]?.arguments).toBe('{"param": "value"}');
    });
  });

  describe("executeLLMCall - Streaming mode", () => {
    it("should execute streaming LLM call successfully", async () => {
      const messages: LLMMessage[] = [{ role: "user", content: "Hello" }];
      const requestData: LLMExecutionRequestData = {
        prompt: "test prompt",
        profileId: "test-profile",
        parameters: {},
        stream: true,
      };

      const mockFinalResult: LLMResult = {
        id: "test-id",
        model: "test-model",
        content: "Streaming response",
        message: { role: "assistant", content: "Streaming response" },
        finishReason: "stop",
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

      expect(result.content).toBe("Streaming response");
      expect(mockLLMWrapper.generateStream).toHaveBeenCalledTimes(1);
      expect(mockStream.done).toHaveBeenCalledTimes(1);
      expect(mockStream.getFinalResult).toHaveBeenCalledTimes(1);
    });

    it("should handle streaming LLM error correctly", async () => {
      const messages: LLMMessage[] = [{ role: "user", content: "Hello" }];
      const requestData: LLMExecutionRequestData = {
        prompt: "test prompt",
        profileId: "test-profile",
        parameters: {},
        stream: true,
      };

      const mockError = new LLMError("Stream failed", "test-provider");
      (mockLLMWrapper.generateStream as any).mockResolvedValue(err(mockError));

      // The executor throws the original error for non-abort errors
      await expect(executor.executeLLMCall(messages, requestData)).rejects.toThrow("Stream failed");
    });
  });

  describe("executeLLMCall - Abort handling", () => {
    it("should handle abort signal correctly", async () => {
      const messages: LLMMessage[] = [{ role: "user", content: "Hello" }];
      const requestData: LLMExecutionRequestData = {
        prompt: "test prompt",
        profileId: "test-profile",
        parameters: {},
        stream: false,
      };

      const abortController = new AbortController();
      abortController.abort();

      const mockError = new LLMError("Aborted", "test-provider");
      (mockError as any).name = "AbortError";
      (mockLLMWrapper.generate as any).mockResolvedValue(err(mockError));

      // For regular abort (not PAUSE/STOP), the executor throws the original AbortError
      await expect(
        executor.executeLLMCall(messages, requestData, {
          abortSignal: abortController.signal,
        }),
      ).rejects.toThrow("Aborted");
    });
  });

  describe("executeLLMCall - Request data", () => {
    it("should pass correct parameters to LLM wrapper", async () => {
      const messages: LLMMessage[] = [{ role: "user", content: "Test" }];
      const requestData: LLMExecutionRequestData = {
        prompt: "test prompt",
        profileId: "test-profile",
        parameters: { temperature: 0.7 },
        tools: [
          {
            id: "tool1",
            description: "Tool 1",
            parameters: { type: "object", properties: {}, required: [] },
          },
        ],
        stream: false,
      };

      const mockResult: LLMResult = {
        id: "test-id",
        model: "test-model",
        content: "Response",
        message: { role: "assistant", content: "Response" },
        finishReason: "stop",
        duration: 50,
        toolCalls: [],
      };

      (mockLLMWrapper.generate as any).mockResolvedValue(ok(mockResult));

      await executor.executeLLMCall(messages, requestData);

      expect(mockLLMWrapper.generate).toHaveBeenCalledWith({
        profileId: "test-profile",
        messages,
        tools: requestData.tools,
        parameters: { temperature: 0.7 },
        stream: false,
        signal: undefined,
      });
    });

    it("should handle empty tools array", async () => {
      const messages: LLMMessage[] = [{ role: "user", content: "Test" }];
      const requestData: LLMExecutionRequestData = {
        prompt: "test prompt",
        profileId: "test-profile",
        parameters: {},
        tools: [],
        stream: false,
      };

      const mockResult: LLMResult = {
        id: "test-id",
        model: "test-model",
        content: "Response",
        message: { role: "assistant", content: "Response" },
        finishReason: "stop",
        duration: 50,
        toolCalls: [],
      };

      (mockLLMWrapper.generate as any).mockResolvedValue(ok(mockResult));

      const result = await executor.executeLLMCall(messages, requestData);

      expect(result.content).toBe("Response");
      expect(mockLLMWrapper.generate).toHaveBeenCalledWith({
        profileId: "test-profile",
        messages,
        tools: [],
        parameters: {},
        stream: false,
        signal: undefined,
      });
    });

    it("should handle dynamicTools configuration", async () => {
      const messages: LLMMessage[] = [{ role: "user", content: "Test" }];
      const requestData: LLMExecutionRequestData = {
        prompt: "test prompt",
        profileId: "test-profile",
        parameters: {},
        dynamicTools: {
          toolIds: ["dynamic-tool-1", "dynamic-tool-2"],
          descriptionTemplate: "Dynamic tool: {{name}}",
        },
        stream: false,
      };

      const mockResult: LLMResult = {
        id: "test-id",
        model: "test-model",
        content: "Response",
        message: { role: "assistant", content: "Response" },
        finishReason: "stop",
        duration: 50,
        toolCalls: [],
      };

      (mockLLMWrapper.generate as any).mockResolvedValue(ok(mockResult));

      const result = await executor.executeLLMCall(messages, requestData);

      expect(result.content).toBe("Response");
      // dynamicTools is not passed to llmWrapper.generate, only tools field is used
      expect(mockLLMWrapper.generate).toHaveBeenCalledWith({
        profileId: "test-profile",
        messages,
        tools: undefined,
        parameters: {},
        stream: false,
        signal: undefined,
      });
    });

    it("should handle maxToolCallsPerRequest configuration", async () => {
      const messages: LLMMessage[] = [{ role: "user", content: "Test" }];
      const requestData: LLMExecutionRequestData = {
        prompt: "test prompt",
        profileId: "test-profile",
        parameters: {},
        maxToolCallsPerRequest: 5,
        stream: false,
      };

      const mockResult: LLMResult = {
        id: "test-id",
        model: "test-model",
        content: "Response",
        message: { role: "assistant", content: "Response" },
        finishReason: "stop",
        duration: 50,
        toolCalls: [],
      };

      (mockLLMWrapper.generate as any).mockResolvedValue(ok(mockResult));

      const result = await executor.executeLLMCall(messages, requestData);

      expect(result.content).toBe("Response");
      // maxToolCallsPerRequest is not passed to llmWrapper.generate
      expect(mockLLMWrapper.generate).toHaveBeenCalled();
    });
  });

  describe("executeLLMCall - Interruption handling", () => {
    it("should throw error for PAUSE interruption", async () => {
      const messages: LLMMessage[] = [{ role: "user", content: "Hello" }];
      const requestData: LLMExecutionRequestData = {
        prompt: "test prompt",
        profileId: "test-profile",
        parameters: {},
        stream: false,
      };

      const abortController = new AbortController();
      // Note: Production code in workflow layer uses simple abort() without structured reason
      abortController.abort();

      // Create a proper AbortError with the interruption reason as cause
      const mockError = new Error("Paused");
      (mockError as any).name = "AbortError";
      (mockLLMWrapper.generate as any).mockResolvedValue(err(mockError));

      // The executor throws error for interruptions
      await expect(
        executor.executeLLMCall(messages, requestData, {
          abortSignal: abortController.signal,
        }),
      ).rejects.toThrow();
    });

    it("should throw error for STOP interruption", async () => {
      const messages: LLMMessage[] = [{ role: "user", content: "Hello" }];
      const requestData: LLMExecutionRequestData = {
        prompt: "test prompt",
        profileId: "test-profile",
        parameters: {},
        stream: false,
      };

      const abortController = new AbortController();
      // Note: Production code in workflow layer uses simple abort() without structured reason
      abortController.abort();

      // Create a proper AbortError with the interruption reason as cause
      const mockError = new Error("Stopped");
      (mockError as any).name = "AbortError";
      (mockLLMWrapper.generate as any).mockResolvedValue(err(mockError));

      // The executor throws error for interruptions
      await expect(
        executor.executeLLMCall(messages, requestData, {
          abortSignal: abortController.signal,
        }),
      ).rejects.toThrow();
    });
  });
});
