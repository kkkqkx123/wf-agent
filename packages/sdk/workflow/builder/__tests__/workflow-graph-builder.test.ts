/**
 * Workflow Graph Builder Unit Tests
 * Tests for WorkflowGraphBuilder class functionality
 */

import { describe, it, expect } from "vitest";
import { WorkflowGraphBuilder } from "../workflow-graph-builder.js";
import type { WorkflowTemplate } from "@wf-agent/types";

describe("WorkflowGraphBuilder", () => {
  describe("build", () => {
    it("should build a basic workflow graph with START and END nodes", () => {
      const workflow: WorkflowTemplate = {
        id: "test-workflow-1",
        name: "Test Workflow",
        description: "A simple test workflow",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          {
            id: "start-node",
            type: "START",
            name: "Start Node",
            config: {},
          },
          {
            id: "end-node",
            type: "END",
            name: "End Node",
            config: {},
          },
        ],
        edges: [
          {
            id: "edge-1",
            sourceNodeId: "start-node",
            targetNodeId: "end-node",
            type: "DEFAULT",
          },
        ],
      };

      const graph = WorkflowGraphBuilder.build(workflow);

      expect(graph).toBeDefined();
      expect(graph.nodes.size).toBe(2);
      expect(graph.edges.size).toBe(1);
      expect(graph.startNodeId).toBe("start-node");
      expect(graph.endNodeIds.has("end-node")).toBe(true);
      expect(graph.getNode("start-node")).toBeDefined();
      expect(graph.getNode("end-node")).toBeDefined();
    });

    it("should set workflowId on all nodes", () => {
      const workflow: WorkflowTemplate = {
        id: "workflow-xyz",
        name: "Test",
        description: "Test",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: "start", type: "START", name: "Start", config: {} },
          { id: "end", type: "END", name: "End", config: {} },
        ],
        edges: [{ id: "e1", sourceNodeId: "start", targetNodeId: "end", type: "DEFAULT" }],
      };

      const graph = WorkflowGraphBuilder.build(workflow);

      graph.nodes.forEach(node => {
        expect(node.workflowId).toBe("workflow-xyz");
      });
    });

    it("should initialize internalMetadata as empty object", () => {
      const workflow: WorkflowTemplate = {
        id: "test",
        name: "Test",
        description: "Test",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [{ id: "start", type: "START", name: "Start", config: {} }],
        edges: [],
      };

      const graph = WorkflowGraphBuilder.build(workflow);
      const startNode = graph.getNode("start");
      expect(startNode?.internalMetadata).toEqual({});
    });
  });

  describe("buildAndValidate", () => {
    it("should build and validate a valid workflow", () => {
      const workflow: WorkflowTemplate = {
        id: "valid-workflow",
        name: "Valid Workflow",
        description: "A valid workflow",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: "start", type: "START", name: "Start", config: {} },
          { id: "end", type: "END", name: "End", config: {} },
        ],
        edges: [{ id: "e1", sourceNodeId: "start", targetNodeId: "end", type: "DEFAULT" }],
      };

      const result = WorkflowGraphBuilder.buildAndValidate(workflow);

      expect(result.isValid).toBe(true);
      expect(result.errors).toHaveLength(0);
      expect(result.graph).toBeDefined();
      expect(result.graph.nodes.size).toBe(2);
    });

    it("should detect invalid workflow (missing END node)", () => {
      const workflow: WorkflowTemplate = {
        id: "invalid-workflow",
        name: "Invalid Workflow",
        description: "Missing END node",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [{ id: "start", type: "START", name: "Start", config: {} }],
        edges: [],
      };

      const result = WorkflowGraphBuilder.buildAndValidate(workflow);

      expect(result.isValid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
      expect(result.errors.some(e => e.includes("END"))).toBe(true);
    });
  });
});
