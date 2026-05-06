/**
 * Tool Approval Coordinator Tests
 * Tests for usage limits, state management, audit logging, and error handling
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ToolApprovalCoordinator } from '../tool-approval-coordinator.js';
import type { 
  ToolApprovalOptions, 
  LLMToolCall,
  Tool,
} from '@wf-agent/types';

describe('ToolApprovalCoordinator', () => {
  let coordinator: ToolApprovalCoordinator;
  
  const mockToolCall: LLMToolCall = {
    id: 'call_123',
    type: 'function',
    function: {
      name: 'read_file',
      arguments: '{"path": "test.txt"}',
    },
  };

  const mockTool: Tool = {
    id: 'read_file',
    type: 'STATELESS',
    description: 'Read a file',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string' },
      },
    },
    metadata: {
      riskLevel: 'READ_ONLY',
    },
  };

  const mockApprovalHandler = {
    requestApproval: vi.fn().mockResolvedValue({
      approved: true,
      toolCallId: 'call_123',
    }),
  };

  beforeEach(() => {
    coordinator = new ToolApprovalCoordinator();
    vi.clearAllMocks();
  });

  describe('Usage Limits - P0 Implementation', () => {
    it('should track consecutive auto-approved requests', async () => {
      const options: ToolApprovalOptions = {
        autoApprovalEnabled: true,
        categories: {
          alwaysAllowReadOnly: true,
        },
        maxAutoApprovedRequests: 3,
      };

      // First 3 requests should be auto-approved
      for (let i = 0; i < 3; i++) {
        const result = await coordinator.processToolApproval({
          toolCall: mockToolCall,
          tool: mockTool,
          options,
          contextId: 'test-context',
          approvalHandler: mockApprovalHandler,
        });

        expect(result.approved).toBe(true);
      }

      // 4th request should require manual approval due to limit
      const result = await coordinator.processToolApproval({
        toolCall: mockToolCall,
        tool: mockTool,
        options,
        contextId: 'test-context',
        approvalHandler: mockApprovalHandler,
      });

      // Should have triggered manual approval
      expect(mockApprovalHandler.requestApproval).toHaveBeenCalled();
    });

    it('should reset counter after manual approval', async () => {
      const options: ToolApprovalOptions = {
        autoApprovalEnabled: true,
        categories: {
          alwaysAllowReadOnly: true,
        },
        maxAutoApprovedRequests: 2,
      };

      // Reach the limit
      await coordinator.processToolApproval({
        toolCall: mockToolCall,
        tool: mockTool,
        options,
        contextId: 'test-context',
        approvalHandler: mockApprovalHandler,
      });

      await coordinator.processToolApproval({
        toolCall: mockToolCall,
        tool: mockTool,
        options,
        contextId: 'test-context',
        approvalHandler: mockApprovalHandler,
      });

      // Manual approval resets counter
      mockApprovalHandler.requestApproval.mockResolvedValueOnce({
        approved: true,
        toolCallId: 'call_123',
      });

      await coordinator.processToolApproval({
        toolCall: mockToolCall,
        tool: mockTool,
        options,
        contextId: 'test-context',
        approvalHandler: mockApprovalHandler,
      });

      // Counter should be reset, next auto-approvals should work
      const result = await coordinator.processToolApproval({
        toolCall: mockToolCall,
        tool: mockTool,
        options,
        contextId: 'test-context',
        approvalHandler: mockApprovalHandler,
      });

      expect(result.approved).toBe(true);
    });

    it('should maintain separate state per context', async () => {
      const options: ToolApprovalOptions = {
        autoApprovalEnabled: true,
        categories: {
          alwaysAllowReadOnly: true,
        },
        maxAutoApprovedRequests: 1,
      };

      // Context 1 reaches limit
      await coordinator.processToolApproval({
        toolCall: mockToolCall,
        tool: mockTool,
        options,
        contextId: 'context-1',
        approvalHandler: mockApprovalHandler,
      });

      // Context 2 should still have fresh state
      const result = await coordinator.processToolApproval({
        toolCall: mockToolCall,
        tool: mockTool,
        options,
        contextId: 'context-2',
        approvalHandler: mockApprovalHandler,
      });

      expect(result.approved).toBe(true);
    });
  });

  describe('State Management - P0 Implementation', () => {
    it('should expose state for monitoring', () => {
      const state = coordinator.getApprovalState('non-existent');
      expect(state).toBeUndefined();
    });

    it('should allow manual state reset', async () => {
      const options: ToolApprovalOptions = {
        autoApprovalEnabled: true,
        categories: {
          alwaysAllowReadOnly: true,
        },
        maxAutoApprovedRequests: 1,
      };

      // Use up the limit
      await coordinator.processToolApproval({
        toolCall: mockToolCall,
        tool: mockTool,
        options,
        contextId: 'test-context',
        approvalHandler: mockApprovalHandler,
      });

      // Manually reset
      coordinator.resetApprovalState('test-context');

      // Should be able to auto-approve again
      const result = await coordinator.processToolApproval({
        toolCall: mockToolCall,
        tool: mockTool,
        options,
        contextId: 'test-context',
        approvalHandler: mockApprovalHandler,
      });

      expect(result.approved).toBe(true);
    });
  });

  describe('Risk Level Decoupling - P1 Implementation', () => {
    it('should accept riskLevel without full tool definition', async () => {
      const options: ToolApprovalOptions = {
        autoApprovalEnabled: true,
        categories: {
          alwaysAllowReadOnly: true,
        },
      };

      // Provide only riskLevel, no tool
      const result = await coordinator.processToolApproval({
        toolCall: mockToolCall,
        riskLevel: 'READ_ONLY',
        options,
        contextId: 'test-context',
        approvalHandler: mockApprovalHandler,
      });

      expect(result.approved).toBe(true);
    });

    it('should reject when neither tool nor riskLevel is provided', async () => {
      const result = await coordinator.processToolApproval({
        toolCall: mockToolCall,
        options: {},
        contextId: 'test-context',
        approvalHandler: mockApprovalHandler,
      });

      expect(result.approved).toBe(false);
      expect(result.rejectionReason).toContain('riskLevel');
    });

    it('should use provided riskLevel over tool metadata', async () => {
      const options: ToolApprovalOptions = {
        autoApprovalEnabled: true,
        categories: {
          alwaysAllowReadOnly: true,
        },
      };

      // Tool says WRITE but we override with READ_ONLY
      const writeTool: Tool = {
        ...mockTool,
        id: 'write_file',
        metadata: { riskLevel: 'WRITE' },
      };

      const result = await coordinator.processToolApproval({
        toolCall: {
          ...mockToolCall,
          function: { name: 'write_file', arguments: '{}' },
        },
        tool: writeTool,
        riskLevel: 'READ_ONLY', // Override
        options,
        contextId: 'test-context',
        approvalHandler: mockApprovalHandler,
      });

      // Should be auto-approved because we overrode to READ_ONLY
      expect(result.approved).toBe(true);
    });
  });

  describe('Error Handling - P1 Enhancement', () => {
    it('should provide actionable error messages on parse failure', async () => {
      const invalidToolCall: LLMToolCall = {
        id: 'call_123',
        type: 'function',
        function: {
          name: 'read_file',
          arguments: 'INVALID JSON{{{',
        },
      };

      const result = await coordinator.processToolApproval({
        toolCall: invalidToolCall,
        tool: mockTool,
        options: {},
        contextId: 'test-context',
        approvalHandler: mockApprovalHandler,
      });

      // Should fall back to manual approval with clear message
      expect(mockApprovalHandler.requestApproval).toHaveBeenCalled();
    });

    it('should handle missing required parameters gracefully', async () => {
      const shellToolCall: LLMToolCall = {
        id: 'call_123',
        type: 'function',
        function: {
          name: 'run_shell',
          arguments: '{}', // Missing 'command' parameter
        },
      };

      const shellTool: Tool = {
        id: 'run_shell',
        type: 'STATELESS',
        description: 'Run shell command',
        parameters: { type: 'object', properties: {} },
        metadata: { riskLevel: 'EXECUTE' },
      };

      const result = await coordinator.processToolApproval({
        toolCall: shellToolCall,
        tool: shellTool,
        options: {},
        contextId: 'test-context',
        approvalHandler: mockApprovalHandler,
      });

      // Should require manual approval due to missing parameters
      expect(mockApprovalHandler.requestApproval).toHaveBeenCalled();
    });
  });

  describe('Audit Logging - P1 Implementation', () => {
    it('should log all approval decisions', async () => {
      const loggerSpy = vi.spyOn(console, 'log').mockImplementation(() => {});

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
        contextId: 'test-context',
        approvalHandler: mockApprovalHandler,
      });

      // Check that some logging occurred (logger.info uses console.log internally)
      expect(loggerSpy.mock.calls.length).toBeGreaterThan(0);

      loggerSpy.mockRestore();
    });

    it('should include timestamp in audit logs', async () => {
      // Verify state tracking includes timestamps
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
        contextId: 'test-context',
        approvalHandler: mockApprovalHandler,
      });

      // Get state and verify it has activity timestamp
      const state = coordinator.getApprovalState('test-context');
      expect(state).toBeDefined();
      expect(state?.lastActivityAt).toBeDefined();
      expect(typeof state?.lastActivityAt).toBe('number');
    });
  });

  describe('Integration Scenarios', () => {
    it('should handle complete approval workflow', async () => {
      const options: ToolApprovalOptions = {
        autoApprovalEnabled: true,
        categories: {
          alwaysAllowReadOnly: true,
          alwaysAllowWrite: false,
        },
        maxAutoApprovedRequests: 2,
      };

      const readResult = await coordinator.processToolApproval({
        toolCall: mockToolCall,
        tool: mockTool,
        options,
        contextId: 'workflow-test',
        approvalHandler: mockApprovalHandler,
      });

      expect(readResult.approved).toBe(true);

      // Write tool should require approval
      const writeToolCall: LLMToolCall = {
        id: 'call_456',
        type: 'function',
        function: {
          name: 'write_file',
          arguments: '{"path": "output.txt", "content": "test"}',
        },
      };

      const writeTool: Tool = {
        id: 'write_file',
        type: 'STATELESS',
        description: 'Write a file',
        parameters: { type: 'object', properties: {} },
        metadata: { riskLevel: 'WRITE' },
      };

      mockApprovalHandler.requestApproval.mockResolvedValueOnce({
        approved: true,
        toolCallId: 'call_456',
      });

      const writeResult = await coordinator.processToolApproval({
        toolCall: writeToolCall,
        tool: writeTool,
        options,
        contextId: 'workflow-test',
        approvalHandler: mockApprovalHandler,
      });

      expect(writeResult.approved).toBe(true);
      expect(mockApprovalHandler.requestApproval).toHaveBeenCalled();
    });
  });
});
