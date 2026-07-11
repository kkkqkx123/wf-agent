/**
 * Tool Approval Coordinator Tests
 * Tests for usage limits, state management, audit logging, and error handling
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { ToolApprovalCoordinator } from "../tool-approval-coordinator.js";
import type { ToolApprovalOptions, LLMToolCall, Tool, ToolApprovalHandler } from "@wf-agent/types";
import { EventRegistry } from "../../registry/event-registry.js";

describe("ToolApprovalCoordinator", () => {
  let coordinator: ToolApprovalCoordinator;

  const mockToolCall: LLMToolCall = {
    id: "call_123",
    type: "function",
    function: {
      name: "read_file",
      arguments: '{"path": "test.txt"}',
    },
  };

  const mockTool: Tool = {
    id: "read_file",
    type: "STATELESS",
    description: "Read a file",
    parameters: {
      type: "object",
      properties: {
        path: { type: "string" },
      },
      required: ["path"],
    },
    metadata: {
      riskLevel: "READ_ONLY",
    },
  };

  const mockApprovalHandler = {
    requestApproval: vi.fn().mockResolvedValue({
      approved: true,
      toolCallId: "call_123",
    }),
  };

  beforeEach(() => {
    coordinator = new ToolApprovalCoordinator();
    vi.clearAllMocks();
  });

  describe("Risk Level Decoupling - P1 Implementation", () => {
    it("should accept riskLevel without full tool definition", async () => {
      const options: ToolApprovalOptions = {
        autoApprovalEnabled: true,
        categories: {
          alwaysAllowReadOnly: true,
        },
      };

      // Provide only riskLevel, no tool
      const result = await coordinator.processToolApproval({
        toolCall: mockToolCall,
        riskLevel: "READ_ONLY",
        options,
        contextId: "test-context",
        approvalHandler: mockApprovalHandler,
      });

      expect(result.approved).toBe(true);
    });

    it("should reject when neither tool nor riskLevel is provided", async () => {
      const result = await coordinator.processToolApproval({
        toolCall: mockToolCall,
        options: {},
        contextId: "test-context",
        approvalHandler: mockApprovalHandler,
      });

      expect(result.approved).toBe(false);
      expect(result.rejectionReason).toContain("riskLevel");
    });

    it("should use provided riskLevel over tool metadata", async () => {
      const options: ToolApprovalOptions = {
        autoApprovalEnabled: true,
        categories: {
          alwaysAllowReadOnly: true,
        },
      };

      // Tool says WRITE but we override with READ_ONLY
      const writeTool: Tool = {
        ...mockTool,
        id: "write_file",
        metadata: { riskLevel: "WRITE" },
      };

      const result = await coordinator.processToolApproval({
        toolCall: {
          ...mockToolCall,
          function: { name: "write_file", arguments: "{}" },
        },
        tool: writeTool,
        riskLevel: "READ_ONLY", // Override
        options,
        contextId: "test-context",
        approvalHandler: mockApprovalHandler,
      });

      // Should be auto-approved because we overrode to READ_ONLY
      expect(result.approved).toBe(true);
    });
  });

  describe("Error Handling - P1 Enhancement", () => {
    it("should provide actionable error messages on parse failure", async () => {
      const invalidToolCall: LLMToolCall = {
        id: "call_123",
        type: "function",
        function: {
          name: "read_file",
          arguments: "INVALID JSON{{{",
        },
      };

      await coordinator.processToolApproval({
        toolCall: invalidToolCall,
        tool: mockTool,
        options: {},
        contextId: "test-context",
        approvalHandler: mockApprovalHandler,
      });

      // Should fall back to manual approval with clear message
      expect(mockApprovalHandler.requestApproval).toHaveBeenCalled();
    });

    it("should handle missing required parameters gracefully", async () => {
      const shellToolCall: LLMToolCall = {
        id: "call_123",
        type: "function",
        function: {
          name: "run_shell",
          arguments: "{}", // Missing 'command' parameter
        },
      };

      const shellTool: Tool = {
        id: "run_shell",
        type: "STATELESS",
        description: "Run shell command",
        parameters: { type: "object", properties: {}, required: [] },
        metadata: { riskLevel: "EXECUTE" },
      };

      await coordinator.processToolApproval({
        toolCall: shellToolCall,
        tool: shellTool,
        options: {},
        contextId: "test-context",
        approvalHandler: mockApprovalHandler,
      });

      // Should require manual approval due to missing parameters
      expect(mockApprovalHandler.requestApproval).toHaveBeenCalled();
    });
  });

  describe("Audit Logging - P1 Implementation", () => {
    it("should log all approval decisions", async () => {
      const loggerSpy = vi.spyOn(console, "log").mockImplementation(() => {});

      const options: ToolApprovalOptions = {
        autoApprovalEnabled: true,
        categories: {
          alwaysAllowReadOnly: true,
        },
      };

      await coordinator.processToolApproval({
        toolCall: mockToolCall,
        tool: mockTool,
        options,
        contextId: "test-context",
        approvalHandler: mockApprovalHandler,
      });

      // Check that some logging occurred (logger.info uses console.log internally)
      expect(loggerSpy.mock.calls.length).toBeGreaterThan(0);

      loggerSpy.mockRestore();
    });
  });

  describe("Integration Scenarios", () => {
    it("should handle complete approval workflow", async () => {
      const options: ToolApprovalOptions = {
        autoApprovalEnabled: true,
        categories: {
          alwaysAllowReadOnly: true,
          alwaysAllowWrite: false,
        },
      };

      const readResult = await coordinator.processToolApproval({
        toolCall: mockToolCall,
        tool: mockTool,
        options,
        contextId: "workflow-test",
        approvalHandler: mockApprovalHandler,
      });

      expect(readResult.approved).toBe(true);

      // Write tool should require approval
      const writeToolCall: LLMToolCall = {
        id: "call_456",
        type: "function",
        function: {
          name: "write_file",
          arguments: '{"path": "output.txt", "content": "test"}',
        },
      };

      const writeTool: Tool = {
        id: "write_file",
        type: "STATELESS",
        description: "Write a file",
        parameters: { type: "object", properties: {}, required: [] },
        metadata: { riskLevel: "WRITE" },
      };

      mockApprovalHandler.requestApproval.mockResolvedValueOnce({
        approved: true,
        toolCallId: "call_456",
      });

      const writeResult = await coordinator.processToolApproval({
        toolCall: writeToolCall,
        tool: writeTool,
        options,
        contextId: "workflow-test",
        approvalHandler: mockApprovalHandler,
      });

      expect(writeResult.approved).toBe(true);
      expect(mockApprovalHandler.requestApproval).toHaveBeenCalled();
    });
  });

  describe("Batch Processing - processToolBatch", () => {
    let eventManager: EventRegistry;
    let mockBatchHandler: ToolApprovalHandler;

    beforeEach(() => {
      eventManager = new EventRegistry();
      mockBatchHandler = {
        requestApproval: vi.fn().mockResolvedValue({
          approved: true,
          toolCallId: "call_2",
          continueBatch: true,
        }),
      };
    });

    const createToolCall = (name: string, id?: string): LLMToolCall => ({
      id: id || `call_${name}`,
      type: "function",
      function: {
        name,
        arguments: "{}",
      },
    });

    it("should handle empty tool list", async () => {
      const result = await coordinator.processToolBatch(
        [],
        {},
        "test-context",
        "test-node",
        mockBatchHandler,
        eventManager,
      );

      expect(result.allCompleted).toBe(true);
      expect(result.autoExecuted.length).toBe(0);
      expect(result.confirmationRequired).toBeNull();
    });

    it("should generate unique batchId for each call", async () => {
      const toolCalls = [createToolCall("tool_1")];

      const result1 = await coordinator.processToolBatch(
        toolCalls,
        {},
        "test-context",
        "test-node",
        mockBatchHandler,
        eventManager,
      );

      const result2 = await coordinator.processToolBatch(
        toolCalls,
        {},
        "test-context",
        "test-node",
        mockBatchHandler,
        eventManager,
      );

      expect(result1.batchId).not.toBe(result2.batchId);
    });

    it("should require approval when no auto-approval enabled", async () => {
      const toolCalls = [createToolCall("tool_1"), createToolCall("tool_2")];

      const result = await coordinator.processToolBatch(
        toolCalls,
        { autoApprovalEnabled: false },
        "test-context",
        "test-node",
        mockBatchHandler,
        eventManager,
      );

      expect(result.allCompleted).toBe(false);
      expect(result.confirmationRequired).toBeDefined();
      expect(result.confirmationRequired?.function?.name).toBe("tool_1");
      expect(result.autoExecuted.length).toBe(0);
    });

    it("should include batch context in approval request", async () => {
      const toolCalls = [createToolCall("tool_1"), createToolCall("tool_2")];

      await coordinator.processToolBatch(
        toolCalls,
        { autoApprovalEnabled: false },
        "test-context",
        "test-node",
        mockBatchHandler,
        eventManager,
      );

      expect(mockBatchHandler.requestApproval).toHaveBeenCalledWith(
        expect.objectContaining({
          batchId: expect.any(String),
          toolIndex: 0,
          totalTools: 2,
          pendingQueue: expect.arrayContaining([expect.any(Object)]),
        }),
      );
    });

    it("should respect continueBatch flag", async () => {
      const toolCalls = [createToolCall("tool_1"), createToolCall("tool_2")];

      mockBatchHandler.requestApproval = vi.fn().mockResolvedValue({
        approved: true,
        toolCallId: "tool_1",
        continueBatch: false,
      });

      const result = await coordinator.processToolBatch(
        toolCalls,
        {},
        "test-context",
        "test-node",
        mockBatchHandler,
        eventManager,
      );

      expect(result.remainingQueue.length).toBe(0);
    });
  });
});
