/**
 * Tests for WorkflowRegistry
 */

import { describe, it, expect, vi, beforeEach, Mock } from "vitest";
import { WorkflowRegistry } from "../workflow-registry.js";
import type { WorkflowTemplate } from "@wf-agent/types";
import type { WorkflowStorageAdapter } from "@wf-agent/storage";
import type { WorkflowExecutionRegistry } from "../workflow-execution-registry.js";
import type { WorkflowRelationshipRegistry } from "../workflow-relationship-registry.js";
import type { WorkflowGraphRegistry } from "../workflow-graph-registry.js";

// Mock dependencies
vi.mock("../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    trace: vi.fn(),
  }),
}));

vi.mock("@wf-agent/common-utils", async (importOriginal) => {
  const actual = await importOriginal();
  return {
    ...(actual as Record<string, unknown>),
    getErrorMessage: (error: unknown) =>
      error instanceof Error ? error.message : String(error),
  };
});

vi.mock("../utils/workflow-preprocessor.js", () => ({
  preprocessWorkflow: vi.fn(),
}));

vi.mock("../utils/workflow-storage-utils.js", () => ({
  persistWorkflow: vi.fn(),
  removeWorkflow: vi.fn(),
  initializeWorkflowsFromStorage: vi.fn(),
  loadWorkflow: vi.fn(),
}));

vi.mock("../../execution/utils/workflow-reference-checker.js", () => ({
  checkWorkflowReferences: vi.fn(),
}));

import { preprocessWorkflow } from "../utils/workflow-preprocessor.js";
import {
  persistWorkflow,
  removeWorkflow,
  loadWorkflow,
} from "../utils/workflow-storage-utils.js";
import { checkWorkflowReferences } from "../../execution/utils/workflow-reference-checker.js";

function createMockWorkflow(id: string, overrides: Partial<WorkflowTemplate> = {}): WorkflowTemplate {
  return {
    id,
    name: `Workflow ${id}`,
    type: "WORKFLOW",
    version: "1.0",
    description: `Description for ${id}`,
    nodes: [
      { id: "node-1", type: "START", config: {} },
      { id: "node-2", type: "END", config: {} },
    ],
    edges: [
      { id: "edge-1", source: "node-1", target: "node-2" },
    ],
    createdAt: Date.now(),
    updatedAt: Date.now(),
    metadata: {
      tags: ["test"],
      category: "general",
      author: "tester",
    },
    ...overrides,
  } as unknown as WorkflowTemplate;
}

function createMockRelationshipRegistry(): WorkflowRelationshipRegistry {
  return {
    clear: vi.fn(),
    cleanupWorkflowReferences: vi.fn(),
    registerSubgraphRelationship: vi.fn(),
    getWorkflowHierarchy: vi.fn(),
    getParentWorkflow: vi.fn(),
    getChildWorkflows: vi.fn(),
  } as unknown as WorkflowRelationshipRegistry;
}

function createMockGraphRegistry(): WorkflowGraphRegistry {
  return {
    register: vi.fn(),
    has: vi.fn(),
    get: vi.fn(),
  } as unknown as WorkflowGraphRegistry;
}

function createMockExecutionRegistry(): WorkflowExecutionRegistry {
  return {
    isWorkflowActive: vi.fn().mockReturnValue(false),
    getActive: vi.fn().mockReturnValue([]),
    getByWorkflowId: vi.fn().mockReturnValue([]),
  } as unknown as WorkflowExecutionRegistry;
}

describe("WorkflowRegistry", () => {
  let registry: WorkflowRegistry;
  let mockStorageAdapter: WorkflowStorageAdapter;
  let mockRelationshipRegistry: WorkflowRelationshipRegistry;
  let mockGraphRegistry: WorkflowGraphRegistry;
  let mockExecutionRegistry: WorkflowExecutionRegistry;

  beforeEach(() => {
    vi.clearAllMocks();

    // Default mock for checkWorkflowReferences: no references found
    (checkWorkflowReferences as Mock).mockResolvedValue({
      hasReferences: false,
      references: [],
      canSafelyDelete: true,
      stats: {
        subgraphReferences: 0,
        triggerReferences: 0,
        workflowExecutionReferences: 0,
        runtimeReferences: 0,
      },
    });

    mockStorageAdapter = {
      save: vi.fn(),
      load: vi.fn(),
      delete: vi.fn(),
      list: vi.fn(),
      initialize: vi.fn(),
      close: vi.fn(),
      clear: vi.fn(),
    } as unknown as WorkflowStorageAdapter;

    mockRelationshipRegistry = createMockRelationshipRegistry();
    mockGraphRegistry = createMockGraphRegistry();
    mockExecutionRegistry = createMockExecutionRegistry();

    registry = new WorkflowRegistry(
      null,
      mockExecutionRegistry,
      mockRelationshipRegistry,
      mockGraphRegistry,
    );
  });

  describe("constructor", () => {
    it("should create an empty registry", () => {
      expect(registry.size()).toBe(0);
      expect(registry.getActiveWorkflows()).toEqual([]);
    });

    it("should accept storage adapter", () => {
      const reg = new WorkflowRegistry(mockStorageAdapter);
      expect(reg).toBeDefined();
      expect(reg.size()).toBe(0);
    });
  });

  // ============================================================
  // Active Workflow Tracking
  // ============================================================

  describe("active workflow tracking", () => {
    it("should add active workflow", () => {
      registry.addActiveWorkflow("wf-1");
      expect(registry.isWorkflowActive("wf-1")).toBe(true);
      expect(registry.getActiveWorkflows()).toContain("wf-1");
    });

    it("should remove active workflow", () => {
      registry.addActiveWorkflow("wf-1");
      registry.removeActiveWorkflow("wf-1");
      expect(registry.isWorkflowActive("wf-1")).toBe(false);
    });

    it("should return false for non-active workflow", () => {
      expect(registry.isWorkflowActive("non-existent")).toBe(false);
    });

    it("should list all active workflows", () => {
      registry.addActiveWorkflow("wf-1");
      registry.addActiveWorkflow("wf-2");
      const active = registry.getActiveWorkflows();
      expect(active).toHaveLength(2);
      expect(active).toContain("wf-1");
      expect(active).toContain("wf-2");
    });
  });

  // ============================================================
  // Register
  // ============================================================

  describe("register", () => {
    it("should register a workflow", () => {
      const workflow = createMockWorkflow("wf-1");
      registry.register(workflow);
      expect(registry.size()).toBe(1);
      expect(registry.has("wf-1")).toBe(true);
    });

    it("should throw if workflow has no id", () => {
      const workflow = createMockWorkflow("wf-1", { id: "" });
      expect(() => registry.register(workflow)).toThrow("Workflow ID is required");
    });

    it("should throw if workflow has no name", () => {
      const workflow = createMockWorkflow("wf-1", { name: "" });
      expect(() => registry.register(workflow)).toThrow("Workflow name is required");
    });

    it("should throw if workflow has no nodes", () => {
      const workflow = createMockWorkflow("wf-1", { nodes: [] });
      expect(() => registry.register(workflow)).toThrow("Workflow must have at least one node");
    });

    it("should throw if workflow id already exists", () => {
      const workflow = createMockWorkflow("wf-1");
      registry.register(workflow);
      expect(() => registry.register(workflow)).toThrow("already exists");
    });

    it("should skip if skipIfExists option is set", () => {
      const workflow = createMockWorkflow("wf-1");
      registry.register(workflow);
      expect(() => registry.register(workflow, { skipIfExists: true })).not.toThrow();
      expect(registry.size()).toBe(1);
    });
  });

  describe("registerAsync", () => {
    it("should register a workflow asynchronously with preprocessing", async () => {
      const workflow = createMockWorkflow("wf-1");
      (preprocessWorkflow as Mock).mockResolvedValue(undefined);

      await registry.registerAsync(workflow);

      expect(registry.has("wf-1")).toBe(true);
      expect(preprocessWorkflow).toHaveBeenCalledWith(
        workflow,
        expect.objectContaining({
          workflowRegistry: registry,
          graphRegistry: mockGraphRegistry,
          relationshipRegistry: mockRelationshipRegistry,
        }),
      );
    });

    it("should persist to storage before registering", async () => {
      const reg = new WorkflowRegistry(
        mockStorageAdapter,
        mockExecutionRegistry,
        mockRelationshipRegistry,
        mockGraphRegistry,
      );
      const workflow = createMockWorkflow("wf-1");
      (preprocessWorkflow as Mock).mockResolvedValue(undefined);

      await reg.registerAsync(workflow);

      expect(persistWorkflow).toHaveBeenCalledWith(workflow, mockStorageAdapter);
      expect(reg.has("wf-1")).toBe(true);
    });

    it("should rollback on preprocessing failure", async () => {
      const reg = new WorkflowRegistry(
        mockStorageAdapter,
        mockExecutionRegistry,
        mockRelationshipRegistry,
        mockGraphRegistry,
      );
      const workflow = createMockWorkflow("wf-1");
      (preprocessWorkflow as Mock).mockRejectedValue(new Error("Preprocessing failed"));

      await expect(reg.registerAsync(workflow)).rejects.toThrow("Preprocessing failed");
      expect(reg.has("wf-1")).toBe(false);
      expect(removeWorkflow).toHaveBeenCalledWith("wf-1", mockStorageAdapter);
    });

    it("should throw on duplicate id", async () => {
      const workflow = createMockWorkflow("wf-1");
      registry.register(workflow);
      await expect(registry.registerAsync(workflow)).rejects.toThrow("already exists");
    });

    it("should skip if skipIfExists option is set", async () => {
      const workflow = createMockWorkflow("wf-1");
      registry.register(workflow);
      await expect(registry.registerAsync(workflow, { skipIfExists: true })).resolves.not.toThrow();
    });
  });

  describe("registerBatch", () => {
    it("should register multiple workflows", () => {
      const workflows = [
        createMockWorkflow("wf-1"),
        createMockWorkflow("wf-2"),
      ];

      registry.registerBatch(workflows);

      expect(registry.size()).toBe(2);
      expect(registry.has("wf-1")).toBe(true);
      expect(registry.has("wf-2")).toBe(true);
    });

    it("should skip errors with skipErrors option", () => {
      const validWorkflow = createMockWorkflow("wf-1");
      const invalidWorkflow = createMockWorkflow("wf-2", { id: "" });

      registry.registerBatch([validWorkflow, invalidWorkflow], { skipErrors: true });

      expect(registry.size()).toBe(1);
      expect(registry.has("wf-1")).toBe(true);
    });

    it("should throw on first error without skipErrors", () => {
      const validWorkflow = createMockWorkflow("wf-1");
      const invalidWorkflow = createMockWorkflow("wf-2", { id: "" });

      expect(() => registry.registerBatch([invalidWorkflow, validWorkflow])).toThrow();
    });
  });

  // ============================================================
  // Update / Upsert
  // ============================================================

  describe("update", () => {
    it("should update an existing workflow", async () => {
      const workflow = createMockWorkflow("wf-1");
      registry.register(workflow);

      await registry.update("wf-1", { name: "Updated Name" });

      const updated = registry.get("wf-1");
      expect(updated?.name).toBe("Updated Name");
    });

    it("should throw if workflow not found", async () => {
      await expect(registry.update("non-existent", { name: "New" })).rejects.toThrow("not found");
    });

    it("should create if createIfNotExists option is set", async () => {
      const validWorkflow = createMockWorkflow("wf-1", { name: "New", id: "wf-1" });
      await registry.update("wf-1", validWorkflow as Partial<WorkflowTemplate>, {
        createIfNotExists: true,
      });

      expect(registry.has("wf-1")).toBe(true);
      expect(registry.get("wf-1")?.name).toBe("New");
    });

    it("should persist to storage when adapter is available", async () => {
      const reg = new WorkflowRegistry(
        mockStorageAdapter,
        mockExecutionRegistry,
        mockRelationshipRegistry,
        mockGraphRegistry,
      );
      const workflow = createMockWorkflow("wf-1");
      reg.register(workflow);

      await reg.update("wf-1", { name: "Updated" });

      expect(persistWorkflow).toHaveBeenCalled();
    });
  });

  describe("upsert", () => {
    it("should register if not exists", async () => {
      const workflow = createMockWorkflow("wf-1");
      await registry.upsert(workflow);
      expect(registry.has("wf-1")).toBe(true);
    });

    it("should update if exists", async () => {
      const workflow = createMockWorkflow("wf-1");
      registry.register(workflow);

      await registry.upsert({ ...workflow, name: "Updated" });

      expect(registry.get("wf-1")?.name).toBe("Updated");
    });
  });

  // ============================================================
  // Query Methods
  // ============================================================

  describe("get", () => {
    it("should return workflow by id", () => {
      const workflow = createMockWorkflow("wf-1");
      registry.register(workflow);
      expect(registry.get("wf-1")).toBe(workflow);
    });

    it("should return undefined for non-existent workflow", () => {
      expect(registry.get("non-existent")).toBeUndefined();
    });
  });

  describe("getByName", () => {
    it("should find workflow by name", () => {
      const workflow = createMockWorkflow("wf-1", { name: "Unique Name" });
      registry.register(workflow);
      expect(registry.getByName("Unique Name")).toBe(workflow);
    });

    it("should return undefined for non-existent name", () => {
      expect(registry.getByName("Non-existent")).toBeUndefined();
    });
  });

  describe("getByTags", () => {
    it("should find workflows by tags", () => {
      const workflow = createMockWorkflow("wf-1", {
        metadata: { tags: ["test", "important"] },
      });
      registry.register(workflow);
      const results = registry.getByTags(["test"]);
      expect(results).toHaveLength(1);
      expect(results[0]).toBe(workflow);
    });

    it("should match all tags (AND logic)", () => {
      const workflow1 = createMockWorkflow("wf-1", {
        metadata: { tags: ["test", "important"] },
      });
      const workflow2 = createMockWorkflow("wf-2", {
        metadata: { tags: ["test"] },
      });
      registry.register(workflow1);
      registry.register(workflow2);
      const results = registry.getByTags(["test", "important"]);
      expect(results).toHaveLength(1);
    });

    it("should return empty array when no match", () => {
      registry.register(createMockWorkflow("wf-1"));
      expect(registry.getByTags(["non-existent"])).toHaveLength(0);
    });
  });

  describe("getByCategory", () => {
    it("should find workflows by category", () => {
      const workflow = createMockWorkflow("wf-1", {
        metadata: { category: "critical" },
      });
      registry.register(workflow);
      const results = registry.getByCategory("critical");
      expect(results).toHaveLength(1);
    });
  });

  describe("getByAuthor", () => {
    it("should find workflows by author", () => {
      const workflow = createMockWorkflow("wf-1", {
        metadata: { author: "developer" },
      });
      registry.register(workflow);
      const results = registry.getByAuthor("developer");
      expect(results).toHaveLength(1);
    });
  });

  describe("list", () => {
    it("should return summaries of all registered workflows", async () => {
      registry.register(createMockWorkflow("wf-1"));
      registry.register(createMockWorkflow("wf-2"));

      const summaries = await registry.list();

      expect(summaries).toHaveLength(2);
      expect(summaries[0]).toHaveProperty("id");
      expect(summaries[0]).toHaveProperty("name");
      expect(summaries[0]).toHaveProperty("nodeCount");
      expect(summaries[0]).toHaveProperty("edgeCount");
    });

    it("should load missing workflows from storage", async () => {
      const reg = new WorkflowRegistry(
        mockStorageAdapter,
        mockExecutionRegistry,
        mockRelationshipRegistry,
        mockGraphRegistry,
      );

      const wf1 = createMockWorkflow("wf-1");
      reg.register(wf1);

      (mockStorageAdapter.list as Mock).mockResolvedValue(["wf-1", "wf-2"]);
      (loadWorkflow as Mock).mockResolvedValue(createMockWorkflow("wf-2"));

      const summaries = await reg.list();

      expect(summaries).toHaveLength(2);
      expect(loadWorkflow).toHaveBeenCalledWith("wf-2", mockStorageAdapter);
    });
  });

  describe("search", () => {
    it("should search by name", async () => {
      registry.register(createMockWorkflow("wf-1", { name: "My Important Workflow" }));
      registry.register(createMockWorkflow("wf-2", { name: "Other" }));

      const results = await registry.search("important");
      expect(results).toHaveLength(1);
      expect(results[0]!.name).toBe("My Important Workflow");
    });

    it("should search by description", async () => {
      registry.register(createMockWorkflow("wf-1", { description: "This is critical" }));
      registry.register(createMockWorkflow("wf-2", { description: "Other" }));

      const results = await registry.search("critical");
      expect(results).toHaveLength(1);
    });

    it("should search by id", async () => {
      registry.register(createMockWorkflow("critical-workflow"));

      const results = await registry.search("critical");
      expect(results).toHaveLength(1);
    });

    it("should return empty array when no match", async () => {
      registry.register(createMockWorkflow("wf-1"));
      const results = await registry.search("nonexistent");
      expect(results).toHaveLength(0);
    });
  });

  // ============================================================
  // Validation
  // ============================================================

  describe("validate", () => {
    it("should return valid for correct workflow", () => {
      const workflow = createMockWorkflow("wf-1");
      const result = registry.validate(workflow);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it("should return errors for missing id", () => {
      const workflow = createMockWorkflow("wf-1", { id: "" });
      const result = registry.validate(workflow);
      expect(result.valid).toBe(false);
      expect(result.errors).toContain("Workflow ID is required");
    });

    it("should return errors for missing name", () => {
      const workflow = createMockWorkflow("wf-1", { name: "" });
      const result = registry.validate(workflow);
      expect(result.valid).toBe(false);
      expect(result.errors).toContain("Workflow name is required");
    });

    it("should return errors for empty nodes", () => {
      const workflow = createMockWorkflow("wf-1", { nodes: [] });
      const result = registry.validate(workflow);
      expect(result.valid).toBe(false);
      expect(result.errors).toContain("Workflow must have at least one node");
    });

    it("should return errors for missing edges", () => {
      const workflow = createMockWorkflow("wf-1", { edges: undefined as any });
      const result = registry.validate(workflow);
      expect(result.valid).toBe(false);
      expect(result.errors).toContain("Workflow edges are required");
    });
  });

  describe("validateBatch", () => {
    it("should validate multiple workflows", () => {
      const validWorkflow = createMockWorkflow("wf-1");
      const invalidWorkflow = createMockWorkflow("wf-2", { id: "" });

      const results = registry.validateBatch([validWorkflow, invalidWorkflow]);

      expect(results).toHaveLength(2);
      expect(results[0]!.valid).toBe(true);
      expect(results[1]!.valid).toBe(false);
    });
  });

  // ============================================================
  // Export / Import
  // ============================================================

  describe("export", () => {
    it("should export workflow as JSON string", () => {
      const workflow = createMockWorkflow("wf-1");
      registry.register(workflow);

      const json = registry.export("wf-1");
      const parsed = JSON.parse(json);
      expect(parsed.id).toBe("wf-1");
      expect(parsed.name).toBe("Workflow wf-1");
    });

    it("should throw for non-existent workflow", () => {
      expect(() => registry.export("non-existent")).toThrow("does not exist");
    });
  });

  describe("import", () => {
    it("should import workflow from JSON string", () => {
      const workflow = createMockWorkflow("wf-1");
      const json = JSON.stringify(workflow);

      const id = registry.import(json);
      expect(id).toBe("wf-1");
      expect(registry.has("wf-1")).toBe(true);
    });

    it("should throw for invalid JSON", () => {
      expect(() => registry.import("invalid json")).toThrow("Failed to import workflow");
    });
  });

  // ============================================================
  // Unregister
  // ============================================================

  describe("unregister", () => {
    it("should unregister a workflow", async () => {
      const workflow = createMockWorkflow("wf-1");
      registry.register(workflow);

      await registry.unregister("wf-1");

      expect(registry.has("wf-1")).toBe(false);
    });

    it("should throw for non-existent workflow", async () => {
      await expect(registry.unregister("non-existent")).rejects.toThrow("not found");
    });

    it("should remove from storage when adapter is available", async () => {
      const reg = new WorkflowRegistry(
        mockStorageAdapter,
        mockExecutionRegistry,
        mockRelationshipRegistry,
        mockGraphRegistry,
      );
      const workflow = createMockWorkflow("wf-1");
      reg.register(workflow);

      await reg.unregister("wf-1");

      expect(removeWorkflow).toHaveBeenCalledWith("wf-1", mockStorageAdapter);
    });

    it("should cleanup relationship references", async () => {
      const workflow = createMockWorkflow("wf-1");
      registry.register(workflow);

      await registry.unregister("wf-1");

      expect(mockRelationshipRegistry.cleanupWorkflowReferences).toHaveBeenCalledWith("wf-1");
    });
  });

  // ============================================================
  // Clear
  // ============================================================

  describe("clear", () => {
    it("should clear all workflows", () => {
      registry.register(createMockWorkflow("wf-1"));
      registry.register(createMockWorkflow("wf-2"));
      registry.addActiveWorkflow("wf-1");

      registry.clear();

      expect(registry.size()).toBe(0);
      expect(registry.getActiveWorkflows()).toEqual([]);
    });

    it("should clear relationship registry", () => {
      registry.clear();
      expect(mockRelationshipRegistry.clear).toHaveBeenCalled();
    });
  });

  // ============================================================
  // has / size
  // ============================================================

  describe("has", () => {
    it("should return true if workflow exists", () => {
      registry.register(createMockWorkflow("wf-1"));
      expect(registry.has("wf-1")).toBe(true);
    });

    it("should return false if workflow does not exist", () => {
      expect(registry.has("non-existent")).toBe(false);
    });
  });

  describe("size", () => {
    it("should return correct count", () => {
      expect(registry.size()).toBe(0);
      registry.register(createMockWorkflow("wf-1"));
      expect(registry.size()).toBe(1);
    });
  });

  // ============================================================
  // Delegated Relationship Methods
  // ============================================================

  describe("relationship delegation methods", () => {
    it("should delegate registerSubgraphRelationship", () => {
      registry.registerSubgraphRelationship("parent", "node-1", "child");
      expect(mockRelationshipRegistry.registerSubgraphRelationship).toHaveBeenCalledWith(
        "parent", "node-1", "child",
      );
    });

    it("should delegate getWorkflowHierarchy", () => {
      registry.getWorkflowHierarchy("wf-1");
      expect(mockRelationshipRegistry.getWorkflowHierarchy).toHaveBeenCalledWith("wf-1");
    });

    it("should delegate getParentWorkflow", () => {
      registry.getParentWorkflow("wf-1");
      expect(mockRelationshipRegistry.getParentWorkflow).toHaveBeenCalledWith("wf-1");
    });

    it("should delegate getChildWorkflows", () => {
      registry.getChildWorkflows("wf-1");
      expect(mockRelationshipRegistry.getChildWorkflows).toHaveBeenCalledWith("wf-1");
    });
  });
});