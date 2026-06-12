import { describe, it, expect, vi, beforeEach } from "vitest";
import { endHandler } from "../end-handler.js";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import type { RuntimeNode } from "@wf-agent/types";

const mockEntity = {
  getStatus: vi.fn(),
  getNodeResults: vi.fn().mockReturnValue([]),
  getOutput: vi.fn().mockReturnValue({ result: "done" }),
  setOutput: vi.fn(),
  addNodeResult: vi.fn(),
} as unknown as WorkflowExecutionEntity;

const mockNode: RuntimeNode = {
  id: "end-node-1",
  type: "END",
  config: {},
} as RuntimeNode;

beforeEach(() => {
  vi.clearAllMocks();
  (mockEntity.getStatus as any).mockReturnValue("RUNNING");
  (mockEntity.getNodeResults as any).mockReturnValue([]);
});

describe("endHandler", () => {
  it("should collect output and return result when status is RUNNING", async () => {
    const result = await endHandler(mockEntity, mockNode);

    expect(mockEntity.setOutput).toHaveBeenCalledWith({ result: "done" });
    expect(mockEntity.addNodeResult).toHaveBeenCalledWith(
      expect.objectContaining({ nodeId: "end-node-1", status: "COMPLETED" }),
    );
    expect(result).toEqual({
      nodeId: "end-node-1",
      nodeType: "END",
      status: "COMPLETED",
      output: { result: "done" },
      executionTime: expect.any(Number),
    });
  });

  it("should return SKIPPED when node already executed", async () => {
    (mockEntity.getNodeResults as any).mockReturnValue([{ nodeId: "end-node-1" }]);

    const result = await endHandler(mockEntity, mockNode);

    expect((result as any).status).toBe("SKIPPED");
  });

  it("should handle empty output", async () => {
    (mockEntity.getOutput as any).mockReturnValue(undefined);

    const result = await endHandler(mockEntity, mockNode);

    expect(result).toEqual({
      nodeId: "end-node-1",
      nodeType: "END",
      status: "COMPLETED",
      output: {},
      executionTime: expect.any(Number),
    });
  });
});
