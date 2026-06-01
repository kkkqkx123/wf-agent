import { describe, it, expect, vi, beforeEach } from "vitest";
import { loopStartHandler } from "../loop-start-handler.js";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import type { RuntimeNode, LoopStartNodeConfig } from "@wf-agent/types";

const mockManager = {
  getVariable: vi.fn(),
  setVariable: vi.fn(),
  deleteVariable: vi.fn(),
  importVariables: vi.fn(),
};

const mockEntity = {
  getStatus: vi.fn(),
  getExecution: vi.fn(),
  variableStateManager: mockManager,
} as unknown as WorkflowExecutionEntity;

const mockExecution = {
  nodeResults: [],
  variables: [],
  input: {},
  output: {},
  currentNodeId: "loop-start-1",
  workflowId: "wf-1",
  variableScopes: { global: {}, execution: {} },
};

beforeEach(() => {
  vi.clearAllMocks();
  (mockEntity.getStatus as any).mockReturnValue("RUNNING");
  (mockEntity.getExecution as any).mockReturnValue(mockExecution);
  mockExecution.nodeResults = [];
  mockExecution.variables = [];
  mockManager.getVariable.mockReturnValue(undefined);
});

describe("loopStartHandler", () => {
  it("should initialize loop state and continue for array iterable", async () => {
    const config: LoopStartNodeConfig = {
      loopId: "loop-1",
      maxIterations: 10,
      dataSource: {
        iterable: [1, 2, 3],
        variableName: "item",
      },
    };
    const node = { id: "loop-start-1", type: "LOOP_START", config } as RuntimeNode;

    const result = await loopStartHandler(mockEntity, node);

    expect(result).toEqual({
      loopId: "loop-1",
      iterationCount: 1,
      maxIterations: 10,
      hasMoreIterations: true,
    });
    expect(mockManager.setVariable).toHaveBeenCalledWith("item", 1);
    expect(mockManager.setVariable).toHaveBeenCalledWith("__loop_state", expect.any(Object));
  });

  it("should resume loop state and continue from saved state", async () => {
    const savedState = {
      loopId: "loop-1",
      iterable: [1, 2, 3],
      currentIndex: 1,
      maxIterations: 10,
      iterationCount: 1,
      variableName: "item",
    };
    mockManager.getVariable.mockReturnValue(savedState);

    const config: LoopStartNodeConfig = {
      loopId: "loop-1",
      maxIterations: 10,
      dataSource: {
        iterable: [1, 2, 3],
        variableName: "item",
      },
    };
    const node = { id: "loop-start-1", type: "LOOP_START", config } as RuntimeNode;

    const result = await loopStartHandler(mockEntity, node);

    expect((result as any).hasMoreIterations).toBe(true);
    expect((result as any).iterationCount).toBe(2);
    expect((result as any).maxIterations).toBe(10);
  });

  it("should return hasMoreIterations false when maxIterations reached", async () => {
    const config: LoopStartNodeConfig = {
      loopId: "loop-1",
      maxIterations: 0,
    };
    const node = { id: "loop-start-1", type: "LOOP_START", config } as RuntimeNode;

    const result = await loopStartHandler(mockEntity, node);

    expect(result).toEqual({
      loopId: "loop-1",
      iterationCount: 0,
      maxIterations: 0,
      hasMoreIterations: false,
    });
    expect(mockManager.deleteVariable).toHaveBeenCalledWith("__loop_state");
  });

  it("should handle counting loop (no dataSource)", async () => {
    const config: LoopStartNodeConfig = {
      loopId: "loop-1",
      maxIterations: 5,
    };
    const node = { id: "loop-start-1", type: "LOOP_START", config } as RuntimeNode;

    const result = await loopStartHandler(mockEntity, node);

    expect(result).toEqual({
      loopId: "loop-1",
      iterationCount: 1,
      maxIterations: 5,
      hasMoreIterations: true,
    });
  });

  it("should handle string iterable", async () => {
    const config: LoopStartNodeConfig = {
      loopId: "loop-1",
      maxIterations: 10,
      dataSource: {
        iterable: "abc",
        variableName: "char",
      },
    };
    const node = { id: "loop-start-1", type: "LOOP_START", config } as RuntimeNode;

    const result = await loopStartHandler(mockEntity, node);

    expect((result as any).hasMoreIterations).toBe(true);
    expect((result as any).iterationCount).toBe(1);
  });
});
