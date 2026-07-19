/**
 * Workflow Cycle Detector Tests
 * Tests for the detectCycles function in workflow-cycle-detector.ts
 */

import { describe, it, expect } from "vitest";
import { detectCycles } from "../workflow-cycle-detector.js";
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

describe("detectCycles", () => {
  it("should detect no cycle in a simple linear graph", () => {
    const graph = new WorkflowGraphStructureImpl();

    // Create nodes: START -> A -> B -> END
    graph.addNode(createNode("start", "START"));
    graph.addNode(createNode("a", "TASK"));
    graph.addNode(createNode("b", "TASK"));
    graph.addNode(createNode("end", "END"));

    // Create edges
    graph.addEdge(createEdge("e1", "start", "a"));
    graph.addEdge(createEdge("e2", "a", "b"));
    graph.addEdge(createEdge("e3", "b", "end"));

    graph.startNodeId = "start";
    graph.endNodeIds.add("end");

    const result = detectCycles(graph);

    expect(result.hasCycle).toBe(false);
    expect(result.cycleNodes).toBeUndefined();
    expect(result.cycleEdges).toBeUndefined();
  });

  it("should detect a simple cycle", () => {
    const graph = new WorkflowGraphStructureImpl();

    // Create nodes: A -> B -> C -> A (cycle)
    graph.addNode(createNode("a", "TASK"));
    graph.addNode(createNode("b", "TASK"));
    graph.addNode(createNode("c", "TASK"));

    // Create edges forming a cycle
    graph.addEdge(createEdge("e1", "a", "b"));
    graph.addEdge(createEdge("e2", "b", "c"));
    graph.addEdge(createEdge("e3", "c", "a"));

    const result = detectCycles(graph);

    expect(result.hasCycle).toBe(true);
    expect(result.cycleNodes).toBeDefined();
    expect(result.cycleNodes!.length).toBeGreaterThan(0);
    expect(result.cycleEdges).toBeDefined();
    expect(result.cycleEdges!.length).toBeGreaterThan(0);
  });

  it("should detect cycle in a graph with multiple components", () => {
    const graph = new WorkflowGraphStructureImpl();

    // Component 1: Linear (no cycle)
    graph.addNode(createNode("a", "TASK"));
    graph.addNode(createNode("b", "TASK"));
    graph.addEdge(createEdge("e1", "a", "b"));

    // Component 2: Cycle
    graph.addNode(createNode("c", "TASK"));
    graph.addNode(createNode("d", "TASK"));
    graph.addEdge(createEdge("e2", "c", "d"));
    graph.addEdge(createEdge("e3", "d", "c"));

    const result = detectCycles(graph);

    expect(result.hasCycle).toBe(true);
    expect(result.cycleNodes).toBeDefined();
    expect(result.cycleNodes).toContain("c");
    expect(result.cycleNodes).toContain("d");
  });

  it("should handle empty graph", () => {
    const graph = new WorkflowGraphStructureImpl();

    const result = detectCycles(graph);

    expect(result.hasCycle).toBe(false);
    expect(result.cycleNodes).toBeUndefined();
    expect(result.cycleEdges).toBeUndefined();
  });

  it("should handle single node graph", () => {
    const graph = new WorkflowGraphStructureImpl();
    graph.addNode(createNode("a", "TASK"));

    const result = detectCycles(graph);

    expect(result.hasCycle).toBe(false);
    expect(result.cycleNodes).toBeUndefined();
    expect(result.cycleEdges).toBeUndefined();
  });

  it("should detect self-loop", () => {
    const graph = new WorkflowGraphStructureImpl();
    graph.addNode(createNode("a", "TASK"));
    graph.addEdge(createEdge("e1", "a", "a"));

    const result = detectCycles(graph);

    expect(result.hasCycle).toBe(true);
    expect(result.cycleNodes).toBeDefined();
    expect(result.cycleNodes).toContain("a");
  });

  it("should detect complex cycle with multiple nodes", () => {
    const graph = new WorkflowGraphStructureImpl();

    // Create a diamond shape with a cycle
    graph.addNode(createNode("a", "TASK"));
    graph.addNode(createNode("b", "TASK"));
    graph.addNode(createNode("c", "TASK"));
    graph.addNode(createNode("d", "TASK"));

    // a -> b -> d
    // a -> c -> d
    // d -> a (cycle)
    graph.addEdge(createEdge("e1", "a", "b"));
    graph.addEdge(createEdge("e2", "b", "d"));
    graph.addEdge(createEdge("e3", "a", "c"));
    graph.addEdge(createEdge("e4", "c", "d"));
    graph.addEdge(createEdge("e5", "d", "a"));

    const result = detectCycles(graph);

    expect(result.hasCycle).toBe(true);
    expect(result.cycleNodes).toBeDefined();
    expect(result.cycleNodes!.length).toBeGreaterThanOrEqual(2);
  });

  it("should not detect cycle in DAG (Directed Acyclic Graph)", () => {
    const graph = new WorkflowGraphStructureImpl();

    // Create a DAG: multiple paths but no cycles
    graph.addNode(createNode("start", "START"));
    graph.addNode(createNode("a", "TASK"));
    graph.addNode(createNode("b", "TASK"));
    graph.addNode(createNode("c", "TASK"));
    graph.addNode(createNode("end", "END"));

    graph.addEdge(createEdge("e1", "start", "a"));
    graph.addEdge(createEdge("e2", "start", "b"));
    graph.addEdge(createEdge("e3", "a", "c"));
    graph.addEdge(createEdge("e4", "b", "c"));
    graph.addEdge(createEdge("e5", "c", "end"));

    graph.startNodeId = "start";
    graph.endNodeIds.add("end");

    const result = detectCycles(graph);

    expect(result.hasCycle).toBe(false);
    expect(result.cycleNodes).toBeUndefined();
    expect(result.cycleEdges).toBeUndefined();
  });
});
