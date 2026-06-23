/**
 * Edge Case Tests: WorkflowGraph Composition Pattern
 *
 * Tests boundary conditions, error scenarios, and edge cases
 * for the new composition-based WorkflowGraph design.
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

describe("WorkflowGraph Composition Edge Cases", () => {
  describe("Empty Graph Operations", () => {
    test("should handle empty graph structure correctly", () => {
      const graph = new WorkflowGraph();

      expect(graph.getNodeCount()).toBe(0);
      expect(graph.getEdgeCount()).toBe(0);
      expect(graph.getAllNodeIds()).toHaveLength(0);
      expect(graph.getAllEdgeIds()).toHaveLength(0);
      expect(graph.getSourceNodes()).toHaveLength(0);
      expect(graph.getSinkNodes()).toHaveLength(0);
    });

    test("should handle operations on non-existent nodes gracefully", () => {
      const graph = new WorkflowGraph();

      expect(graph.getNode("nonexistent")).toBeUndefined();
      expect(graph.hasNode("nonexistent")).toBe(false);
      expect(graph.getOutgoingNeighbors("nonexistent")).toEqual(new Set());
      expect(graph.getIncomingNeighbors("nonexistent")).toEqual(new Set());
      expect(graph.getOutgoingEdges("nonexistent")).toHaveLength(0);
      expect(graph.getIncomingEdges("nonexistent")).toHaveLength(0);
    });

    test("should handle empty metadata operations", () => {
      const graph = new WorkflowGraph();

      expect(graph.getNodeConfig("nonexistent")).toBeUndefined();
      expect(graph.getTriggerConfig("nonexistent")).toBeUndefined();
      expect(graph.isNodeOfType("nonexistent", "START")).toBe(false);
      expect(graph.getNodeIdsByType("START")).toHaveLength(0);
      expect(graph.isPreprocessed()).toBe(false);
    });
  });

  describe("Invalid Operations", () => {
    test("should handle adding edges with non-existent source/target nodes", () => {
      const graph = new WorkflowGraph();
      const edge: WorkflowEdge = {
        id: "edge1",
        sourceNodeId: "nonexistent1",
        targetNodeId: "nonexistent2",
        type: "DEFAULT",
      };

      // Should not throw, but edge is added
      expect(() => graph.addEdge(edge)).not.toThrow();
      expect(graph.hasEdge("edge1")).toBe(true);
    });

    test("should handle duplicate node additions", () => {
      const graph = new WorkflowGraph();
      const node1 = createTestNode({ id: "node1", type: "START", config: { version: 1 } });
      const node2 = createTestNode({ id: "node1", type: "START", config: { version: 2 } });

      graph.addNode(node1);
      graph.addNode(node2);

      expect(graph.getNodeCount()).toBe(1);
      expect(graph.getNode("node1")?.config).toEqual({ version: 2 }); // Last one wins
    });

    test("should handle duplicate edge additions", () => {
      const graph = new WorkflowGraph();
      const node1 = createTestNode({ id: "node1", type: "START", config: {} });
      const node2 = createTestNode({ id: "node2", type: "END", config: {} });
      const edge1: WorkflowEdge = { id: "edge1", sourceNodeId: "node1", targetNodeId: "node2", type: "DEFAULT" };
      const edge2: WorkflowEdge = { id: "edge1", sourceNodeId: "node1", targetNodeId: "node2", type: "CONDITIONAL" };

      graph.addNode(node1);
      graph.addNode(node2);
      graph.addEdge(edge1);
      graph.addEdge(edge2);

      expect(graph.getEdgeCount()).toBe(1);
      expect(graph.getEdge("edge1")?.type).toBe("CONDITIONAL"); // Last one wins
    });
  });

  describe("Large Graph Performance", () => {
    test("should handle graphs with many nodes efficiently", () => {
      const graph = new WorkflowGraph();
      const nodeCount = 1000;

      // Add nodes
      for (let i = 0; i < nodeCount; i++) {
        const node = createTestNode({ id: `node${i}`, type: "LLM", config: { profileId: "test-profile" } });
        graph.addNode(node);
      }

      expect(graph.getNodeCount()).toBe(nodeCount);
      expect(graph.getAllNodeIds()).toHaveLength(nodeCount);
    });

    test("should handle graphs with many edges efficiently", () => {
      const graph = new WorkflowGraph();
      const nodeCount = 100;
      const edgesPerNode = 10;

    // Add nodes
    for (let i = 0; i < nodeCount; i++) {
      const node = createTestNode({ id: `node${i}`, type: "LLM", config: { profileId: "test-profile" } });
      graph.addNode(node);
    }

    // Add edges
      let edgeCount = 0;
      for (let i = 0; i < nodeCount; i++) {
        for (let j = 0; j < edgesPerNode; j++) {
          const targetIndex = (i + j + 1) % nodeCount;
          const edge: WorkflowEdge = {
            id: `edge${edgeCount++}`,
            sourceNodeId: `node${i}`,
            targetNodeId: `node${targetIndex}`,
            type: "DEFAULT",
          };
          graph.addEdge(edge);
        }
      }

      expect(graph.getEdgeCount()).toBe(nodeCount * edgesPerNode);

      // Test neighbor queries performance
      const neighbors = graph.getOutgoingNeighbors("node0");
      expect(neighbors.size).toBe(edgesPerNode);
    });
  });

  describe("Circular References", () => {
    test("should handle circular edge references", () => {
      const graph = new WorkflowGraph();
      const node1 = createTestNode({ id: "node1", type: "START", config: {} });
      const node2 = createTestNode({ id: "node2", type: "LLM", config: { profileId: "test-profile" } });
      const node3 = createTestNode({ id: "node3", type: "LLM", config: { profileId: "test-profile" } });

      graph.addNode(node1);
      graph.addNode(node2);
      graph.addNode(node3);

      // Create circular reference
      graph.addEdge({ id: "edge1", sourceNodeId: "node1", targetNodeId: "node2", type: "DEFAULT" });
      graph.addEdge({ id: "edge2", sourceNodeId: "node2", targetNodeId: "node3", type: "DEFAULT" });
      graph.addEdge({ id: "edge3", sourceNodeId: "node3", targetNodeId: "node1", type: "DEFAULT" });

      expect(graph.hasEdgeBetween("node1", "node2")).toBe(true);
      expect(graph.hasEdgeBetween("node2", "node3")).toBe(true);
      expect(graph.hasEdgeBetween("node3", "node1")).toBe(true);

      // Should still be able to get neighbors
      expect(graph.getOutgoingNeighbors("node1")).toContain("node2");
      expect(graph.getIncomingNeighbors("node1")).toContain("node3");
    });
  });

  describe("Metadata Edge Cases", () => {
    test("should handle null/undefined metadata values", () => {
      const metadata = new WorkflowGraphMetadata();

      expect(metadata.workflowId).toBe("");
      expect(metadata.workflowVersion).toBe("1.0.0");
      expect(metadata.triggers).toBeUndefined();
      expect(metadata.variables).toBeUndefined();
      expect(metadata.hasSubgraphs).toBe(false);
      expect(metadata.subworkflowIds).toEqual(new Set());
    });

    test("should handle type guard operations on invalid nodes", () => {
      const graph = new WorkflowGraph();
      const nodeConfig = {
        id: "node1",
        name: "Node 1",
        type: "LLM" as const,
        config: { profileId: "test-profile" },
      };

      graph.setNodeConfig("node1", nodeConfig);

      // Valid type check
      const llmConfig = graph.getNodeConfigByType<typeof nodeConfig.type>("node1", "LLM");
      expect(llmConfig).toBe(nodeConfig);

      // Invalid type check
      const forkConfig = graph.getNodeConfigByType("node1", "FORK");
      expect(forkConfig).toBeUndefined();
    });

    test("should handle preprocessing state transitions", () => {
      const metadata = new WorkflowGraphMetadata();

      expect(metadata.isPreprocessed()).toBe(false);

      // Mark as preprocessed
      metadata.markPreprocessed(1234567890);
      expect(metadata.isPreprocessed()).toBe(true);
      expect(metadata.processedAt).toBe(1234567890);
      expect(metadata.validationResult.validatedAt).toBe(1234567890);

      // Multiple calls should update timestamp
      metadata.markPreprocessed(1234567990);
      expect(metadata.processedAt).toBe(1234567990);
    });
  });

  describe("Graph Transformation Edge Cases", () => {
    test("should handle withStructure when structure has different nodes", () => {
      const originalStructure = new WorkflowGraphStructure();
      const originalNode = createTestNode({ id: "original", type: "START", config: {} });
      originalStructure.addNode(originalNode);

      const newStructure = new WorkflowGraphStructure();
      const newNode = createTestNode({ id: "new", type: "START", config: {} });
      newStructure.addNode(newNode);

      const metadata = new WorkflowGraphMetadata();
      const originalGraph = new WorkflowGraph(originalStructure, metadata);
      const newGraph = originalGraph.withStructure(newStructure);

      expect(originalGraph.structure).toBe(originalStructure);
      expect(newGraph.structure).toBe(newStructure);
      expect(newGraph.metadata).toBe(metadata);

      expect(originalGraph.hasNode("original")).toBe(true);
      expect(originalGraph.hasNode("new")).toBe(false);

      expect(newGraph.hasNode("original")).toBe(false);
      expect(newGraph.hasNode("new")).toBe(true);
    });

    test("should handle withMetadata when metadata has different state", () => {
      const structure = new WorkflowGraphStructure();
      const originalMetadata = new WorkflowGraphMetadata();
      originalMetadata.workflowId = "original";
      originalMetadata.markPreprocessed(1111111111);

      const newMetadata = new WorkflowGraphMetadata();
      newMetadata.workflowId = "new";
      newMetadata.markPreprocessed(2222222222);

      const originalGraph = new WorkflowGraph(structure, originalMetadata);
      const newGraph = originalGraph.withMetadata(newMetadata);

      expect(originalGraph.metadata).toBe(originalMetadata);
      expect(newGraph.metadata).toBe(newMetadata);
      expect(newGraph.structure).toBe(structure);

      expect(originalGraph.workflowId).toBe("original");
      expect(newGraph.workflowId).toBe("new");

      expect(originalGraph.metadata.processedAt).toBe(1111111111);
      expect(newGraph.metadata.processedAt).toBe(2222222222);
    });
  });

  describe("Memory Management", () => {
    test("should not leak references when creating transformed graphs", () => {
      const structure = new WorkflowGraphStructure();
      const metadata = new WorkflowGraphMetadata();
      const originalGraph = new WorkflowGraph(structure, metadata);

      // Create multiple transformations
      const transformations = [];
      for (let i = 0; i < 100; i++) {
        const newStructure = new WorkflowGraphStructure();
        const transformedGraph = originalGraph.withStructure(newStructure);
        transformations.push(transformedGraph);
      }

      // Original graph should still reference the same structure and metadata
      expect(originalGraph.structure).toBe(structure);
      expect(originalGraph.metadata).toBe(metadata);

      // All transformations should have different structures but same metadata
      transformations.forEach((graph, _index) => {
        expect(graph.structure).not.toBe(structure);
        expect(graph.metadata).toBe(metadata);
      });
    });
  });

  describe("Concurrent Access Simulation", () => {
    test("should handle rapid successive modifications", () => {
      const graph = new WorkflowGraph();

      // Simulate rapid modifications
      for (let i = 0; i < 100; i++) {
        const node = createTestNode({ id: `node${i}`, type: "LLM", config: { profileId: "test-profile" } });
        graph.addNode(node);

        if (i > 0) {
          const edge: WorkflowEdge = {
            id: `edge${i}`,
            sourceNodeId: `node${i - 1}`,
            targetNodeId: `node${i}`,
            type: "DEFAULT",
          };
          graph.addEdge(edge);
        }
      }

      expect(graph.getNodeCount()).toBe(100);
      expect(graph.getEdgeCount()).toBe(99);

      // Verify chain integrity
      for (let i = 0; i < 99; i++) {
        expect(graph.hasEdgeBetween(`node${i}`, `node${i + 1}`)).toBe(true);
      }
    });
  });
});