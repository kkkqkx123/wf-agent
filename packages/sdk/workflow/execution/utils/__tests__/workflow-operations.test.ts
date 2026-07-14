/**
 * Workflow Operations Unit Tests
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { fork, join, copy, type ForkConfig, type JoinStrategy } from "../workflow-operations.js";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { WorkflowExecutionBuilder } from "../../factories/workflow-execution-builder.js";
import type { WorkflowExecutionRegistry } from "../../../registry/workflow-execution-registry.js";
import type { EventRegistry } from "../../../../shared/registry/event-registry.js";
import type { WorkflowStateCoordinator } from "../../../state-managers/workflow-state-coordinator.js";
import { RuntimeValidationError, ExecutionError } from "@wf-agent/types";

// Mock the contextual logger
vi.mock("../../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

// Mock the event builders
vi.mock("../../../shared/events/builders/index.js", () => ({
  buildWorkflowExecutionForkStartedEvent: vi.fn(data => ({
    type: "WORKFLOW_EXECUTION_FORK_STARTED",
    ...data,
  })),
  buildWorkflowExecutionForkCompletedEvent: vi.fn(data => ({
    type: "WORKFLOW_EXECUTION_FORK_COMPLETED",
    ...data,
  })),
  buildWorkflowExecutionJoinStartedEvent: vi.fn(data => ({
    type: "WORKFLOW_EXECUTION_JOIN_STARTED",
    ...data,
  })),
  buildWorkflowExecutionJoinConditionMetEvent: vi.fn(data => ({
    type: "WORKFLOW_EXECUTION_JOIN_CONDITION_MET",
    ...data,
  })),
  buildWorkflowExecutionCopyStartedEvent: vi.fn(data => ({
    type: "WORKFLOW_EXECUTION_COPY_STARTED",
    ...data,
  })),
  buildWorkflowExecutionCopyCompletedEvent: vi.fn(data => ({
    type: "WORKFLOW_EXECUTION_COPY_COMPLETED",
    ...data,
  })),
}));

// Mock the emit function
vi.mock("../../../shared/events/emit-event.js", () => ({
  emit: vi.fn(),
}));

// Mock the message array utils
vi.mock("../../../../shared/messaging/message-array-utils.js", () => ({
  MessageArrayUtils: {
    cloneMessages: vi.fn(messages => messages.map((m: unknown) => ({ ...(m as object) }))),
  },
}));

// Mock the event waiter
vi.mock("./event/event-waiter.js", () => ({
  waitForMultipleWorkflowExecutionsCompleted: vi.fn().mockResolvedValue(undefined),
  waitForAnyWorkflowExecutionCompleted: vi.fn().mockResolvedValue("child-exec-1"),
  waitForAnyWorkflowExecutionCompletion: vi.fn().mockResolvedValue({
    executionId: "child-exec-1",
    status: "COMPLETED",
  }),
}));

describe("fork", () => {
  let mockParentEntity: WorkflowExecutionEntity;
  let mockExecutionBuilder: WorkflowExecutionBuilder;
  let mockEventManager: EventRegistry;
  let mockChildEntity: WorkflowExecutionEntity;

  beforeEach(() => {
    vi.clearAllMocks();

    mockParentEntity = {
      id: "parent-exec-1",
      getWorkflowId: vi.fn().mockReturnValue("workflow-1"),
      getCurrentNodeId: vi.fn().mockReturnValue("node-1"),
    } as unknown as WorkflowExecutionEntity;

    mockChildEntity = {
      id: "child-exec-1",
    } as unknown as WorkflowExecutionEntity;

    mockExecutionBuilder = {
      createChildExecution: vi.fn().mockResolvedValue({
        workflowExecutionEntity: mockChildEntity,
      }),
    } as unknown as WorkflowExecutionBuilder;

    mockEventManager = {
      emit: vi.fn(),
      waitFor: vi.fn(),
    } as unknown as EventRegistry;
  });

  describe("validation", () => {
    it("should throw error when forkId is missing", async () => {
      const forkConfig: ForkConfig = { forkId: "" };

      await expect(fork(mockParentEntity, forkConfig, mockExecutionBuilder)).rejects.toThrow(
        RuntimeValidationError,
      );
    });

    it("should throw error when forkStrategy is invalid", async () => {
      const forkConfig: ForkConfig = {
        forkId: "fork-1",
        forkStrategy: "invalid" as "serial" | "parallel",
      };

      await expect(fork(mockParentEntity, forkConfig, mockExecutionBuilder)).rejects.toThrow(
        RuntimeValidationError,
      );
    });

    it("should accept valid forkStrategy values", async () => {
      const serialConfig: ForkConfig = { forkId: "fork-1", forkStrategy: "serial" };
      const parallelConfig: ForkConfig = { forkId: "fork-2", forkStrategy: "parallel" };

      await fork(mockParentEntity, serialConfig, mockExecutionBuilder);
      await fork(mockParentEntity, parallelConfig, mockExecutionBuilder);

      expect(mockExecutionBuilder.createChildExecution).toHaveBeenCalledTimes(2);
    });
  });

  describe("child execution creation", () => {
    it("should create child execution with fork config", async () => {
      const forkConfig: ForkConfig = {
        forkId: "fork-1",
        forkPathId: "path-1",
        startNodeId: "node-2",
      };

      const childEntity = await fork(mockParentEntity, forkConfig, mockExecutionBuilder);

      expect(mockExecutionBuilder.createChildExecution).toHaveBeenCalledWith(mockParentEntity, {
        type: "FORK_BRANCH",
        config: {
          forkPathId: "path-1",
          startNodeId: "node-2",
        },
      });
      expect(childEntity.id).toBe("child-exec-1");
    });

    it("should create child execution without optional config", async () => {
      const forkConfig: ForkConfig = { forkId: "fork-1" };

      await fork(mockParentEntity, forkConfig, mockExecutionBuilder);

      expect(mockExecutionBuilder.createChildExecution).toHaveBeenCalledWith(mockParentEntity, {
        type: "FORK_BRANCH",
        config: {
          forkPathId: undefined,
          startNodeId: undefined,
        },
      });
    });
  });

  describe("event emission", () => {
    it("should emit fork started event when eventManager provided", async () => {
      const forkConfig: ForkConfig = { forkId: "fork-1" };

      await fork(mockParentEntity, forkConfig, mockExecutionBuilder, mockEventManager);

      expect(mockEventManager.emit).toHaveBeenCalled();
    });

    it("should emit fork completed event when eventManager provided", async () => {
      const forkConfig: ForkConfig = { forkId: "fork-1" };

      await fork(mockParentEntity, forkConfig, mockExecutionBuilder, mockEventManager);

      // Should emit both started and completed events
      expect(mockEventManager.emit).toHaveBeenCalledTimes(2);
    });

    it("should not emit events when eventManager not provided", async () => {
      const forkConfig: ForkConfig = { forkId: "fork-1" };

      await fork(mockParentEntity, forkConfig, mockExecutionBuilder);

      expect(mockEventManager.emit).not.toHaveBeenCalled();
    });

    it("should handle event emission errors gracefully", async () => {
      (mockEventManager.emit as ReturnType<typeof vi.fn>).mockRejectedValue(
        new Error("Emit failed"),
      );

      const forkConfig: ForkConfig = { forkId: "fork-1" };

      // Should not throw
      await expect(
        fork(mockParentEntity, forkConfig, mockExecutionBuilder, mockEventManager),
      ).resolves.not.toThrow();
    });
  });
});

describe("join", () => {
  let mockRegistry: WorkflowExecutionRegistry;
  let mockEventManager: EventRegistry;
  let mockParentEntity: WorkflowExecutionEntity;
  let mockChildEntity: WorkflowExecutionEntity;
  let mockStateCoordinatorMap: Map<string, WorkflowStateCoordinator>;
  let mockSourceStateCoordinator: WorkflowStateCoordinator;
  let mockParentStateCoordinator: WorkflowStateCoordinator;

  beforeEach(() => {
    vi.clearAllMocks();

    mockSourceStateCoordinator = {
      exportMessagesForChild: vi.fn().mockReturnValue([]),
      importMessagesFromChild: vi.fn(),
      cleanup: vi.fn(),
    } as unknown as WorkflowStateCoordinator;

    mockParentStateCoordinator = {
      exportMessagesForChild: vi.fn().mockReturnValue([]),
      importMessagesFromChild: vi.fn(),
      cleanup: vi.fn(),
    } as unknown as WorkflowStateCoordinator;

    mockStateCoordinatorMap = new Map([
      ["child-exec-1", mockSourceStateCoordinator],
      ["parent-exec-1", mockParentStateCoordinator],
    ]);

    mockParentEntity = {
      id: "parent-exec-1",
      getWorkflowId: vi.fn().mockReturnValue("workflow-1"),
      variableStateManager: {
        getVariable: vi.fn(),
        setVariable: vi.fn(),
      },
    } as unknown as WorkflowExecutionEntity;

    mockChildEntity = {
      id: "child-exec-1",
      getWorkflowId: vi.fn().mockReturnValue("workflow-1"),
      getStatus: vi.fn().mockReturnValue("COMPLETED"),
      getWorkflowExecutionData: vi.fn().mockReturnValue({
        id: "child-exec-1",
        forkJoinContext: { forkPathId: "path-1" },
      }),
      variableStateManager: {
        getVariable: vi.fn().mockReturnValue("value"),
      },
    } as unknown as WorkflowExecutionEntity;

    mockRegistry = {
      get: vi.fn().mockImplementation((id: string) => {
        if (id === "parent-exec-1") return mockParentEntity;
        if (id === "child-exec-1") return mockChildEntity;
        return null;
      }),
    } as unknown as WorkflowExecutionRegistry;

    mockEventManager = {
      emit: vi.fn(),
      waitFor: vi.fn().mockResolvedValue(undefined),
    } as unknown as EventRegistry;
  });

  describe("validation", () => {
    it("should throw error when joinStrategy is missing", async () => {
      await expect(
        join(["child-exec-1"], undefined as unknown as JoinStrategy, mockRegistry, "path-1"),
      ).rejects.toThrow(RuntimeValidationError);
    });

    it("should throw error when timeout is negative", async () => {
      await expect(
        join(["child-exec-1"], "ALL_COMPLETED", mockRegistry, "path-1", -1),
      ).rejects.toThrow(RuntimeValidationError);
    });

    it("should accept zero timeout (no timeout)", async () => {
      await expect(
        join(
          ["child-exec-1"],
          "ALL_COMPLETED",
          mockRegistry,
          "path-1",
          0,
          "parent-exec-1",
          mockEventManager,
          undefined,
          undefined,
          mockStateCoordinatorMap,
        ),
      ).resolves.not.toThrow();
    });
  });

  describe("ALL_COMPLETED strategy", () => {
    it("should wait for all executions to complete", async () => {
      const result = await join(
        ["child-exec-1"],
        "ALL_COMPLETED",
        mockRegistry,
        "path-1",
        0,
        "parent-exec-1",
        mockEventManager,
        undefined,
        undefined,
        mockStateCoordinatorMap,
      );

      expect(result.success).toBe(true);
    });
  });

  describe("ANY_COMPLETED strategy", () => {
    it("should wait for any execution to complete", async () => {
      const result = await join(
        ["child-exec-1"],
        "ANY_COMPLETED",
        mockRegistry,
        "path-1",
        0,
        "parent-exec-1",
        mockEventManager,
        undefined,
        undefined,
        mockStateCoordinatorMap,
      );

      expect(result.success).toBe(true);
    });
  });

  describe("event emission", () => {
    it("should emit join started event when eventManager provided", async () => {
      await join(
        ["child-exec-1"],
        "ALL_COMPLETED",
        mockRegistry,
        "path-1",
        0,
        "parent-exec-1",
        mockEventManager,
        undefined,
        undefined,
        mockStateCoordinatorMap,
      );

      expect(mockEventManager.emit).toHaveBeenCalled();
    });

    it("should throw error when eventManager not provided", async () => {
      await expect(
        join(["child-exec-1"], "ALL_COMPLETED", mockRegistry, "path-1", 0, "parent-exec-1"),
      ).rejects.toThrow(ExecutionError);
    });
  });

  describe("message export", () => {
    it("should export messages from main path via stateCoordinatorMap", async () => {
      const messages = [{ role: "user", content: "test" }];
      (mockSourceStateCoordinator.exportMessagesForChild as ReturnType<typeof vi.fn>).mockReturnValue(
        messages,
      );

      await join(
        ["child-exec-1"],
        "ALL_COMPLETED",
        mockRegistry,
        "path-1",
        0,
        "parent-exec-1",
        mockEventManager,
        undefined,
        undefined,
        mockStateCoordinatorMap,
      );

      expect(mockSourceStateCoordinator.exportMessagesForChild).toHaveBeenCalled();
      expect(mockParentStateCoordinator.importMessagesFromChild).toHaveBeenCalled();
    });
  });

  describe("variable export", () => {
    it("should export variables from branches", async () => {
      const variableOutputs = [
        { sourcePathId: "path-1", variableName: "var1", targetName: "targetVar1" },
      ];

      await join(
        ["child-exec-1"],
        "ALL_COMPLETED",
        mockRegistry,
        "path-1",
        0,
        "parent-exec-1",
        mockEventManager,
        variableOutputs,
        undefined,
        mockStateCoordinatorMap,
      );

      expect(mockParentEntity.variableStateManager.setVariable).toHaveBeenCalledWith(
        "targetVar1",
        "value",
      );
    });

    it("should handle missing variables gracefully", async () => {
      (
        mockChildEntity.variableStateManager.getVariable as ReturnType<typeof vi.fn>
      ).mockReturnValue(undefined);

      const variableOutputs = [{ sourcePathId: "path-1", variableName: "missingVar" }];

      await join(
        ["child-exec-1"],
        "ALL_COMPLETED",
        mockRegistry,
        "path-1",
        0,
        "parent-exec-1",
        mockEventManager,
        variableOutputs,
        undefined,
        mockStateCoordinatorMap,
      );

      // Should not throw
      expect(mockParentEntity.variableStateManager.setVariable).not.toHaveBeenCalled();
    });
  });
});

  describe("copy", () => {
    let mockSourceEntity: WorkflowExecutionEntity;
    let mockExecutionBuilder: WorkflowExecutionBuilder;
    let mockSourceStateCoordinator: WorkflowStateCoordinator;
    let mockEventManager: EventRegistry;
    let mockCopiedEntity: WorkflowExecutionEntity;

    beforeEach(() => {
      vi.clearAllMocks();

      mockSourceEntity = {
        id: "source-exec-1",
        getWorkflowId: vi.fn().mockReturnValue("workflow-1"),
        cleanup: vi.fn(),
      } as unknown as WorkflowExecutionEntity;

      mockCopiedEntity = {
        id: "copied-exec-1",
      } as unknown as WorkflowExecutionEntity;

      mockExecutionBuilder = {
        createCopy: vi.fn().mockResolvedValue({
          workflowExecutionEntity: mockCopiedEntity,
        }),
      } as unknown as WorkflowExecutionBuilder;

      mockSourceStateCoordinator = {
        cleanup: vi.fn(),
      } as unknown as WorkflowStateCoordinator;

      mockEventManager = {
        emit: vi.fn(),
      } as unknown as EventRegistry;
    });

  describe("validation", () => {
    it("should throw error when source entity is null", async () => {
      await expect(
        copy(
          null as unknown as WorkflowExecutionEntity,
          mockExecutionBuilder,
          mockSourceStateCoordinator,
        ),
      ).rejects.toThrow(ExecutionError);
    });

    it("should throw error when source entity is undefined", async () => {
      await expect(
        copy(
          undefined as unknown as WorkflowExecutionEntity,
          mockExecutionBuilder,
          mockSourceStateCoordinator,
        ),
      ).rejects.toThrow(ExecutionError);
    });
  });

  describe("copy creation", () => {
    it("should create copy of workflow execution", async () => {
      const copiedEntity = await copy(
        mockSourceEntity,
        mockExecutionBuilder,
        mockSourceStateCoordinator,
      );

      expect(mockExecutionBuilder.createCopy).toHaveBeenCalledWith(
        mockSourceEntity,
        mockSourceStateCoordinator,
      );
      expect(copiedEntity.id).toBe("copied-exec-1");
    });
  });

  describe("event emission", () => {
    it("should emit copy started event when eventManager provided", async () => {
      await copy(
        mockSourceEntity,
        mockExecutionBuilder,
        mockSourceStateCoordinator,
        mockEventManager,
      );

      expect(mockEventManager.emit).toHaveBeenCalled();
    });

    it("should emit copy completed event when eventManager provided", async () => {
      await copy(
        mockSourceEntity,
        mockExecutionBuilder,
        mockSourceStateCoordinator,
        mockEventManager,
      );

      // Should emit both started and completed events
      expect(mockEventManager.emit).toHaveBeenCalledTimes(2);
    });

    it("should not emit events when eventManager not provided", async () => {
      await copy(mockSourceEntity, mockExecutionBuilder, mockSourceStateCoordinator);

      expect(mockEventManager.emit).not.toHaveBeenCalled();
    });

    it("should handle event emission errors gracefully", async () => {
      (mockEventManager.emit as ReturnType<typeof vi.fn>).mockRejectedValue(
        new Error("Emit failed"),
      );

      // Should not throw
      await expect(
        copy(mockSourceEntity, mockExecutionBuilder, mockSourceStateCoordinator, mockEventManager),
      ).resolves.not.toThrow();
    });
  });
});
