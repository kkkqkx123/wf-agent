/**
 * WorkflowExecutor Unit Tests
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { WorkflowExecutor } from "../workflow-executor.js";
import type { WorkflowExecutorDependencies } from "../workflow-executor.js";
import type { WorkflowExecutionResult } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { WorkflowGraphRegistry } from "../../../stores/workflow-graph-registry.js";

const createMockWorkflowGraphRegistry = () => ({
  get: vi.fn(),
});

const createMockWorkflowExecutionCoordinator = () => ({
  execute: vi.fn(),
});

const createMockWorkflowExecutionCoordinatorFactory = (
  mockCoordinator: ReturnType<typeof createMockWorkflowExecutionCoordinator>,
) => ({
  create: vi.fn().mockReturnValue(mockCoordinator),
});

const createMockWorkflowExecutionEntity = () => ({
  id: "exec-1",
  getWorkflowId: vi.fn().mockReturnValue("wf-1"),
});

const makeResult = (): WorkflowExecutionResult => ({
  executionId: "exec-1",
  output: {},
  executionTime: 100,
  nodeResults: [],
  metadata: {
    status: "COMPLETED",
    startTime: 1000,
    endTime: 1100,
    executionTime: 100,
    nodeCount: 0,
    errorCount: 0,
  },
});

describe("WorkflowExecutor", () => {
  let executor: WorkflowExecutor;
  let mockRegistry: ReturnType<typeof createMockWorkflowGraphRegistry>;
  let mockCoordinator: ReturnType<typeof createMockWorkflowExecutionCoordinator>;
  let mockFactory: ReturnType<typeof createMockWorkflowExecutionCoordinatorFactory>;
  let mockEntity: ReturnType<typeof createMockWorkflowExecutionEntity>;

  beforeEach(() => {
    mockCoordinator = createMockWorkflowExecutionCoordinator();
    mockFactory = createMockWorkflowExecutionCoordinatorFactory(mockCoordinator);
    mockRegistry = createMockWorkflowGraphRegistry();
    mockEntity = createMockWorkflowExecutionEntity();

    const deps: WorkflowExecutorDependencies = {
      workflowGraphRegistry: mockRegistry as unknown as WorkflowGraphRegistry,
      workflowExecutionCoordinatorFactory: mockFactory,
    };

    executor = new WorkflowExecutor(deps);

    vi.clearAllMocks();
  });

  describe("executeWorkflow", () => {
    it("should execute a workflow successfully when graph exists", async () => {
      const expectedResult = makeResult();
      mockRegistry.get.mockReturnValue({});
      mockCoordinator.execute.mockResolvedValue(expectedResult);

      const result = await executor.executeWorkflow(
        mockEntity as unknown as WorkflowExecutionEntity,
      );

      expect(result).toBe(expectedResult);
      expect(mockEntity.getWorkflowId).toHaveBeenCalledTimes(1);
      expect(mockRegistry.get).toHaveBeenCalledWith("wf-1");
      expect(mockFactory.create).toHaveBeenCalledWith(mockEntity);
      expect(mockCoordinator.execute).toHaveBeenCalledTimes(1);
    });

    it("should throw when workflow graph is not found", async () => {
      mockRegistry.get.mockReturnValue(undefined);

      await expect(
        executor.executeWorkflow(mockEntity as unknown as WorkflowExecutionEntity),
      ).rejects.toThrow("Workflow graph not found for workflow: wf-1");

      expect(mockEntity.getWorkflowId).toHaveBeenCalledTimes(1);
      expect(mockRegistry.get).toHaveBeenCalledWith("wf-1");
      expect(mockFactory.create).not.toHaveBeenCalled();
      expect(mockCoordinator.execute).not.toHaveBeenCalled();
    });

    it("should propagate coordinator execution error", async () => {
      mockRegistry.get.mockReturnValue({});
      const testError = new Error("Coordinator failed");
      mockCoordinator.execute.mockRejectedValue(testError);

      await expect(
        executor.executeWorkflow(mockEntity as unknown as WorkflowExecutionEntity),
      ).rejects.toThrow("Coordinator failed");

      expect(mockRegistry.get).toHaveBeenCalledWith("wf-1");
      expect(mockFactory.create).toHaveBeenCalledWith(mockEntity);
      expect(mockCoordinator.execute).toHaveBeenCalledTimes(1);
    });

    it("should handle empty nodeResults in logging", async () => {
      const result = makeResult();
      result.nodeResults = [];
      mockRegistry.get.mockReturnValue({});
      mockCoordinator.execute.mockResolvedValue(result);

      const actual = await executor.executeWorkflow(
        mockEntity as unknown as WorkflowExecutionEntity,
      );

      expect(actual.nodeResults).toEqual([]);
      expect(actual.metadata.nodeCount).toBe(0);
    });
  });
});