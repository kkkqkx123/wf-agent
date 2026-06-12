/**
 * Tests for WorkflowGraphRegistry
 */

import { describe, it, expect, beforeEach } from "vitest";
import { WorkflowGraphRegistry } from "../workflow-graph-registry.js";
import type { WorkflowGraph } from "../../types/graph/preprocessed-graph.js";

function createMockGraph(
  workflowId: string,
  overrides: Partial<WorkflowGraph> = {},
): WorkflowGraph {
  return {
    workflowId,
    workflowVersion: "1.0",
    idMapping: new Map(),
    nodeConfigs: new Map(),
    triggerConfigs: new Map(),
    subgraphRelationships: [],
    graphAnalysis: {
      entryNodeId: "node-1",
      exitNodeIds: ["node-2"],
      hasCycles: false,
      forkJoinPairs: [],
      loopPairs: [],
    },
    validationResult: { valid: true, errors: [] },
    topologicalOrder: ["node-1", "node-2"],
    subgraphMergeLogs: [],
    processedAt: Date.now(),
    hasSubgraphs: false,
    subworkflowIds: new Set(),
    nodes: [],
    edges: [],
    ...overrides,
  } as unknown as WorkflowGraph;
}

describe("WorkflowGraphRegistry", () => {
  let registry: WorkflowGraphRegistry;

  beforeEach(() => {
    registry = new WorkflowGraphRegistry();
  });

  describe("constructor", () => {
    it("should create an empty registry", () => {
      expect(registry.size()).toBe(0);
      expect(registry.getAllWorkflowIds()).toEqual([]);
    });
  });

  describe("register", () => {
    it("should register a workflow graph", () => {
      const graph = createMockGraph("workflow-1");
      registry.register(graph);
      expect(registry.size()).toBe(1);
      expect(registry.has("workflow-1")).toBe(true);
    });

    it("should overwrite existing graph with same id", () => {
      const graph1 = createMockGraph("workflow-1", { workflowVersion: "1.0" });
      const graph2 = createMockGraph("workflow-1", { workflowVersion: "2.0" });
      registry.register(graph1);
      registry.register(graph2);
      expect(registry.size()).toBe(1);
      expect(registry.get("workflow-1")?.workflowVersion).toBe("2.0");
    });
  });

  describe("get", () => {
    it("should get a registered graph", () => {
      const graph = createMockGraph("workflow-1");
      registry.register(graph);
      expect(registry.get("workflow-1")).toBe(graph);
    });

    it("should return undefined for non-existent graph", () => {
      expect(registry.get("non-existent")).toBeUndefined();
    });
  });

  describe("has", () => {
    it("should return true for existing graph", () => {
      registry.register(createMockGraph("workflow-1"));
      expect(registry.has("workflow-1")).toBe(true);
    });

    it("should return false for non-existent graph", () => {
      expect(registry.has("non-existent")).toBe(false);
    });
  });

  describe("unregister", () => {
    it("should remove a registered graph", () => {
      registry.register(createMockGraph("workflow-1"));
      registry.unregister("workflow-1");
      expect(registry.has("workflow-1")).toBe(false);
      expect(registry.size()).toBe(0);
    });

    it("should not throw when unregistering non-existent graph", () => {
      expect(() => registry.unregister("non-existent")).not.toThrow();
    });
  });

  describe("clear", () => {
    it("should clear all registered graphs", () => {
      registry.register(createMockGraph("workflow-1"));
      registry.register(createMockGraph("workflow-2"));
      registry.clear();
      expect(registry.size()).toBe(0);
      expect(registry.getAllWorkflowIds()).toEqual([]);
    });
  });

  describe("getAllWorkflowIds", () => {
    it("should return all registered workflow IDs", () => {
      registry.register(createMockGraph("workflow-1"));
      registry.register(createMockGraph("workflow-2"));
      const ids = registry.getAllWorkflowIds();
      expect(ids).toContain("workflow-1");
      expect(ids).toContain("workflow-2");
      expect(ids.length).toBe(2);
    });
  });

  describe("size", () => {
    it("should return correct count", () => {
      expect(registry.size()).toBe(0);
      registry.register(createMockGraph("workflow-1"));
      expect(registry.size()).toBe(1);
      registry.register(createMockGraph("workflow-2"));
      expect(registry.size()).toBe(2);
    });
  });

  describe("registerBatch", () => {
    it("should register multiple graphs", () => {
      const graphs = [
        createMockGraph("workflow-1"),
        createMockGraph("workflow-2"),
        createMockGraph("workflow-3"),
      ];
      registry.registerBatch(graphs);
      expect(registry.size()).toBe(3);
      expect(registry.has("workflow-1")).toBe(true);
      expect(registry.has("workflow-2")).toBe(true);
      expect(registry.has("workflow-3")).toBe(true);
    });

    it("should handle empty array", () => {
      registry.registerBatch([]);
      expect(registry.size()).toBe(0);
    });
  });

  describe("unregisterBatch", () => {
    it("should remove multiple graphs", () => {
      registry.register(createMockGraph("workflow-1"));
      registry.register(createMockGraph("workflow-2"));
      registry.register(createMockGraph("workflow-3"));
      registry.unregisterBatch(["workflow-1", "workflow-3"]);
      expect(registry.size()).toBe(1);
      expect(registry.has("workflow-2")).toBe(true);
    });

    it("should handle empty array", () => {
      registry.register(createMockGraph("workflow-1"));
      registry.unregisterBatch([]);
      expect(registry.size()).toBe(1);
    });
  });
});
