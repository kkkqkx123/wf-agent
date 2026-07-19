/**
 * Workflow Traversal Tests
 * Tests for getReachableNodes and getNodesReachingTo functions
 */

import { describe, it, expect } from "vitest";
import { getReachableNodes, getNodesReachingTo } from "../workflow-traversal.js";
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

describe("getReachableNodes", () => {
  it("should get all reachable nodes from start node", () => {
    const graph = new WorkflowGraphStructureImpl();
    graph.addNode(createNode("a"));
    graph.addNode(createNode("b"));
    graph.addNode(createNode("c"));
    graph.addNode(createNode("d"));
    graph.addEdge(createEdge("e1", "a", "b"));
    graph.addEdge(createEdge("e2", "a", "c"));
    graph.addEdge(createEdge("e3", "b", "d"));

    const reachable = getReachableNodes(graph, "a");

    expect(reachable.has("a")).toBe(true);
    expect(reachable.has("b")).toBe(true);
    expect(reachable.has("c")).toBe(true);
    expect(reachable.has("d")).toBe(true);
    expect(reachable.size).toBe(4);
  });

  it("should return empty set for non-existent node", () => {
    const graph = new WorkflowGraphStructureImpl();
    graph.addNode(createNode("a"));

    const reachable = getReachableNodes(graph, "nonexistent");

    expect(reachable.size).toBe(0);
  });

  it("should handle disconnected nodes", () => {
    const graph = new WorkflowGraphStructureImpl();
    graph.addNode(createNode("a"));
    graph.addNode(createNode("b"));
    graph.addNode(createNode("c"));
    graph.addEdge(createEdge("e1", "a", "b"));
    // 'c' is disconnected

    const reachable = getReachableNodes(graph, "a");

    expect(reachable.has("a")).toBe(true);
    expect(reachable.has("b")).toBe(true);
    expect(reachable.has("c")).toBe(false);
    expect(reachable.size).toBe(2);
  });
});

describe("getNodesReachingTo", () => {
  it("should get all nodes that can reach the target node", () => {
    const graph = new WorkflowGraphStructureImpl();
    graph.addNode(createNode("a"));
    graph.addNode(createNode("b"));
    graph.addNode(createNode("c"));
    graph.addNode(createNode("d"));
    graph.addEdge(createEdge("e1", "a", "b"));
    graph.addEdge(createEdge("e2", "b", "d"));
    graph.addEdge(createEdge("e3", "c", "d"));

    const reaching = getNodesReachingTo(graph, "d");

    expect(reaching.has("d")).toBe(true);
    expect(reaching.has("b")).toBe(true);
    expect(reaching.has("c")).toBe(true);
    expect(reaching.has("a")).toBe(true);
    expect(reaching.size).toBe(4);
  });

  it("should return empty set for non-existent target node", () => {
    const graph = new WorkflowGraphStructureImpl();
    graph.addNode(createNode("a"));

    const reaching = getNodesReachingTo(graph, "nonexistent");

    expect(reaching.size).toBe(0);
  });

  it("should handle node with no incoming edges", () => {
    const graph = new WorkflowGraphStructureImpl();
    graph.addNode(createNode("a"));
    graph.addNode(createNode("b"));
    graph.addEdge(createEdge("e1", "a", "b"));

    const reaching = getNodesReachingTo(graph, "a");

    expect(reaching.has("a")).toBe(true);
    expect(reaching.has("b")).toBe(false);
    expect(reaching.size).toBe(1);
  });

  it("should handle complex graph with multiple paths", () => {
    const graph = new WorkflowGraphStructureImpl();
    graph.addNode(createNode("a"));
    graph.addNode(createNode("b"));
    graph.addNode(createNode("c"));
    graph.addNode(createNode("d"));
    graph.addNode(createNode("e"));
    graph.addEdge(createEdge("e1", "a", "b"));
    graph.addEdge(createEdge("e2", "a", "c"));
    graph.addEdge(createEdge("e3", "b", "d"));
    graph.addEdge(createEdge("e4", "c", "d"));
    graph.addEdge(createEdge("e5", "d", "e"));

    const reaching = getNodesReachingTo(graph, "e");

    expect(reaching.has("e")).toBe(true);
    expect(reaching.has("d")).toBe(true);
    expect(reaching.has("b")).toBe(true);
    expect(reaching.has("c")).toBe(true);
    expect(reaching.has("a")).toBe(true);
    expect(reaching.size).toBe(5);
  });
});
