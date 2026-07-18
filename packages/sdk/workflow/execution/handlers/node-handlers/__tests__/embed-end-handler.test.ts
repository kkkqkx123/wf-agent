import { describe, it, expect, vi, beforeEach } from "vitest";
import { embedEndHandler } from "../embed-end-handler.js";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import type { RuntimeNode } from "@wf-agent/types";

const mockEntity = {
  getStatus: vi.fn(),
  getNodeResults: vi.fn().mockReturnValue([]),
  setCurrentNodeId: vi.fn(),
  addNodeResult: vi.fn(),
} as unknown as WorkflowExecutionEntity;

const mockNode: RuntimeNode = {
  id: "embed-end-1",
  type: "EMBED_END",
  config: {},
} as RuntimeNode;

beforeEach(() => {
  vi.clearAllMocks();
  (mockEntity.getStatus as any).mockReturnValue("RUNNING");
  (mockEntity.getNodeResults as any).mockReturnValue([]);
});

describe("embedEndHandler", () => {
  it("should pass through and record execution when RUNNING", async () => {
    const result = await embedEndHandler(mockEntity, mockNode);

    expect(mockEntity.setCurrentNodeId).toHaveBeenCalledWith("embed-end-1");
    expect(result).toEqual({
      nodeId: "embed-end-1",
      nodeType: "EMBED_END",
      status: "COMPLETED",
      message: "Embedded graph boundary (end) - pass through",
    });
  });

  it("should return SKIPPED when node already executed", async () => {
    (mockEntity.getNodeResults as any).mockReturnValue([{ nodeId: "embed-end-1" }]);

    const result = await embedEndHandler(mockEntity, mockNode);

    expect((result as any).status).toBe("SKIPPED");
  });
});
