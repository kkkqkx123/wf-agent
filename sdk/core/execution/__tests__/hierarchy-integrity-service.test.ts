/**
 * HierarchyIntegrityService Unit Tests
 *
 * Tests for:
 * - validateIntegrity: validates parent/child references against a registry
 * - cleanupOrphanedReferences: removes orphaned references
 * - repairRootInfo: recalculates root execution info from parent
 */

import { describe, it, expect } from "vitest";
import {
  HierarchyIntegrityService,
  type IHierarchyRegistry,
} from "../hierarchy-integrity-service.js";
import type { ExecutionHierarchyMetadata } from "@wf-agent/types";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function createMockRegistry(entries: Record<string, any>): IHierarchyRegistry {
  return {
    has(id: string): boolean {
      return id in entries;
    },
    get(id: string) {
      return entries[id];
    },
  };
}

const makeHierarchy = (overrides?: Partial<ExecutionHierarchyMetadata>): ExecutionHierarchyMetadata => ({
  parent: { parentId: "parent-1", parentType: "WORKFLOW" },
  children: [
    { childId: "child-1", childType: "AGENT_LOOP", createdAt: Date.now() },
    { childId: "child-2", childType: "WORKFLOW", createdAt: Date.now() },
  ],
  depth: 1,
  rootExecutionId: "root-1",
  rootExecutionType: "WORKFLOW",
  ...overrides,
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("HierarchyIntegrityService", () => {
  describe("validateIntegrity", () => {
    it("should return valid=true when all references exist in registry", () => {
      const hierarchy = makeHierarchy();
      const registry = createMockRegistry({
        "parent-1": {},
        "child-1": {},
        "child-2": {},
      });

      const result = HierarchyIntegrityService.validateIntegrity(hierarchy, registry);

      expect(result.valid).toBe(true);
      expect(result.issues).toHaveLength(0);
    });

    it("should report missing parent", () => {
      const hierarchy = makeHierarchy();
      const registry = createMockRegistry({
        "child-1": {},
        "child-2": {},
      });

      const result = HierarchyIntegrityService.validateIntegrity(hierarchy, registry);

      expect(result.valid).toBe(false);
      expect(result.issues).toContain("Parent parent-1 (WORKFLOW) not found in registry");
    });

    it("should report missing children", () => {
      const hierarchy = makeHierarchy();
      const registry = createMockRegistry({
        "parent-1": {},
        "child-2": {},
      });

      const result = HierarchyIntegrityService.validateIntegrity(hierarchy, registry);

      expect(result.valid).toBe(false);
      expect(result.issues).toContain("Child child-1 (AGENT_LOOP) not found in registry");
    });

    it("should report multiple missing references", () => {
      const hierarchy = makeHierarchy();
      const registry = createMockRegistry({}); // nothing exists

      const result = HierarchyIntegrityService.validateIntegrity(hierarchy, registry);

      expect(result.valid).toBe(false);
      expect(result.issues).toHaveLength(3);
    });

    it("should be valid when hierarchy has no parent", () => {
      const hierarchy = makeHierarchy({ parent: undefined, rootExecutionId: "root-1", rootExecutionType: "WORKFLOW" });
      const registry = createMockRegistry({
        "child-1": {},
        "child-2": {},
      });

      const result = HierarchyIntegrityService.validateIntegrity(hierarchy, registry);
      expect(result.valid).toBe(true);
    });

    it("should be valid when hierarchy has no children", () => {
      const hierarchy = makeHierarchy({ children: [] });
      const registry = createMockRegistry({
        "parent-1": {},
      });

      const result = HierarchyIntegrityService.validateIntegrity(hierarchy, registry);
      expect(result.valid).toBe(true);
    });
  });

  describe("cleanupOrphanedReferences", () => {
    it("should keep references that exist in registry", () => {
      const hierarchy = makeHierarchy();
      const registry = createMockRegistry({
        "parent-1": {},
        "child-1": {},
        "child-2": {},
      });

      const cleaned = HierarchyIntegrityService.cleanupOrphanedReferences(hierarchy, registry);

      expect(cleaned.parent).toEqual(hierarchy.parent);
      expect(cleaned.children).toHaveLength(2);
    });

    it("should remove orphaned parent reference", () => {
      const hierarchy = makeHierarchy();
      const registry = createMockRegistry({
        "child-1": {},
        "child-2": {},
      });

      const cleaned = HierarchyIntegrityService.cleanupOrphanedReferences(hierarchy, registry);

      expect(cleaned.parent).toBeUndefined();
    });

    it("should remove orphaned child references", () => {
      const hierarchy = makeHierarchy();
      const registry = createMockRegistry({
        "parent-1": {},
        "child-1": {},
      });

      const cleaned = HierarchyIntegrityService.cleanupOrphanedReferences(hierarchy, registry);

      expect(cleaned.children).toHaveLength(1);
      expect(cleaned.children[0]!.childId).toBe("child-1");
    });

    it("should remove all orphaned references when none exist", () => {
      const hierarchy = makeHierarchy();
      const registry = createMockRegistry({});

      const cleaned = HierarchyIntegrityService.cleanupOrphanedReferences(hierarchy, registry);

      expect(cleaned.parent).toBeUndefined();
      expect(cleaned.children).toHaveLength(0);
    });

    it("should preserve other fields on the hierarchy", () => {
      const hierarchy = makeHierarchy();
      const registry = createMockRegistry({});

      const cleaned = HierarchyIntegrityService.cleanupOrphanedReferences(hierarchy, registry);

      expect(cleaned.depth).toBe(1);
      expect(cleaned.rootExecutionId).toBe("root-1");
      expect(cleaned.rootExecutionType).toBe("WORKFLOW");
    });

    it("should handle hierarchy without parent (no orphan to clean)", () => {
      const hierarchy = makeHierarchy({ parent: undefined });
      const registry = createMockRegistry({});

      const cleaned = HierarchyIntegrityService.cleanupOrphanedReferences(hierarchy, registry);

      expect(cleaned.parent).toBeUndefined();
      expect(cleaned.children).toHaveLength(0);
    });
  });

  describe("repairRootInfo", () => {
    it("should return unchanged when hierarchy has no parent (it is root)", () => {
      const hierarchy = makeHierarchy({
        parent: undefined,
        rootExecutionId: "self-1",
        rootExecutionType: "WORKFLOW",
      });
      const registry = createMockRegistry({});

      const repaired = HierarchyIntegrityService.repairRootInfo(hierarchy, registry);

      expect(repaired).toBe(hierarchy);
    });

    it("should inherit root info from parent when parent is available", () => {
      const hierarchy = makeHierarchy({
        rootExecutionId: "old-root",
        rootExecutionType: "AGENT_LOOP",
      });
      const parentEntity = {
        getRootExecutionId(): string {
          return "correct-root";
        },
        getRootExecutionType(): string {
          return "WORKFLOW";
        },
      };
      const registry = createMockRegistry({
        "parent-1": parentEntity,
      });

      const repaired = HierarchyIntegrityService.repairRootInfo(hierarchy, registry);

      expect(repaired.rootExecutionId).toBe("correct-root");
      expect(repaired.rootExecutionType).toBe("WORKFLOW");
    });

    it("should return unchanged when parent entity is not in registry", () => {
      const hierarchy = makeHierarchy({
        rootExecutionId: "old-root",
        rootExecutionType: "AGENT_LOOP",
      });
      const registry = createMockRegistry({});

      const repaired = HierarchyIntegrityService.repairRootInfo(hierarchy, registry);

      expect(repaired.rootExecutionId).toBe("old-root");
      expect(repaired.rootExecutionType).toBe("AGENT_LOOP");
    });

    it("should return unchanged when parent entity lacks getRootExecutionId", () => {
      const hierarchy = makeHierarchy();
      const registry = createMockRegistry({
        "parent-1": { notARootMethod: true },
      });

      const repaired = HierarchyIntegrityService.repairRootInfo(hierarchy, registry);

      expect(repaired.rootExecutionId).toBe("root-1");
      expect(repaired.rootExecutionType).toBe("WORKFLOW");
    });

    it("should preserve children and parent on repaired metadata", () => {
      const hierarchy = makeHierarchy();
      const parentEntity = {
        getRootExecutionId(): string {
          return "correct-root";
        },
        getRootExecutionType(): string {
          return "WORKFLOW";
        },
      };
      const registry = createMockRegistry({
        "parent-1": parentEntity,
        "child-1": {},
        "child-2": {},
      });

      const repaired = HierarchyIntegrityService.repairRootInfo(hierarchy, registry);

      expect(repaired.parent).toEqual(hierarchy.parent);
      expect(repaired.children).toEqual(hierarchy.children);
      expect(repaired.depth).toBe(1);
    });
  });
});
