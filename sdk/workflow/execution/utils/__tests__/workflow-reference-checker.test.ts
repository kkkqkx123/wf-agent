/**
 * Workflow Reference Checker Unit Tests
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { checkWorkflowReferences } from "../workflow-reference-checker.js";
import type { WorkflowRegistry } from "../../../stores/workflow-registry.js";
import type { WorkflowExecutionRegistry } from "../../../stores/workflow-execution-registry.js";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { WorkflowTemplate, WorkflowTrigger, TriggerReference } from "@wf-agent/types";

describe("checkWorkflowReferences", () => {
  let mockWorkflowRegistry: WorkflowRegistry;
  let mockExecutionRegistry: WorkflowExecutionRegistry;

  beforeEach(() => {
    vi.clearAllMocks();

    mockWorkflowRegistry = {
      getParentWorkflow: vi.fn(),
      get: vi.fn(),
      getWorkflowHierarchy: vi.fn(),
      list: vi.fn(),
    } as unknown as WorkflowRegistry;

    mockExecutionRegistry = {
      isWorkflowActive: vi.fn(),
      getAll: vi.fn(),
    } as unknown as WorkflowExecutionRegistry;
  });

  describe("no references", () => {
    it("should return no references when workflow has no parent", async () => {
      (mockWorkflowRegistry.getParentWorkflow as ReturnType<typeof vi.fn>).mockReturnValue(null);
      (mockWorkflowRegistry.list as ReturnType<typeof vi.fn>).mockResolvedValue([]);
      (mockExecutionRegistry.isWorkflowActive as ReturnType<typeof vi.fn>).mockReturnValue(false);

      const result = await checkWorkflowReferences(
        mockWorkflowRegistry,
        mockExecutionRegistry,
        "workflow-1"
      );

      expect(result.hasReferences).toBe(false);
      expect(result.references).toHaveLength(0);
      expect(result.canSafelyDelete).toBe(true);
    });
  });

  describe("subgraph references", () => {
    it("should detect parent workflow reference", async () => {
      const parentWorkflow: WorkflowTemplate = {
        id: "parent-workflow",
        name: "Parent Workflow",
        type: "WORKFLOW",
        version: "1.0",
        nodes: [],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      } as unknown as WorkflowTemplate;

      (mockWorkflowRegistry.getParentWorkflow as ReturnType<typeof vi.fn>).mockReturnValue("parent-workflow");
      (mockWorkflowRegistry.get as ReturnType<typeof vi.fn>).mockReturnValue(parentWorkflow);
      (mockWorkflowRegistry.getWorkflowHierarchy as ReturnType<typeof vi.fn>).mockReturnValue({
        depth: 1,
      });
      (mockWorkflowRegistry.list as ReturnType<typeof vi.fn>).mockResolvedValue([]);
      (mockExecutionRegistry.isWorkflowActive as ReturnType<typeof vi.fn>).mockReturnValue(false);

      const result = await checkWorkflowReferences(
        mockWorkflowRegistry,
        mockExecutionRegistry,
        "workflow-1"
      );

      expect(result.hasReferences).toBe(true);
      expect(result.references).toHaveLength(1);
      expect(result.references[0]!.type).toBe("subgraph");
      expect(result.references[0]!.sourceId).toBe("parent-workflow");
      expect(result.references[0]!.isRuntimeReference).toBe(false);
      expect(result.canSafelyDelete).toBe(true);
    });

    it("should handle parent workflow not found", async () => {
      (mockWorkflowRegistry.getParentWorkflow as ReturnType<typeof vi.fn>).mockReturnValue("parent-workflow");
      (mockWorkflowRegistry.get as ReturnType<typeof vi.fn>).mockReturnValue(null);
      (mockWorkflowRegistry.list as ReturnType<typeof vi.fn>).mockResolvedValue([]);
      (mockExecutionRegistry.isWorkflowActive as ReturnType<typeof vi.fn>).mockReturnValue(false);

      const result = await checkWorkflowReferences(
        mockWorkflowRegistry,
        mockExecutionRegistry,
        "workflow-1"
      );

      expect(result.hasReferences).toBe(false);
      expect(result.references).toHaveLength(0);
    });
  });

  describe("trigger references", () => {
    it("should detect trigger referencing workflow via WorkflowTrigger", async () => {
      const workflowWithTrigger: WorkflowTemplate = {
        id: "trigger-workflow",
        name: "Trigger Workflow",
        type: "WORKFLOW",
        version: "1.0",
        nodes: [],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
        triggers: [
          {
            id: "trigger-1",
            name: "Trigger One",
            action: {
              type: "execute_triggered_subworkflow",
              parameters: {
                triggeredWorkflowId: "workflow-1",
              },
            },
          } as WorkflowTrigger,
        ],
      } as unknown as WorkflowTemplate;

      (mockWorkflowRegistry.getParentWorkflow as ReturnType<typeof vi.fn>).mockReturnValue(null);
      (mockWorkflowRegistry.list as ReturnType<typeof vi.fn>).mockResolvedValue([
        { id: "trigger-workflow", name: "Trigger Workflow" },
      ]);
      (mockWorkflowRegistry.get as ReturnType<typeof vi.fn>).mockReturnValue(workflowWithTrigger);
      (mockExecutionRegistry.isWorkflowActive as ReturnType<typeof vi.fn>).mockReturnValue(false);

      const result = await checkWorkflowReferences(
        mockWorkflowRegistry,
        mockExecutionRegistry,
        "workflow-1"
      );

      expect(result.hasReferences).toBe(true);
      expect(result.references).toHaveLength(1);
      expect(result.references[0]!.type).toBe("trigger");
      expect(result.references[0]!.isRuntimeReference).toBe(false);
    });

    it("should detect trigger referencing workflow via TriggerReference", async () => {
      const workflowWithTriggerRef: WorkflowTemplate = {
        id: "trigger-ref-workflow",
        name: "Trigger Ref Workflow",
        type: "WORKFLOW",
        version: "1.0",
        nodes: [],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
        triggers: [
          {
            templateName: "trigger-template",
            triggerId: "trigger-2",
            triggerName: "Trigger Two",
            configOverride: {
              action: {
                type: "execute_triggered_subworkflow",
                parameters: {
                  triggeredWorkflowId: "workflow-1",
                },
              },
            },
          } as TriggerReference,
        ],
      } as unknown as WorkflowTemplate;

      (mockWorkflowRegistry.getParentWorkflow as ReturnType<typeof vi.fn>).mockReturnValue(null);
      (mockWorkflowRegistry.list as ReturnType<typeof vi.fn>).mockResolvedValue([
        { id: "trigger-ref-workflow", name: "Trigger Ref Workflow" },
      ]);
      (mockWorkflowRegistry.get as ReturnType<typeof vi.fn>).mockReturnValue(workflowWithTriggerRef);
      (mockExecutionRegistry.isWorkflowActive as ReturnType<typeof vi.fn>).mockReturnValue(false);

      const result = await checkWorkflowReferences(
        mockWorkflowRegistry,
        mockExecutionRegistry,
        "workflow-1"
      );

      expect(result.hasReferences).toBe(true);
      expect(result.references).toHaveLength(1);
      expect(result.references[0]!.type).toBe("trigger");
    });

    it("should skip workflows without triggers", async () => {
      const workflowNoTriggers: WorkflowTemplate = {
        id: "no-trigger-workflow",
        name: "No Trigger Workflow",
        type: "WORKFLOW",
        version: "1.0",
        nodes: [],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      } as unknown as WorkflowTemplate;

      (mockWorkflowRegistry.getParentWorkflow as ReturnType<typeof vi.fn>).mockReturnValue(null);
      (mockWorkflowRegistry.list as ReturnType<typeof vi.fn>).mockResolvedValue([
        { id: "no-trigger-workflow", name: "No Trigger Workflow" },
      ]);
      (mockWorkflowRegistry.get as ReturnType<typeof vi.fn>).mockReturnValue(workflowNoTriggers);
      (mockExecutionRegistry.isWorkflowActive as ReturnType<typeof vi.fn>).mockReturnValue(false);

      const result = await checkWorkflowReferences(
        mockWorkflowRegistry,
        mockExecutionRegistry,
        "workflow-1"
      );

      expect(result.hasReferences).toBe(false);
    });

    it("should handle trigger with no id or name", async () => {
      const workflowWithUnnamedTrigger: WorkflowTemplate = {
        id: "unnamed-trigger-workflow",
        name: "Unnamed Trigger Workflow",
        type: "WORKFLOW",
        version: "1.0",
        nodes: [],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
        triggers: [
          {
            action: {
              type: "execute_triggered_subworkflow",
              parameters: {
                triggeredWorkflowId: "workflow-1",
              },
            },
          } as WorkflowTrigger,
        ],
      } as unknown as WorkflowTemplate;

      (mockWorkflowRegistry.getParentWorkflow as ReturnType<typeof vi.fn>).mockReturnValue(null);
      (mockWorkflowRegistry.list as ReturnType<typeof vi.fn>).mockResolvedValue([
        { id: "unnamed-trigger-workflow", name: "Unnamed Trigger Workflow" },
      ]);
      (mockWorkflowRegistry.get as ReturnType<typeof vi.fn>).mockReturnValue(workflowWithUnnamedTrigger);
      (mockExecutionRegistry.isWorkflowActive as ReturnType<typeof vi.fn>).mockReturnValue(false);

      const result = await checkWorkflowReferences(
        mockWorkflowRegistry,
        mockExecutionRegistry,
        "workflow-1"
      );

      expect(result.hasReferences).toBe(true);
      expect(result.references[0]!.sourceId).toContain("unnamed-trigger");
    });
  });

  describe("execution references", () => {
    it("should detect main workflow execution reference", async () => {
      const mockExecution: WorkflowExecutionEntity = {
        id: "exec-1",
        getWorkflowId: vi.fn().mockReturnValue("workflow-1"),
        getStatus: vi.fn().mockReturnValue("RUNNING"),
        getExecutionType: vi.fn().mockReturnValue("main"),
        getTriggeredSubworkflowId: vi.fn().mockReturnValue(null),
        getSubgraphStack: vi.fn().mockReturnValue([]),
      } as unknown as WorkflowExecutionEntity;

      (mockWorkflowRegistry.getParentWorkflow as ReturnType<typeof vi.fn>).mockReturnValue(null);
      (mockWorkflowRegistry.list as ReturnType<typeof vi.fn>).mockResolvedValue([]);
      (mockExecutionRegistry.isWorkflowActive as ReturnType<typeof vi.fn>).mockReturnValue(true);
      (mockExecutionRegistry.getAll as ReturnType<typeof vi.fn>).mockReturnValue([mockExecution]);

      const result = await checkWorkflowReferences(
        mockWorkflowRegistry,
        mockExecutionRegistry,
        "workflow-1"
      );

      expect(result.hasReferences).toBe(true);
      expect(result.references).toHaveLength(1);
      expect(result.references[0]!.type).toBe("workflowExecution");
      expect(result.references[0]!.isRuntimeReference).toBe(true);
      expect(result.canSafelyDelete).toBe(false);
    });

    it("should detect triggered subworkflow execution reference", async () => {
      const mockExecution: WorkflowExecutionEntity = {
        id: "exec-2",
        getWorkflowId: vi.fn().mockReturnValue("other-workflow"),
        getStatus: vi.fn().mockReturnValue("RUNNING"),
        getExecutionType: vi.fn().mockReturnValue("triggered"),
        getTriggeredSubworkflowId: vi.fn().mockReturnValue("workflow-1"),
        getSubgraphStack: vi.fn().mockReturnValue([]),
      } as unknown as WorkflowExecutionEntity;

      (mockWorkflowRegistry.getParentWorkflow as ReturnType<typeof vi.fn>).mockReturnValue(null);
      (mockWorkflowRegistry.list as ReturnType<typeof vi.fn>).mockResolvedValue([]);
      (mockExecutionRegistry.isWorkflowActive as ReturnType<typeof vi.fn>).mockReturnValue(true);
      (mockExecutionRegistry.getAll as ReturnType<typeof vi.fn>).mockReturnValue([mockExecution]);

      const result = await checkWorkflowReferences(
        mockWorkflowRegistry,
        mockExecutionRegistry,
        "workflow-1"
      );

      expect(result.hasReferences).toBe(true);
      expect(result.references).toHaveLength(1);
      expect(result.references[0]!.type).toBe("workflowExecution");
      expect(result.references[0]!.details?.["contextType"]).toBe("triggered-subworkflow");
    });

    it("should detect subgraph stack reference", async () => {
      const mockExecution: WorkflowExecutionEntity = {
        id: "exec-3",
        getWorkflowId: vi.fn().mockReturnValue("other-workflow"),
        getStatus: vi.fn().mockReturnValue("RUNNING"),
        getExecutionType: vi.fn().mockReturnValue("subgraph"),
        getTriggeredSubworkflowId: vi.fn().mockReturnValue(null),
        getSubgraphStack: vi.fn().mockReturnValue([
          { workflowId: "workflow-1", depth: 2, parentWorkflowId: "parent-workflow" },
        ]),
      } as unknown as WorkflowExecutionEntity;

      (mockWorkflowRegistry.getParentWorkflow as ReturnType<typeof vi.fn>).mockReturnValue(null);
      (mockWorkflowRegistry.list as ReturnType<typeof vi.fn>).mockResolvedValue([]);
      (mockExecutionRegistry.isWorkflowActive as ReturnType<typeof vi.fn>).mockReturnValue(true);
      (mockExecutionRegistry.getAll as ReturnType<typeof vi.fn>).mockReturnValue([mockExecution]);

      const result = await checkWorkflowReferences(
        mockWorkflowRegistry,
        mockExecutionRegistry,
        "workflow-1"
      );

      expect(result.hasReferences).toBe(true);
      expect(result.references).toHaveLength(1);
      expect(result.references[0]!.type).toBe("workflowExecution");
      expect(result.references[0]!.details?.["contextType"]).toBe("subgraph-stack");
    });

    it("should skip quick check when workflow is not active", async () => {
      (mockWorkflowRegistry.getParentWorkflow as ReturnType<typeof vi.fn>).mockReturnValue(null);
      (mockWorkflowRegistry.list as ReturnType<typeof vi.fn>).mockResolvedValue([]);
      (mockExecutionRegistry.isWorkflowActive as ReturnType<typeof vi.fn>).mockReturnValue(false);

      const result = await checkWorkflowReferences(
        mockWorkflowRegistry,
        mockExecutionRegistry,
        "workflow-1"
      );

      expect(mockExecutionRegistry.getAll).not.toHaveBeenCalled();
      expect(result.references).toHaveLength(0);
    });
  });

  describe("combined references", () => {
    it("should combine all reference types", async () => {
      const parentWorkflow: WorkflowTemplate = {
        id: "parent-workflow",
        name: "Parent Workflow",
        type: "WORKFLOW",
        version: "1.0",
        nodes: [],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      } as unknown as WorkflowTemplate;

      const workflowWithTrigger: WorkflowTemplate = {
        id: "trigger-workflow",
        name: "Trigger Workflow",
        type: "WORKFLOW",
        version: "1.0",
        nodes: [],
        edges: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
        triggers: [
          {
            id: "trigger-1",
            name: "Trigger One",
            action: {
              type: "execute_triggered_subworkflow",
              parameters: {
                triggeredWorkflowId: "workflow-1",
              },
            },
          } as WorkflowTrigger,
        ],
      } as unknown as WorkflowTemplate;

      const mockExecution: WorkflowExecutionEntity = {
        id: "exec-1",
        getWorkflowId: vi.fn().mockReturnValue("workflow-1"),
        getStatus: vi.fn().mockReturnValue("RUNNING"),
        getExecutionType: vi.fn().mockReturnValue("main"),
        getTriggeredSubworkflowId: vi.fn().mockReturnValue(null),
        getSubgraphStack: vi.fn().mockReturnValue([]),
      } as unknown as WorkflowExecutionEntity;

      (mockWorkflowRegistry.getParentWorkflow as ReturnType<typeof vi.fn>).mockReturnValue("parent-workflow");
      (mockWorkflowRegistry.get as ReturnType<typeof vi.fn>).mockImplementation((id: string) => {
        if (id === "parent-workflow") return parentWorkflow;
        if (id === "trigger-workflow") return workflowWithTrigger;
        return null;
      });
      (mockWorkflowRegistry.getWorkflowHierarchy as ReturnType<typeof vi.fn>).mockReturnValue({ depth: 1 });
      (mockWorkflowRegistry.list as ReturnType<typeof vi.fn>).mockResolvedValue([
        { id: "trigger-workflow", name: "Trigger Workflow" },
      ]);
      (mockExecutionRegistry.isWorkflowActive as ReturnType<typeof vi.fn>).mockReturnValue(true);
      (mockExecutionRegistry.getAll as ReturnType<typeof vi.fn>).mockReturnValue([mockExecution]);

      const result = await checkWorkflowReferences(
        mockWorkflowRegistry,
        mockExecutionRegistry,
        "workflow-1"
      );

      expect(result.hasReferences).toBe(true);
      expect(result.references.length).toBeGreaterThanOrEqual(3);
      expect(result.stats.subgraphReferences).toBe(1);
      expect(result.stats.triggerReferences).toBe(1);
      expect(result.stats.workflowExecutionReferences).toBe(1);
      expect(result.stats.runtimeReferences).toBe(1);
      expect(result.canSafelyDelete).toBe(false);
    });
  });

  describe("stats", () => {
    it("should provide accurate stats", async () => {
      (mockWorkflowRegistry.getParentWorkflow as ReturnType<typeof vi.fn>).mockReturnValue(null);
      (mockWorkflowRegistry.list as ReturnType<typeof vi.fn>).mockResolvedValue([]);
      (mockExecutionRegistry.isWorkflowActive as ReturnType<typeof vi.fn>).mockReturnValue(false);

      const result = await checkWorkflowReferences(
        mockWorkflowRegistry,
        mockExecutionRegistry,
        "workflow-1"
      );

      expect(result.stats).toEqual({
        subgraphReferences: 0,
        triggerReferences: 0,
        workflowExecutionReferences: 0,
        runtimeReferences: 0,
      });
    });
  });
});