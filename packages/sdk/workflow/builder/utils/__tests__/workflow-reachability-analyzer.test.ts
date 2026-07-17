/**
 * Workflow Reachability Analyzer Tests
 * Tests for the analyzeReachability function
 */

import { describe, it, expect } from "vitest";
import { analyzeReachability } from "../workflow-reachability-analyzer.js";
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

describe("analyzeReachability", () => {
  it("should analyze a simple linear graph correctly", () => {
    const graph = new WorkflowGraphStructure();
    graph.addNode(createNode("start", "START"));
    graph.addNode(createNode("a", "TASK"));
    graph.addNode(createNode("b", "TASK"));
    graph.addNode(createNode("end", "END"));

    graph.addEdge(createEdge("e1", "start", "a"));
    graph.addEdge(createEdge("e2", "a", "b"));
    graph.addEdge(createEdge("e3", "b", "end"));

    graph.startNodeId = "start";
    graph.endNodeIds.add("end");

    const result = analyzeReachability(graph);

    // All nodes should be reachable from start
    expect(result.reachableFromStart.has("start")).toBe(true);
    expect(result.reachableFromStart.has("a")).toBe(true);
    expect(result.reachableFromStart.has("b")).toBe(true);
    expect(result.reachableFromStart.has("end")).toBe(true);

    // All nodes should be able to reach end
    expect(result.reachableToEnd.has("start")).toBe(true);
    expect(result.reachableToEnd.has("a")).toBe(true);
    expect(result.reachableToEnd.has("b")).toBe(true);
    expect(result.reachableToEnd.has("end")).toBe(true);

    // No unreachable or dead-end nodes
    expect(result.unreachableNodes.size).toBe(0);
    expect(result.deadEndNodes.size).toBe(0);
  });

  it("should detect unreachable nodes", () => {
    const graph = new WorkflowGraphStructure();
    graph.addNode(createNode("start", "START"));
    graph.addNode(createNode("a", "TASK"));
    graph.addNode(createNode("orphan", "TASK")); // This node is not connected

    graph.addEdge(createEdge("e1", "start", "a"));

    graph.startNodeId = "start";
    graph.endNodeIds.add("a");

    const result = analyzeReachability(graph);

    expect(result.unreachableNodes.has("orphan")).toBe(true);
    expect(result.unreachableNodes.size).toBe(1);
  });

  it("should detect dead-end nodes", () => {
    const graph = new WorkflowGraphStructure();
    graph.addNode(createNode("start", "START"));
    graph.addNode(createNode("a", "TASK"));
    graph.addNode(createNode("dead-end", "TASK")); // Can't reach END
    graph.addNode(createNode("end", "END"));

    graph.addEdge(createEdge("e1", "start", "a"));
    graph.addEdge(createEdge("e2", "a", "dead-end"));
    graph.addEdge(createEdge("e3", "a", "end"));

    graph.startNodeId = "start";
    graph.endNodeIds.add("end");

    const result = analyzeReachability(graph);

    // 'dead-end' is reachable from start but can't reach end
    expect(result.deadEndNodes.has("dead-end")).toBe(true);
    expect(result.deadEndNodes.size).toBe(1);
  });

  it("should handle graph with no start node", () => {
    const graph = new WorkflowGraphStructure();
    graph.addNode(createNode("a", "TASK"));
    graph.addNode(createNode("b", "TASK"));
    graph.addEdge(createEdge("e1", "a", "b"));

    // No startNodeId set

    const result = analyzeReachability(graph);

    // No nodes should be reachable from start (since there's no start)
    expect(result.reachableFromStart.size).toBe(0);
  });

  it("should handle graph with multiple end nodes", () => {
    const graph = new WorkflowGraphStructure();
    graph.addNode(createNode("start", "START"));
    graph.addNode(createNode("a", "TASK"));
    graph.addNode(createNode("end1", "END"));
    graph.addNode(createNode("end2", "END"));

    graph.addEdge(createEdge("e1", "start", "a"));
    graph.addEdge(createEdge("e2", "a", "end1"));
    graph.addEdge(createEdge("e3", "a", "end2"));

    graph.startNodeId = "start";
    graph.endNodeIds.add("end1");
    graph.endNodeIds.add("end2");

    const result = analyzeReachability(graph);

    // All nodes should be able to reach at least one end
    expect(result.reachableToEnd.has("start")).toBe(true);
    expect(result.reachableToEnd.has("a")).toBe(true);
    expect(result.reachableToEnd.has("end1")).toBe(true);
    expect(result.reachableToEnd.has("end2")).toBe(true);

    expect(result.deadEndNodes.size).toBe(0);
  });

  it("should handle complex graph with multiple paths", () => {
    const graph = new WorkflowGraphStructure();
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

    const result = analyzeReachability(graph);

    expect(result.reachableFromStart.size).toBe(5);
    expect(result.reachableToEnd.size).toBe(5);
    expect(result.unreachableNodes.size).toBe(0);
    expect(result.deadEndNodes.size).toBe(0);
  });

  it("should handle empty graph", () => {
    const graph = new WorkflowGraphStructure();

    const result = analyzeReachability(graph);

    expect(result.reachableFromStart.size).toBe(0);
    expect(result.reachableToEnd.size).toBe(0);
    expect(result.unreachableNodes.size).toBe(0);
    expect(result.deadEndNodes.size).toBe(0);
  });
});
