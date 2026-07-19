/**
 * Subgraph Validator Unit Tests
 *
 * Tests for:
 * - validateSubgraphExistence: Validates SUBGRAPH nodes have subgraphId configuration
 * - validateSubgraphCompatibility: Validates SUBGRAPH variable mapping format
 */

import { describe, it, expect } from "vitest";
import { validateSubgraphExistence, validateSubgraphCompatibility } from "../subgraph-validator.js";
import { WorkflowGraphStructureImpl } from "../../../entities/workflow-graph-structure.js";
import type { WorkflowNode, WorkflowEdge } from "@wf-agent/types";

describe("validateSubgraphExistence", () => {
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

  describe("Valid SUBGRAPH nodes", () => {
    it("should pass validation for a valid SUBGRAPH node with subgraphId", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphExistence(graph);
      expect(errors).toHaveLength(0);
    });

    it("should pass validation when there are no SUBGRAPH nodes", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        { id: "script1", type: "SCRIPT" },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphExistence(graph);
      expect(errors).toHaveLength(0);
    });
  });

  describe("Invalid SUBGRAPH nodes", () => {
    it("should fail validation when SUBGRAPH node is missing subgraphId", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {},
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphExistence(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("missing subgraphId");
      expect(errors[0]!.context?.["code"]).toBe("MISSING_SUBGRAPH_ID");
      expect(errors[0]!.context?.["nodeId"]).toBe("subgraph1");
    });

    it("should fail validation when SUBGRAPH node has empty subgraphId", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "",
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphExistence(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("missing subgraphId");
    });

    it("should fail validation when SUBGRAPH node has undefined config", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphExistence(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("missing subgraphId");
    });

    it("should report multiple errors for multiple invalid SUBGRAPH nodes", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {},
        },
        {
          id: "subgraph2",
          type: "SUBGRAPH",
          config: {
            subgraphId: "",
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphExistence(graph);
      expect(errors).toHaveLength(2);
      expect(errors[0]!.context?.["nodeId"]).toBe("subgraph1");
      expect(errors[1]!.context?.["nodeId"]).toBe("subgraph2");
    });
  });
});

describe("validateSubgraphCompatibility", () => {
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

  describe("Valid SUBGRAPH compatibility", () => {
    it("should pass validation for SUBGRAPH without variable mappings", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(0);
    });

    it("should pass validation for SUBGRAPH with valid variableInputs", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableInputs: [
              {
                sourcePath: "input1",
                internalName: "var1",
                required: true,
                defaultValue: "default",
              },
            ],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(0);
    });

    it("should pass validation for SUBGRAPH with valid variableOutputs", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableOutputs: [{ internalName: "var1", targetPath: "output1" }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(0);
    });

    it("should pass validation for SUBGRAPH with both valid inputs and outputs", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableInputs: [{ sourcePath: "input1", internalName: "var1" }],
            variableOutputs: [{ internalName: "var1", targetPath: "output1" }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(0);
    });

    it("should pass validation when there are no SUBGRAPH nodes", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        { id: "script1", type: "SCRIPT" },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(0);
    });
  });

  describe("Invalid SUBGRAPH compatibility - variableInputs", () => {
    it("should fail validation when variableInput has missing sourcePath", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableInputs: [{ sourcePath: "", internalName: "var1" }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("missing sourcePath");
      expect(errors[0]!.context?.["code"]).toBe("MISSING_SOURCE_PATH");
      expect(errors[0]!.context?.["nodeId"]).toBe("subgraph1");
    });

    it("should fail validation when variableInput has whitespace-only sourcePath", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableInputs: [{ sourcePath: "   ", internalName: "var1" }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("missing sourcePath");
    });

    it("should fail validation when variableInput has missing internalName", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableInputs: [{ sourcePath: "input1", internalName: "" }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("missing internalName");
      expect(errors[0]!.context?.["code"]).toBe("MISSING_INTERNAL_NAME");
    });

    it("should fail validation when variableInput has whitespace-only internalName", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableInputs: [{ sourcePath: "input1", internalName: "   " }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("missing internalName");
    });

    it("should report multiple errors for multiple invalid variableInputs", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableInputs: [
              { sourcePath: "", internalName: "" },
              { sourcePath: "input2", internalName: "" },
            ],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors.length).toBeGreaterThanOrEqual(3);
    });
  });

  describe("Invalid SUBGRAPH compatibility - variableOutputs", () => {
    it("should fail validation when variableOutput has missing internalName", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableOutputs: [{ internalName: "", targetPath: "output1" }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("missing internalName");
      expect(errors[0]!.context?.["code"]).toBe("MISSING_OUTPUT_INTERNAL_NAME");
      expect(errors[0]!.context?.["nodeId"]).toBe("subgraph1");
    });

    it("should fail validation when variableOutput has whitespace-only internalName", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableOutputs: [{ internalName: "   ", targetPath: "output1" }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("missing internalName");
    });

    it("should fail validation when variableOutput has missing targetPath", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableOutputs: [{ internalName: "var1", targetPath: "" }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("missing targetPath");
      expect(errors[0]!.context?.["code"]).toBe("MISSING_OUTPUT_TARGET_PATH");
    });

    it("should fail validation when variableOutput has whitespace-only targetPath", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableOutputs: [{ internalName: "var1", targetPath: "   " }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(1);
      expect(errors[0]!.message).toContain("missing targetPath");
    });

    it("should report multiple errors for multiple invalid variableOutputs", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableOutputs: [
              { internalName: "", targetPath: "" },
              { internalName: "var2", targetPath: "" },
            ],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors.length).toBeGreaterThanOrEqual(3);
    });
  });

  describe("Mixed scenarios", () => {
    it("should report errors for multiple SUBGRAPH nodes with different issues", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "workflow-1",
            variableInputs: [{ sourcePath: "", internalName: "var1" }],
          },
        },
        {
          id: "subgraph2",
          type: "SUBGRAPH",
          config: {
            subgraphId: "workflow-2",
            variableOutputs: [{ internalName: "var2", targetPath: "" }],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors.length).toBeGreaterThanOrEqual(2);

      const inputErrors = errors.filter(
        e => e.context?.["code"] === "MISSING_SOURCE_PATH",
      );
      const outputErrors = errors.filter(
        e => e.context?.["code"] === "MISSING_OUTPUT_TARGET_PATH",
      );

      expect(inputErrors.length).toBeGreaterThanOrEqual(1);
      expect(outputErrors.length).toBeGreaterThanOrEqual(1);
    });

    it("should not report error for empty variableInputs array", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableInputs: [],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(0);
    });

    it("should not report error for empty variableOutputs array", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {
            subgraphId: "subworkflow-1",
            variableOutputs: [],
          },
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      expect(errors).toHaveLength(0);
    });

    it("should handle SUBGRAPH node without subgraphId in compatibility check", () => {
      const graph = createGraph([
        { id: "start", type: "START" },
        {
          id: "subgraph1",
          type: "SUBGRAPH",
          config: {},
        },
        { id: "end", type: "END" },
      ]);

      const errors = validateSubgraphCompatibility(graph);
      // Should not crash, just skip this node since it has no subgraphId
      expect(errors).toHaveLength(0);
    });
  });
});
