/**
 * Embed Graph Validator Unit Tests
 *
 * Tests for:
 * - validateEmbedGraphExistence: Validates EMBED_GRAPH nodes have embedId configuration
 * - validateEmbedGraphConstraints: Validates EMBED_GRAPH nodes don't have variable mappings
 */

import { describe, it, expect } from "vitest";
import {
  validateEmbedGraphExistence,
  validateEmbedGraphConstraints,
} from "../embed-graph-validator.js";
import { WorkflowGraphStructureImpl } from "../../../entities/workflow-graph-structure.js";
import type { WorkflowNode, WorkflowEdge } from "@wf-agent/types";

describe("validateEmbedGraphExistence", () => {
  // Helper function to create a basic graph
  function createGraph(
    nodes: Array<{ id: string; type: string; config?: any }>,
    edges: Array<{ sourceNodeId: string; targetNodeId: string }> = [],
  ): WorkflowGraphStructure {
    const graph = new WorkflowGraphStructureImpl();

    // Add nodes
    for (const nodeData of nodes) {
      const node: WorkflowNode = {
        id: nodeData.id,
        type: nodeData.type as any,
        config: nodeData.config || {},
        workflowId: "test-workflow",
        outgoingEdgeIds: [],
        incomingEdgeIds: [],
        originalNode: {
          id: nodeData.id,
          type: nodeData.type as any,
          name: `${nodeData.type}-${nodeData.id}`,
          config: nodeData.config || {},
        } as any,
      };
      graph.addNode(node);
    }

    // Add edges
    let edgeIndex = 0;
    for (const edgeData of edges) {
      const edge: WorkflowEdge = {
        id: `edge-${edgeIndex++}`,
        sourceNodeId: edgeData.sourceNodeId,
        targetNodeId: edgeData.targetNodeId,
        type: "DEFAULT",
      };
      graph.addEdge(edge);
    }

    return graph;
  }

  describe("Valid EMBED_GRAPH nodes", () => {
    it("should pass validation for a valid EMBED_GRAPH node with embedId", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "embed1",
          type: "EMBED_GRAPH",
          config: {
            embedId: "embedded-workflow-1",
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateEmbedGraphExistence(graph);
      expect(errors).toHaveLength(0);
    });

    it("should pass validation when there are no EMBED_GRAPH nodes", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        { id: "script1", type: "SCRIPT" },
        { id: "end", type: "END" },
      ]);

      const errors = validateEmbedGraphExistence(graph);
      expect(errors).toHaveLength(0);
    });
  });

  describe("Invalid EMBED_GRAPH nodes", () => {
    it("should fail validation when EMBED_GRAPH node is missing embedId", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "embed1",
          type: "EMBED_GRAPH",
          config: {},
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateEmbedGraphExistence(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("missing embedId");
      expect(errors[0]!.context?.["code"]).toBe("MISSING_EMBED_ID");
      expect(errors[0]!.context?.["nodeId"]).toBe("embed1");
    });

    it("should fail validation when EMBED_GRAPH node has empty embedId", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "embed1",
          type: "EMBED_GRAPH",
          config: {
            embedId: "",
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateEmbedGraphExistence(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("missing embedId");
    });

    it("should fail validation when EMBED_GRAPH node has undefined config", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "embed1",
          type: "EMBED_GRAPH",
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateEmbedGraphExistence(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("missing embedId");
    });

    it("should report multiple errors for multiple invalid EMBED_GRAPH nodes", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "embed1",
          type: "EMBED_GRAPH",
          config: {},
        },
        {
          id: "embed2",
          type: "EMBED_GRAPH",
          config: {
            embedId: "",
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateEmbedGraphExistence(graph);
      expect(errors).toHaveLength(2);
      expect(errors[0]!.context?.["nodeId"]).toBe("embed1");
      expect(errors[1]!.context?.["nodeId"]).toBe("embed2");
    });
  });
});

describe("validateEmbedGraphConstraints", () => {
  // Helper function to create a basic graph
  function createGraph(
    nodes: Array<{ id: string; type: string; config?: any }>,
    edges: Array<{ sourceNodeId: string; targetNodeId: string }> = [],
  ): WorkflowGraphStructure {
    const graph = new WorkflowGraphStructureImpl();

    // Add nodes
    for (const nodeData of nodes) {
      const node: WorkflowNode = {
        id: nodeData.id,
        type: nodeData.type as any,
        config: nodeData.config || {},
        workflowId: "test-workflow",
        outgoingEdgeIds: [],
        incomingEdgeIds: [],
        originalNode: {
          id: nodeData.id,
          type: nodeData.type as any,
          name: `${nodeData.type}-${nodeData.id}`,
          config: nodeData.config || {},
        } as any,
      };
      graph.addNode(node);
    }

    // Add edges
    let edgeIndex = 0;
    for (const edgeData of edges) {
      const edge: WorkflowEdge = {
        id: `edge-${edgeIndex++}`,
        sourceNodeId: edgeData.sourceNodeId,
        targetNodeId: edgeData.targetNodeId,
        type: "DEFAULT",
      };
      graph.addEdge(edge);
    }

    return graph;
  }

  describe("Valid EMBED_GRAPH constraints", () => {
    it("should pass validation for EMBED_GRAPH without variable mappings", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "embed1",
          type: "EMBED_GRAPH",
          config: {
            embedId: "embedded-workflow-1",
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateEmbedGraphConstraints(graph);
      expect(errors).toHaveLength(0);
    });

    it("should pass validation when there are no EMBED_GRAPH nodes", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        { id: "script1", type: "SCRIPT" },
        { id: "end", type: "END" },
      ]);

      const errors = validateEmbedGraphConstraints(graph);
      expect(errors).toHaveLength(0);
    });
  });

  describe("Invalid EMBED_GRAPH constraints", () => {
    it("should fail validation when EMBED_GRAPH has variableInputs", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "embed1",
          type: "EMBED_GRAPH",
          config: {
            embedId: "embedded-workflow-1",
            variableInputs: [{ sourcePath: "input1", internalName: "var1" }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateEmbedGraphConstraints(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("should not have variableInputs");
      expect(errors[0]!.context?.["code"]).toBe("EMBED_GRAPH_HAS_VARIABLE_INPUTS");
      expect(errors[0]!.context?.["nodeId"]).toBe("embed1");
    });

    it("should fail validation when EMBED_GRAPH has variableOutputs", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "embed1",
          type: "EMBED_GRAPH",
          config: {
            embedId: "embedded-workflow-1",
            variableOutputs: [{ internalName: "var1", targetPath: "output1" }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateEmbedGraphConstraints(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("should not have variableOutputs");
      expect(errors[0]!.context?.["code"]).toBe("EMBED_GRAPH_HAS_VARIABLE_OUTPUTS");
      expect(errors[0]!.context?.["nodeId"]).toBe("embed1");
    });

    it("should fail validation when EMBED_GRAPH has both variableInputs and variableOutputs", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "embed1",
          type: "EMBED_GRAPH",
          config: {
            embedId: "embedded-workflow-1",
            variableInputs: [{ sourcePath: "input1", internalName: "var1" }],
            variableOutputs: [{ internalName: "var1", targetPath: "output1" }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateEmbedGraphConstraints(graph);
      expect(errors).toHaveLength(2);
      expect(errors[0]!.context?.["code"]).toBe("EMBED_GRAPH_HAS_VARIABLE_INPUTS");
      expect(errors[1]!.context?.["code"]).toBe("EMBED_GRAPH_HAS_VARIABLE_OUTPUTS");
    });

    it("should report errors for multiple EMBED_GRAPH nodes with violations", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "embed1",
          type: "EMBED_GRAPH",
          config: {
            embedId: "workflow-1",
            variableInputs: [{ sourcePath: "input1", internalName: "var1" }],
          },
        },
        {
          id: "embed2",
          type: "EMBED_GRAPH",
          config: {
            embedId: "workflow-2",
            variableOutputs: [{ internalName: "var2", targetPath: "output2" }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateEmbedGraphConstraints(graph);
      expect(errors).toHaveLength(2);
      expect(errors[0]!.context?.["nodeId"]).toBe("embed1");
      expect(errors[1]!.context?.["nodeId"]).toBe("embed2");
    });

    it("should not report error for empty variableInputs array", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "embed1",
          type: "EMBED_GRAPH",
          config: {
            embedId: "embedded-workflow-1",
            variableInputs: [],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateEmbedGraphConstraints(graph);
      expect(errors).toHaveLength(0);
    });

    it("should not report error for empty variableOutputs array", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "embed1",
          type: "EMBED_GRAPH",
          config: {
            embedId: "embedded-workflow-1",
            variableOutputs: [],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateEmbedGraphConstraints(graph);
      expect(errors).toHaveLength(0);
    });
  });
});
