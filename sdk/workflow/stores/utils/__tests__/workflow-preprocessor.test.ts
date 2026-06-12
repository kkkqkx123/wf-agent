/**
 * Tests for workflow-preprocessor
 */

import { describe, it, expect, vi, beforeEach, Mock } from "vitest";
import { preprocessWorkflow } from "../workflow-preprocessor.js";
import type { WorkflowTemplate } from "@wf-agent/types";
import type { WorkflowRegistry } from "../../workflow-registry.js";
import type { WorkflowGraphRegistry } from "../../workflow-graph-registry.js";
import type { WorkflowRelationshipRegistry } from "../../workflow-relationship-registry.js";

// Mock WorkflowGraphBuilder
vi.mock("../../../builder/workflow-graph-builder.js", () => ({
  WorkflowGraphBuilder: {
    buildAndValidate: vi.fn(),
    processSubgraphs: vi.fn(),
  },
}));

import { WorkflowGraphBuilder } from "../../../builder/workflow-graph-builder.js";

function createMockWorkflow(id: string): WorkflowTemplate {
  return {
    id,
    name: `Workflow ${id}`,
    type: "WORKFLOW",
    version: "1.0",
    nodes: [],
    edges: [],
    createdAt: Date.now(),
    updatedAt: Date.now(),
  } as unknown as WorkflowTemplate;
}

function createMockDeps() {
  return {
    workflowRegistry: {
      get: vi.fn(),
    } as unknown as WorkflowRegistry,
    graphRegistry: {
      has: vi.fn().mockReturnValue(false),
      register: vi.fn(),
    } as unknown as WorkflowGraphRegistry,
    relationshipRegistry: {
      registerSubgraphRelationship: vi.fn(),
    } as unknown as WorkflowRelationshipRegistry,
  };
}

describe("preprocessWorkflow", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should skip preprocessing if graph already exists", async () => {
    const workflow = createMockWorkflow("wf-1");
    const deps = createMockDeps();
    (deps.graphRegistry.has as Mock).mockReturnValue(true);

    await preprocessWorkflow(workflow, deps);

    expect(WorkflowGraphBuilder.buildAndValidate).not.toHaveBeenCalled();
    expect(deps.graphRegistry.register).not.toHaveBeenCalled();
  });

  it("should build and validate the workflow graph", async () => {
    const workflow = createMockWorkflow("wf-1");
    const deps = createMockDeps();

    const mockGraph = {
      workflowId: "wf-1",
      workflowVersion: "1.0",
    };

    (WorkflowGraphBuilder.buildAndValidate as Mock).mockReturnValue({
      graph: mockGraph,
      isValid: true,
      errors: [],
    });
    (WorkflowGraphBuilder.processSubgraphs as Mock).mockResolvedValue({
      success: true,
      errors: [],
    });

    await preprocessWorkflow(workflow, deps);

    expect(WorkflowGraphBuilder.buildAndValidate).toHaveBeenCalledWith(workflow);
    expect(deps.graphRegistry.register).toHaveBeenCalledWith(mockGraph);
  });

  it("should throw when build validation fails", async () => {
    const workflow = createMockWorkflow("wf-1");
    const deps = createMockDeps();

    (WorkflowGraphBuilder.buildAndValidate as Mock).mockReturnValue({
      graph: null,
      isValid: false,
      errors: ["Invalid graph structure"],
    });

    await expect(preprocessWorkflow(workflow, deps)).rejects.toThrow(
      "Workflow validation failed: Invalid graph structure",
    );
  });

  it("should throw when subgraph processing fails", async () => {
    const workflow = createMockWorkflow("wf-1");
    const deps = createMockDeps();

    const mockGraph = {
      workflowId: "wf-1",
      workflowVersion: "1.0",
    };

    (WorkflowGraphBuilder.buildAndValidate as Mock).mockReturnValue({
      graph: mockGraph,
      isValid: true,
      errors: [],
    });
    (WorkflowGraphBuilder.processSubgraphs as Mock).mockResolvedValue({
      success: false,
      errors: ["Subgraph merge failed"],
    });

    await expect(preprocessWorkflow(workflow, deps)).rejects.toThrow(
      "EMBED_GRAPH expansion failed for workflow 'wf-1': Subgraph merge failed",
    );
  });

  it("should recursively preprocess EMBED_GRAPH dependents", async () => {
    const childWorkflow = createMockWorkflow("child-wf");
    const parentWorkflow = {
      ...createMockWorkflow("parent-wf"),
      nodes: [
        {
          id: "node-1",
          name: "Embedded Graph",
          type: "EMBED_GRAPH" as const,
          config: { embedId: "child-wf" },
        },
      ],
    };

    const deps = createMockDeps();

    // First call: parent workflow, graphRegistry.has returns false for parent
    // Second call (recursive): child workflow, graphRegistry.has returns false for child
    (deps.graphRegistry.has as Mock)
      .mockReturnValueOnce(false) // parent check
      .mockReturnValueOnce(false); // child check (recursive)

    (deps.workflowRegistry.get as Mock).mockReturnValue(childWorkflow);

    const mockGraph = {
      workflowId: "parent-wf",
      workflowVersion: "1.0",
    };

    (WorkflowGraphBuilder.buildAndValidate as Mock).mockReturnValue({
      graph: mockGraph,
      isValid: true,
      errors: [],
    });
    (WorkflowGraphBuilder.processSubgraphs as Mock).mockResolvedValue({
      success: true,
      errors: [],
    });

    await preprocessWorkflow(parentWorkflow, deps);

    // Should have looked up the child workflow
    expect(deps.workflowRegistry.get).toHaveBeenCalledWith("child-wf");
    // buildAndValidate should have been called for the parent
    expect(WorkflowGraphBuilder.buildAndValidate).toHaveBeenCalledWith(parentWorkflow);
  });

  it("should skip recursive preprocessing if subworkflow already preprocessed", async () => {
    const parentWorkflow = {
      ...createMockWorkflow("parent-wf"),
      nodes: [
        {
          id: "node-1",
          name: "Embedded Graph",
          type: "EMBED_GRAPH" as const,
          config: { embedId: "child-wf" },
        },
      ],
    };

    const deps = createMockDeps();

    // Parent not yet preprocessed, child already preprocessed
    (deps.graphRegistry.has as Mock)
      .mockReturnValueOnce(false) // parent check
      .mockReturnValueOnce(true); // child already exists

    const mockGraph = {
      workflowId: "parent-wf",
      workflowVersion: "1.0",
    };

    (WorkflowGraphBuilder.buildAndValidate as Mock).mockReturnValue({
      graph: mockGraph,
      isValid: true,
      errors: [],
    });
    (WorkflowGraphBuilder.processSubgraphs as Mock).mockResolvedValue({
      success: true,
      errors: [],
    });

    await preprocessWorkflow(parentWorkflow, deps);

    // Should not have tried to get the child workflow since it's already preprocessed
    expect(deps.workflowRegistry.get).not.toHaveBeenCalled();
  });
});