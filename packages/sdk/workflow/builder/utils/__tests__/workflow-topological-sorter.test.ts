/**
 * Workflow Topological Sorter Tests
 * Tests for the topologicalSort function
 */

import { describe, it, expect } from "vitest";
import { topologicalSort } from "../workflow-topological-sorter.js";
import { WorkflowGraphStructureImpl } from "../../../entities/workflow-graph-structure.js";
import type { WorkflowNode, WorkflowEdge } from "@wf-agent/types";

// Helper function to create a simple node
function createNode(id: string, type: string = "TASK"): WorkflowNode {
  return {
    id,
    type,
    config: {},
    workflowId: "test-workflow",
    outgoingEdgeIds: [],
    incomingEdgeIds: [],
    createdAt: Date.now(),
    updatedAt: Date.now(),
  } as WorkflowNode;
}

// Helper function to create a simple edge
function createEdge(id: string, sourceNodeId: string, targetNodeId: string): WorkflowEdge {
  return {
    id,
    sourceNodeId,
    targetNodeId,
    type: "DEFAULT",
    condition: undefined,
    createdAt: Date.now(),
    updatedAt: Date.now(),
  } as WorkflowEdge;
}

describe("topologicalSort", () => {
  it("should sort a simple linear graph", () => {
    const graph = new WorkflowGraphStructure();
    graph.addNode(createNode("a"));
    graph.addNode(createNode("b"));
    graph.addNode(createNode("c"));
    graph.addEdge(createEdge("e1", "a", "b"));
    graph.addEdge(createEdge("e2", "b", "c"));

    const result = topologicalSort(graph);

    expect(result.success).toBe(true);
    expect(result.sortedNodes.length).toBe(3);
    expect(result.sortedNodes).toContain("a");
    expect(result.sortedNodes).toContain("b");
    expect(result.sortedNodes).toContain("c");

    // Check ordering: a should come before b, b before c
    const aIndex = result.sortedNodes.indexOf("a");
    const bIndex = result.sortedNodes.indexOf("b");
    const cIndex = result.sortedNodes.indexOf("c");
    expect(aIndex).toBeLessThan(bIndex);
    expect(bIndex).toBeLessThan(cIndex);
  });

  it("should handle empty graph", () => {
    const graph = new WorkflowGraphStructure();

    const result = topologicalSort(graph);

    expect(result.success).toBe(true);
    expect(result.sortedNodes.length).toBe(0);
    expect(result.cycleNodes).toBeUndefined();
  });

  it("should handle single node graph", () => {
    const graph = new WorkflowGraphStructure();
    graph.addNode(createNode("a"));

    const result = topologicalSort(graph);

    expect(result.success).toBe(true);
    expect(result.sortedNodes.length).toBe(1);
    expect(result.sortedNodes[0]).toBe("a");
  });

  it("should detect cycle in graph with cycle", () => {
    const graph = new WorkflowGraphStructure();
    graph.addNode(createNode("a"));
    graph.addNode(createNode("b"));
    graph.addNode(createNode("c"));
    graph.addEdge(createEdge("e1", "a", "b"));
    graph.addEdge(createEdge("e2", "b", "c"));
    graph.addEdge(createEdge("e3", "c", "a")); // Creates cycle

    const result = topologicalSort(graph);

    expect(result.success).toBe(false);
    expect(result.sortedNodes.length).toBeLessThan(3);
    expect(result.cycleNodes).toBeDefined();
    expect(result.cycleNodes!.length).toBeGreaterThan(0);
  });

  it("should sort diamond-shaped DAG correctly", () => {
    const graph = new WorkflowGraphStructure();
    graph.addNode(createNode("a"));
    graph.addNode(createNode("b"));
    graph.addNode(createNode("c"));
    graph.addNode(createNode("d"));
    graph.addEdge(createEdge("e1", "a", "b"));
    graph.addEdge(createEdge("e2", "a", "c"));
    graph.addEdge(createEdge("e3", "b", "d"));
    graph.addEdge(createEdge("e4", "c", "d"));

    const result = topologicalSort(graph);

    expect(result.success).toBe(true);
    expect(result.sortedNodes.length).toBe(4);

    // 'a' should come before 'b' and 'c'
    const aIndex = result.sortedNodes.indexOf("a");
    const bIndex = result.sortedNodes.indexOf("b");
    const cIndex = result.sortedNodes.indexOf("c");
    const dIndex = result.sortedNodes.indexOf("d");

    expect(aIndex).toBeLessThan(bIndex);
    expect(aIndex).toBeLessThan(cIndex);
    expect(bIndex).toBeLessThan(dIndex);
    expect(cIndex).toBeLessThan(dIndex);
  });

  it("should handle graph with multiple disconnected components", () => {
    const graph = new WorkflowGraphStructure();
    // Component 1: a -> b
    graph.addNode(createNode("a"));
    graph.addNode(createNode("b"));
    graph.addEdge(createEdge("e1", "a", "b"));

    // Component 2: c -> d
    graph.addNode(createNode("c"));
    graph.addNode(createNode("d"));
    graph.addEdge(createEdge("e2", "c", "d"));

    const result = topologicalSort(graph);

    expect(result.success).toBe(true);
    expect(result.sortedNodes.length).toBe(4);

    // Within each component, order should be preserved
    const aIndex = result.sortedNodes.indexOf("a");
    const bIndex = result.sortedNodes.indexOf("b");
    const cIndex = result.sortedNodes.indexOf("c");
    const dIndex = result.sortedNodes.indexOf("d");

    expect(aIndex).toBeLessThan(bIndex);
    expect(cIndex).toBeLessThan(dIndex);
  });

  it("should handle complex DAG with multiple paths", () => {
    const graph = new WorkflowGraphStructure();
    graph.addNode(createNode("start"));
    graph.addNode(createNode("a"));
    graph.addNode(createNode("b"));
    graph.addNode(createNode("c"));
    graph.addNode(createNode("end"));

    graph.addEdge(createEdge("e1", "start", "a"));
    graph.addEdge(createEdge("e2", "start", "b"));
    graph.addEdge(createEdge("e3", "a", "c"));
    graph.addEdge(createEdge("e4", "b", "c"));
    graph.addEdge(createEdge("e5", "c", "end"));

    const result = topologicalSort(graph);

    expect(result.success).toBe(true);
    expect(result.sortedNodes.length).toBe(5);

    const startIndex = result.sortedNodes.indexOf("start");
    const endIndex = result.sortedNodes.indexOf("end");

    expect(startIndex).toBe(0); // start should be first (in-degree 0)
    expect(endIndex).toBe(4); // end should be last
  });

  it("should detect self-loop", () => {
    const graph = new WorkflowGraphStructure();
    graph.addNode(createNode("a"));
    graph.addEdge(createEdge("e1", "a", "a")); // Self-loop

    const result = topologicalSort(graph);

    expect(result.success).toBe(false);
    expect(result.cycleNodes).toBeDefined();
    expect(result.cycleNodes).toContain("a");
  });

  it("should handle graph with only one edge", () => {
    const graph = new WorkflowGraphStructure();
    graph.addNode(createNode("a"));
    graph.addNode(createNode("b"));
    graph.addEdge(createEdge("e1", "a", "b"));

    const result = topologicalSort(graph);

    expect(result.success).toBe(true);
    expect(result.sortedNodes.length).toBe(2);

    const aIndex = result.sortedNodes.indexOf("a");
    const bIndex = result.sortedNodes.indexOf("b");
    expect(aIndex).toBeLessThan(bIndex);
  });
});
