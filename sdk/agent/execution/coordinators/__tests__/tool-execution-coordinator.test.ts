/**
 * ToolExecutionCoordinator Unit Tests
 *
 * Tests for tool execution coordination including:
 * - Direct execution (no approval handler)
 * - Batch execution with approval coordinator
 * - Stream execution
 * - Single approved tool execution
 * - Hook lifecycle during execution
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { ToolCallExecutor } from "../../../../core/executors/tool-call-executor.js";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import type { ToolApprovalHandler, ToolBatchResult, LLMToolCall } from "@wf-agent/types";
import { ToolExecutionCoordinator } from "../tool-execution-coordinator.js";

// Mock executeAgentHook
vi.mock("../../handlers/hook-handlers/index.js", () => ({
  executeAgentHook: vi.fn().mockResolvedValue(undefined),
}));

// Mock ToolApprovalCoordinator with class to support `new` keyword
const mockProcessToolBatch = vi.hoisted(() => vi.fn());
vi.mock("../../../../core/coordinators/tool-approval-coordinator.js", () => {
  return {
    ToolApprovalCoordinator: class {
      processToolBatch = mockProcessToolBatch;
    },
  };
});

describe("ToolExecutionCoordinator", () => {
  let coordinator: ToolExecutionCoordinator;
  let mockToolCallExecutor: ToolCallExecutor;
  let mockEventManager: EventRegistry;
  let mockEntity: any;
  let mockConversationManager: any;

  beforeEach(() => {
    vi.clearAllMocks();

    mockToolCallExecutor = {
      executeToolCalls: vi.fn(),
    } as unknown as ToolCallExecutor;

    mockEventManager = {
      emit: vi.fn().mockResolvedValue(undefined),
    } as unknown as EventRegistry;

    mockEntity = {
      id: "agent-loop-1",
      nodeId: "node-1",
      state: {
        currentIteration: 1,
        toolCallCount: 0,
        pendingToolCalls: new Set<string>(),
        recordToolCallStart: vi.fn(),
        recordToolCallEnd: vi.fn(),
      },
      config: {},
      getAbortSignal: vi.fn().mockReturnValue(new AbortController().signal),
    };

    mockConversationManager = {
      addMessage: vi.fn(),
    };
  });

  describe("executeToolCalls (without approval handler)", () => {
    beforeEach(() => {
      coordinator = new ToolExecutionCoordinator({
        toolCallExecutor: mockToolCallExecutor,
        eventManager: mockEventManager,
      });

      mockToolCallExecutor.executeToolCalls = vi.fn().mockResolvedValue([
        {
          toolCallId: "tool-1",
          success: true,
          result: { data: "result-1" },
          executionTime: 100,
        },
        {
          toolCallId: "tool-2",
          success: true,
          result: { data: "result-2" },
          executionTime: 50,
        },
      ]);
    });

    it("should execute all tool calls directly when no approval handler", async () => {
      const toolCalls = [
        { id: "tool-1", name: "search", arguments: '{"query":"test"}' },
        { id: "tool-2", name: "calculator", arguments: '{"a":1,"b":2}' },
      ];

      await coordinator.executeToolCalls(mockEntity, mockConversationManager, toolCalls);

      // Each tool call is executed individually via executeSingleApprovedTool
      expect(mockEntity.state.recordToolCallStart).toHaveBeenCalledTimes(2);
      expect(mockEntity.state.recordToolCallStart).toHaveBeenNthCalledWith(
        1, "tool-1", "search", '{"query":"test"}',
      );
      expect(mockEntity.state.recordToolCallStart).toHaveBeenNthCalledWith(
        2, "tool-2", "calculator", '{"a":1,"b":2}',
      );

      // executeToolCalls is called once per tool call (each with a single-element array)
      expect(mockToolCallExecutor.executeToolCalls).toHaveBeenCalledTimes(2);
      expect(mockToolCallExecutor.executeToolCalls).toHaveBeenNthCalledWith(
        1,
        [toolCalls[0]],
        mockConversationManager,
        mockEntity.id,
        mockEntity.nodeId,
        { abortSignal: mockEntity.getAbortSignal() },
      );
    });

    it("should handle empty tool calls array", async () => {
      mockToolCallExecutor.executeToolCalls = vi.fn().mockResolvedValue([]);

      await coordinator.executeToolCalls(mockEntity, mockConversationManager, []);

      expect(mockEntity.state.recordToolCallStart).not.toHaveBeenCalled();
      expect(mockToolCallExecutor.executeToolCalls).not.toHaveBeenCalled();
    });
  });

  describe("executeToolCalls (with approval handler)", () => {
    let mockApprovalHandler: ToolApprovalHandler;

    beforeEach(() => {
      mockApprovalHandler = {
        requestApproval: vi.fn(),
      };

      coordinator = new ToolExecutionCoordinator({
        toolCallExecutor: mockToolCallExecutor,
        eventManager: mockEventManager,
        toolApprovalHandler: mockApprovalHandler,
      });
    });

    it("should process batch through approval coordinator", async () => {
      const toolCalls = [
        { id: "tool-1", name: "search", arguments: '{"query":"test"}' },
      ];

      mockProcessToolBatch.mockResolvedValue({
        batchId: "batch-1",
        autoExecuted: [],
        confirmationRequired: null,
        confirmationResult: undefined,
        remainingQueue: [],
        allCompleted: true,
      } as ToolBatchResult);

      await coordinator.executeToolCalls(mockEntity, mockConversationManager, toolCalls);

      expect(mockProcessToolBatch).toHaveBeenCalledOnce();

      // Verify the batch was processed with expected args
      const callArgs = mockProcessToolBatch.mock.calls[0];
      expect(callArgs[0]).toHaveLength(1);
      expect(callArgs[0][0].id).toBe("tool-1");
      expect(callArgs[0][0].function.name).toBe("search");
    });

    it("should execute auto-approved tools from batch result", async () => {
      const toolCalls = [
        { id: "tool-1", name: "search", arguments: '{"query":"test"}' },
        { id: "tool-2", name: "read", arguments: '{"file":"test.txt"}' },
      ];

      mockToolCallExecutor.executeToolCalls = vi.fn().mockResolvedValue([
        { toolCallId: "tool-1", success: true, result: { data: "ok" }, executionTime: 50 },
      ]);

      mockProcessToolBatch.mockResolvedValue({
        batchId: "batch-1",
        autoExecuted: [{ success: true, result: {} }],
        confirmationRequired: null,
        confirmationResult: undefined,
        remainingQueue: [],
        allCompleted: true,
      } as unknown as ToolBatchResult);

      await coordinator.executeToolCalls(mockEntity, mockConversationManager, toolCalls);

      // Should execute the first tool via single tool path
      expect(mockEntity.state.recordToolCallStart).toHaveBeenCalledWith(
        "tool-1",
        "search",
        '{"query":"test"}',
      );
    });

    it("should handle confirmation-required tools", async () => {
      const toolCalls = [
        { id: "tool-1", name: "delete", arguments: '{"path":"/important"}' },
      ];

      mockToolCallExecutor.executeToolCalls = vi.fn().mockResolvedValue([
        { toolCallId: "tool-1", success: true, result: { deleted: true }, executionTime: 100 },
      ]);

      mockProcessToolBatch.mockResolvedValue({
        batchId: "batch-1",
        autoExecuted: [],
        confirmationRequired: { id: "tool-1", type: "function", function: { name: "delete", arguments: '{"path":"/important"}' } },
        confirmationResult: { approved: true, toolCallId: "tool-1", editedParameters: { path: "/safe" } },
        remainingQueue: [],
        allCompleted: false,
      } as ToolBatchResult);

      await coordinator.executeToolCalls(mockEntity, mockConversationManager, toolCalls);

      // Should have added user instruction - but since none was provided, no message added
      expect(mockConversationManager.addMessage).not.toHaveBeenCalled();
    });

    it("should handle approval rejection", async () => {
      const toolCalls = [
        { id: "tool-1", name: "delete", arguments: '{"path":"/important"}' },
      ];

      mockProcessToolBatch.mockResolvedValue({
        batchId: "batch-1",
        autoExecuted: [],
        confirmationRequired: { id: "tool-1", type: "function", function: { name: "delete", arguments: '{"path":"/important"}' } },
        confirmationResult: { approved: false, toolCallId: "tool-1", rejectionReason: "Not allowed" },
        remainingQueue: [],
        allCompleted: false,
      } as ToolBatchResult);

      await coordinator.executeToolCalls(mockEntity, mockConversationManager, toolCalls);

      // Should not execute the rejected tool
      expect(mockToolCallExecutor.executeToolCalls).not.toHaveBeenCalled();
    });
  });

  describe("executeToolCallsStream", () => {
    beforeEach(() => {
      coordinator = new ToolExecutionCoordinator({
        toolCallExecutor: mockToolCallExecutor,
        eventManager: mockEventManager,
      });

      mockToolCallExecutor.executeToolCalls = vi.fn().mockResolvedValue([
        {
          toolCallId: "tool-1",
          success: true,
          result: { data: "result" },
          executionTime: 100,
        },
      ]);
    });

    it("should yield start and end events for each tool call", async () => {
      const toolCalls = [
        { id: "tool-1", function: { name: "search", arguments: '{"q":"test"}' } },
      ];

      const events: any[] = [];
      for await (const event of coordinator.executeToolCallsStream(
        mockEntity,
        mockConversationManager,
        toolCalls,
      )) {
        events.push(event);
      }

      expect(events).toHaveLength(2);
      expect(events[0].type).toBe("tool_execution_start");
      expect(events[0].toolCallId).toBe("tool-1");
      expect(events[1].type).toBe("tool_execution_end");
      expect(events[1].toolCallId).toBe("tool-1");
    });

    it("should emit failure events when tool execution fails", async () => {
      mockToolCallExecutor.executeToolCalls = vi.fn().mockResolvedValue([
        {
          toolCallId: "tool-1",
          success: false,
          error: "Execution failed",
          executionTime: 50,
        },
      ]);

      const toolCalls = [
        { id: "tool-1", function: { name: "search", arguments: '{"q":"test"}' } },
      ];

      const events: any[] = [];
      for await (const event of coordinator.executeToolCallsStream(
        mockEntity,
        mockConversationManager,
        toolCalls,
      )) {
        events.push(event);
      }

      expect(events).toHaveLength(2);
      expect(events[0].type).toBe("tool_execution_start");
      expect(events[1].type).toBe("tool_execution_end");
      expect(events[1].result.success).toBe(false);
      expect(events[1].result.error).toBe("Execution failed");
    });

    it("should handle empty tool calls in stream", async () => {
      mockToolCallExecutor.executeToolCalls = vi.fn().mockResolvedValue([]);

      const events: any[] = [];
      for await (const event of coordinator.executeToolCallsStream(
        mockEntity,
        mockConversationManager,
        [],
      )) {
        events.push(event);
      }

      expect(events).toHaveLength(0);
      expect(mockToolCallExecutor.executeToolCalls).toHaveBeenCalledWith(
        [],
        mockConversationManager,
        mockEntity.id,
        mockEntity.nodeId,
        { abortSignal: mockEntity.getAbortSignal() },
      );
    });
  });
});