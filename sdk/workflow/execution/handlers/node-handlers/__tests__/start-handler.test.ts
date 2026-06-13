import { describe, it, expect, vi, beforeEach } from "vitest";
import { startHandler } from "../start-handler.js";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import type { RuntimeNode } from "@wf-agent/types";

const mockEntity = {
  getStatus: vi.fn(),
  setCurrentNodeId: vi.fn(),
  state: { start: vi.fn() },
  getExecution: vi.fn(),
  getNodeResults: vi.fn().mockReturnValue([]),
  addNodeResult: vi.fn(),
  getInput: vi.fn().mockReturnValue({}),
} as unknown as WorkflowExecutionEntity;

const mockExecution = {
  variables: [],
  errors: [],
  input: {},
};

const mockNode: RuntimeNode = {
  id: "start-node-1",
  type: "START",
  config: {},
} as RuntimeNode;

beforeEach(() => {
  vi.clearAllMocks();
  mockExecution.variables = [];
  mockExecution.errors = [];
  mockExecution.input = {};
  (mockEntity.getExecution as any).mockReturnValue(mockExecution);
  (mockEntity.getStatus as any).mockReturnValue("CREATED");
  (mockEntity.getNodeResults as any).mockReturnValue([]);
});

describe("startHandler", () => {
  it("should initialize workflow and return start message when status is CREATED", async () => {
    const result = await startHandler(mockEntity, mockNode);

    expect(mockEntity.state.start).toHaveBeenCalled();
    expect(mockEntity.setCurrentNodeId).toHaveBeenCalledWith("start-node-1");
    expect(mockEntity.addNodeResult).toHaveBeenCalledWith(
      expect.objectContaining({ nodeId: "start-node-1", status: "COMPLETED" }),
    );
    expect(result).toEqual({
      message: "Workflow started",
      input: {},
    });
  });

  it("should initialize workflow when status is RUNNING and node not executed", async () => {
    (mockEntity.getStatus as any).mockReturnValue("RUNNING");

    const result = await startHandler(mockEntity, mockNode);

    expect(mockEntity.state.start).toHaveBeenCalled();
    expect(result).toEqual({ message: "Workflow started", input: {} });
  });

  it("should return SKIPPED when node already executed", async () => {
    (mockEntity.getNodeResults as any).mockReturnValue([{ nodeId: "start-node-1" }]);

    const result = await startHandler(mockEntity, mockNode);

    expect((result as any).status).toBe("SKIPPED");
  });

  it("should initialize variables array if not present", async () => {
    mockExecution.variables = undefined as any;

    await startHandler(mockEntity, mockNode);

    expect(mockExecution.variables).toEqual([]);
  });

  it("should initialize errors array if not present", async () => {
    mockExecution.errors = undefined as any;

    await startHandler(mockEntity, mockNode);

    expect(mockExecution.errors).toEqual([]);
  });

  it("should initialize input object if not present", async () => {
    mockExecution.input = undefined as any;

    await startHandler(mockEntity, mockNode);

    expect(mockExecution.input).toEqual({});
  });
});
