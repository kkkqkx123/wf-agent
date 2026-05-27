/**
 * SUBGRAPH Node Handler Unit Tests
 * Tests for subgraph-handler.ts functionality
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { subgraphHandler } from "../subgraph-handler.js";
import type { GlobalContext } from "../../../../../core/global-context.js";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import type { RuntimeNode, SubgraphNodeConfig } from "@wf-agent/types";

// Mock dependencies
const mockGlobalContext = {
  container: {
    get: vi.fn(),
  },
} as unknown as GlobalContext;

const mockParentEntity = {
  id: "parent-execution-123",
  getWorkflowId: vi.fn().mockReturnValue("parent-workflow"),
  variableStateManager: {
    importVariables: vi.fn(),
    exportVariables: vi.fn(),
  },
  unregisterChild: vi.fn(),
  getHierarchyMetadata: vi.fn().mockReturnValue({ depth: 0 }),
  getRootExecutionId: vi.fn().mockReturnValue("root-execution-123"),
  getRootExecutionType: vi.fn().mockReturnValue("WORKFLOW"),
  registerChild: vi.fn(),
  getAbortSignal: vi.fn().mockReturnValue({ aborted: false }),
  getWorkflowExecutionData: vi.fn().mockReturnValue({
    id: "parent-execution-123",
    messageContextRegistry: {}, // Add registry for parent entity
  }),
  getGraph: vi.fn().mockReturnValue({
    startNodeId: "start-node",
    getNode: vi.fn().mockReturnValue({
      id: "start-node",
      type: "START",
    }),
  }),
  enterSubgraph: vi.fn(),
  getCurrentSubgraphContext: vi.fn().mockReturnValue({
    workflowId: "child-workflow",
    nodeId: "subgraph-node-1",
  }),
  exitSubgraph: vi.fn(),
} as unknown as WorkflowExecutionEntity;

const createMockSubgraphNode = (config?: Partial<SubgraphNodeConfig>): RuntimeNode => {
  const baseConfig: SubgraphNodeConfig = {
    subgraphId: "child-workflow",
    async: false,
    variableInputs: [
      { externalName: "parentVar1", internalName: "childVar1", required: true },
      {
        externalName: "parentVar2",
        internalName: "childVar2",
        required: false,
        defaultValue: "default",
      },
    ],
    variableOutputs: [{ internalName: "childResult", externalName: "parentResult" }],
    messagePassing: {
      inputs: [],
      outputs: [],
    },
  };

  const mergedConfig = { ...baseConfig, ...config };

  return {
    id: "subgraph-node-1",
    type: "SUBGRAPH",
    workflowId: "parent-workflow",
    outgoingEdgeIds: [],
    incomingEdgeIds: [],
    config: mergedConfig,
    originalNode: {
      id: "subgraph-node-1",
      type: "SUBGRAPH",
      config: mergedConfig,
    },
  } as RuntimeNode;
};

const mockSubgraphEntity = {
  id: "subgraph-execution-456",
  getWorkflowId: vi.fn().mockReturnValue("child-workflow"),
  getStatus: vi.fn().mockReturnValue("COMPLETED"),
  getOutput: vi.fn().mockReturnValue({ result: "success" }),
  variableStateManager: {
    exportVariables: vi.fn(),
  },
  getHierarchyMetadata: vi.fn().mockReturnValue({ depth: 1 }),
  getAbortSignal: vi.fn().mockReturnValue({ aborted: false }),
  getWorkflowExecutionData: vi.fn().mockReturnValue({
    id: "subgraph-execution-456",
    messageContextRegistry: {}, // Add registry to the returned object
  }),
  getGraph: vi.fn().mockReturnValue({
    startNodeId: "start-node",
    getNode: vi.fn().mockReturnValue({
      id: "start-node",
      type: "START",
    }),
  }),
  getParentContext: vi.fn().mockReturnValue({
    parentType: "WORKFLOW",
    parentId: "parent-execution-123",
  }),
} as unknown as WorkflowExecutionEntity;

const mockExecutionBuilder = {
  createChildExecution: vi.fn().mockResolvedValue({
    workflowExecutionEntity: mockSubgraphEntity,
    stateCoordinator: {},
    conversationManager: {},
  }),
};

const mockWorkflowExecutor = {
  executeWorkflow: vi.fn().mockResolvedValue({
    metadata: { status: "COMPLETED" },
    output: { result: "success" },
  }),
};

const mockMetricsRegistry = {
  getNodeCollector: vi.fn().mockReturnValue({
    recordSubgraphExecution: vi.fn(),
  }),
};

beforeEach(() => {
  vi.clearAllMocks();
  (mockGlobalContext.container.get as any).mockReturnValue(mockMetricsRegistry);
});

describe("subgraphHandler", () => {
  it("should execute subgraph successfully with variable mappings", async () => {
    // Arrange
    const node = createMockSubgraphNode();
    const context = {
      executionBuilder: mockExecutionBuilder as any,
      workflowExecutor: mockWorkflowExecutor as any,
    };

    // Act
    const result = await subgraphHandler(mockGlobalContext, mockParentEntity, node, context);

    // Assert
    expect(mockExecutionBuilder.createChildExecution).toHaveBeenCalledWith(
      mockParentEntity,
      expect.objectContaining({
        type: "SUBGRAPH",
        config: expect.objectContaining({
          subworkflowId: "child-workflow",
          nodeId: "subgraph-node-1",
          variableMapping: {
            inputs: (node.config as SubgraphNodeConfig).variableInputs,
            outputs: (node.config as SubgraphNodeConfig).variableOutputs,
          },
        }),
      }),
    );

    expect(mockWorkflowExecutor.executeWorkflow).toHaveBeenCalledWith(mockSubgraphEntity);
    expect(mockParentEntity.variableStateManager.exportVariables).toHaveBeenCalledWith(
      mockSubgraphEntity.variableStateManager,
      (node.config as SubgraphNodeConfig).variableOutputs,
    );

    expect(result).toEqual({
      executionResult: { status: "COMPLETED", output: { result: "success" } },
      duration: expect.any(Number),
    });
  });

    it("should handle subgraph without variable mappings", async () => {
      // Arrange
      const nodeWithoutMappings = createMockSubgraphNode({
        variableInputs: undefined,
        variableOutputs: undefined,
      });
      const context = {
        executionBuilder: mockExecutionBuilder as any,
        workflowExecutor: mockWorkflowExecutor as any,
      };

      // Act
      await subgraphHandler(mockGlobalContext, mockParentEntity, nodeWithoutMappings, context);

      // Assert - exportVariables should not be called when no outputs
      expect(mockParentEntity.variableStateManager.exportVariables).not.toHaveBeenCalled();
    });

    it("should throw error when subgraphId is missing", async () => {
      // Arrange
      const invalidNode = createMockSubgraphNode({
        subgraphId: undefined as any,
      });
      const context = {
        executionBuilder: mockExecutionBuilder as any,
        workflowExecutor: mockWorkflowExecutor as any,
      };

      // Act & Assert
      await expect(
        subgraphHandler(mockGlobalContext, mockParentEntity, invalidNode, context),
      ).rejects.toThrow("SUBGRAPH node 'subgraph-node-1' missing subgraphId configuration");
    });

    it("should throw error when executionBuilder is not provided", async () => {
      // Arrange
      const node = createMockSubgraphNode();
      const context = {
        workflowExecutor: mockWorkflowExecutor as any,
      };

      // Act & Assert
      await expect(
        subgraphHandler(mockGlobalContext, mockParentEntity, node, context as any),
      ).rejects.toThrow("WorkflowExecutionBuilder required for SUBGRAPH execution");
    });

    it("should throw error when workflowExecutor is not provided", async () => {
      // Arrange
      const node = createMockSubgraphNode();
      const context = {
        executionBuilder: mockExecutionBuilder as any,
      };

      // Act & Assert
      await expect(
        subgraphHandler(mockGlobalContext, mockParentEntity, node, context as any),
      ).rejects.toThrow("WorkflowExecutor required for SUBGRAPH execution");
    });

    it("should handle subgraph execution failure", async () => {
      // Arrange
      const node = createMockSubgraphNode();
      const executionError = new Error("Subgraph execution failed");
      mockWorkflowExecutor.executeWorkflow.mockRejectedValue(executionError);

      const context = {
        executionBuilder: mockExecutionBuilder as any,
        workflowExecutor: mockWorkflowExecutor as any,
      };

      // Act & Assert
      await expect(
        subgraphHandler(mockGlobalContext, mockParentEntity, node, context),
      ).rejects.toThrow(
        `Subgraph execution failed for node 'subgraph-node-1': ${executionError.message}`,
      );

      // Verify cleanup was attempted
      expect(mockParentEntity.unregisterChild).toHaveBeenCalled();
    });

    it("should handle optional output variable that is undefined", async () => {
      // Arrange
      const nodeWithOptionalOutput = createMockSubgraphNode({
        variableOutputs: [{ internalName: "optionalResult", externalName: "parentOptionalResult" }],
      });

      // Reset mock to return success (previous test set it to reject)
      mockWorkflowExecutor.executeWorkflow.mockResolvedValue({
        metadata: { status: "COMPLETED" },
        output: { result: "success" },
      });

      const context = {
        executionBuilder: mockExecutionBuilder as any,
        workflowExecutor: mockWorkflowExecutor as any,
      };

      // Act
      await subgraphHandler(mockGlobalContext, mockParentEntity, nodeWithOptionalOutput, context);

      // Assert - exportVariables should be called even if variable might be undefined
      expect(mockParentEntity.variableStateManager.exportVariables).toHaveBeenCalledWith(
        mockSubgraphEntity.variableStateManager,
        (nodeWithOptionalOutput.config as SubgraphNodeConfig).variableOutputs,
      );
    });
  });
});
