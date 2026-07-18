import { describe, it, expect, vi, beforeEach } from "vitest";
import { routeHandler } from "../route-handler.js";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import type { RuntimeNode, RouteNodeConfig } from "@wf-agent/types";

const createMockEntity = (variables: Record<string, unknown> = {}) =>
  ({
    getStatus: vi.fn(),
    getNodeResults: vi.fn().mockReturnValue([]),
    getAllVariables: vi.fn().mockReturnValue(variables),
    getInput: vi.fn().mockReturnValue({}),
    getOutput: vi.fn().mockReturnValue({}),
    getCurrentNodeId: vi.fn().mockReturnValue("route-node-1"),
    getWorkflowId: vi.fn().mockReturnValue("workflow-1"),
  }) as unknown as WorkflowExecutionEntity;

let mockEntity: WorkflowExecutionEntity;

beforeEach(() => {
  vi.clearAllMocks();
  mockEntity = createMockEntity();
  (mockEntity.getStatus as any).mockReturnValue("RUNNING");
});

describe("routeHandler", () => {
  it("should select route with highest priority matching condition", async () => {
    const routeConfig: RouteNodeConfig = {
      routes: [
        { targetNodeId: "node-b", condition: { type: "expression", expression: "false" }, priority: 1 },
        { targetNodeId: "node-c", condition: { type: "expression", expression: "true" }, priority: 2 },
      ],
    };
    const node = { id: "route-node-1", type: "ROUTE", config: routeConfig } as RuntimeNode;

    const result = await routeHandler(mockEntity, node);

    expect(result).toMatchObject({ selectedRoute: "node-c" });
  });

  it("should use defaultTargetNodeId when no routes match", async () => {
    const routeConfig: RouteNodeConfig = {
      routes: [{ targetNodeId: "node-b", condition: { type: "expression", expression: "false" }, priority: 1 }],
      defaultTargetNodeId: "default-node",
    };
    const node = { id: "route-node-1", type: "ROUTE", config: routeConfig } as RuntimeNode;

    const result = await routeHandler(mockEntity, node);

    expect(result).toMatchObject({ selectedRoute: "default-node" });
  });

  it("should throw ExecutionError when no route matches and no default", async () => {
    const routeConfig: RouteNodeConfig = {
      routes: [{ targetNodeId: "node-b", condition: { type: "expression", expression: "false" }, priority: 1 }],
    };
    const node = { id: "route-node-1", type: "ROUTE", config: routeConfig } as RuntimeNode;

    await expect(routeHandler(mockEntity, node)).rejects.toThrow();
  });

  it("should select predicate condition route when variable is empty", async () => {
    mockEntity = createMockEntity({ myVar: "" });
    const routeConfig: RouteNodeConfig = {
      routes: [
        { targetNodeId: "node-a", condition: { type: "expression", expression: "false" }, priority: 1 },
        { targetNodeId: "node-b", condition: { type: "predicate", predicateType: "isEmpty", variable: "myVar" }, priority: 2 },
      ],
    };
    const node = { id: "route-node-1", type: "ROUTE", config: routeConfig } as RuntimeNode;

    const result = await routeHandler(mockEntity, node);

    expect(result).toMatchObject({ selectedRoute: "node-b" });
  });

  it("should select schema condition route when variable matches schema", async () => {
    mockEntity = createMockEntity({ myObj: { name: "test" } });
    const routeConfig: RouteNodeConfig = {
      routes: [
        { targetNodeId: "node-a", condition: { type: "expression", expression: "false" }, priority: 1 },
        {
          targetNodeId: "node-b",
          condition: {
            type: "schema",
            variable: "myObj",
            schema: {
              type: "object",
              properties: { name: { type: "string" } },
              required: ["name"],
            },
          },
          priority: 2,
        },
      ],
    };
    const node = { id: "route-node-1", type: "ROUTE", config: routeConfig } as RuntimeNode;

    const result = await routeHandler(mockEntity, node);

    expect(result).toMatchObject({ selectedRoute: "node-b" });
  });

  it("should select script condition route when script returns true", async () => {
    mockEntity = createMockEntity({ x: 42 });
    const routeConfig: RouteNodeConfig = {
      routes: [
        { targetNodeId: "node-a", condition: { type: "expression", expression: "false" }, priority: 1 },
        { targetNodeId: "node-b", condition: { type: "script", script: "variables.x > 10" }, priority: 2 },
      ],
    };
    const node = { id: "route-node-1", type: "ROUTE", config: routeConfig } as RuntimeNode;

    const result = await routeHandler(mockEntity, node);

    expect(result).toMatchObject({ selectedRoute: "node-b" });
  });

  it("should maintain deterministic order for routes with equal priority", async () => {
    const routeConfig: RouteNodeConfig = {
      routes: [
        { targetNodeId: "node-b", condition: { type: "expression", expression: "false" }, priority: 1 },
        { targetNodeId: "node-c", condition: { type: "expression", expression: "false" }, priority: 1 },
        { targetNodeId: "node-d", condition: { type: "expression", expression: "true" }, priority: 1 },
      ],
    };
    const node = { id: "route-node-1", type: "ROUTE", config: routeConfig } as RuntimeNode;

    const result = await routeHandler(mockEntity, node);

    expect(result).toMatchObject({ selectedRoute: "node-d" });
  });
});
