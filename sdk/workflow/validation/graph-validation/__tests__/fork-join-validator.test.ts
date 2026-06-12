/**
 * Fork-Join Validator Unit Tests
 *
 * Tests for validateForkJoinPairs function which validates:
 * - FORK node forkPaths configuration
 * - JOIN node forkPathIds and mainPathId configuration
 * - FORK-JOIN pairing relationships
 * - Global uniqueness of forkPathIds
 * - Reachability from FORK to JOIN
 */

import { describe, it, expect } from "vitest";
import { validateForkJoinPairs } from "../fork-join-validator.js";
import { WorkflowGraphData } from "../../../entities/workflow-graph-data.js";
import type { WorkflowNode, WorkflowEdge } from "@wf-agent/types";

describe("validateForkJoinPairs", () => {
  // Helper function to create a basic graph
  function createGraph(
    nodes: Array<{ id: string; type: string; config?: any }>,
    edges: Array<{ sourceNodeId: string; targetNodeId: string }>,
  ): WorkflowGraphData {
    const graph = new WorkflowGraphData();

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

  describe("Valid FORK-JOIN pairs", () => {
    it("should pass validation for a valid FORK-JOIN pair", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {
              forkPaths: [
                { pathId: "path-a", childNodeId: "node-a" },
                { pathId: "path-b", childNodeId: "node-b" },
              ],
            },
          },
          { id: "node-a", type: "SCRIPT" },
          { id: "node-b", type: "SCRIPT" },
          {
            id: "join1",
            type: "JOIN",
            config: {
              forkPathIds: ["path-a", "path-b"],
            },
          },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "fork1", targetNodeId: "node-a" },
          { sourceNodeId: "fork1", targetNodeId: "node-b" },
          { sourceNodeId: "node-a", targetNodeId: "join1" },
          { sourceNodeId: "node-b", targetNodeId: "join1" },
          { sourceNodeId: "join1", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      // Should have no errors for a valid configuration
      expect(errors).toHaveLength(0);
    });

    it("should pass validation with mainPathId specified", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {
              forkPaths: [
                { pathId: "path-a", childNodeId: "node-a" },
                { pathId: "path-b", childNodeId: "node-b" },
              ],
            },
          },
          { id: "node-a", type: "SCRIPT" },
          { id: "node-b", type: "SCRIPT" },
          {
            id: "join1",
            type: "JOIN",
            config: {
              forkPathIds: ["path-a", "path-b"],
              mainPathId: "path-a",
            },
          },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "fork1", targetNodeId: "node-a" },
          { sourceNodeId: "fork1", targetNodeId: "node-b" },
          { sourceNodeId: "node-a", targetNodeId: "join1" },
          { sourceNodeId: "node-b", targetNodeId: "join1" },
          { sourceNodeId: "join1", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      expect(errors).toHaveLength(0);
    });
  });

  describe("FORK node validation errors", () => {
    it("should error when FORK node has empty forkPaths", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {
              forkPaths: [],
            },
          },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "fork1", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      // Now reports both config error and unpaired error
      expect(errors.length).toBeGreaterThanOrEqual(1);
      expect(errors[0]!.message).toContain("forkPaths must be a non-empty array");
      expect(errors[0]!.context?.["code"]).toBe("INVALID_FORK_PATHS");
      // Should also report unpaired fork
      const unpairedError = errors.find(e => e.context?.["code"] === "UNPAIRED_FORK");
      expect(unpairedError).toBeDefined();
    });

    it("should error when FORK node has missing forkPaths", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {},
          },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "fork1", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      // Now reports both config error and unpaired error
      expect(errors.length).toBeGreaterThanOrEqual(1);
      expect(errors[0]!.message).toContain("forkPaths must be a non-empty array");
      // Should also report unpaired fork
      const unpairedError = errors.find(e => e.context?.["code"] === "UNPAIRED_FORK");
      expect(unpairedError).toBeDefined();
    });

    it("should error when forkPaths item is missing pathId", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {
              forkPaths: [
                { pathId: "path-a", childNodeId: "node-a" },
                { childNodeId: "node-b" } as any,
              ],
            },
          },
          { id: "node-a", type: "SCRIPT" },
          { id: "node-b", type: "SCRIPT" },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "fork1", targetNodeId: "node-a" },
          { sourceNodeId: "fork1", targetNodeId: "node-b" },
          { sourceNodeId: "node-a", targetNodeId: "end" },
          { sourceNodeId: "node-b", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      expect(errors.length).toBeGreaterThan(0);
      expect(errors[0]!.message).toContain("must contain pathId and childNodeId");
    });

    it("should error when forkPaths item is missing childNodeId", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {
              forkPaths: [{ pathId: "path-a", childNodeId: "node-a" }, { pathId: "path-b" } as any],
            },
          },
          { id: "node-a", type: "SCRIPT" },
          { id: "node-b", type: "SCRIPT" },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "fork1", targetNodeId: "node-a" },
          { sourceNodeId: "fork1", targetNodeId: "node-b" },
          { sourceNodeId: "node-a", targetNodeId: "end" },
          { sourceNodeId: "node-b", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      expect(errors.length).toBeGreaterThan(0);
      expect(errors[0]!.message).toContain("must contain pathId and childNodeId");
    });

    it("should error when pathId is duplicated within workflow", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {
              forkPaths: [
                { pathId: "path-a", childNodeId: "node-a" },
                { pathId: "path-a", childNodeId: "node-b" },
              ],
            },
          },
          { id: "node-a", type: "SCRIPT" },
          { id: "node-b", type: "SCRIPT" },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "fork1", targetNodeId: "node-a" },
          { sourceNodeId: "fork1", targetNodeId: "node-b" },
          { sourceNodeId: "node-a", targetNodeId: "end" },
          { sourceNodeId: "node-b", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      expect(errors.length).toBeGreaterThan(0);
      const duplicateError = errors.find(e => e.context?.["code"] === "DUPLICATE_FORK_PATH_ID");
      expect(duplicateError).toBeDefined();
      expect(duplicateError?.message).toContain("is not unique within the workflow definition");
    });

    it("should error when multiple FORK nodes use same first pathId", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {
              forkPaths: [{ pathId: "path-a", childNodeId: "node-a" }],
            },
          },
          {
            id: "fork2",
            type: "FORK",
            config: {
              forkPaths: [{ pathId: "path-a", childNodeId: "node-b" }],
            },
          },
          { id: "node-a", type: "SCRIPT" },
          { id: "node-b", type: "SCRIPT" },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "start", targetNodeId: "fork2" },
          { sourceNodeId: "fork1", targetNodeId: "node-a" },
          { sourceNodeId: "fork2", targetNodeId: "node-b" },
          { sourceNodeId: "node-a", targetNodeId: "end" },
          { sourceNodeId: "node-b", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      expect(errors.length).toBeGreaterThan(0);
      const duplicateError = errors.find(e => e.context?.["code"] === "DUPLICATE_FORK_PAIR_ID");
      expect(duplicateError).toBeDefined();
      expect(duplicateError?.message).toContain("is already used by another FORK node");
    });
  });

  describe("JOIN node validation errors", () => {
    it("should error when JOIN node has empty forkPathIds", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          { id: "node-a", type: "SCRIPT" },
          {
            id: "join1",
            type: "JOIN",
            config: {
              forkPathIds: [],
            },
          },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "node-a" },
          { sourceNodeId: "node-a", targetNodeId: "join1" },
          { sourceNodeId: "join1", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      expect(errors.length).toBeGreaterThan(0);
      expect(errors[0]!.message).toContain("forkPathIds of JOIN node");
      expect(errors[0]!.context?.["code"]).toBe("INVALID_FORK_PATH_IDS");
    });

    it("should error when mainPathId is not in forkPathIds", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {
              forkPaths: [
                { pathId: "path-a", childNodeId: "node-a" },
                { pathId: "path-b", childNodeId: "node-b" },
              ],
            },
          },
          { id: "node-a", type: "SCRIPT" },
          { id: "node-b", type: "SCRIPT" },
          {
            id: "join1",
            type: "JOIN",
            config: {
              forkPathIds: ["path-a", "path-b"],
              mainPathId: "path-c",
            },
          },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "fork1", targetNodeId: "node-a" },
          { sourceNodeId: "fork1", targetNodeId: "node-b" },
          { sourceNodeId: "node-a", targetNodeId: "join1" },
          { sourceNodeId: "node-b", targetNodeId: "join1" },
          { sourceNodeId: "join1", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      expect(errors.length).toBeGreaterThan(0);
      const mainPathError = errors.find(e => e.context?.["code"] === "MAIN_PATH_ID_NOT_FOUND");
      expect(mainPathError).toBeDefined();
      expect(mainPathError?.message).toContain("mainPathId");
    });

    it("should error when multiple JOIN nodes use same first forkPathId", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {
              forkPaths: [
                { pathId: "path-a", childNodeId: "node-a" },
                { pathId: "path-b", childNodeId: "node-b" },
                { pathId: "path-c", childNodeId: "node-c" },
              ],
            },
          },
          { id: "node-a", type: "SCRIPT" },
          { id: "node-b", type: "SCRIPT" },
          { id: "node-c", type: "SCRIPT" },
          {
            id: "join1",
            type: "JOIN",
            config: {
              forkPathIds: ["path-a", "path-b"],
            },
          },
          {
            id: "join2",
            type: "JOIN",
            config: {
              forkPathIds: ["path-a", "path-c"],
            },
          },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "fork1", targetNodeId: "node-a" },
          { sourceNodeId: "fork1", targetNodeId: "node-b" },
          { sourceNodeId: "fork1", targetNodeId: "node-c" },
          { sourceNodeId: "node-a", targetNodeId: "join1" },
          { sourceNodeId: "node-b", targetNodeId: "join1" },
          { sourceNodeId: "node-c", targetNodeId: "join2" },
          { sourceNodeId: "join1", targetNodeId: "end" },
          { sourceNodeId: "join2", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      expect(errors.length).toBeGreaterThan(0);
      const duplicateError = errors.find(e => e.context?.["code"] === "DUPLICATE_JOIN_PAIR_ID");
      expect(duplicateError).toBeDefined();
      expect(duplicateError?.message).toContain("is already used by another JOIN node");
    });
  });

  describe("FORK-JOIN pairing errors", () => {
    it("should error when FORK has no matching JOIN", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {
              forkPaths: [{ pathId: "path-a", childNodeId: "node-a" }],
            },
          },
          { id: "node-a", type: "SCRIPT" },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "fork1", targetNodeId: "node-a" },
          { sourceNodeId: "node-a", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      expect(errors.length).toBeGreaterThan(0);
      const unpairedError = errors.find(e => e.context?.["code"] === "UNPAIRED_FORK");
      expect(unpairedError).toBeDefined();
      expect(unpairedError?.message).toContain("has no matching JOIN node");
    });

    it("should error when JOIN has no matching FORK", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {
              forkPaths: [{ pathId: "path-b", childNodeId: "node-b" }],
            },
          },
          { id: "node-b", type: "SCRIPT" },
          {
            id: "join1",
            type: "JOIN",
            config: {
              forkPathIds: ["path-a"],
            },
          },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "fork1", targetNodeId: "node-b" },
          { sourceNodeId: "node-b", targetNodeId: "join1" },
          { sourceNodeId: "join1", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      expect(errors.length).toBeGreaterThan(0);
      const unpairedError = errors.find(e => e.context?.["code"] === "UNPAIRED_JOIN");
      expect(unpairedError).toBeDefined();
      expect(unpairedError?.message).toContain("has no matching FORK node");
    });

    it("should error when FORK and JOIN forkPathIds do not match", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {
              forkPaths: [
                { pathId: "path-a", childNodeId: "node-a" },
                { pathId: "path-b", childNodeId: "node-b" },
              ],
            },
          },
          { id: "node-a", type: "SCRIPT" },
          { id: "node-b", type: "SCRIPT" },
          {
            id: "join1",
            type: "JOIN",
            config: {
              forkPathIds: ["path-a", "path-c"],
            },
          },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "fork1", targetNodeId: "node-a" },
          { sourceNodeId: "fork1", targetNodeId: "node-b" },
          { sourceNodeId: "node-a", targetNodeId: "join1" },
          { sourceNodeId: "node-b", targetNodeId: "join1" },
          { sourceNodeId: "join1", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      expect(errors.length).toBeGreaterThan(0);
      const mismatchError = errors.find(e => e.context?.["code"] === "FORK_JOIN_MISMATCH");
      expect(mismatchError).toBeDefined();
      expect(mismatchError?.message).toContain("do not match");
    });

    it("should error when FORK cannot reach JOIN", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {
              forkPaths: [{ pathId: "path-a", childNodeId: "node-a" }],
            },
          },
          {
            id: "fork2",
            type: "FORK",
            config: {
              forkPaths: [{ pathId: "path-b", childNodeId: "join1" }],
            },
          },
          { id: "node-a", type: "SCRIPT" },
          {
            id: "join1",
            type: "JOIN",
            config: {
              forkPathIds: ["path-a"],
            },
          },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "start", targetNodeId: "fork2" },
          { sourceNodeId: "fork1", targetNodeId: "node-a" },
          { sourceNodeId: "fork2", targetNodeId: "join1" },
          { sourceNodeId: "node-a", targetNodeId: "end" },
          { sourceNodeId: "join1", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      expect(errors.length).toBeGreaterThan(0);
      const reachableError = errors.find(e => e.context?.["code"] === "FORK_JOIN_NOT_REACHABLE");
      expect(reachableError).toBeDefined();
      expect(reachableError?.message).toContain("cannot reach the paired JOIN node");
    });
  });

  describe("Multiple FORK-JOIN pairs", () => {
    it("should validate multiple independent FORK-JOIN pairs", () => {
      const graph = createGraph(
        [
          { id: "start", type: "START" },
          {
            id: "fork1",
            type: "FORK",
            config: {
              forkPaths: [{ pathId: "path-a", childNodeId: "node-a" }],
            },
          },
          {
            id: "fork2",
            type: "FORK",
            config: {
              forkPaths: [{ pathId: "path-b", childNodeId: "node-b" }],
            },
          },
          { id: "node-a", type: "SCRIPT" },
          { id: "node-b", type: "SCRIPT" },
          {
            id: "join1",
            type: "JOIN",
            config: {
              forkPathIds: ["path-a"],
            },
          },
          {
            id: "join2",
            type: "JOIN",
            config: {
              forkPathIds: ["path-b"],
            },
          },
          { id: "end", type: "END" },
        ],
        [
          { sourceNodeId: "start", targetNodeId: "fork1" },
          { sourceNodeId: "start", targetNodeId: "fork2" },
          { sourceNodeId: "fork1", targetNodeId: "node-a" },
          { sourceNodeId: "fork2", targetNodeId: "node-b" },
          { sourceNodeId: "node-a", targetNodeId: "join1" },
          { sourceNodeId: "node-b", targetNodeId: "join2" },
          { sourceNodeId: "join1", targetNodeId: "end" },
          { sourceNodeId: "join2", targetNodeId: "end" },
        ],
      );

      const errors = validateForkJoinPairs(graph);
      expect(errors).toHaveLength(0);
    });
  });
});
