/**
 * Tests for CheckpointCoordinator
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { CheckpointCoordinator } from "../checkpoint-coordinator.js";
import type { CheckpointDependencies, CheckpointOptions } from "../checkpoint-coordinator.js";
import type { WorkflowExecution } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { WorkflowExecutionRegistry } from "../../registry/workflow-execution-registry.js";
import type { WorkflowRegistry } from "../../registry/workflow-registry.js";
import type { WorkflowGraphRegistry } from "../../registry/workflow-graph-registry.js";
import type { CheckpointState } from "../checkpoint-state-manager.js";
import type { FileCheckpointManager } from "@wf-agent/common-utils";

// Mock dependencies
vi.mock("../../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

vi.mock("../../../utils/index.js", () => ({
  generateId: vi.fn(() => "generated-cp-id"),
  mergeMetadata: vi.fn((...args) => Object.assign({}, ...args)),
}));

vi.mock("@wf-agent/common-utils", async importOriginal => {
  const actual = (await importOriginal()) as Record<string, unknown>;
  return {
    ...actual,
    now: vi.fn(() => 1234567890),
  };
});

vi.mock("../../../shared/messaging/conversation-session.js", () => ({
  ConversationSession: class MockConversationSession {
    getAllMessages = vi.fn().mockReturnValue([]);
    getMarkMap = vi.fn().mockReturnValue({
      currentBatch: 0,
      batchBoundaries: [0],
      originalIndices: [],
      boundaryToBatch: [],
    });
    getTokenUsage = vi
      .fn()
      .mockReturnValue({ totalTokens: 0, promptTokens: 0, completionTokens: 0 });
    getCurrentRequestUsage = vi
      .fn()
      .mockReturnValue({ totalTokens: 0, promptTokens: 0, completionTokens: 0 });
    addMessages = vi.fn();
    setMarkMap = vi.fn();
    setTokenUsageState = vi.fn();
  },
}));

vi.mock("../../state-managers/execution-state.js", () => ({
  ExecutionState: class MockExecutionState {
    getOperationStateSnapshot = vi.fn().mockReturnValue(null);
  },
}));

vi.mock("../../entities/workflow-execution-entity.js", () => ({
  WorkflowExecutionEntity: class MockWorkflowExecutionEntity {
    id: string;
    constructor(execution: any) {
      this.id = execution.id;
    }
    getExecution = vi.fn().mockReturnValue({
      graph: {
        getNode: vi.fn().mockReturnValue({ id: "node-1", name: "Start Node", type: "START" }),
      },
    });
    getWorkflowExecutionData = vi.fn().mockReturnValue({
      id: "exec-1",
      workflowId: "wf-1",
      graph: {
        getNode: vi.fn().mockReturnValue({ id: "node-1", name: "Start Node", type: "START" }),
      },
    });
    getGraph = vi.fn().mockReturnValue({
      getNode: vi.fn().mockReturnValue({ id: "node-1", name: "Start Node", type: "START" }),
    });
    getCurrentNodeId = vi.fn().mockReturnValue("node-1");
    getWorkflowId = vi.fn().mockReturnValue("wf-1");
    getStatus = vi.fn().mockReturnValue("RUNNING");
    variableStateManager = {
      createSnapshot: vi.fn().mockReturnValue({ variables: new Map() }),
      restoreFromSnapshot: vi.fn(),
    };
    getTriggerStateSnapshot = vi.fn().mockReturnValue({ triggers: [] });
    restoreTriggerState = vi.fn();
    getChildExecutionIds = vi.fn().mockReturnValue([]);
    state = {
      getOperationStateSnapshot: vi.fn().mockReturnValue(null),
      getErrorRecords: vi.fn().mockReturnValue([]),
      getInterruptionRecords: vi.fn().mockReturnValue([]),
      getEventRecords: vi.fn().mockReturnValue([]),
    };
  },
}));

vi.mock("../../state-managers/workflow-state-coordinator.js", () => ({
  WorkflowStateCoordinator: class MockWorkflowStateCoordinator {
    getConversationManager = vi.fn().mockReturnValue({
      getAllMessages: vi.fn().mockReturnValue([]),
      getMarkMap: vi.fn().mockReturnValue({
        currentBatch: 0,
        batchBoundaries: [0],
        originalIndices: [],
        boundaryToBatch: [],
      }),
      getTokenUsage: vi
        .fn()
        .mockReturnValue({ totalTokens: 0, promptTokens: 0, completionTokens: 0 }),
      getCurrentRequestUsage: vi
        .fn()
        .mockReturnValue({ totalTokens: 0, promptTokens: 0, completionTokens: 0 }),
    });
  },
}));

// BaseDiffCalculator and BaseDeltaRestorer use real implementations

function createMockWorkflowExecution(): WorkflowExecution {
  return {
    id: "exec-1",
    workflowId: "wf-1",
    workflowVersion: "1.0.0",
    currentNodeId: "node-1",
    input: { text: "hello" },
    output: {},
    nodeResults: [],
    errors: [],
    variables: [],
    forkJoinContext: undefined,
    triggeredSubworkflowContext: undefined,
    graph: {
      id: "graph-1",
      nodes: [],
      edges: [],
      getNode: vi.fn().mockReturnValue(undefined),
    } as any,
  };
}

function createMockDependencies(
  overrides?: Partial<CheckpointDependencies>,
): CheckpointDependencies {
  const mockCheckpointState = {
    list: vi.fn().mockResolvedValue([]),
    get: vi.fn().mockResolvedValue(null),
    create: vi.fn().mockResolvedValue("cp-1"),
    delete: vi.fn().mockResolvedValue(undefined),
  } as unknown as CheckpointState;

  const mockWorkflowExecutionRegistry = {
    get: vi.fn().mockReturnValue(null),
    register: vi.fn(),
    registerStateCoordinator: vi.fn(),
  } as unknown as WorkflowExecutionRegistry;

  const mockWorkflowRegistry = {} as unknown as WorkflowRegistry;

  const mockWorkflowGraphRegistry = {
    get: vi.fn().mockReturnValue(null),
  } as unknown as WorkflowGraphRegistry;

  return {
    workflowExecutionRegistry: mockWorkflowExecutionRegistry,
    checkpointStateManager: mockCheckpointState,
    workflowRegistry: mockWorkflowRegistry,
    workflowGraphRegistry: mockWorkflowGraphRegistry,
    ...overrides,
  };
}

describe("CheckpointCoordinator", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("createCheckpoint", () => {
    it("should throw WorkflowExecutionNotFoundError when execution entity is not found", async () => {
      const deps = createMockDependencies();

      await expect(
        CheckpointCoordinator.createCheckpoint("nonexistent-exec", deps),
      ).rejects.toThrow("WorkflowExecutionEntity not found");
    });

    it("should create a FULL checkpoint when no previous checkpoints exist", async () => {
      const execution = createMockWorkflowExecution();
      const mockEntity = {
        id: execution.id,
        getExecution: vi.fn().mockReturnValue(execution),
        getWorkflowExecutionData: vi.fn().mockReturnValue(execution),
        getGraph: vi.fn().mockReturnValue(execution.graph),
        getWorkflowId: vi.fn().mockReturnValue(execution.workflowId),
        getStatus: vi.fn().mockReturnValue("RUNNING"),
        variableStateManager: {
          createSnapshot: vi.fn().mockReturnValue({
            variables: new Map(),
          }),
          restoreFromSnapshot: vi.fn(),
        },
        getTriggerStateSnapshot: vi.fn().mockReturnValue({ triggers: [] }),
        restoreTriggerState: vi.fn(),
        getChildExecutionIds: vi.fn().mockReturnValue([]),
        state: {
          getOperationStateSnapshot: vi.fn().mockReturnValue(null),
          getErrorRecords: vi.fn().mockReturnValue([]),
          getInterruptionRecords: vi.fn().mockReturnValue([]),
          getEventRecords: vi.fn().mockReturnValue([]),
        },
      } as unknown as WorkflowExecutionEntity;

      const mockCheckpointState = {
        list: vi.fn().mockResolvedValue([]),
        create: vi.fn().mockResolvedValue("cp-1"),
      } as unknown as CheckpointState;

      const deps = createMockDependencies({
        checkpointStateManager: mockCheckpointState,
      });

      deps.workflowExecutionRegistry.get = vi.fn().mockReturnValue(mockEntity);

      const result = await CheckpointCoordinator.createCheckpoint("exec-1", deps);

      expect(result).toBe("cp-1");
      expect(mockCheckpointState.list).toHaveBeenCalledWith({ parentId: "exec-1" });
      expect(mockCheckpointState.create).toHaveBeenCalled();

      // Verify the checkpoint has FULL type and snapshot
      const createdCheckpoint = (mockCheckpointState.create as any).mock.calls[0][0];
      expect(createdCheckpoint.type).toBe("FULL");
      expect(createdCheckpoint.executionId).toBe("exec-1");
      expect(createdCheckpoint.workflowId).toBe("wf-1");
    });

    it("should create a FULL checkpoint when delta config is disabled", async () => {
      const execution = createMockWorkflowExecution();
      const mockEntity = {
        id: execution.id,
        getExecution: vi.fn().mockReturnValue(execution),
        getWorkflowExecutionData: vi.fn().mockReturnValue(execution),
        getGraph: vi.fn().mockReturnValue(execution.graph),
        getWorkflowId: vi.fn().mockReturnValue(execution.workflowId),
        getStatus: vi.fn().mockReturnValue("RUNNING"),
        variableStateManager: {
          createSnapshot: vi.fn().mockReturnValue({
            variables: new Map(),
          }),
          restoreFromSnapshot: vi.fn(),
        },
        getTriggerStateSnapshot: vi.fn().mockReturnValue({ triggers: [] }),
        restoreTriggerState: vi.fn(),
        getChildExecutionIds: vi.fn().mockReturnValue([]),
        state: {
          getOperationStateSnapshot: vi.fn().mockReturnValue(null),
          getErrorRecords: vi.fn().mockReturnValue([]),
          getInterruptionRecords: vi.fn().mockReturnValue([]),
          getEventRecords: vi.fn().mockReturnValue([]),
        },
      } as unknown as WorkflowExecutionEntity;

      const deps = createMockDependencies({
        deltaConfig: { enabled: false, baselineInterval: 10, maxDeltaChainLength: 20 },
      });
      deps.workflowExecutionRegistry.get = vi.fn().mockReturnValue(mockEntity);

      const result = await CheckpointCoordinator.createCheckpoint("exec-1", deps);

      expect(result).toBeDefined();
    });

    it("should create a DELTA checkpoint when there is a previous checkpoint", async () => {
      const execution = createMockWorkflowExecution();
      const mockEntity = {
        id: execution.id,
        getExecution: vi.fn().mockReturnValue(execution),
        getWorkflowExecutionData: vi.fn().mockReturnValue(execution),
        getGraph: vi.fn().mockReturnValue(execution.graph),
        getWorkflowId: vi.fn().mockReturnValue(execution.workflowId),
        getStatus: vi.fn().mockReturnValue("RUNNING"),
        variableStateManager: {
          createSnapshot: vi.fn().mockReturnValue({
            variables: new Map(),
          }),
          restoreFromSnapshot: vi.fn(),
        },
        getTriggerStateSnapshot: vi.fn().mockReturnValue({ triggers: [] }),
        restoreTriggerState: vi.fn(),
        getChildExecutionIds: vi.fn().mockReturnValue([]),
        state: {
          getOperationStateSnapshot: vi.fn().mockReturnValue(null),
          getErrorRecords: vi.fn().mockReturnValue([]),
          getInterruptionRecords: vi.fn().mockReturnValue([]),
          getEventRecords: vi.fn().mockReturnValue([]),
        },
      } as unknown as WorkflowExecutionEntity;

      const previousCheckpoint = {
        id: "cp-prev",
        executionId: "exec-1",
        workflowId: "wf-1",
        timestamp: 1000,
        type: "FULL" as const,
        snapshot: {
          status: "RUNNING",
          currentNodeId: "node-1",
          variables: [],
          variableState: { variables: {} },
          input: {},
          output: {},
          nodeResults: {},
          errors: [],
          conversationState: {
            messages: [],
            markMap: {
              currentBatch: 0,
              batchBoundaries: [0],
              originalIndices: [],
              boundaryToBatch: [],
            },
            tokenUsage: { totalTokens: 0, promptTokens: 0, completionTokens: 0 },
            currentRequestUsage: { totalTokens: 0, promptTokens: 0, completionTokens: 0 },
          },
        },
      };

      const mockCheckpointState = {
        list: vi.fn().mockResolvedValue(["cp-prev"]),
        get: vi.fn().mockResolvedValue(previousCheckpoint),
        create: vi.fn().mockResolvedValue("cp-delta"),
      } as unknown as CheckpointState;

      const deps = createMockDependencies({
        checkpointStateManager: mockCheckpointState,
        deltaConfig: { enabled: true, baselineInterval: 5, maxDeltaChainLength: 10 },
      });
      deps.workflowExecutionRegistry.get = vi.fn().mockReturnValue(mockEntity);

      const result = await CheckpointCoordinator.createCheckpoint("exec-1", deps);

      expect(result).toBe("cp-delta");

      const createdCheckpoint = (mockCheckpointState.create as any).mock.calls[0][0];
      expect(createdCheckpoint.type).toBe("DELTA");
    });

    it("should include metadata from options", async () => {
      const execution = createMockWorkflowExecution();
      const mockEntity = {
        id: execution.id,
        getExecution: vi.fn().mockReturnValue(execution),
        getWorkflowExecutionData: vi.fn().mockReturnValue(execution),
        getGraph: vi.fn().mockReturnValue(execution.graph),
        getWorkflowId: vi.fn().mockReturnValue(execution.workflowId),
        getStatus: vi.fn().mockReturnValue("RUNNING"),
        variableStateManager: {
          createSnapshot: vi.fn().mockReturnValue({
            variables: new Map(),
          }),
          restoreFromSnapshot: vi.fn(),
        },
        getTriggerStateSnapshot: vi.fn().mockReturnValue({ triggers: [] }),
        restoreTriggerState: vi.fn(),
        getChildExecutionIds: vi.fn().mockReturnValue([]),
        state: {
          getOperationStateSnapshot: vi.fn().mockReturnValue(null),
          getErrorRecords: vi.fn().mockReturnValue([]),
          getInterruptionRecords: vi.fn().mockReturnValue([]),
          getEventRecords: vi.fn().mockReturnValue([]),
        },
      } as unknown as WorkflowExecutionEntity;

      const deps = createMockDependencies();
      deps.workflowExecutionRegistry.get = vi.fn().mockReturnValue(mockEntity);

      const options: CheckpointOptions = {
        description: "Before tool execution",
        tags: ["tool", "important"],
        nodeId: "node-1",
        toolId: "tool-1",
      };

      await CheckpointCoordinator.createCheckpoint("exec-1", deps, options);

      const createdCheckpoint = (deps.checkpointStateManager.create as any).mock.calls[0][0];
      expect(createdCheckpoint.metadata?.description).toBe("Before tool execution");
      expect(createdCheckpoint.metadata?.tags).toEqual(["tool", "important"]);
    });

    it("should work with fileCheckpointManager", async () => {
      const execution = createMockWorkflowExecution();
      const mockEntity = {
        id: execution.id,
        getExecution: vi.fn().mockReturnValue(execution),
        getWorkflowExecutionData: vi.fn().mockReturnValue(execution),
        getGraph: vi.fn().mockReturnValue(execution.graph),
        getWorkflowId: vi.fn().mockReturnValue(execution.workflowId),
        getStatus: vi.fn().mockReturnValue("RUNNING"),
        variableStateManager: {
          createSnapshot: vi.fn().mockReturnValue({
            variables: new Map(),
          }),
          restoreFromSnapshot: vi.fn(),
        },
        getTriggerStateSnapshot: vi.fn().mockReturnValue({ triggers: [] }),
        restoreTriggerState: vi.fn(),
        getChildExecutionIds: vi.fn().mockReturnValue([]),
        state: {
          getOperationStateSnapshot: vi.fn().mockReturnValue(null),
          getErrorRecords: vi.fn().mockReturnValue([]),
          getInterruptionRecords: vi.fn().mockReturnValue([]),
          getEventRecords: vi.fn().mockReturnValue([]),
        },
      } as unknown as WorkflowExecutionEntity;

      const mockFileCheckpointManager = {
        createCheckpoint: vi.fn().mockResolvedValue({ id: "file-cp-1" }),
      } as unknown as FileCheckpointManager;

      const deps = createMockDependencies({
        fileCheckpointManager: mockFileCheckpointManager,
      });
      deps.workflowExecutionRegistry.get = vi.fn().mockReturnValue(mockEntity);

      await CheckpointCoordinator.createCheckpoint("exec-1", deps);

      expect(mockFileCheckpointManager.createCheckpoint).toHaveBeenCalledWith("exec-1");
    });

    it("should not fail when fileCheckpointManager throws", async () => {
      const execution = createMockWorkflowExecution();
      const mockEntity = {
        id: execution.id,
        getExecution: vi.fn().mockReturnValue(execution),
        getWorkflowExecutionData: vi.fn().mockReturnValue(execution),
        getGraph: vi.fn().mockReturnValue(execution.graph),
        getWorkflowId: vi.fn().mockReturnValue(execution.workflowId),
        getStatus: vi.fn().mockReturnValue("RUNNING"),
        variableStateManager: {
          createSnapshot: vi.fn().mockReturnValue({
            variables: new Map(),
          }),
          restoreFromSnapshot: vi.fn(),
        },
        getTriggerStateSnapshot: vi.fn().mockReturnValue({ triggers: [] }),
        restoreTriggerState: vi.fn(),
        getChildExecutionIds: vi.fn().mockReturnValue([]),
        state: {
          getOperationStateSnapshot: vi.fn().mockReturnValue(null),
          getErrorRecords: vi.fn().mockReturnValue([]),
          getInterruptionRecords: vi.fn().mockReturnValue([]),
          getEventRecords: vi.fn().mockReturnValue([]),
        },
      } as unknown as WorkflowExecutionEntity;

      const mockFileCheckpointManager = {
        createCheckpoint: vi.fn().mockRejectedValue(new Error("File CP failed")),
      } as unknown as FileCheckpointManager;

      const deps = createMockDependencies({
        fileCheckpointManager: mockFileCheckpointManager,
      });
      deps.workflowExecutionRegistry.get = vi.fn().mockReturnValue(mockEntity);

      // Should not throw despite file checkpoint failure
      await expect(CheckpointCoordinator.createCheckpoint("exec-1", deps)).resolves.toBeDefined();
    });
  });

  describe("restoreFromCheckpoint", () => {
    it("should throw CheckpointNotFoundError when checkpoint is not found", async () => {
      const mockCheckpointState = {
        get: vi.fn().mockResolvedValue(null),
      } as unknown as CheckpointState;

      const deps = createMockDependencies({
        checkpointStateManager: mockCheckpointState,
      });

      await expect(
        CheckpointCoordinator.restoreFromCheckpoint("nonexistent-cp", deps),
      ).rejects.toThrow("Checkpoint not found");
    });

    it("should throw WorkflowNotFoundError when workflow graph is not found", async () => {
      const checkpoint = {
        id: "cp-1",
        executionId: "exec-1",
        workflowId: "wf-1",
        timestamp: 1000,
        type: "FULL" as const,
        snapshot: {
          status: "RUNNING",
          currentNodeId: "node-1",
          variables: [],
          variableState: { variables: {} },
          input: {},
          output: {},
          nodeResults: {},
          errors: [],
          conversationState: {
            messages: [],
            markMap: {
              currentBatch: 0,
              batchBoundaries: [0],
              originalIndices: [],
              boundaryToBatch: [],
            },
            tokenUsage: { totalTokens: 0, promptTokens: 0, completionTokens: 0 },
            currentRequestUsage: { totalTokens: 0, promptTokens: 0, completionTokens: 0 },
          },
        },
      };

      const mockCheckpointState = {
        get: vi.fn().mockResolvedValue(checkpoint),
      } as unknown as CheckpointState;

      const deps = createMockDependencies({
        checkpointStateManager: mockCheckpointState,
      });
      deps.workflowGraphRegistry.get = vi.fn().mockReturnValue(null);

      await expect(CheckpointCoordinator.restoreFromCheckpoint("cp-1", deps)).rejects.toThrow(
        "Processed workflow not found",
      );
    });

    it("should successfully restore from FULL checkpoint", async () => {
      const checkpoint = {
        id: "cp-1",
        executionId: "exec-1",
        workflowId: "wf-1",
        timestamp: 1000,
        type: "FULL" as const,
        snapshot: {
          status: "RUNNING",
          currentNodeId: "node-1",
          variables: [],
          variableState: { variables: {} },
          input: { text: "hello" },
          output: {},
          nodeResults: {},
          errors: [],
          conversationState: {
            messages: [],
            markMap: {
              currentBatch: 0,
              batchBoundaries: [0],
              originalIndices: [],
              boundaryToBatch: [],
            },
            tokenUsage: { totalTokens: 0, promptTokens: 0, completionTokens: 0 },
            currentRequestUsage: { totalTokens: 0, promptTokens: 0, completionTokens: 0 },
          },
        },
        metadata: { description: "Test checkpoint" },
      };

      const mockCheckpointState = {
        get: vi.fn().mockResolvedValue(checkpoint),
      } as unknown as CheckpointState;

      const mockGraph = {
        id: "graph-1",
        nodes: [{ id: "node-1", name: "Start Node", type: "START" }],
        edges: [],
        getNode: vi.fn().mockReturnValue({ id: "node-1", name: "Start Node", type: "START" }),
      };

      const deps = createMockDependencies({
        checkpointStateManager: mockCheckpointState,
      });
      deps.workflowGraphRegistry.get = vi.fn().mockReturnValue(mockGraph);

      const result = await CheckpointCoordinator.restoreFromCheckpoint("cp-1", deps);

      expect(result).toBeDefined();
      expect(result.workflowExecutionEntity).toBeDefined();
      expect(result.stateCoordinator).toBeDefined();
      expect(result.conversationManager).toBeDefined();
    });
  });

  describe("determineCheckpointType (instance method)", () => {
    it("should return FULL when delta is disabled", () => {
      const coordinator = new CheckpointCoordinator();
      const type = (coordinator as any).determineCheckpointType(5, {
        enabled: false,
        baselineInterval: 10,
        maxDeltaChainLength: 20,
      });
      expect(type).toBe("FULL");
    });

    it("should return FULL for first checkpoint", () => {
      const coordinator = new CheckpointCoordinator();
      const type = (coordinator as any).determineCheckpointType(0, {
        enabled: true,
        baselineInterval: 10,
        maxDeltaChainLength: 20,
      });
      expect(type).toBe("FULL");
    });

    it("should return FULL at baseline interval", () => {
      const coordinator = new CheckpointCoordinator();
      const type = (coordinator as any).determineCheckpointType(10, {
        enabled: true,
        baselineInterval: 10,
        maxDeltaChainLength: 20,
      });
      expect(type).toBe("FULL");
    });

    it("should return DELTA for non-baseline checkpoints", () => {
      const coordinator = new CheckpointCoordinator();
      const type = (coordinator as any).determineCheckpointType(3, {
        enabled: true,
        baselineInterval: 10,
        maxDeltaChainLength: 20,
      });
      expect(type).toBe("DELTA");
    });

    it("should return FULL when checkpointCount % baselineInterval === 0", () => {
      const coordinator = new CheckpointCoordinator();
      const type = (coordinator as any).determineCheckpointType(20, {
        enabled: true,
        baselineInterval: 5,
        maxDeltaChainLength: 10,
      });
      expect(type).toBe("FULL");
    });
  });

  describe("validateCheckpoint (instance method)", () => {
    it("should throw for missing workflowId", () => {
      const coordinator = new CheckpointCoordinator();
      expect(() =>
        (coordinator as any).validateCheckpoint({
          id: "cp-1",
          executionId: "exec-1",
          workflowId: "",
        }),
      ).toThrow("Invalid checkpoint: missing workflowId");
    });

    it("should throw for missing executionId", () => {
      const coordinator = new CheckpointCoordinator();
      expect(() =>
        (coordinator as any).validateCheckpoint({
          id: "cp-1",
          executionId: "",
          workflowId: "wf-1",
        }),
      ).toThrow("Invalid checkpoint: missing executionId");
    });

    it("should not throw for valid FULL checkpoint", () => {
      const coordinator = new CheckpointCoordinator();
      expect(() =>
        (coordinator as any).validateCheckpoint({
          id: "cp-1",
          executionId: "exec-1",
          workflowId: "wf-1",
          type: "FULL",
          snapshot: { status: "RUNNING" },
        }),
      ).not.toThrow();
    });

    it("should not throw for valid DELTA checkpoint", () => {
      const coordinator = new CheckpointCoordinator();
      expect(() =>
        (coordinator as any).validateCheckpoint({
          id: "cp-1",
          executionId: "exec-1",
          workflowId: "wf-1",
          type: "DELTA",
          delta: {},
          baseCheckpointId: "base-1",
          previousCheckpointId: "prev-1",
        }),
      ).not.toThrow();
    });
  });

  describe("findChildCheckpoint (instance method)", () => {
    it("should return undefined when no checkpoints found", async () => {
      const coordinator = new CheckpointCoordinator();
      const mockCheckpointState = {
        list: vi.fn().mockResolvedValue([]),
      } as unknown as CheckpointState;

      const result = await (coordinator as any)._findChildCheckpoint(
        "child-exec-1",
        mockCheckpointState,
      );

      expect(result).toBeUndefined();
    });

    it("should return latest checkpoint ID", async () => {
      const coordinator = new CheckpointCoordinator();
      const mockCheckpointState = {
        list: vi.fn().mockResolvedValue(["cp-old", "cp-latest"]),
      } as unknown as CheckpointState;

      const result = await (coordinator as any)._findChildCheckpoint(
        "child-exec-1",
        mockCheckpointState,
      );

      expect(result).toBe("cp-latest");
    });
  });
});
