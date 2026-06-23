/**
 * Integration Tests: WorkflowGraph Composition Pattern
 *
 * Tests the new composition-based design where WorkflowGraph combines
 * WorkflowGraphStructure (immutable) with WorkflowGraphMetadata (mutable).
 */

import { WorkflowGraph } from "../workflow-graph.js";
import { WorkflowGraphStructure } from "../workflow-graph-structure.js";
import { WorkflowGraphMetadata } from "../workflow-graph-metadata.js";
import type { WorkflowNode, WorkflowEdge, RuntimeNodeType } from "@wf-agent/types";

function createTestNode(partial: Partial<WorkflowNode> & { id: string; type: RuntimeNodeType }): WorkflowNode {
  return {
    workflowId: "test-workflow",
    outgoingEdgeIds: [],
    incomingEdgeIds: [],
    hooks: [],
    checkpointBeforeExecute: false,
    checkpointAfterExecute: false,
    ...partial,
  } as WorkflowNode;
}

describe("WorkflowGraph Composition Pattern", () => {
  let structure: WorkflowGraphStructure;
  let metadata: WorkflowGraphMetadata;
  let graph: WorkflowGraph;

  beforeEach(() => {
    structure = new WorkflowGraphStructure();
    metadata = new WorkflowGraphMetadata();
    graph = new WorkflowGraph(structure, metadata);
  });

  describe("Composition Structure", () => {
    test("should properly compose structure and metadata", () => {
      expect(graph.structure).toBe(structure);
      expect(graph.metadata).toBe(metadata);
    });

    test("should delegate structure properties correctly", () => {
      expect(graph.nodes).toBe(structure.nodes);
      expect(graph.edges).toBe(structure.edges);
      expect(graph.adjacencyList).toBe(structure.adjacencyList);
      expect(graph.reverseAdjacencyList).toBe(structure.reverseAdjacencyList);
    });

    test("should provide access to metadata properties", () => {
      expect(graph.workflowId).toBe(metadata.workflowId);
      expect(graph.workflowVersion).toBe(metadata.workflowVersion);
      expect(graph.idMapping).toBe(metadata.idMapping);
      expect(graph.nodeConfigs).toBe(metadata.nodeConfigs);
    });
  });

  describe("Delegated Structure Methods", () => {
    test("should delegate addNode to structure", () => {
      const node = createTestNode({ id: "node1", type: "START", config: {} });

      graph.addNode(node);

      expect(structure.hasNode("node1")).toBe(true);
      expect(graph.hasNode("node1")).toBe(true);
      expect(graph.getNode("node1")).toBe(node);
    });

    test("should delegate addEdge to structure", () => {
      const node1 = createTestNode({ id: "node1", type: "START", config: {} });
      const node2 = createTestNode({ id: "node2", type: "END", config: {} });
      const edge: WorkflowEdge = {
        id: "edge1",
        sourceNodeId: "node1",
        targetNodeId: "node2",
        type: "DEFAULT",
      };

      graph.addNode(node1);
      graph.addNode(node2);
      graph.addEdge(edge);

      expect(structure.hasEdge("edge1")).toBe(true);
      expect(graph.hasEdge("edge1")).toBe(true);
      expect(graph.getEdge("edge1")).toBe(edge);
    });

    test("should delegate graph traversal methods", () => {
      const node1 = createTestNode({ id: "node1", type: "START", config: {} });
      const node2 = createTestNode({ id: "node2", type: "END", config: {} });
      const edge: WorkflowEdge = {
        id: "edge1",
        sourceNodeId: "node1",
        targetNodeId: "node2",
        type: "DEFAULT",
      };

      graph.addNode(node1);
      graph.addNode(node2);
      graph.addEdge(edge);

      expect(graph.getOutgoingNeighbors("node1")).toContain("node2");
      expect(graph.getIncomingNeighbors("node2")).toContain("node1");
      expect(graph.hasEdgeBetween("node1", "node2")).toBe(true);
    });
  });

  describe("Metadata Access Methods", () => {
    test("should provide access to node configs from metadata", () => {
      const nodeConfig = {
        id: "node1",
        name: "LLM Node",
        type: "LLM" as const,
        config: { profileId: "test-profile" },
      };

      graph.setNodeConfig("node1", nodeConfig);

      expect(graph.getNodeConfig("node1")).toBe(nodeConfig);
      expect(metadata.getNodeConfig("node1")).toBe(nodeConfig);
    });

    test("should provide access to trigger configs from metadata", () => {
      const triggerConfig = {
        id: "trigger1",
        name: "Webhook Trigger",
        condition: { eventType: "WORKFLOW_START" as const },
        action: { type: "execute_triggered_subworkflow" as const, subworkflowId: "sub-1" },
      } as any;

      graph.setTriggerConfig("trigger1", triggerConfig);

      expect(graph.getTriggerConfig("trigger1")).toBe(triggerConfig);
      expect(metadata.getTriggerConfig("trigger1")).toBe(triggerConfig);
    });

    test("should check node types correctly", () => {
      const nodeConfig = {
        id: "node1",
        name: "Fork Node",
        type: "FORK" as const,
        config: { forkPaths: [], forkStrategy: "parallel" as const },
      };

      graph.setNodeConfig("node1", nodeConfig);

      expect(graph.isNodeOfType("node1", "FORK")).toBe(true);
      expect(graph.isNodeOfType("node1", "LLM")).toBe(false);
    });
  });

  describe("Graph Lifecycle", () => {
    test("should track preprocessing state", () => {
      expect(graph.isPreprocessed()).toBe(false);

      metadata.markPreprocessed(Date.now());

      expect(graph.isPreprocessed()).toBe(true);
      expect(metadata.processedAt).toBeGreaterThan(0);
      expect(metadata.validationResult.validatedAt).toBeGreaterThan(0);
    });

    test("should allow creation with new structure", () => {
      const newStructure = new WorkflowGraphStructure();
      const newNode = createTestNode({ id: "newNode", type: "START", config: {} });
      newStructure.addNode(newNode);

      const newGraph = graph.withStructure(newStructure);

      expect(newGraph.structure).toBe(newStructure);
      expect(newGraph.metadata).toBe(metadata);
      expect(newGraph.hasNode("newNode")).toBe(true);
    });

    test("metadata should track changes correctly", () => {
      const nodeConfig = {
        id: "node1",
        name: "LLM Node",
        type: "LLM" as const,
        config: { profileId: "test-profile" },
      };

      graph.setNodeConfig("node1", nodeConfig);

      expect(metadata.nodeConfigs.size).toBe(1);
      expect(graph.getNodeConfig("node1")).toBe(nodeConfig);
    });
  });

  describe("Graph Statistics", () => {
    test("should provide correct node and edge counts", () => {
      const node1 = createTestNode({ id: "node1", type: "START", config: {} });
      const node2 = createTestNode({ id: "node2", type: "END", config: {} });
      const edge: WorkflowEdge = {
        id: "edge1",
        sourceNodeId: "node1",
        targetNodeId: "node2",
        type: "DEFAULT",
      };

      graph.addNode(node1);
      graph.addNode(node2);
      graph.addEdge(edge);

      expect(graph.getNodeCount()).toBe(2);
      expect(graph.getEdgeCount()).toBe(1);
      expect(graph.getAllNodeIds()).toHaveLength(2);
      expect(graph.getAllEdgeIds()).toHaveLength(1);
    });

    test("should identify source and sink nodes correctly", () => {
      const sourceNode = createTestNode({ id: "source", type: "START" as const, config: {} });
      const middleNode = createTestNode({ id: "middle", type: "LLM" as const, config: { profileId: "test" } });
      const sinkNode = createTestNode({ id: "sink", type: "END" as const, config: {} });

      graph.addNode(sourceNode);
      graph.addNode(middleNode);
      graph.addNode(sinkNode);

      graph.addEdge({
        id: "edge1",
        sourceNodeId: "source",
        targetNodeId: "middle",
        type: "DEFAULT",
      });
      graph.addEdge({
        id: "edge2",
        sourceNodeId: "middle",
        targetNodeId: "sink",
        type: "DEFAULT",
      });

      expect(graph.getSourceNodes()).toContainEqual(sourceNode);
      expect(graph.getSinkNodes()).toContainEqual(sinkNode);
    });

    test("should allow creation with new metadata", () => {
      const newMetadata = new WorkflowGraphMetadata();
      newMetadata.workflowId = "new-workflow";

      const newGraph = graph.withMetadata(newMetadata);

      expect(newGraph.structure).toBe(structure);
      expect(newGraph.metadata).toBe(newMetadata);
      expect(newGraph.workflowId).toBe("new-workflow");
    });
  });

  describe("Immutability Guarantees", () => {
    test("structure should remain immutable through graph operations", () => {
      const node = createTestNode({ id: "node1", type: "START", config: {} });
      const originalNodesSize = structure.nodes.size;

      graph.addNode(node);

      expect(structure.nodes.size).toBe(originalNodesSize + 1);
      expect(graph.getNode("node1")).toBe(node);
    });

    test("metadata should track changes correctly", () => {
      const nodeConfig = {
        id: "node1",
        name: "LLM Node",
        type: "LLM" as const,
        config: { profileId: "test-profile" },
      };

      graph.setNodeConfig("node1", nodeConfig);

      expect(metadata.nodeConfigs.size).toBe(1);
      expect(graph.getNodeConfig("node1")).toBe(nodeConfig);
    });
  });

  describe("Graph Statistics", () => {
    test("should provide correct node and edge counts", () => {
      const node1 = createTestNode({ id: "node1", type: "START", config: {} });
      const node2 = createTestNode({ id: "node2", type: "END", config: {} });
      const edge: WorkflowEdge = {
        id: "edge1",
        sourceNodeId: "node1",
        targetNodeId: "node2",
        type: "DEFAULT",
      };

      graph.addNode(node1);
      graph.addNode(node2);
      graph.addEdge(edge);

      expect(graph.getNodeCount()).toBe(2);
      expect(graph.getEdgeCount()).toBe(1);
      expect(graph.getAllNodeIds()).toHaveLength(2);
      expect(graph.getAllEdgeIds()).toHaveLength(1);
    });

    test("should identify source and sink nodes correctly", () => {
      const sourceNode = createTestNode({ id: "source", type: "START", config: {} });
      const middleNode = createTestNode({ id: "middle", type: "LLM", config: { profileId: "test" } });
      const sinkNode = createTestNode({ id: "sink", type: "END", config: {} });

      graph.addNode(sourceNode);
      graph.addNode(middleNode);
      graph.addNode(sinkNode);

      graph.addEdge({
        id: "edge1",
        sourceNodeId: "source",
        targetNodeId: "middle",
        type: "DEFAULT",
      });
      graph.addEdge({
        id: "edge2",
        sourceNodeId: "middle",
        targetNodeId: "sink",
        type: "DEFAULT",
      });

      expect(graph.getSourceNodes()).toContainEqual(sourceNode);
      expect(graph.getSinkNodes()).toContainEqual(sinkNode);
    });
  });
});