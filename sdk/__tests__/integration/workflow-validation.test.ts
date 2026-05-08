import { describe, it, expect } from "vitest";
import { WorkflowValidator } from "../../workflow/validation/workflow-validator.js";
import type { WorkflowTemplate, Node, Edge } from "@wf-agent/types";

import { now } from "@wf-agent/common-utils";

const createNode = (id: string, name: string, type: string): any => {
  return {
    id,
    name,
    type,
    outgoingEdgeIds: [],
    incomingEdgeIds: [],
  };
};

const createEdge = (id: string, source: string, target: string): any => {
  return {
    id,
    sourceNodeId: source,
    targetNodeId: target,
    type: "NORMAL",
  };
};

describe("SDK Workflow Validation Integration Tests", () => {
  const validator = new WorkflowValidator();

  const createBaseWorkflow = (overrides: Partial<WorkflowTemplate> = {}): WorkflowTemplate => ({
    id: "test-wf",
    name: "Test Workflow",
    type: "STANDALONE",
    version: "1.0.0",
    createdAt: now(),
    updatedAt: now(),
    nodes: [createNode("start", "Start", "START"), createNode("end", "End", "END")],
    edges: [createEdge("e1", "start", "end")],
    ...overrides,
  });

  describe("Node Type Validation", () => {
    it("should fail for invalid node types", () => {
      const workflow = createBaseWorkflow({
        nodes: [createNode("start", "Start", "INVALID_TYPE")],
      });
      const result = validator.validate(workflow);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some((e: any) => e.message.includes("Unknown node type"))).toBe(true);
      }
    });
  });

  describe("Node ID Uniqueness", () => {
    it("should fail for duplicate node IDs", () => {
      const workflow = createBaseWorkflow({
        nodes: [
          createNode("node1", "Node 1", "SCRIPT"),
          createNode("node1", "Node 2", "SCRIPT"),
        ],
      });
      const result = validator.validate(workflow);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some((e: any) => e.message.includes("Node ID must be unique"))).toBe(true);
      }
    });
  });

  describe("Edge Reference Integrity", () => {
    it("should fail for invalid edge references", () => {
      const workflow = createBaseWorkflow({
        nodes: [createNode("start", "Start", "START")],
        edges: [createEdge("e1", "start", "nonexistent")],
      });
      const result = validator.validate(workflow);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some((e: any) => e.message.includes("Edge target node not found"))).toBe(true);
      }
    });
  });

  describe("Graph Topology Rules", () => {
    it("should fail for multiple START nodes in STANDALONE workflow", () => {
      const workflow = createBaseWorkflow({
        nodes: [
          createNode("start1", "Start 1", "START"),
          createNode("start2", "Start 2", "START"),
          createNode("end", "End", "END"),
        ],
      });
      const result = validator.validate(workflow);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some((e: any) => e.message.includes("exactly one START node"))).toBe(true);
      }
    });

    it("should fail for missing END node in STANDALONE workflow", () => {
      const workflow = createBaseWorkflow({
        nodes: [createNode("start", "Start", "START")],
        edges: [],
      });
      const result = validator.validate(workflow);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some((e: any) => e.message.includes("at least one END node"))).toBe(true);
      }
    });
  });

  describe("Workflow Type Matching", () => {
    it("should fail if TRIGGERED_SUBWORKFLOW lacks START_FROM_TRIGGER", () => {
      const workflow = createBaseWorkflow({
        type: "TRIGGERED_SUBWORKFLOW",
        nodes: [
          createNode("start", "Start", "START_FROM_TRIGGER"),
          // Missing CONTINUE_FROM_TRIGGER
        ],
      });
      const result = validator.validate(workflow);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some((e: any) => e.message.includes("CONTINUE_FROM_TRIGGER"))).toBe(true);
      }
    });

    it("should fail if STANDALONE contains SUBGRAPH node", () => {
      const workflow = createBaseWorkflow({
        nodes: [
          createNode("start", "Start", "START"),
          createNode("sub", "Sub", "SUBGRAPH"),
          createNode("end", "End", "END"),
        ],
      });
      const result = validator.validate(workflow);
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some((e: any) => e.message.includes("should not contain SUBGRAPH"))).toBe(true);
      }
    });
  });
});
