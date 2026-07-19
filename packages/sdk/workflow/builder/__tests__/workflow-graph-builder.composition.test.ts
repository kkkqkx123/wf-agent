/**
 * Integration Tests: WorkflowGraph Builder with Composition Pattern
 *
 * Tests the WorkflowGraphBuilder's integration with the new composition-based
 * WorkflowGraph design, ensuring proper separation of structure and metadata.
 */

import { WorkflowGraphBuilder } from "../workflow-graph-builder.js";
import { WorkflowGraphStructureImpl } from "../../entities/workflow-graph-structure.js";
import { WorkflowGraphMetadata } from "../../entities/workflow-graph-metadata.js";
import type { WorkflowTemplate, WorkflowTemplateType } from "@wf-agent/types";

describe("WorkflowGraphBuilder with Composition Pattern", () => {
  const createSimpleWorkflow = (): WorkflowTemplate => ({
    id: "test-workflow",
    name: "Test Workflow",
    version: "1.0.0",
    description: "A test workflow",
    type: "STANDALONE",
    nodes: [
      {
        id: "start",
        name: "Start",
        type: "START",
        config: {},
      },
      {
        id: "llm1",
        name: "LLM Node",
        type: "LLM",
        config: { profileId: "test-profile" },
      },
      {
        id: "end",
        name: "End",
        type: "END",
        config: {},
      },
    ],
    edges: [
      {
        id: "edge1",
        sourceNodeId: "start",
        targetNodeId: "llm1",
        type: "DEFAULT",
      },
      {
        id: "edge2",
        sourceNodeId: "llm1",
        targetNodeId: "end",
        type: "DEFAULT",
      },
    ],
    variables: [],
    triggers: [],
    createdAt: Date.now(),
    updatedAt: Date.now(),
  });

  describe("Build Method", () => {
    test("should build WorkflowGraphStructure by default", () => {
      const workflow = createSimpleWorkflow();
      const graph = WorkflowGraphBuilder.build(workflow);

      expect(graph).toBeInstanceOf(WorkflowGraphStructureImpl);
      expect(graph.getAllNodeIds()).toHaveLength(3);
      expect(graph.getAllEdgeIds()).toHaveLength(2);
      expect(graph.startNodeId).toBe("start");
      expect(graph.endNodeIds.has("end")).toBe(true);
    });

    test("should build into existing structure when provided", () => {
      const workflow = createSimpleWorkflow();
      const existingStructure = new WorkflowGraphStructureImpl();
      const graph = WorkflowGraphBuilder.build(workflow, existingStructure);

      expect(graph).toBe(existingStructure);
      expect(graph.getAllNodeIds()).toHaveLength(3);
      expect(graph.getAllEdgeIds()).toHaveLength(2);
    });

    test("should handle nodes with metadata correctly", () => {
      const workflow = {
        ...createSimpleWorkflow(),
        nodes: [
          {
            id: "start",
            name: "Start",
            type: "START" as const,
            config: { variableInputs: ["input1"] },
          },
          {
            id: "llm1",
            name: "LLM Node",
            type: "LLM" as const,
            config: {
              profileId: "test-profile",
              checkpointBeforeExecute: true,
              checkpointAfterExecute: true,
            },
          },
          {
            id: "end",
            name: "End",
            type: "END" as const,
            config: { variableOutputs: ["output1"] },
          },
        ],
      } as any;

      const graph = WorkflowGraphBuilder.build(workflow);

      expect(graph.hasNode("start")).toBe(true);
      expect(graph.hasNode("llm1")).toBe(true);
      expect(graph.hasNode("end")).toBe(true);

      const startNode = graph.getNode("start");
      expect(startNode?.config).toEqual({ variableInputs: ["input1"] });

      const llmNode = graph.getNode("llm1");
      expect(llmNode?.config).toEqual({
        profileId: "test-profile",
        checkpointBeforeExecute: true,
        checkpointAfterExecute: true,
      });
    });

    test("should preserve edge properties", () => {
      const workflow = {
        ...createSimpleWorkflow(),
        edges: [
          {
            id: "edge1",
            sourceNodeId: "start",
            targetNodeId: "llm1",
            type: "CONDITIONAL" as const,
            label: "condition1",
            description: "Conditional edge",
            weight: 2,
          },
          {
            id: "edge2",
            sourceNodeId: "llm1",
            targetNodeId: "end",
            type: "DEFAULT" as const,
            label: "default",
            description: "Default edge",
            weight: 1,
          },
        ],
      } as any;

      const graph = WorkflowGraphBuilder.build(workflow);

      const edge1 = graph.getEdge("edge1");
      expect(edge1?.type).toBe("CONDITIONAL");
      expect(edge1?.label).toBe("condition1");
      expect(edge1?.description).toBe("Conditional edge");
      expect(edge1?.weight).toBe(2);

      const edge2 = graph.getEdge("edge2");
      expect(edge2?.type).toBe("DEFAULT");
      expect(edge2?.label).toBe("default");
      expect(edge2?.description).toBe("Default edge");
      expect(edge2?.weight).toBe(1);
    });
  });

  describe("BuildAndValidate Method", () => {
    test("should return WorkflowGraph with composition", () => {
      const workflow = createSimpleWorkflow();
      const result = WorkflowGraphBuilder.buildAndValidate(workflow);

      expect(result.graph).toBeDefined();
      expect(result.isValid).toBe(true);
      expect(result.errors).toHaveLength(0);

      // Verify the graph is a composition
      expect(result.graph.structure).toBeInstanceOf(WorkflowGraphStructureImpl);
      expect(result.graph.metadata).toBeInstanceOf(WorkflowGraphMetadata);
    });

    test("should set workflow metadata correctly", () => {
      const workflow = createSimpleWorkflow();
      const result = WorkflowGraphBuilder.buildAndValidate(workflow);

      expect(result.graph.workflowId).toBe("test-workflow");
      expect(result.graph.workflowVersion).toBe("1.0.0");
      expect(result.graph.metadata.workflowId).toBe("test-workflow");
      expect(result.graph.metadata.workflowVersion).toBe("1.0.0");
    });

    test("should validate workflow structure", () => {
      const invalidWorkflow: WorkflowTemplate = {
        id: "invalid-workflow",
        name: "Invalid Workflow",
        version: "1.0.0",
        description: "Invalid workflow without end node",
        type: "workflow" as WorkflowTemplateType,
        nodes: [
          {
            id: "start",
            name: "Start",
            type: "START",
            config: {},
          },
          {
            id: "llm1",
            name: "LLM Node",
            type: "LLM",
            config: { profileId: "test-profile" },
          },
          // Missing END node
        ],
        edges: [
          {
            id: "edge1",
            sourceNodeId: "start",
            targetNodeId: "llm1",
            type: "DEFAULT",
          },
        ],
        variables: [],
        triggers: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      const result = WorkflowGraphBuilder.buildAndValidate(invalidWorkflow);

      expect(result.isValid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
    });

    test("should handle complex workflows with multiple paths", () => {
      const complexWorkflow: WorkflowTemplate = {
        id: "complex-workflow",
        name: "Complex Workflow",
        version: "1.0.0",
        description: "Complex workflow with fork and join",
        type: "STANDALONE",
        nodes: [
          { id: "start", name: "Start", type: "START", config: {} },
          {
            id: "fork1",
            name: "Fork",
            type: "FORK",
            config: {
              forkPaths: [
                { pathId: "path1", childNodeId: "llm1" },
                { pathId: "path2", childNodeId: "llm2" },
              ],
              forkStrategy: "parallel",
            },
          },
          { id: "llm1", name: "LLM 1", type: "LLM", config: { profileId: "p1" } },
          { id: "llm2", name: "LLM 2", type: "LLM", config: { profileId: "p2" } },
          {
            id: "join1",
            name: "Join",
            type: "JOIN",
            config: { forkPathIds: ["path1", "path2"], joinStrategy: "ALL_COMPLETED", mainPathId: "path1" },
          },
          { id: "end", name: "End", type: "END", config: {} },
        ],
        edges: [
          { id: "e1", sourceNodeId: "start", targetNodeId: "fork1", type: "DEFAULT" },
          { id: "e2", sourceNodeId: "fork1", targetNodeId: "llm1", type: "DEFAULT", label: "path1" },
          { id: "e3", sourceNodeId: "fork1", targetNodeId: "llm2", type: "DEFAULT", label: "path2" },
          { id: "e4", sourceNodeId: "llm1", targetNodeId: "join1", type: "DEFAULT" },
          { id: "e5", sourceNodeId: "llm2", targetNodeId: "join1", type: "DEFAULT" },
          { id: "e6", sourceNodeId: "join1", targetNodeId: "end", type: "DEFAULT" },
        ],
        variables: [],
        triggers: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      const result = WorkflowGraphBuilder.buildAndValidate(complexWorkflow);

      expect(result.isValid).toBe(true);
      expect(result.errors).toHaveLength(0);
      expect(result.graph.getNodeCount()).toBe(6);
      expect(result.graph.getEdgeCount()).toBe(6);

      // Verify Fork/Join path ID processing
      const forkNode = result.graph.getNode("fork1");
      const forkConfig = forkNode?.originalNode?.config as any;
      expect(forkConfig.forkPaths).toHaveLength(2);
      expect(forkConfig.forkPaths[0].pathId).toMatch(/^path-/);
      expect(forkConfig.forkPaths[1].pathId).toMatch(/^path-/);

      const joinNode = result.graph.getNode("join1");
      const joinConfig = joinNode?.originalNode?.config as any;
      expect(joinConfig.forkPathIds).toHaveLength(2);
      expect(joinConfig.forkPathIds[0]).toMatch(/^path-/);
      expect(joinConfig.forkPathIds[1]).toMatch(/^path-/);
      expect(joinConfig.mainPathId).toMatch(/^path-/);
    });
  });

  describe("Graph Composition Benefits", () => {
    test("should maintain clear separation between structure and metadata", () => {
      const workflow = createSimpleWorkflow();
      const result = WorkflowGraphBuilder.buildAndValidate(workflow);

      // Structure should be immutable
      expect(result.graph.structure.nodes.size).toBe(3);
      expect(result.graph.structure.edges.size).toBe(2);

      // Metadata should be mutable
      result.graph.metadata.setNodeConfig("start", {
        id: "start",
        name: "Start",
        type: "START",
        config: { variableInputs: [{ sourcePath: "val", internalName: "v1" }] },
      });

      expect(result.graph.metadata.nodeConfigs.size).toBe(1);
      expect(result.graph.getNodeConfig("start")?.config).toEqual({ variableInputs: [{ sourcePath: "val", internalName: "v1" }] });
    });

    test("should support graph transformations while preserving metadata", () => {
      const workflow = createSimpleWorkflow();
      const result = WorkflowGraphBuilder.buildAndValidate(workflow);

      // Add some metadata
      result.graph.metadata.validationResult = {
        isValid: true,
        errors: [],
        warnings: [],
        validatedAt: Date.now(),
      };

      // Create new structure
      const newStructure = new WorkflowGraphStructureImpl();
      const newNode = { id: "newStart", workflowId: "test-wf", type: "START" as const, outgoingEdgeIds: [] as string[], incomingEdgeIds: [] as string[], config: {} };
      newStructure.addNode(newNode);

      // Transform graph
      const transformedGraph = result.graph.withStructure(newStructure);

      // Metadata should be preserved
      expect(transformedGraph.metadata.validationResult.isValid).toBe(true);
      expect(transformedGraph.metadata.workflowId).toBe("test-workflow");

      // Structure should be new
      expect(transformedGraph.structure).toBe(newStructure);
      expect(transformedGraph.hasNode("newStart")).toBe(true);
      expect(transformedGraph.hasNode("start")).toBe(false);
    });
  });
});