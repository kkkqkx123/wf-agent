/**
 * Tests for WorkflowRelationshipRegistry
 */

import { describe, it, expect, beforeEach } from "vitest";
import { WorkflowRelationshipRegistry } from "../workflow-relationship-registry.js";
import type { WorkflowReferenceRelation } from "../../types/reference.js";

describe("WorkflowRelationshipRegistry", () => {
  let registry: WorkflowRelationshipRegistry;

  beforeEach(() => {
    registry = new WorkflowRelationshipRegistry();
  });

  // ============================================================
  // Reference Relationship Methods
  // ============================================================

  describe("addReferenceRelation", () => {
    it("should add a reference relation", () => {
      const relation: WorkflowReferenceRelation = {
        sourceWorkflowId: "workflow-a",
        targetWorkflowId: "workflow-b",
        referenceType: "subgraph",
        isRuntime: false,
      };

      registry.addReferenceRelation(relation);

      const relations = registry.getReferenceRelations("workflow-b");
      expect(relations).toHaveLength(1);
      expect(relations[0]!.sourceWorkflowId).toBe("workflow-a");
    });

    it("should add multiple reference relations to the same target", () => {
      const relation1: WorkflowReferenceRelation = {
        sourceWorkflowId: "workflow-a",
        targetWorkflowId: "workflow-b",
        referenceType: "subgraph",
        isRuntime: false,
      };
      const relation2: WorkflowReferenceRelation = {
        sourceWorkflowId: "workflow-c",
        targetWorkflowId: "workflow-b",
        referenceType: "trigger",
        isRuntime: true,
      };

      registry.addReferenceRelation(relation1);
      registry.addReferenceRelation(relation2);

      const relations = registry.getReferenceRelations("workflow-b");
      expect(relations).toHaveLength(2);
    });
  });

  describe("removeReferenceRelation", () => {
    it("should remove a specific reference relation", () => {
      const relation: WorkflowReferenceRelation = {
        sourceWorkflowId: "workflow-a",
        targetWorkflowId: "workflow-b",
        referenceType: "subgraph",
        isRuntime: false,
      };
      registry.addReferenceRelation(relation);

      registry.removeReferenceRelation("workflow-a", "workflow-b", "subgraph");

      expect(registry.getReferenceRelations("workflow-b")).toHaveLength(0);
    });

    it("should not remove relations that do not match", () => {
      const relation: WorkflowReferenceRelation = {
        sourceWorkflowId: "workflow-a",
        targetWorkflowId: "workflow-b",
        referenceType: "subgraph",
        isRuntime: false,
      };
      registry.addReferenceRelation(relation);

      registry.removeReferenceRelation("workflow-a", "workflow-b", "trigger");

      expect(registry.getReferenceRelations("workflow-b")).toHaveLength(1);
    });

    it("should not throw when removing from empty registry", () => {
      expect(() =>
        registry.removeReferenceRelation("workflow-a", "workflow-b", "subgraph"),
      ).not.toThrow();
    });
  });

  describe("hasReferences", () => {
    it("should return true when reference relations exist", () => {
      const relation: WorkflowReferenceRelation = {
        sourceWorkflowId: "workflow-a",
        targetWorkflowId: "workflow-b",
        referenceType: "subgraph",
        isRuntime: false,
      };
      registry.addReferenceRelation(relation);
      expect(registry.hasReferences("workflow-b")).toBe(true);
    });

    it("should return true when parent relationship exists", () => {
      registry.registerSubgraphRelationship("parent", "subgraph-node-1", "child");
      expect(registry.hasReferences("child")).toBe(true);
    });

    it("should return false when no references exist", () => {
      expect(registry.hasReferences("workflow-unknown")).toBe(false);
    });
  });

  describe("getReferenceRelations", () => {
    it("should return empty array for workflow with no relations", () => {
      expect(registry.getReferenceRelations("non-existent")).toEqual([]);
    });
  });

  describe("clearReferenceRelations", () => {
    it("should clear all reference relations for a workflow", () => {
      const relation: WorkflowReferenceRelation = {
        sourceWorkflowId: "workflow-a",
        targetWorkflowId: "workflow-b",
        referenceType: "subgraph",
        isRuntime: false,
      };
      registry.addReferenceRelation(relation);
      registry.clearReferenceRelations("workflow-b");
      expect(registry.getReferenceRelations("workflow-b")).toHaveLength(0);
    });
  });

  describe("cleanupWorkflowReferences", () => {
    it("should cleanup all references related to the workflow", () => {
      const relation1: WorkflowReferenceRelation = {
        sourceWorkflowId: "workflow-a",
        targetWorkflowId: "workflow-b",
        referenceType: "subgraph",
        isRuntime: false,
      };
      const relation2: WorkflowReferenceRelation = {
        sourceWorkflowId: "workflow-b",
        targetWorkflowId: "workflow-c",
        referenceType: "subgraph",
        isRuntime: false,
      };

      registry.addReferenceRelation(relation1);
      registry.addReferenceRelation(relation2);

      registry.cleanupWorkflowReferences("workflow-b");

      // workflow-b as target should be removed
      expect(registry.getReferenceRelations("workflow-b")).toHaveLength(0);
      // workflow-b as source in workflow-c's relations should be removed
      const cRelations = registry.getReferenceRelations("workflow-c");
      expect(cRelations).toHaveLength(0);
    });

    it("should also clean up hierarchical relations", () => {
      registry.registerSubgraphRelationship("parent", "subgraph-node-1", "child");
      registry.cleanupWorkflowReferences("child");
      expect(registry.getParentWorkflow("child")).toBeNull();
    });
  });

  describe("getReferencingWorkflows", () => {
    it("should return source workflow IDs from reference relations", () => {
      const relation: WorkflowReferenceRelation = {
        sourceWorkflowId: "workflow-a",
        targetWorkflowId: "workflow-b",
        referenceType: "subgraph",
        isRuntime: false,
      };
      registry.addReferenceRelation(relation);
      const sources = registry.getReferencingWorkflows("workflow-b");
      expect(sources).toContain("workflow-a");
    });

    it("should include parent workflow from hierarchy", () => {
      registry.registerSubgraphRelationship("parent", "subgraph-node-1", "child");
      const sources = registry.getReferencingWorkflows("child");
      expect(sources).toContain("parent");
    });
  });

  // ============================================================
  // Hierarchy Relationship Methods
  // ============================================================

  describe("registerSubgraphRelationship", () => {
    it("should register parent-child relationship", () => {
      registry.registerSubgraphRelationship("parent", "subgraph-node-1", "child");

      expect(registry.getParentWorkflow("child")).toBe("parent");
      expect(registry.getChildWorkflows("parent")).toContain("child");
    });

    it("should register multiple children for same parent", () => {
      registry.registerSubgraphRelationship("parent", "subgraph-node-1", "child-1");
      registry.registerSubgraphRelationship("parent", "subgraph-node-2", "child-2");

      const children = registry.getChildWorkflows("parent");
      expect(children).toHaveLength(2);
      expect(children).toContain("child-1");
      expect(children).toContain("child-2");
    });

    it("should set correct depth for child", () => {
      registry.registerSubgraphRelationship("root", "node-1", "mid");
      registry.registerSubgraphRelationship("mid", "node-2", "leaf");

      const hierarchy = registry.getWorkflowHierarchy("leaf");
      expect(hierarchy.depth).toBe(2);
    });
  });

  describe("getWorkflowHierarchy", () => {
    it("should return hierarchy for root workflow", () => {
      const hierarchy = registry.getWorkflowHierarchy("root");
      expect(hierarchy.ancestors).toEqual([]);
      expect(hierarchy.depth).toBe(0);
      expect(hierarchy.rootWorkflowId).toBe("root");
    });

    it("should return hierarchy with ancestors and descendants", () => {
      registry.registerSubgraphRelationship("root", "node-1", "mid");
      registry.registerSubgraphRelationship("mid", "node-2", "leaf");

      const hierarchy = registry.getWorkflowHierarchy("mid");
      expect(hierarchy.ancestors).toEqual(["root"]);
      expect(hierarchy.descendants).toContain("leaf");
      expect(hierarchy.rootWorkflowId).toBe("root");
    });
  });

  describe("getParentWorkflow", () => {
    it("should return parent for child workflow", () => {
      registry.registerSubgraphRelationship("parent", "node-1", "child");
      expect(registry.getParentWorkflow("child")).toBe("parent");
    });

    it("should return null for workflow with no parent", () => {
      expect(registry.getParentWorkflow("orphan")).toBeNull();
    });
  });

  describe("getChildWorkflows", () => {
    it("should return children for parent", () => {
      registry.registerSubgraphRelationship("parent", "node-1", "child-1");
      registry.registerSubgraphRelationship("parent", "node-2", "child-2");
      const children = registry.getChildWorkflows("parent");
      expect(children).toHaveLength(2);
    });

    it("should return empty array for leaf workflow", () => {
      expect(registry.getChildWorkflows("leaf")).toEqual([]);
    });
  });

  describe("clear", () => {
    it("should clear all relationships and references", () => {
      registry.registerSubgraphRelationship("parent", "node-1", "child");
      registry.addReferenceRelation({
        sourceWorkflowId: "a",
        targetWorkflowId: "b",
        referenceType: "subgraph",
        isRuntime: false,
      });

      registry.clear();

      expect(registry.getParentWorkflow("child")).toBeNull();
      expect(registry.getReferenceRelations("b")).toHaveLength(0);
      expect(registry.getReferencingWorkflows("b")).toHaveLength(0);
    });
  });
});
