/**
 * Graph Validator Unit Tests
 * Tests for GraphValidator class functionality
 */

import { describe, it, expect } from "vitest";
import { WorkflowGraphBuilder } from "../workflow-graph-builder.js";
import { GraphValidator } from "../../validation/graph-validation/graph-validator.js";
import type { WorkflowTemplate } from "@wf-agent/types";

describe("GraphValidator", () => {
  describe("validate - START/END nodes", () => {
    it("should validate a workflow with valid START and END nodes", () => {
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

      const graph = WorkflowGraphBuilder.build(workflow);
      const result = GraphValidator.validate(graph);

      expect(result.isOk()).toBe(true);
    });

    it("should fail validation when START node is missing", () => {
      const workflow: WorkflowTemplate = {
        id: "no-start-workflow",
        name: "No Start Workflow",
        description: "Missing START node",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [{ id: "end", type: "END", name: "End", config: {} }],
        edges: [],
      };

      const graph = WorkflowGraphBuilder.build(workflow);
      const result = GraphValidator.validate(graph);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some(e => e.message.includes("START"))).toBe(true);
      }
    });

    it("should fail validation when END node is missing", () => {
      const workflow: WorkflowTemplate = {
        id: "no-end-workflow",
        name: "No End Workflow",
        description: "Missing END node",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [{ id: "start", type: "START", name: "Start", config: {} }],
        edges: [],
      };

      const graph = WorkflowGraphBuilder.build(workflow);
      const result = GraphValidator.validate(graph);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some(e => e.message.includes("END"))).toBe(true);
      }
    });

    it("should fail validation when START node has incoming edges", () => {
      const workflow: WorkflowTemplate = {
        id: "start-with-incoming",
        name: "Start With Incoming",
        description: "START node has incoming edges",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: "start", type: "START", name: "Start", config: {} },
          { id: "node-a", type: "LLM", name: "Node A", config: {} as any },
          { id: "end", type: "END", name: "End", config: {} },
        ],
        edges: [
          { id: "e1", sourceNodeId: "node-a", targetNodeId: "start", type: "DEFAULT" },
          { id: "e2", sourceNodeId: "start", targetNodeId: "end", type: "DEFAULT" },
        ],
      };

      const graph = WorkflowGraphBuilder.build(workflow);
      const result = GraphValidator.validate(graph);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some(e => e.message.includes("incoming"))).toBe(true);
      }
    });

    it("should fail validation when END node has outgoing edges", () => {
      const workflow: WorkflowTemplate = {
        id: "end-with-outgoing",
        name: "End With Outgoing",
        description: "END node has outgoing edges",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: "start", type: "START", name: "Start", config: {} },
          { id: "end", type: "END", name: "End", config: {} },
          { id: "node-a", type: "LLM", name: "Node A", config: {} as any },
        ],
        edges: [
          { id: "e1", sourceNodeId: "start", targetNodeId: "end", type: "DEFAULT" },
          { id: "e2", sourceNodeId: "end", targetNodeId: "node-a", type: "DEFAULT" },
        ],
      };

      const graph = WorkflowGraphBuilder.build(workflow);
      const result = GraphValidator.validate(graph);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some(e => e.message.includes("outgoing"))).toBe(true);
      }
    });
  });

  describe("validate - isolated nodes", () => {
    it("should detect isolated nodes", () => {
      const workflow: WorkflowTemplate = {
        id: "isolated-node-workflow",
        name: "Isolated Node Workflow",
        description: "Has isolated node",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: "start", type: "START", name: "Start", config: {} },
          { id: "isolated", type: "LLM", name: "Isolated", config: {} as any },
          { id: "end", type: "END", name: "End", config: {} },
        ],
        edges: [{ id: "e1", sourceNodeId: "start", targetNodeId: "end", type: "DEFAULT" }],
      };

      const graph = WorkflowGraphBuilder.build(workflow);
      const result = GraphValidator.validate(graph);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.some(e => e.message.includes("isolated"))).toBe(true);
      }
    });
  });

  describe("validate - FORK/JOIN pairs", () => {
    it("should validate correct FORK/JOIN pairing", () => {
      const workflow: WorkflowTemplate = {
        id: "fork-join-workflow",
        name: "Fork Join Workflow",
        description: "Valid fork join",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: "start", type: "START", name: "Start", config: {} },
          {
            id: "fork",
            type: "FORK",
            name: "Fork",
            config: {
              forkPaths: [
                { pathId: "path-1", childNodeId: "node-a" },
                { pathId: "path-2", childNodeId: "node-b" },
              ],
            } as any,
          },
          { id: "node-a", type: "LLM", name: "Node A", config: {} as any },
          { id: "node-b", type: "LLM", name: "Node B", config: {} },
          {
            id: "join",
            type: "JOIN",
            name: "Join",
            config: {
              forkPathIds: ["path-1", "path-2"],
            } as any,
          },
          { id: "end", type: "END", name: "End", config: {} },
        ],
        edges: [
          { id: "e1", sourceNodeId: "start", targetNodeId: "fork", type: "DEFAULT" },
          { id: "e2", sourceNodeId: "fork", targetNodeId: "node-a", type: "DEFAULT" },
          { id: "e3", sourceNodeId: "fork", targetNodeId: "node-b", type: "DEFAULT" },
          { id: "e4", sourceNodeId: "node-a", targetNodeId: "join", type: "DEFAULT" },
          { id: "e5", sourceNodeId: "node-b", targetNodeId: "join", type: "DEFAULT" },
          { id: "e6", sourceNodeId: "join", targetNodeId: "end", type: "DEFAULT" },
        ],
      };

      const graph = WorkflowGraphBuilder.build(workflow);
      const result = GraphValidator.validate(graph);

      expect(result.isOk()).toBe(true);
    });

    it("should detect unpaired FORK node", () => {
      const workflow: WorkflowTemplate = {
        id: "unpaired-fork",
        name: "Unpaired Fork",
        description: "FORK without JOIN",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: "start", type: "START", name: "Start", config: {} },
          {
            id: "fork",
            type: "FORK",
            name: "Fork",
            config: {
              forkPaths: [{ pathId: "path-1", childNodeId: "node-a" }],
            } as any,
          },
          { id: "node-a", type: "LLM", name: "Node A", config: {} as any },
          { id: "end", type: "END", name: "End", config: {} },
        ],
        edges: [
          { id: "e1", sourceNodeId: "start", targetNodeId: "fork", type: "DEFAULT" },
          { id: "e2", sourceNodeId: "fork", targetNodeId: "node-a", type: "DEFAULT" },
          { id: "e3", sourceNodeId: "node-a", targetNodeId: "end", type: "DEFAULT" },
        ],
      };

      const graph = WorkflowGraphBuilder.build(workflow);
      const result = GraphValidator.validate(graph);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(
          result.error.some(e => e.message.includes("FORK") && e.message.includes("matching JOIN")),
        ).toBe(true);
      }
    });
  });

  describe("validate - SYNC nodes", () => {
    it("should validate a workflow with valid SYNC node in fork-join structure", () => {
      const workflow: WorkflowTemplate = {
        id: "sync-workflow",
        name: "Sync Workflow",
        description: "Workflow with SYNC node",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: "start", type: "START", name: "Start", config: {} },
          {
            id: "fork",
            type: "FORK",
            name: "Fork",
            config: {
              forkPaths: [
                { pathId: "path-a", childNodeId: "node-a" },
                { pathId: "path-b", childNodeId: "node-b" },
              ],
              forkStrategy: "parallel",
            } as any,
          },
          { id: "node-a", type: "LLM", name: "Node A", config: {} as any },
          { id: "node-b", type: "LLM", name: "Node B", config: {} as any },
          {
            id: "sync-node",
            type: "SYNC",
            name: "Sync Node",
            config: {
              sourcePathId: "path-a",
              targetPathId: "path-b",
              variableMappings: [{ externalName: "result_a", internalName: "synced_result" }],
            } as any,
          },
          {
            id: "join",
            type: "JOIN",
            name: "Join",
            config: {
              forkPathIds: ["path-a", "path-b"],
              mainPathId: "path-a",
            } as any,
          },
          { id: "end", type: "END", name: "End", config: {} },
        ],
        edges: [
          { id: "e1", sourceNodeId: "start", targetNodeId: "fork", type: "DEFAULT" },
          { id: "e2", sourceNodeId: "fork", targetNodeId: "node-a", type: "DEFAULT" },
          { id: "e3", sourceNodeId: "fork", targetNodeId: "node-b", type: "DEFAULT" },
          { id: "e4", sourceNodeId: "node-a", targetNodeId: "sync-node", type: "DEFAULT" },
          { id: "e5", sourceNodeId: "node-b", targetNodeId: "sync-node", type: "DEFAULT" },
          { id: "e6", sourceNodeId: "sync-node", targetNodeId: "join", type: "DEFAULT" },
          { id: "e7", sourceNodeId: "join", targetNodeId: "end", type: "DEFAULT" },
        ],
      };

      const graph = WorkflowGraphBuilder.build(workflow);
      const result = GraphValidator.validate(graph);

      expect(result.isOk()).toBe(true);
    });

    it("should fail validation when SYNC node has missing sourcePathId", () => {
      const workflow: WorkflowTemplate = {
        id: "sync-missing-source",
        name: "Sync Missing Source",
        description: "SYNC node without sourcePathId",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: "start", type: "START", name: "Start", config: {} },
          {
            id: "fork",
            type: "FORK",
            name: "Fork",
            config: {
              forkPaths: [
                { pathId: "path-a", childNodeId: "node-a" },
                { pathId: "path-b", childNodeId: "node-b" },
              ],
              forkStrategy: "parallel",
            } as any,
          },
          { id: "node-a", type: "LLM", name: "Node A", config: {} as any },
          { id: "node-b", type: "LLM", name: "Node B", config: {} as any },
          {
            id: "sync-node",
            type: "SYNC",
            name: "Sync Node",
            config: {
              targetPathId: "path-b",
            } as any,
          },
          {
            id: "join",
            type: "JOIN",
            name: "Join",
            config: {
              forkPathIds: ["path-a", "path-b"],
              mainPathId: "path-a",
            } as any,
          },
          { id: "end", type: "END", name: "End", config: {} },
        ],
        edges: [
          { id: "e1", sourceNodeId: "start", targetNodeId: "fork", type: "DEFAULT" },
          { id: "e2", sourceNodeId: "fork", targetNodeId: "node-a", type: "DEFAULT" },
          { id: "e3", sourceNodeId: "fork", targetNodeId: "node-b", type: "DEFAULT" },
          { id: "e4", sourceNodeId: "node-a", targetNodeId: "sync-node", type: "DEFAULT" },
          { id: "e5", sourceNodeId: "node-b", targetNodeId: "sync-node", type: "DEFAULT" },
          { id: "e6", sourceNodeId: "sync-node", targetNodeId: "join", type: "DEFAULT" },
          { id: "e7", sourceNodeId: "join", targetNodeId: "end", type: "DEFAULT" },
        ],
      };

      const graph = WorkflowGraphBuilder.build(workflow);
      const result = GraphValidator.validate(graph);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(
          result.error.some(e => e.message.includes("SYNC") && e.message.includes("sourcePathId")),
        ).toBe(true);
      }
    });

    it("should fail validation when SYNC node has invalid sourcePathId", () => {
      const workflow: WorkflowTemplate = {
        id: "sync-invalid-source",
        name: "Sync Invalid Source",
        description: "SYNC node with non-existent sourcePathId",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: "start", type: "START", name: "Start", config: {} },
          {
            id: "fork",
            type: "FORK",
            name: "Fork",
            config: {
              forkPaths: [
                { pathId: "path-a", childNodeId: "node-a" },
                { pathId: "path-b", childNodeId: "node-b" },
              ],
              forkStrategy: "parallel",
            } as any,
          },
          { id: "node-a", type: "LLM", name: "Node A", config: {} as any },
          { id: "node-b", type: "LLM", name: "Node B", config: {} as any },
          {
            id: "sync-node",
            type: "SYNC",
            name: "Sync Node",
            config: {
              sourcePathId: "non-existent-path",
              targetPathId: "path-b",
            } as any,
          },
          {
            id: "join",
            type: "JOIN",
            name: "Join",
            config: {
              forkPathIds: ["path-a", "path-b"],
              mainPathId: "path-a",
            } as any,
          },
          { id: "end", type: "END", name: "End", config: {} },
        ],
        edges: [
          { id: "e1", sourceNodeId: "start", targetNodeId: "fork", type: "DEFAULT" },
          { id: "e2", sourceNodeId: "fork", targetNodeId: "node-a", type: "DEFAULT" },
          { id: "e3", sourceNodeId: "fork", targetNodeId: "node-b", type: "DEFAULT" },
          { id: "e4", sourceNodeId: "node-a", targetNodeId: "sync-node", type: "DEFAULT" },
          { id: "e5", sourceNodeId: "node-b", targetNodeId: "sync-node", type: "DEFAULT" },
          { id: "e6", sourceNodeId: "sync-node", targetNodeId: "join", type: "DEFAULT" },
          { id: "e7", sourceNodeId: "join", targetNodeId: "end", type: "DEFAULT" },
        ],
      };

      const graph = WorkflowGraphBuilder.build(workflow);
      const result = GraphValidator.validate(graph);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(
          result.error.some(e => e.message.includes("SYNC") && e.message.includes("sourcePathId")),
        ).toBe(true);
      }
    });

    it("should fail validation when SYNC node is isolated", () => {
      const workflow: WorkflowTemplate = {
        id: "sync-isolated",
        name: "Sync Isolated",
        description: "Isolated SYNC node",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: "start", type: "START", name: "Start", config: {} },
          {
            id: "fork",
            type: "FORK",
            name: "Fork",
            config: {
              forkPaths: [
                { pathId: "path-a", childNodeId: "node-a" },
                { pathId: "path-b", childNodeId: "node-b" },
              ],
              forkStrategy: "parallel",
            } as any,
          },
          { id: "node-a", type: "LLM", name: "Node A", config: {} as any },
          { id: "node-b", type: "LLM", name: "Node B", config: {} as any },
          {
            id: "sync-node",
            type: "SYNC",
            name: "Sync Node",
            config: {
              sourcePathId: "path-a",
              targetPathId: "path-b",
            } as any,
          },
          {
            id: "join",
            type: "JOIN",
            name: "Join",
            config: {
              forkPathIds: ["path-a", "path-b"],
              mainPathId: "path-a",
            } as any,
          },
          { id: "end", type: "END", name: "End", config: {} },
        ],
        edges: [
          { id: "e1", sourceNodeId: "start", targetNodeId: "fork", type: "DEFAULT" },
          { id: "e2", sourceNodeId: "fork", targetNodeId: "node-a", type: "DEFAULT" },
          { id: "e3", sourceNodeId: "fork", targetNodeId: "node-b", type: "DEFAULT" },
          // sync-node has no edges - isolated
          { id: "e4", sourceNodeId: "node-a", targetNodeId: "join", type: "DEFAULT" },
          { id: "e5", sourceNodeId: "node-b", targetNodeId: "join", type: "DEFAULT" },
          { id: "e6", sourceNodeId: "join", targetNodeId: "end", type: "DEFAULT" },
        ],
      };

      const graph = WorkflowGraphBuilder.build(workflow);
      const result = GraphValidator.validate(graph);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(
          result.error.some(e => e.message.includes("SYNC") && e.message.includes("isolated")),
        ).toBe(true);
      }
    });

    it("should fail validation when SYNC node has invalid variableMapping", () => {
      const workflow: WorkflowTemplate = {
        id: "sync-invalid-mapping",
        name: "Sync Invalid Mapping",
        description: "SYNC node with invalid variable mapping",
        version: "1.0.0",
        type: "STANDALONE",
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodes: [
          { id: "start", type: "START", name: "Start", config: {} },
          {
            id: "fork",
            type: "FORK",
            name: "Fork",
            config: {
              forkPaths: [
                { pathId: "path-a", childNodeId: "node-a" },
                { pathId: "path-b", childNodeId: "node-b" },
              ],
              forkStrategy: "parallel",
            } as any,
          },
          { id: "node-a", type: "LLM", name: "Node A", config: {} as any },
          { id: "node-b", type: "LLM", name: "Node B", config: {} as any },
          {
            id: "sync-node",
            type: "SYNC",
            name: "Sync Node",
            config: {
              sourcePathId: "path-a",
              targetPathId: "path-b",
              variableMappings: [
                { externalName: "", internalName: "target_var" }, // Empty externalName
              ],
            } as any,
          },
          {
            id: "join",
            type: "JOIN",
            name: "Join",
            config: {
              forkPathIds: ["path-a", "path-b"],
              mainPathId: "path-a",
            } as any,
          },
          { id: "end", type: "END", name: "End", config: {} },
        ],
        edges: [
          { id: "e1", sourceNodeId: "start", targetNodeId: "fork", type: "DEFAULT" },
          { id: "e2", sourceNodeId: "fork", targetNodeId: "node-a", type: "DEFAULT" },
          { id: "e3", sourceNodeId: "fork", targetNodeId: "node-b", type: "DEFAULT" },
          { id: "e4", sourceNodeId: "node-a", targetNodeId: "sync-node", type: "DEFAULT" },
          { id: "e5", sourceNodeId: "node-b", targetNodeId: "sync-node", type: "DEFAULT" },
          { id: "e6", sourceNodeId: "sync-node", targetNodeId: "join", type: "DEFAULT" },
          { id: "e7", sourceNodeId: "join", targetNodeId: "end", type: "DEFAULT" },
        ],
      };

      const graph = WorkflowGraphBuilder.build(workflow);
      const result = GraphValidator.validate(graph);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(
          result.error.some(e => e.message.includes("SYNC") && e.message.includes("externalName")),
        ).toBe(true);
      }
    });
  });
});
