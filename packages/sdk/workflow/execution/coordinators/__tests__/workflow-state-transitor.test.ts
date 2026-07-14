/**
 * WorkflowStateTransitor Unit Tests
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { WorkflowStateTransitor } from "../workflow-state-transitor.js";
import type { EventRegistry } from "../../../../shared/registry/event-registry.js";
import type { ConversationSession } from "../../../../shared/messaging/conversation-session.js";
import type { WorkflowExecutionRegistry } from "../../../registry/workflow-execution-registry.js";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { GlobalContext } from "../../../../shared/global-context.js";
import type { WorkflowExecutionResult } from "@wf-agent/types";

// ============================================================================
// Mock Helpers
// ============================================================================

function createMockEventRegistry(): EventRegistry {
  return {
    on: vi.fn(),
    off: vi.fn(),
    emit: vi.fn().mockResolvedValue(undefined),
    once: vi.fn(),
    getEmitter: vi.fn().mockReturnValue({ on: vi.fn(), emit: vi.fn() }),
    removeAllListeners: vi.fn(),
    listenerCount: vi.fn().mockReturnValue(0),
    listeners: vi.fn().mockReturnValue([]),
    rawListeners: vi.fn().mockReturnValue([]),
    waitFor: vi.fn().mockResolvedValue(undefined),
    cleanupExecutionListeners: vi.fn(),
    getExecutionListenerStats: vi.fn().mockReturnValue({}),
    getMetricsCollector: vi.fn(),
    registerWorkflowListeners: vi.fn(),
  } as unknown as EventRegistry;
}

function createMockConversationSession(): ConversationSession {
  return {
    addMessage: vi.fn(),
    getMessages: vi.fn().mockReturnValue([]),
    cleanup: vi.fn(),
    clearContext: vi.fn(),
    getContextId: vi.fn().mockReturnValue("test-context"),
    getTokenCount: vi.fn().mockReturnValue(0),
    getMessageCount: vi.fn().mockReturnValue(0),
  } as unknown as ConversationSession;
}

function createMockEntity(
  overrides: Partial<WorkflowExecutionEntity> = {},
): WorkflowExecutionEntity {
  const errors: string[] = [];
  return {
    id: "test-exec-1",
    getWorkflowId: vi.fn().mockReturnValue("workflow-1"),
    getStatus: vi.fn().mockReturnValue("CREATED"),
    getCurrentNodeId: vi.fn().mockReturnValue("node-1"),
    setCurrentNodeId: vi.fn(),
    getOutput: vi.fn().mockReturnValue({}),
    setOutput: vi.fn(),
    getInput: vi.fn().mockReturnValue({}),
    getStartTime: vi.fn().mockReturnValue(1000),
    getEndTime: vi.fn().mockReturnValue(undefined),
    getErrors: vi.fn().mockReturnValue(errors),
    getNodeResults: vi.fn().mockReturnValue([]),
    addNodeResult: vi.fn(),
    getAllVariables: vi.fn().mockReturnValue({}),
    getAbortSignal: vi.fn().mockReturnValue(new AbortController().signal),
    resetInterrupt: vi.fn(),
    cleanup: vi.fn(),
    getWorkflowVersion: vi.fn().mockReturnValue("1.0.0"),
    getHierarchyDepth: vi.fn().mockReturnValue(0),
    getChildExecutionIds: vi.fn().mockReturnValue([]),
    state: {
      start: vi.fn(),
      pause: vi.fn(),
      resume: vi.fn(),
      complete: vi.fn(),
      fail: vi.fn(),
      cancel: vi.fn(),
      setCurrentOperation: vi.fn(),
      clearOperation: vi.fn(),
    },
    buildEvent: vi.fn().mockReturnValue({ type: "NODE_STARTED" }),
    ...overrides,
  } as unknown as WorkflowExecutionEntity;
}

function createMockRegistry(): WorkflowExecutionRegistry {
  return {
    register: vi.fn(),
    get: vi.fn(),
    delete: vi.fn(),
    getAll: vi.fn().mockReturnValue([]),
    getAllIds: vi.fn().mockReturnValue([]),
    size: vi.fn().mockReturnValue(0),
    clear: vi.fn(),
    has: vi.fn().mockReturnValue(false),
    isWorkflowActive: vi.fn().mockReturnValue(false),
    getByStatus: vi.fn().mockReturnValue([]),
    getActive: vi.fn().mockReturnValue([]),
    cleanupTerminated: vi.fn().mockReturnValue(0),
  } as unknown as WorkflowExecutionRegistry;
}

function createMockGlobalContext(): GlobalContext {
  return {
    container: {
      get: vi.fn(),
    },
    eventRegistry: createMockEventRegistry(),
    llmExecutor: {},
    metricsRegistry: {
      getWorkflowCollector: vi.fn().mockReturnValue({
        recordExecutionStart: vi.fn(),
        recordExecutionEnd: vi.fn(),
      }),
      getNodeCollector: vi.fn().mockReturnValue({
        recordNodeExecutionStart: vi.fn(),
        recordNodeExecution: vi.fn(),
      }),
    },
  } as unknown as GlobalContext;
}

function createWorkflowExecutionResult(
  overrides: Partial<WorkflowExecutionResult> = {},
): WorkflowExecutionResult {
  return {
    executionId: "test-exec-1",
    output: {},
    executionTime: 100,
    nodeResults: [],
    metadata: {
      status: "COMPLETED",
      startTime: 1000,
      endTime: 1100,
      executionTime: 100,
      nodeCount: 1,
      errorCount: 0,
    },
    ...overrides,
  };
}

// ============================================================================
// Tests
// ============================================================================

describe("WorkflowStateTransitor", () => {
  let transitor: WorkflowStateTransitor;
  let mockEventRegistry: EventRegistry;
  let mockConversationSession: ConversationSession;
  let mockRegistry: WorkflowExecutionRegistry;
  let mockGlobalContext: GlobalContext;
  let mockEntity: WorkflowExecutionEntity;

  beforeEach(() => {
    mockEventRegistry = createMockEventRegistry();
    mockConversationSession = createMockConversationSession();
    mockRegistry = createMockRegistry();
    mockGlobalContext = createMockGlobalContext();
    mockEntity = createMockEntity();

    transitor = new WorkflowStateTransitor(
      mockEventRegistry,
      mockConversationSession,
      mockRegistry,
      mockGlobalContext,
    );
  });

  describe("startWorkflowExecution", () => {
    it("should transition from CREATED to RUNNING", async () => {
      await transitor.startWorkflowExecution(mockEntity);

      expect(mockEntity.state.start).toHaveBeenCalled();
      expect(mockEventRegistry.emit).toHaveBeenCalledTimes(2); // started + stateChanged
    });

    it("should throw on invalid transition (COMPLETED -> RUNNING)", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("COMPLETED");

      await expect(transitor.startWorkflowExecution(mockEntity)).rejects.toThrow();
    });
  });

  describe("pauseWorkflowExecution", () => {
    it("should transition from RUNNING to PAUSED", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("RUNNING");

      await transitor.pauseWorkflowExecution(mockEntity);

      expect(mockEntity.state.pause).toHaveBeenCalled();
      expect(mockEventRegistry.emit).toHaveBeenCalledTimes(2); // paused + stateChanged
    });

    it("should skip if already PAUSED", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("PAUSED");

      await transitor.pauseWorkflowExecution(mockEntity);

      expect(mockEntity.state.pause).not.toHaveBeenCalled();
      expect(mockEventRegistry.emit).not.toHaveBeenCalled();
    });

    it("should throw on invalid transition (CREATED -> PAUSED)", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("CREATED");

      await expect(transitor.pauseWorkflowExecution(mockEntity)).rejects.toThrow();
    });
  });

  describe("resumeWorkflowExecution", () => {
    it("should transition from PAUSED to RUNNING", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("PAUSED");

      await transitor.resumeWorkflowExecution(mockEntity);

      expect(mockEntity.state.resume).toHaveBeenCalled();
      expect(mockEventRegistry.emit).toHaveBeenCalledTimes(2); // resumed + stateChanged
    });

    it("should skip if already RUNNING", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("RUNNING");

      await transitor.resumeWorkflowExecution(mockEntity);

      expect(mockEntity.state.resume).not.toHaveBeenCalled();
      expect(mockEventRegistry.emit).not.toHaveBeenCalled();
    });
  });

  describe("completeWorkflowExecution", () => {
    it("should transition from RUNNING to COMPLETED", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("RUNNING");
      const result = createWorkflowExecutionResult();

      await transitor.completeWorkflowExecution(mockEntity, result);

      expect(mockEntity.state.complete).toHaveBeenCalled();
      expect(mockConversationSession.cleanup).toHaveBeenCalled();
      expect(mockEventRegistry.emit).toHaveBeenCalledTimes(2); // completed + stateChanged
    });

    it("should handle already COMPLETED status gracefully", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("COMPLETED");
      vi.mocked(mockEntity.getEndTime).mockReturnValue(2000);
      const result = createWorkflowExecutionResult();

      await transitor.completeWorkflowExecution(mockEntity, result);

      // Should still emit completed event and cleanup
      expect(mockConversationSession.cleanup).toHaveBeenCalled();
      expect(mockEventRegistry.emit).toHaveBeenCalledTimes(1); // only completedEvent
      // Should NOT call state.complete (already completed)
      expect(mockEntity.state.complete).not.toHaveBeenCalled();
    });

    it("should call state.complete when already COMPLETED but no endTime", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("COMPLETED");
      vi.mocked(mockEntity.getEndTime).mockReturnValue(null); // no endTime set
      const result = createWorkflowExecutionResult();

      await transitor.completeWorkflowExecution(mockEntity, result);

      // Should still complete the state
      expect(mockEntity.state.complete).toHaveBeenCalled();
    });

    it("should throw on invalid transition (CREATED -> COMPLETED)", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("CREATED");
      const result = createWorkflowExecutionResult();

      await expect(transitor.completeWorkflowExecution(mockEntity, result)).rejects.toThrow();
    });
  });

  describe("failWorkflowExecution", () => {
    it("should transition from RUNNING to FAILED", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("RUNNING");
      const error = new Error("Something went wrong");

      await transitor.failWorkflowExecution(mockEntity, error);

      expect(mockEntity.state.fail).toHaveBeenCalledWith(error);
      expect(mockConversationSession.cleanup).toHaveBeenCalled();
      expect(mockEventRegistry.emit).toHaveBeenCalledTimes(2); // failed + stateChanged
    });

    it("should add error message to errors array", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("RUNNING");
      const error = new Error("test error");
      const errors: string[] = [];
      vi.mocked(mockEntity.getErrors).mockReturnValue(errors);

      await transitor.failWorkflowExecution(mockEntity, error);

      expect(errors).toContain("test error");
    });

    it("should throw on invalid transition (CREATED -> FAILED)", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("CREATED");
      const error = new Error("fail");

      await expect(transitor.failWorkflowExecution(mockEntity, error)).rejects.toThrow();
    });
  });

  describe("cancelWorkflowExecution", () => {
    it("should transition from RUNNING to CANCELLED", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("RUNNING");

      await transitor.cancelWorkflowExecution(mockEntity, "user_requested");

      expect(mockEntity.state.cancel).toHaveBeenCalled();
      expect(mockConversationSession.cleanup).toHaveBeenCalled();
      expect(mockEventRegistry.emit).toHaveBeenCalledTimes(2); // cancelled + stateChanged
    });

    it("should skip if already CANCELLED", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("CANCELLED");

      await transitor.cancelWorkflowExecution(mockEntity, "user_requested");

      expect(mockEntity.state.cancel).not.toHaveBeenCalled();
      expect(mockEventRegistry.emit).not.toHaveBeenCalled();
    });

    it("should throw on invalid transition (COMPLETED -> CANCELLED)", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("COMPLETED");

      await expect(
        transitor.cancelWorkflowExecution(mockEntity, "user_requested"),
      ).rejects.toThrow();
    });

    it("should work without a reason", async () => {
      vi.mocked(mockEntity.getStatus).mockReturnValue("RUNNING");

      await transitor.cancelWorkflowExecution(mockEntity);

      expect(mockEntity.state.cancel).toHaveBeenCalled();
    });
  });

  describe("cascadeCancel", () => {
    it("should return 0 when parent execution not found", async () => {
      vi.mocked(mockRegistry.get).mockReturnValue(null);

      const count = await transitor.cascadeCancel("non-existent");

      expect(count).toBe(0);
    });

    it("should return 0 when there are no children", async () => {
      vi.mocked(mockEntity.getChildExecutionIds).mockReturnValue([]);
      vi.mocked(mockRegistry.get).mockReturnValue(mockEntity);

      const count = await transitor.cascadeCancel("test-exec-1");

      expect(count).toBe(0);
    });
  });

  describe("cancelChildWorkflowExecution", () => {
    it("should cancel a running child execution", async () => {
      const childEntity = createMockEntity({ id: "child-1" });
      vi.mocked(childEntity.getStatus).mockReturnValue("RUNNING");
      vi.mocked(mockRegistry.get).mockImplementation((id: string) => {
        if (id === "child-1") return childEntity;
        return null;
      });

      // Access the private method through `any` cast
      const result = await (transitor as any).cancelChildWorkflowExecution("child-1");

      expect(result).toBe(true);
      expect(childEntity.state.cancel).toHaveBeenCalled();
    });

    it("should cancel a paused child execution", async () => {
      const childEntity = createMockEntity({ id: "child-1" });
      vi.mocked(childEntity.getStatus).mockReturnValue("PAUSED");
      vi.mocked(mockRegistry.get).mockImplementation((id: string) => {
        if (id === "child-1") return childEntity;
        return null;
      });

      const result = await (transitor as any).cancelChildWorkflowExecution("child-1");

      expect(result).toBe(true);
      expect(childEntity.state.cancel).toHaveBeenCalled();
    });

    it("should skip completed child execution", async () => {
      const childEntity = createMockEntity({ id: "child-1" });
      vi.mocked(childEntity.getStatus).mockReturnValue("COMPLETED");
      vi.mocked(mockRegistry.get).mockImplementation((id: string) => {
        if (id === "child-1") return childEntity;
        return null;
      });

      const result = await (transitor as any).cancelChildWorkflowExecution("child-1");

      expect(result).toBe(false);
      expect(childEntity.state.cancel).not.toHaveBeenCalled();
    });

    it("should return false when child execution not found", async () => {
      vi.mocked(mockRegistry.get).mockReturnValue(null);

      const result = await (transitor as any).cancelChildWorkflowExecution("non-existent");

      expect(result).toBe(false);
    });
  });
});
