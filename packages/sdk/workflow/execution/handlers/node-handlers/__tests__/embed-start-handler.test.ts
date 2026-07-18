import { describe, it, expect, vi, beforeEach } from "vitest";
import { embedStartHandler } from "../embed-start-handler.js";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import type { RuntimeNode } from "@wf-agent/types";

const mockEntity = {
  getStatus: vi.fn(),
  getNodeResults: vi.fn().mockReturnValue([]),
  setCurrentNodeId: vi.fn(),
  addNodeResult: vi.fn(),
} as unknown as WorkflowExecutionEntity;

const mockNode: RuntimeNode = {
  id: "embed-start-1",
  type: "EMBED_START",
  config: {},
} as RuntimeNode;

beforeEach(() => {
  vi.clearAllMocks();
  (mockEntity.getStatus as any).mockReturnValue("RUNNING");
  (mockEntity.getNodeResults as any).mockReturnValue([]);
});

describe("embedStartHandler", () => {
  it("should pass through and record execution when RUNNING", async () => {
    const result = await embedStartHandler(mockEntity, mockNode);

    expect(mockEntity.setCurrentNodeId).toHaveBeenCalledWith("embed-start-1");
    expect(result).toEqual({
      nodeId: "embed-start-1",
      nodeType: "EMBED_START",
      status: "COMPLETED",
      message: "Embedded graph boundary (start) - pass through",
    });
  });

  it("should return SKIPPED when node already executed", async () => {
    (mockEntity.getNodeResults as any).mockReturnValue([{ nodeId: "embed-start-1" }]);

    const result = await embedStartHandler(mockEntity, mockNode);

    expect((result as any).status).toBe("SKIPPED");
  });
});
