/**
 * Tool Call Executor Unit Tests
 * Tests for the ToolCallExecutor class
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { ToolCallExecutor } from "../tool-call-executor.js";
import { InterruptedException } from "@sdk/shared/types/interruption-types.js";
import type { ToolRegistry } from "../../registry/tool-registry.js";
import type { ConversationSession } from "../../messaging/conversation-session.js";
import type { Tool } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";

// Mock dependencies
const createMockToolRegistry = () => ({
  getTool: vi.fn(),
  execute: vi.fn(),
});

const createMockConversationSession = () => ({
  addMessage: vi.fn(),
  getMessages: vi.fn().mockReturnValue([]),
});

describe("ToolCallExecutor", () => {
  let executor: ToolCallExecutor;
  let mockToolRegistry: ReturnType<typeof createMockToolRegistry>;
  let mockConversationSession: ReturnType<typeof createMockConversationSession>;

  beforeEach(() => {
    mockToolRegistry = createMockToolRegistry();
    mockConversationSession = createMockConversationSession();

    executor = new ToolCallExecutor(mockToolRegistry as unknown as ToolRegistry);

    vi.clearAllMocks();
  });

  describe("executeToolCalls", () => {
    it("should execute a single tool call successfully", async () => {
      const toolCalls = [
        {
          id: "call_1",
          name: "test-tool",
          arguments: '{"param": "value"}',
        },
      ];

      const mockTool: Tool = {
        id: "test-tool",
        type: "STATELESS",
        description: "A test tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);
      (mockToolRegistry.execute as any).mockResolvedValue(ok("Tool result"));

      const results = await executor.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(true);
      expect(results[0]?.toolCallId).toBe("call_1");
      expect(results[0]?.result).toBe("Tool result");
      expect(mockToolRegistry.execute).toHaveBeenCalledTimes(1);
    });

    it("should handle tool execution failure", async () => {
      const toolCalls = [
        {
          id: "call_1",
          name: "failing-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "failing-tool",
        type: "STATELESS",
        description: "A failing tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);
      (mockToolRegistry.execute as any).mockResolvedValue(err(new Error("Tool execution failed")));

      const results = await executor.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error).toBe("Tool execution failed");
    });

    it("should execute multiple tool calls in parallel", async () => {
      const toolCalls = [
        {
          id: "call_1",
          name: "tool-1",
          arguments: "{}",
        },
        {
          id: "call_2",
          name: "tool-2",
          arguments: "{}",
        },
      ];

      const mockTool1: Tool = {
        id: "tool-1",
        type: "STATELESS",
        description: "First tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      const mockTool2: Tool = {
        id: "tool-2",
        type: "STATELESS",
        description: "Second tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any)
        .mockReturnValueOnce(mockTool1)
        .mockReturnValueOnce(mockTool2);

      (mockToolRegistry.execute as any)
        .mockResolvedValueOnce(ok("Result 1"))
        .mockResolvedValueOnce(ok("Result 2"));

      const results = await executor.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(results).toHaveLength(2);
      expect(results[0]?.success).toBe(true);
      expect(results[1]?.success).toBe(true);
      expect(mockToolRegistry.execute).toHaveBeenCalledTimes(2);
    });

    it("should handle mixed success and failure results", async () => {
      const toolCalls = [
        {
          id: "call_1",
          name: "success-tool",
          arguments: "{}",
        },
        {
          id: "call_2",
          name: "fail-tool",
          arguments: "{}",
        },
      ];

      const mockTool1: Tool = {
        id: "success-tool",
        type: "STATELESS",
        description: "Successful tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      const mockTool2: Tool = {
        id: "fail-tool",
        type: "STATELESS",
        description: "Failing tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any)
        .mockReturnValueOnce(mockTool1)
        .mockReturnValueOnce(mockTool2);

      (mockToolRegistry.execute as any)
        .mockResolvedValueOnce(ok("Success result"))
        .mockResolvedValueOnce(err(new Error("Failed")));

      const results = await executor.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(results).toHaveLength(2);
      expect(results[0]?.success).toBe(true);
      expect(results[1]?.success).toBe(false);
    });

    it("should handle abort signal", async () => {
      const toolCalls = [
        {
          id: "call_1",
          name: "test-tool",
          arguments: "{}",
        },
      ];

      const abortController = new AbortController();
      abortController.abort();

      const mockTool: Tool = {
        id: "test-tool",
        type: "STATELESS",
        description: "A test tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);

      // Should return cancelled results instead of throwing
      const results = await executor.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
        { abortSignal: abortController.signal },
      );

      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error).toContain("cancelled");
      expect(results[0]?.executionTime).toBe(0);
    });

    it("should handle invalid JSON arguments", async () => {
      const toolCalls = [
        {
          id: "call_1",
          name: "test-tool",
          arguments: "invalid json",
        },
      ];

      const mockTool: Tool = {
        id: "test-tool",
        type: "STATELESS",
        description: "A test tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);

      // Invalid JSON should result in a failed execution, not a thrown error
      const results = await executor.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error).toContain("JSON");
    });

    it("should track execution time", async () => {
      const toolCalls = [
        {
          id: "call_1",
          name: "timed-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "timed-tool",
        type: "STATELESS",
        description: "A timed tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);
      (mockToolRegistry.execute as any).mockResolvedValue(ok("Result"));

      const startTime = Date.now();
      const results = await executor.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );
      const endTime = Date.now();

      expect(results[0]?.executionTime).toBeGreaterThanOrEqual(0);
      expect(results[0]?.executionTime).toBeLessThanOrEqual(endTime - startTime + 100);
    });

    it("should handle tool not found in registry", async () => {
      const toolCalls = [
        {
          id: "call_1",
          name: "non-existent-tool",
          arguments: "{}",
        },
      ];

      // Mock getTool to return undefined (tool not found)
      (mockToolRegistry.getTool as any).mockReturnValue(undefined);
      // execute will fail because tool is undefined
      (mockToolRegistry.execute as any).mockRejectedValue(
        new Error("Cannot read properties of undefined"),
      );

      const results = await executor.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error).toBeDefined();
    });

    it("should handle tool visibility check failure", async () => {
      const mockToolVisibilityStore = {
        isToolVisible: vi.fn().mockReturnValue(false),
        getVisibleTools: vi.fn().mockReturnValue(new Set(["other-tool"])),
      };

      const executorWithVisibility = new ToolCallExecutor(
        mockToolRegistry as unknown as ToolRegistry,
        { toolVisibilityStore: mockToolVisibilityStore as any },
      );

      const toolCalls = [
        {
          id: "call_1",
          name: "hidden-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "hidden-tool",
        type: "STATELESS",
        description: "A hidden tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);

      const results = await executorWithVisibility.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error).toContain("is not available in the current scope");
      expect(mockToolVisibilityStore.isToolVisible).toHaveBeenCalledWith("exec-1", "hidden-tool");
    });

    it("should handle tool failure protection blocking", async () => {
      const mockToolFailureProtection = {
        canExecuteTool: vi.fn().mockReturnValue({
          allowed: false,
          reason: "Tool blocked due to consecutive failures",
          failureCount: 3,
          remainingCooldown: 60000,
          lastError: "Previous error",
        }),
        recordFailure: vi.fn(),
        recordSuccess: vi.fn(),
      };

      const executorWithProtection = new ToolCallExecutor(
        mockToolRegistry as unknown as ToolRegistry,
        { toolFailureProtection: mockToolFailureProtection as any },
      );

      const toolCalls = [
        {
          id: "call_1",
          name: "blocked-tool",
          arguments: "{}",
        },
      ];

      const results = await executorWithProtection.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error).toContain("blocked");
      expect(mockToolFailureProtection.canExecuteTool).toHaveBeenCalledWith("blocked-tool");
    });

    it("should record success for tool failure protection", async () => {
      const mockToolFailureProtection = {
        canExecuteTool: vi.fn().mockReturnValue({ allowed: true }),
        recordFailure: vi.fn(),
        recordSuccess: vi.fn(),
      };

      const executorWithProtection = new ToolCallExecutor(
        mockToolRegistry as unknown as ToolRegistry,
        { toolFailureProtection: mockToolFailureProtection as any },
      );

      const toolCalls = [
        {
          id: "call_1",
          name: "test-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "test-tool",
        type: "STATELESS",
        description: "A test tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);
      (mockToolRegistry.execute as any).mockResolvedValue(ok("Success"));

      await executorWithProtection.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(mockToolFailureProtection.recordSuccess).toHaveBeenCalledWith("test-tool");
    });

    it("should handle checkpoint creation before tool execution", async () => {
      const mockCheckpointFn = vi.fn().mockResolvedValue("checkpoint-id");
      const mockCheckpointDependencies = {
        workflowExecutionRegistry: {
          get: vi.fn().mockReturnValue({
            state: {
              setCurrentOperation: vi.fn(),
              clearOperation: vi.fn(),
            },
          }),
        },
      };

      const executorWithCheckpoint = new ToolCallExecutor(
        mockToolRegistry as unknown as ToolRegistry,
        {
          checkpointDependencies: mockCheckpointDependencies as any,
          createCheckpointFn: mockCheckpointFn,
        },
      );

      const toolCalls = [
        {
          id: "call_1",
          name: "checkpoint-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "checkpoint-tool",
        type: "STATELESS",
        description: "A tool with checkpoint",
        parameters: { type: "object", properties: {}, required: [] },
        createCheckpoint: "before",
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);
      (mockToolRegistry.execute as any).mockResolvedValue(ok("Result"));

      await executorWithCheckpoint.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(mockCheckpointFn).toHaveBeenCalled();
      expect(mockCheckpointFn).toHaveBeenCalledWith(
        expect.objectContaining({
          workflowExecutionId: "exec-1",
          toolId: "checkpoint-tool",
        }),
        mockCheckpointDependencies,
      );
    });

    it("should handle checkpoint creation after tool execution", async () => {
      const mockCheckpointFn = vi.fn().mockResolvedValue("checkpoint-id");
      const mockCheckpointDependencies = {
        workflowExecutionRegistry: {
          get: vi.fn().mockReturnValue({
            state: {
              setCurrentOperation: vi.fn(),
              clearOperation: vi.fn(),
            },
          }),
        },
      };

      const executorWithCheckpoint = new ToolCallExecutor(
        mockToolRegistry as unknown as ToolRegistry,
        {
          checkpointDependencies: mockCheckpointDependencies as any,
          createCheckpointFn: mockCheckpointFn,
        },
      );

      const toolCalls = [
        {
          id: "call_1",
          name: "checkpoint-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "checkpoint-tool",
        type: "STATELESS",
        description: "A tool with checkpoint",
        parameters: { type: "object", properties: {}, required: [] },
        createCheckpoint: "after",
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);
      (mockToolRegistry.execute as any).mockResolvedValue(ok("Result"));

      await executorWithCheckpoint.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(mockCheckpointFn).toHaveBeenCalled();
    });

    it("should handle checkpoint creation both before and after", async () => {
      const mockCheckpointFn = vi.fn().mockResolvedValue("checkpoint-id");
      const mockCheckpointDependencies = {
        workflowExecutionRegistry: {
          get: vi.fn().mockReturnValue({
            state: {
              setCurrentOperation: vi.fn(),
              clearOperation: vi.fn(),
            },
          }),
        },
      };

      const executorWithCheckpoint = new ToolCallExecutor(
        mockToolRegistry as unknown as ToolRegistry,
        {
          checkpointDependencies: mockCheckpointDependencies as any,
          createCheckpointFn: mockCheckpointFn,
        },
      );

      const toolCalls = [
        {
          id: "call_1",
          name: "checkpoint-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "checkpoint-tool",
        type: "STATELESS",
        description: "A tool with checkpoint",
        parameters: { type: "object", properties: {}, required: [] },
        createCheckpoint: "both",
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);
      (mockToolRegistry.execute as any).mockResolvedValue(ok("Result"));

      await executorWithCheckpoint.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(mockCheckpointFn).toHaveBeenCalledTimes(2);
    });

    it("should handle event emission for tool calls", async () => {
      const mockEventManager = {};
      const mockEventBuilder = {
        buildMessageAddedEvent: vi.fn().mockReturnValue({ type: "message-added" }),
        buildToolCallStartedEvent: vi.fn().mockReturnValue({ type: "tool-started" }),
        buildToolCallCompletedEvent: vi.fn().mockReturnValue({ type: "tool-completed" }),
        buildToolCallFailedEvent: vi.fn().mockReturnValue({ type: "tool-failed" }),
      };
      const mockSafeEmitFn = vi.fn();

      const executorWithEvents = new ToolCallExecutor(mockToolRegistry as unknown as ToolRegistry, {
        eventManager: mockEventManager as any,
        eventBuilder: mockEventBuilder as any,
        safeEmitFn: mockSafeEmitFn,
      });

      const toolCalls = [
        {
          id: "call_1",
          name: "event-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "event-tool",
        type: "STATELESS",
        description: "An event tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);
      (mockToolRegistry.execute as any).mockResolvedValue(ok("Result"));

      await executorWithEvents.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(mockEventBuilder.buildToolCallStartedEvent).toHaveBeenCalled();
      expect(mockEventBuilder.buildToolCallCompletedEvent).toHaveBeenCalled();
      expect(mockEventBuilder.buildMessageAddedEvent).toHaveBeenCalled();
      expect(mockSafeEmitFn).toHaveBeenCalledTimes(3); // started, completed, message-added
    });

    it("should handle interactive tool execution with context", async () => {
      const toolCalls = [
        {
          id: "call_1",
          name: "interactive-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "interactive-tool",
        type: "STATELESS",
        description: "An interactive tool",
        parameters: { type: "object", properties: {}, required: [] },
        metadata: {
          requiresUserInteraction: true,
        },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);
      (mockToolRegistry.execute as any).mockResolvedValue(ok("Interactive result"));

      await executor.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      // Verify that execute was called with context parameter
      expect(mockToolRegistry.execute).toHaveBeenCalledWith(
        "interactive-tool",
        {},
        expect.objectContaining({
          timeout: 0,
          retries: 0,
          retryDelay: 1000,
        }),
        "exec-1",
        expect.objectContaining({
          executionId: "exec-1",
          nodeId: "node-1",
        }),
      );
    });

    it("should handle PAUSE interruption during tool execution", async () => {
      const toolCalls = [
        {
          id: "call_1",
          name: "test-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "test-tool",
        type: "STATELESS",
        description: "A test tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);

      // Simulate InterruptedException with PAUSE type from tool execution
      const pauseError = new InterruptedException("Tool execution paused by user request", "PAUSE");
      (mockToolRegistry.execute as any).mockRejectedValue(pauseError);

      // Should return paused result instead of throwing
      const results = await executor.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error).toContain("paused");
    });

    it("should handle STOP interruption during tool execution", async () => {
      const toolCalls = [
        {
          id: "call_1",
          name: "test-tool",
          arguments: "{}",
        },
      ];

      const abortController = new AbortController();
      // Note: Production code in workflow layer uses simple abort() without structured reason
      abortController.abort();

      const mockTool: Tool = {
        id: "test-tool",
        type: "STATELESS",
        description: "A test tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);

      // Simulate AbortError from tool execution
      const abortError = new Error("Aborted");
      (abortError as any).name = "AbortError";
      (mockToolRegistry.execute as any).mockRejectedValue(abortError);

      // Should return cancelled result instead of throwing
      const results = await executor.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
        { abortSignal: abortController.signal },
      );

      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error).toContain("cancelled");
    });

    it("should use custom timeout and retry configuration from tool config", async () => {
      const toolCalls = [
        {
          id: "call_1",
          name: "configured-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "configured-tool",
        type: "STATELESS",
        description: "A configured tool",
        parameters: { type: "object", properties: {}, required: [] },
        config: {
          timeout: 5000,
          maxRetries: 3,
          retryDelay: 2000,
        },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);
      (mockToolRegistry.execute as any).mockResolvedValue(ok("Result"));

      await executor.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      expect(mockToolRegistry.execute).toHaveBeenCalledWith(
        "configured-tool",
        {},
        expect.objectContaining({
          timeout: 0,
          retries: 3,
          retryDelay: 2000,
        }),
        "exec-1",
        undefined,
      );
    });

    it("should NOT record interruption as failure in failure protection", async () => {
      const mockFailureProtection = {
        recordFailure: vi.fn(),
        recordSuccess: vi.fn(),
        canExecuteTool: vi.fn(() => ({ allowed: true })),
      };

      const executorWithFailureProtection = new ToolCallExecutor(mockToolRegistry as unknown as ToolRegistry, {
        toolFailureProtection: mockFailureProtection as any,
      });

      const toolCalls = [
        {
          id: "call_1",
          name: "test-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "test-tool",
        type: "STATELESS",
        description: "A test tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);

      // Simulate InterruptedException from tool execution
      const pauseError = new InterruptedException("Tool execution paused", "PAUSE");
      (mockToolRegistry.execute as any).mockResolvedValue(err(pauseError));

      const results = await executorWithFailureProtection.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      // Verify result is failure
      expect(results[0]?.success).toBe(false);

      // Verify failure protection was NOT called for interruption
      expect(mockFailureProtection.recordFailure).not.toHaveBeenCalled();
    });

    it("should record real failures in failure protection", async () => {
      const mockFailureProtection = {
        recordFailure: vi.fn(),
        recordSuccess: vi.fn(),
        canExecuteTool: vi.fn(() => ({ allowed: true })),
      };

      const executorWithFailureProtection = new ToolCallExecutor(mockToolRegistry as unknown as ToolRegistry, {
        toolFailureProtection: mockFailureProtection as any,
      });

      const toolCalls = [
        {
          id: "call_1",
          name: "test-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "test-tool",
        type: "STATELESS",
        description: "A test tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);

      // Simulate real tool execution error
      (mockToolRegistry.execute as any).mockResolvedValue(
        err(new Error("Network timeout"))
      );

      const results = await executorWithFailureProtection.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
      );

      // Verify result is failure
      expect(results[0]?.success).toBe(false);

      // Verify failure protection WAS called for real failure
      expect(mockFailureProtection.recordFailure).toHaveBeenCalledWith(
        "test-tool",
        "Network timeout"
      );
    });

    it("should trigger progress callback for interruption", async () => {
      const progressCallback = vi.fn();

      const toolCalls = [
        {
          id: "call_1",
          name: "test-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "test-tool",
        type: "STATELESS",
        description: "A test tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);

      // Simulate InterruptedException from tool execution
      const pauseError = new InterruptedException("Tool execution paused", "PAUSE");
      (mockToolRegistry.execute as any).mockResolvedValue(err(pauseError));

      const results = await executor.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
        {
          onProgress: progressCallback,
        },
      );

      // Verify result
      expect(results[0]?.success).toBe(false);

      // Verify progress callback was called with running status
      expect(progressCallback).toHaveBeenCalledWith(
        "call_1",
        expect.objectContaining({
          status: "running",
          toolName: "test-tool",
        })
      );
    });

    it("should NOT trigger progress callback with interrupted status for real failures", async () => {
      const progressCallback = vi.fn();

      const toolCalls = [
        {
          id: "call_1",
          name: "test-tool",
          arguments: "{}",
        },
      ];

      const mockTool: Tool = {
        id: "test-tool",
        type: "STATELESS",
        description: "A test tool",
        parameters: { type: "object", properties: {}, required: [] },
      };

      (mockToolRegistry.getTool as any).mockReturnValue(mockTool);

      // Simulate real tool execution error
      (mockToolRegistry.execute as any).mockResolvedValue(
        err(new Error("Network timeout"))
      );

      const results = await executor.executeToolCalls(
        toolCalls,
        mockConversationSession as unknown as ConversationSession,
        "exec-1",
        "node-1",
        {
          onProgress: progressCallback,
        },
      );

      // Verify result
      expect(results[0]?.success).toBe(false);

      // Verify progress callback was NOT called for real failure
      // (no callback means no progress update during failure handling)
      const interruptedCalls = progressCallback.mock.calls.filter(
        call => call[1]?.status === "interrupted"
      );
      expect(interruptedCalls).toHaveLength(0);
    });
  });
});
