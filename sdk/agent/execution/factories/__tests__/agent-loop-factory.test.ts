/**
 * AgentLoopFactory Unit Tests
 *
 * Tests for the Agent Loop factory covering:
 * - Creating new instances (AgentLoopFactory.create)
 * - Restoring from checkpoints (AgentLoopFactory.fromCheckpoint)
 * - Parent-child registration logic
 * - Error handling in edge cases
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";

// =============================================================================
// Hoisted mocks
// vi.mock is hoisted to top of file, so any reference inside factory must
// use vi.hoisted() to be available at that point.
// =============================================================================

const {
  mockRandomUUID,
  mockEntityInitializeMessages,
  mockEntitySetMessages,
  mockEntitySetParentContext,
  MockAgentLoopEntity,
  MockAgentLoopCheckpointCoordinator,
  mockRestoreFromCheckpoint,
  mockRegisterChildAgentLoop,
} = vi.hoisted(() => {
  const _mockRandomUUID = vi.fn();

  const _mockEntityInitializeMessages = vi.fn();
  const _mockEntitySetMessages = vi.fn();
  const _mockEntitySetParentContext = vi.fn();

  const _MockAgentLoopEntity = vi.fn(function (
    this: any,
    id: string,
    config: AgentLoopRuntimeConfig,
  ) {
    this.id = id;
    this.config = config;
    this.state = { currentIteration: 0 };
    this.setMessages = _mockEntitySetMessages;
    this.setParentContext = _mockEntitySetParentContext;
  });

  const _mockRestoreFromCheckpoint = vi.fn();

  const _MockAgentLoopCheckpointCoordinator = vi.fn(function () {
    return {
      restoreFromCheckpoint: _mockRestoreFromCheckpoint,
    };
  });

  const _mockRegisterChildAgentLoop = vi.fn();

  return {
    mockRandomUUID: _mockRandomUUID,
    mockEntityInitializeMessages: _mockEntityInitializeMessages,
    mockEntitySetMessages: _mockEntitySetMessages,
    mockEntitySetParentContext: _mockEntitySetParentContext,
    MockAgentLoopEntity: _MockAgentLoopEntity,
    MockAgentLoopCheckpointCoordinator: _MockAgentLoopCheckpointCoordinator,
    mockRestoreFromCheckpoint: _mockRestoreFromCheckpoint,
    mockRegisterChildAgentLoop: _mockRegisterChildAgentLoop,
  };
});

// =============================================================================
// Module-level mocks (hoisted by vitest)
// =============================================================================

vi.mock("crypto", () => ({
  randomUUID: mockRandomUUID,
}));

vi.mock("../../../entities/agent-loop-entity.js", () => ({
  AgentLoopEntity: MockAgentLoopEntity,
}));

vi.mock("../../../checkpoint/index.js", () => ({
  AgentLoopCheckpointCoordinator: MockAgentLoopCheckpointCoordinator,
}));

// =============================================================================
// Import after mocks are set up
// =============================================================================
import { AgentLoopFactory } from "../agent-loop-factory.js";

describe("AgentLoopFactory", () => {
  let mockGlobalContext: any;
  let mockConfig: AgentLoopRuntimeConfig;
  let mockOptions: any;
  let mockContainer: any;
  let mockExecutionHierarchyRegistry: any;
  let mockWorkflowExecutionRegistry: any;
  let mockWorkflowExecutionEntity: any;

  beforeEach(() => {
    vi.clearAllMocks();

    mockRandomUUID.mockReturnValue("test-uuid-123");

    mockExecutionHierarchyRegistry = {
      register: vi.fn(),
    };

    mockWorkflowExecutionEntity = {
      registerChildAgentLoop: mockRegisterChildAgentLoop,
    };

    mockWorkflowExecutionRegistry = {
      get: vi.fn().mockReturnValue(mockWorkflowExecutionEntity),
    };

    mockContainer = {
      get: vi.fn((id: symbol) => {
        // ExecutionHierarchyRegistry
        if (id.toString().includes("ExecutionHierarchyRegistry")) {
          return mockExecutionHierarchyRegistry;
        }
        // WorkflowExecutionRegistry
        if (id.toString().includes("WorkflowExecutionRegistry")) {
          return mockWorkflowExecutionRegistry;
        }
        return {};
      }),
    };

    mockGlobalContext = {
      container: mockContainer,
    };

    mockConfig = {
      profileId: "test-profile",
      maxIterations: 10,
      availableTools: { tools: ["tool1", "tool2"] },
      systemPrompt: "You are a helpful assistant.",
      initialUserMessage: "Hello!",
    };

    mockOptions = {};
  });

  // ===========================================================================
  // AgentLoopFactory.create Tests
  // ===========================================================================

  describe("create", () => {
    it("should create a new AgentLoopEntity with minimal options", async () => {
      const result = await AgentLoopFactory.create(mockGlobalContext, mockConfig);

      expect(mockRandomUUID).toHaveBeenCalled();
      expect(MockAgentLoopEntity).toHaveBeenCalledWith(
        "agent-loop-test-uuid-123",
        mockConfig,
        undefined,
        undefined,
        mockExecutionHierarchyRegistry,
      );
      // Messages are now set via stateCoordinator.setMessages() in the factory
      // No longer calls entity.initializeMessages()
      expect(result.entity.id).toBe("agent-loop-test-uuid-123");
    });

    it("should call setMessages when initialMessages.length > 0", async () => {
      const initialMessages = [{ role: "user", content: "Override" } as any];
      mockOptions.initialMessages = initialMessages;

      await AgentLoopFactory.create(mockGlobalContext, mockConfig, mockOptions);

      // Messages are set on stateCoordinator, not entity
      expect(mockEntitySetMessages).not.toHaveBeenCalled();
    });

    it("should NOT call setMessages when initialMessages is empty", async () => {
      mockOptions.initialMessages = [];

      await AgentLoopFactory.create(mockGlobalContext, mockConfig, mockOptions);

      expect(mockEntitySetMessages).not.toHaveBeenCalled();
    });

    it("should NOT call setMessages when initialMessages is undefined", async () => {
      await AgentLoopFactory.create(mockGlobalContext, mockConfig);

      expect(mockEntitySetMessages).not.toHaveBeenCalled();
    });

    it("should set parent context when parentExecutionId is provided", async () => {
      mockOptions.parentExecutionId = "parent-workflow-1";
      mockOptions.nodeId = "node-1";

      await AgentLoopFactory.create(mockGlobalContext, mockConfig, mockOptions);

      expect(mockEntitySetParentContext).toHaveBeenCalledWith({
        parentType: "WORKFLOW",
        parentId: "parent-workflow-1",
        nodeId: "node-1",
      });
    });

    it("should NOT set parent context when parentExecutionId is not provided", async () => {
      await AgentLoopFactory.create(mockGlobalContext, mockConfig);

      expect(mockEntitySetParentContext).not.toHaveBeenCalled();
    });

    it("should register with parent execution when parentExecutionId is provided", async () => {
      mockOptions.parentExecutionId = "parent-workflow-1";

      await AgentLoopFactory.create(mockGlobalContext, mockConfig, mockOptions);

      expect(mockContainer.get).toHaveBeenCalled();
      // Verify that WorkflowExecutionRegistry was accessed
      expect(mockWorkflowExecutionRegistry.get).toHaveBeenCalledWith("parent-workflow-1");
      expect(mockRegisterChildAgentLoop).toHaveBeenCalledWith("agent-loop-test-uuid-123");
    });

    it("should NOT register with parent when parentExecutionId is not provided", async () => {
      await AgentLoopFactory.create(mockGlobalContext, mockConfig);

      expect(mockRegisterChildAgentLoop).not.toHaveBeenCalled();
    });

    it("should handle parent workflow execution not found gracefully", async () => {
      mockWorkflowExecutionRegistry.get.mockReturnValue(undefined);
      mockOptions.parentExecutionId = "parent-workflow-1";

      // Should not throw
      const entity = await AgentLoopFactory.create(mockGlobalContext, mockConfig, mockOptions);

      expect(entity).toBeDefined();
      expect(mockRegisterChildAgentLoop).not.toHaveBeenCalled();
    });

    it("should handle workflow execution registry resolution failure gracefully", async () => {
      mockContainer.get = vi.fn((id: symbol) => {
        if (id.toString().includes("ExecutionHierarchyRegistry")) {
          return mockExecutionHierarchyRegistry;
        }
        // Fail only on WorkflowExecutionRegistry resolution
        throw new Error("Container resolution failed");
      });
      mockOptions.parentExecutionId = "parent-workflow-1";

      // Should not throw
      const entity = await AgentLoopFactory.create(mockGlobalContext, mockConfig, mockOptions);

      expect(entity).toBeDefined();
    });

    it("should use DEFAULT profileId in log when config.profileId is not set", async () => {
      const configWithoutProfile: AgentLoopRuntimeConfig = {
        ...mockConfig,
        profileId: undefined,
      };

      const entity = await AgentLoopFactory.create(mockGlobalContext, configWithoutProfile);

      expect(entity).toBeDefined();
      expect(MockAgentLoopEntity).toHaveBeenCalledWith(
        "agent-loop-test-uuid-123",
        configWithoutProfile,
        undefined,
        undefined,
        mockExecutionHierarchyRegistry,
      );
    });
  });

  // ===========================================================================
  // AgentLoopFactory.fromCheckpoint Tests
  // ===========================================================================

  describe("fromCheckpoint", () => {
    const checkpointId = "checkpoint-1";
    const mockCheckpointDeps = {
      saveCheckpoint: vi.fn(),
      getCheckpoint: vi.fn(),
      listCheckpoints: vi.fn(),
    };

    it("should restore entity from checkpoint successfully", async () => {
      const restoredEntity = {
        id: "agent-loop-restored",
        state: { currentIteration: 5 },
      };
      mockRestoreFromCheckpoint.mockResolvedValue(restoredEntity);

      const entity = await AgentLoopFactory.fromCheckpoint(
        checkpointId,
        mockConfig,
        mockCheckpointDeps,
      );

      expect(MockAgentLoopCheckpointCoordinator).toHaveBeenCalledTimes(1);
      expect(mockRestoreFromCheckpoint).toHaveBeenCalledWith(checkpointId, mockCheckpointDeps);
      expect(entity).toBe(restoredEntity);
    });

    it("should propagate errors when checkpoint not found", async () => {
      const error = new Error("Checkpoint not found: checkpoint-1");
      mockRestoreFromCheckpoint.mockRejectedValue(error);

      await expect(
        AgentLoopFactory.fromCheckpoint(checkpointId, mockConfig, mockCheckpointDeps),
      ).rejects.toThrow("Checkpoint not found: checkpoint-1");
    });

    it("should pass deltaConfig when provided", async () => {
      const depsWithDelta = {
        ...mockCheckpointDeps,
        deltaConfig: { enabled: true },
      };
      const restoredEntity = {
        id: "agent-loop-delta",
        state: { currentIteration: 3 },
      };
      mockRestoreFromCheckpoint.mockResolvedValue(restoredEntity);

      const entity = await AgentLoopFactory.fromCheckpoint(checkpointId, mockConfig, depsWithDelta);

      expect(mockRestoreFromCheckpoint).toHaveBeenCalledWith(checkpointId, depsWithDelta);
      expect(entity).toBe(restoredEntity);
    });
  });
});
