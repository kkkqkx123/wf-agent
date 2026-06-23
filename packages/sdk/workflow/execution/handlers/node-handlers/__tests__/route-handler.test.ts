import { describe, it, expect, vi, beforeEach } from "vitest";
import { routeHandler } from "../route-handler.js";
import { DependencyManager } from "../../../../services/evaluation/index.js";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import type { RuntimeNode, RouteNodeConfig } from "@wf-agent/types";

const mockEntity = {
  getStatus: vi.fn(),
  getNodeResults: vi.fn().mockReturnValue([]),
  getAllVariables: vi.fn().mockReturnValue({}),
  getInput: vi.fn().mockReturnValue({}),
  getOutput: vi.fn().mockReturnValue({}),
  getCurrentNodeId: vi.fn().mockReturnValue("route-node-1"),
  getWorkflowId: vi.fn().mockReturnValue("workflow-1"),
  getDepManager: vi.fn().mockReturnValue(new DependencyManager()),
} as unknown as WorkflowExecutionEntity;

beforeEach(() => {
  vi.clearAllMocks();
  (mockEntity.getStatus as any).mockReturnValue("RUNNING");
});

describe("routeHandler", () => {
  it("should select route with highest priority matching condition", async () => {
    const routeConfig: RouteNodeConfig = {
      routes: [
        { targetNodeId: "node-b", condition: { expression: "false" }, priority: 1 },
        { targetNodeId: "node-c", condition: { expression: "true" }, priority: 2 },
      ],
    };
    const node = { id: "route-node-1", type: "ROUTE", config: routeConfig } as RuntimeNode;

    const result = await routeHandler(mockEntity, node);

    expect(result).toMatchObject({ selectedRoute: "node-c" });
  });

  it("should use defaultTargetNodeId when no routes match", async () => {
    const routeConfig: RouteNodeConfig = {
      routes: [{ targetNodeId: "node-b", condition: { expression: "false" }, priority: 1 }],
      defaultTargetNodeId: "default-node",
    };
    const node = { id: "route-node-1", type: "ROUTE", config: routeConfig } as RuntimeNode;

    const result = await routeHandler(mockEntity, node);

    expect(result).toMatchObject({ selectedRoute: "default-node" });
  });

  it("should throw ExecutionError when no route matches and no default", async () => {
    const routeConfig: RouteNodeConfig = {
      routes: [{ targetNodeId: "node-b", condition: { expression: "false" }, priority: 1 }],
    };
    const node = { id: "route-node-1", type: "ROUTE", config: routeConfig } as RuntimeNode;

    await expect(routeHandler(mockEntity, node)).rejects.toThrow();
  });
});
