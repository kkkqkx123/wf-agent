/**
 * ExecutionHierarchyManager Unit Tests
 *
 * Tests for:
 * - Root node creation and state
 * - Parent setting with validation (cycle detection, depth limit)
 * - Child management (add / remove)
 * - Hierarchy metadata serialization (toMetadata)
 * - Registry-based operations (cycle detection via registry)
 * - Restoration from existing metadata
 */

import { describe, it, expect } from "vitest";
import { ExecutionHierarchyManager } from "../execution-hierarchy-manager.js";
import type {
  ExecutionHierarchyMetadata,
  ParentExecutionContext,
  ChildExecutionReference,
  ExecutionType,
  ID,
} from "@wf-agent/types";
import type { ExecutionHierarchyRegistry } from "../../registry/execution-hierarchy-registry.js";

// ---------------------------------------------------------------------------
// Mock registry
// ---------------------------------------------------------------------------

interface MockEntity {
  id: ID;
  getParentContext(): ParentExecutionContext | undefined;
  getHierarchyDepth(): number;
  getRootExecutionId(): ID;
  getRootExecutionType(): ExecutionType;
}

function createMockEntity(overrides: Partial<MockEntity> & { id: ID }): MockEntity {
  return {
    getParentContext(): ParentExecutionContext | undefined {
      return undefined;
    },
    getHierarchyDepth(): number {
      return 0;
    },
    getRootExecutionId(): ID {
      return overrides.id;
    },
    getRootExecutionType(): ExecutionType {
      return "WORKFLOW";
    },
    ...overrides,
  };
}

function createMockRegistry(entities: Map<ID, unknown>): ExecutionHierarchyRegistry {
  return {
    get(id: ID): unknown {
      return entities.get(id);
    },
  } as unknown as ExecutionHierarchyRegistry;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("ExecutionHierarchyManager", () => {
  // -----------------------------------------------------------------------
  // Construction — root node
  // -----------------------------------------------------------------------

  describe("root node construction", () => {
    it("should create a root node with depth 0", () => {
      const mgr = new ExecutionHierarchyManager("exec-1", "WORKFLOW");
      expect(mgr.getDepth()).toBe(0);
      expect(mgr.getParent()).toBeUndefined();
      expect(mgr.getChildren()).toEqual([]);
    });

    it("should set root execution info to own identity", () => {
      const mgr = new ExecutionHierarchyManager("exec-1", "WORKFLOW");
      expect(mgr.getRootExecutionId()).toBe("exec-1");
      expect(mgr.getRootExecutionType()).toBe("WORKFLOW");
    });
  });

  // -----------------------------------------------------------------------
  // Restoration from existing metadata
  // -----------------------------------------------------------------------

  describe("restoration from existing metadata", () => {
    it("should restore parent, depth, root info, and children", () => {
      const existing: ExecutionHierarchyMetadata = {
        parent: { parentId: "parent-1", parentType: "WORKFLOW" },
        children: [
          { childId: "child-1", childType: "AGENT_LOOP", createdAt: Date.now() },
        ],
        depth: 3,
        rootExecutionId: "root-1",
        rootExecutionType: "WORKFLOW",
      };

      const mgr = new ExecutionHierarchyManager("exec-1", "AGENT_LOOP", existing);

      expect(mgr.getParent()).toEqual(existing.parent);
      expect(mgr.getDepth()).toBe(3);
      expect(mgr.getRootExecutionId()).toBe("root-1");
      expect(mgr.getRootExecutionType()).toBe("WORKFLOW");
      expect(mgr.getChildren()).toHaveLength(1);
      expect(mgr.getChildren()[0]).toEqual(existing.children[0]);
    });

    it("should restore multiple children", () => {
      const existing: ExecutionHierarchyMetadata = {
        parent: undefined,
        children: [
          { childId: "child-a", childType: "AGENT_LOOP", createdAt: Date.now() },
          { childId: "child-b", childType: "WORKFLOW", createdAt: Date.now() },
          { childId: "child-c", childType: "AGENT_LOOP", createdAt: Date.now() },
        ],
        depth: 0,
        rootExecutionId: "root-1",
        rootExecutionType: "WORKFLOW",
      };

      const mgr = new ExecutionHierarchyManager("exec-1", "WORKFLOW", existing);
      expect(mgr.getChildren()).toHaveLength(3);
    });

    it("should handle existing metadata with no children", () => {
      const existing: ExecutionHierarchyMetadata = {
        parent: { parentId: "p", parentType: "WORKFLOW" },
        children: [],
        depth: 1,
        rootExecutionId: "root-1",
        rootExecutionType: "WORKFLOW",
      };

      const mgr = new ExecutionHierarchyManager("exec-1", "AGENT_LOOP", existing);
      expect(mgr.getChildren()).toEqual([]);
    });

    it("should handle existing metadata with no parent", () => {
      const existing: ExecutionHierarchyMetadata = {
        parent: undefined,
        children: [],
        depth: 0,
        rootExecutionId: "self",
        rootExecutionType: "WORKFLOW",
      };

      const mgr = new ExecutionHierarchyManager("exec-1", "WORKFLOW", existing);
      expect(mgr.getParent()).toBeUndefined();
      expect(mgr.getRootExecutionId()).toBe("self");
    });
  });

  // -----------------------------------------------------------------------
  // setParent & getParent
  // -----------------------------------------------------------------------

  describe("setParent / getParent", () => {
    it("should set parent and update depth", () => {
      const mgr = new ExecutionHierarchyManager("child-1", "AGENT_LOOP");
      mgr.setParent({ parentId: "parent-1", parentType: "WORKFLOW" });

      expect(mgr.getParent()).toEqual({ parentId: "parent-1", parentType: "WORKFLOW" });
      expect(mgr.getDepth()).toBe(1); // parent depth=0 + 1
    });

    it("should reject setting self as parent (cycle detection)", () => {
      const mgr = new ExecutionHierarchyManager("exec-1", "WORKFLOW");
      expect(() =>
        mgr.setParent({ parentId: "exec-1", parentType: "WORKFLOW" }),
      ).toThrow(/circular/i);
    });

    it("should allow overwriting parent", () => {
      const mgr = new ExecutionHierarchyManager("child-1", "AGENT_LOOP");
      mgr.setParent({ parentId: "parent-a", parentType: "WORKFLOW" });
      mgr.setParent({ parentId: "parent-b", parentType: "WORKFLOW" });

      expect(mgr.getParent()!.parentId).toBe("parent-b");
    });
  });

  // -----------------------------------------------------------------------
  // Depth calculation
  // -----------------------------------------------------------------------

  describe("depth calculation with registry", () => {
    it("should calculate depth based on parent from registry", () => {
      const parentEntity = createMockEntity({
        id: "parent-1",
        getHierarchyDepth(): number {
          return 2;
        },
      });

      const entities = new Map<ID, unknown>();
      entities.set("parent-1", parentEntity);

      const registry = createMockRegistry(entities);
      const mgr = new ExecutionHierarchyManager("child-1", "AGENT_LOOP", undefined, registry);
      mgr.setParent({ parentId: "parent-1", parentType: "WORKFLOW" });

      expect(mgr.getDepth()).toBe(3);
    });

    it("should fall back to depth 0 when parent not in registry", () => {
      const entities = new Map<ID, unknown>();
      const registry = createMockRegistry(entities);
      const mgr = new ExecutionHierarchyManager("child-1", "AGENT_LOOP", undefined, registry);
      mgr.setParent({ parentId: "nonexistent", parentType: "WORKFLOW" });

      // Falls back: parent depth=0 → child depth=1
      expect(mgr.getDepth()).toBe(1);
    });
  });

  // -----------------------------------------------------------------------
  // Root info inheritance
  // -----------------------------------------------------------------------

  describe("root info inheritance from parent", () => {
    it("should inherit root info from parent via registry", () => {
      const parentEntity = createMockEntity({
        id: "parent-1",
        getRootExecutionId(): ID {
          return "root-ancestor";
        },
        getRootExecutionType(): ExecutionType {
          return "WORKFLOW";
        },
      });

      const entities = new Map<ID, unknown>();
      entities.set("parent-1", parentEntity);

      const registry = createMockRegistry(entities);
      const mgr = new ExecutionHierarchyManager("child-1", "AGENT_LOOP", undefined, registry);
      mgr.setParent({ parentId: "parent-1", parentType: "WORKFLOW" });

      expect(mgr.getRootExecutionId()).toBe("root-ancestor");
      expect(mgr.getRootExecutionType()).toBe("WORKFLOW");
    });

    it("should fall back to parent ID when registry not available", () => {
      const mgr = new ExecutionHierarchyManager("child-1", "AGENT_LOOP");
      mgr.setParent({ parentId: "parent-1", parentType: "WORKFLOW" });

      // No registry → fallback to parent's own ID
      expect(mgr.getRootExecutionId()).toBe("parent-1");
      expect(mgr.getRootExecutionType()).toBe("WORKFLOW");
    });
  });

  // -----------------------------------------------------------------------
  // Cycle detection via registry
  // -----------------------------------------------------------------------

  describe("cycle detection via registry", () => {
    it("should detect cycle with depth 1 ancestor chain", () => {
      // exec-A → exec-B → exec-A (cycle)
      const entityA = createMockEntity({
        id: "exec-A",
        getParentContext(): ParentExecutionContext | undefined {
          return { parentId: "exec-B", parentType: "WORKFLOW" };
        },
      });
      const entityB = createMockEntity({
        id: "exec-B",
        getParentContext(): ParentExecutionContext | undefined {
          return { parentId: "exec-A", parentType: "WORKFLOW" };
        },
      });

      const entities = new Map<ID, unknown>();
      entities.set("exec-A", entityA);
      entities.set("exec-B", entityB);

      const registry = createMockRegistry(entities);

      // Create manager for exec-A, try to set parent to exec-B
      const mgrA = new ExecutionHierarchyManager("exec-A", "WORKFLOW", undefined, registry);

      // Setting parent to exec-B would create a cycle (exec-B's parent is exec-A)
      expect(() => mgrA.setParent({ parentId: "exec-B", parentType: "WORKFLOW" })).toThrow(/circular/i);
    });

    it("should allow valid parent chain", () => {
      // exec-C → exec-B → exec-A (valid chain)
      const entityA = createMockEntity({
        id: "exec-A",
        getParentContext(): ParentExecutionContext | undefined {
          return undefined;
        },
      });
      const entityB = createMockEntity({
        id: "exec-B",
        getParentContext(): ParentExecutionContext | undefined {
          return { parentId: "exec-A", parentType: "WORKFLOW" };
        },
      });

      const entities = new Map<ID, unknown>();
      entities.set("exec-A", entityA);
      entities.set("exec-B", entityB);

      const registry = createMockRegistry(entities);

      const mgrC = new ExecutionHierarchyManager("exec-C", "WORKFLOW", undefined, registry);
      expect(() =>
        mgrC.setParent({ parentId: "exec-B", parentType: "WORKFLOW" }),
      ).not.toThrow();
    });

    it("should detect self-cycle without registry", () => {
      const mgr = new ExecutionHierarchyManager("self", "WORKFLOW");
      expect(() =>
        mgr.setParent({ parentId: "self", parentType: "WORKFLOW" }),
      ).toThrow(/circular/i);
    });
  });

  // -----------------------------------------------------------------------
  // Depth limit enforcement
  // -----------------------------------------------------------------------

  describe("depth limit enforcement", () => {
    it("should throw when depth exceeds MAX_DEPTH (default 10)", () => {
      // Build a chain of 11 depth
      const entities = new Map<ID, unknown>();
      for (let i = 0; i <= 11; i++) {
        const id = `node-${i}`;
        const parentId = i > 0 ? `node-${i - 1}` : undefined;
        entities.set(id, {
          id,
          getParentContext(): ParentExecutionContext | undefined {
            return parentId ? { parentId, parentType: "WORKFLOW" } : undefined;
          },
          getHierarchyDepth(): number {
            return i;
          },
          getRootExecutionId(): ID {
            return "node-0";
          },
          getRootExecutionType(): ExecutionType {
            return "WORKFLOW";
          },
        } satisfies MockEntity);
      }

      const registry = createMockRegistry(entities);

      // node-11 has depth 11, node-12 would have depth 12 which exceeds MAX_DEPTH (10)?
      // Wait, MAX_DEPTH default is 10, so depth=11 is already too deep.
      // Let's set parent of a new node to node-10 which has depth 10,
      // new node would have depth 11 which exceeds 10.

      // Actually, MAX_DEPTH = 10 means max allowed depth is 10 (root=0, child=1, ..., depth 10 is the max).
      // So a node at depth 10 (node-10) can have a child at depth 11 which would fail.

      const newNode = new ExecutionHierarchyManager("new-node", "AGENT_LOOP", undefined, registry);
      expect(() =>
        newNode.setParent({ parentId: "node-10", parentType: "WORKFLOW" }),
      ).toThrow(/depth.*exceeded|maximum/i);
    });
  });

  // -----------------------------------------------------------------------
  // Child management
  // -----------------------------------------------------------------------

  describe("addChild / removeChild / getChildren", () => {
    it("should add and retrieve a child", () => {
      const mgr = new ExecutionHierarchyManager("parent", "WORKFLOW");
      const childRef: ChildExecutionReference = {
        childId: "child-1",
        childType: "AGENT_LOOP",
        createdAt: Date.now(),
      };

      mgr.addChild(childRef);
      const children = mgr.getChildren();
      expect(children).toHaveLength(1);
      expect(children[0]).toEqual(childRef);
    });

    it("should add multiple children", () => {
      const mgr = new ExecutionHierarchyManager("parent", "WORKFLOW");
      mgr.addChild({ childId: "c1", childType: "AGENT_LOOP", createdAt: Date.now() });
      mgr.addChild({ childId: "c2", childType: "AGENT_LOOP", createdAt: Date.now() });
      mgr.addChild({ childId: "c3", childType: "WORKFLOW", createdAt: Date.now() });

      expect(mgr.getChildren()).toHaveLength(3);
    });

    it("should not add duplicate children with same (type, id) — key collision overwrites", () => {
      const mgr = new ExecutionHierarchyManager("parent", "WORKFLOW");
      mgr.addChild({ childId: "c1", childType: "AGENT_LOOP", createdAt: Date.now() });
      mgr.addChild({ childId: "c1", childType: "AGENT_LOOP", createdAt: Date.now() }); // same key

      expect(mgr.getChildren()).toHaveLength(1); // overwritten, not duplicated
    });

    it("should allow children with same id but different type", () => {
      const mgr = new ExecutionHierarchyManager("parent", "WORKFLOW");
      mgr.addChild({ childId: "c1", childType: "AGENT_LOOP", createdAt: Date.now() });
      mgr.addChild({ childId: "c1", childType: "WORKFLOW", createdAt: Date.now() });

      expect(mgr.getChildren()).toHaveLength(2);
    });

    it("should remove an existing child", () => {
      const mgr = new ExecutionHierarchyManager("parent", "WORKFLOW");
      mgr.addChild({ childId: "c1", childType: "AGENT_LOOP", createdAt: Date.now() });
      mgr.addChild({ childId: "c2", childType: "WORKFLOW", createdAt: Date.now() });

      const removed = mgr.removeChild("c1", "AGENT_LOOP");
      expect(removed).toBe(true);
      expect(mgr.getChildren()).toHaveLength(1);
      expect(mgr.getChildren()[0]!.childId).toBe("c2");
    });

    it("should return false when removing a non-existent child", () => {
      const mgr = new ExecutionHierarchyManager("parent", "WORKFLOW");
      const removed = mgr.removeChild("nonexistent", "AGENT_LOOP");
      expect(removed).toBe(false);
    });

    it("should return empty array when no children exist", () => {
      const mgr = new ExecutionHierarchyManager("parent", "WORKFLOW");
      expect(mgr.getChildren()).toEqual([]);
    });
  });

  // -----------------------------------------------------------------------
  // toMetadata
  // -----------------------------------------------------------------------

  describe("toMetadata", () => {
    it("should serialize current state to metadata object", () => {
      const mgr = new ExecutionHierarchyManager("exec-1", "WORKFLOW");
      mgr.addChild({ childId: "child-1", childType: "AGENT_LOOP", createdAt: Date.now() });

      const metadata = mgr.toMetadata();

      expect(metadata.parent).toBeUndefined();
      expect(metadata.children).toHaveLength(1);
      expect(metadata.depth).toBe(0);
      expect(metadata.rootExecutionId).toBe("exec-1");
      expect(metadata.rootExecutionType).toBe("WORKFLOW");
    });

    it("should serialize with parent info", () => {
      const mgr = new ExecutionHierarchyManager("child-1", "AGENT_LOOP");
      mgr.setParent({ parentId: "parent-1", parentType: "WORKFLOW" });

      const metadata = mgr.toMetadata();

      expect(metadata.parent).toEqual({ parentId: "parent-1", parentType: "WORKFLOW" });
      expect(metadata.depth).toBe(1);
    });

    it("should not share mutable child array reference", () => {
      const mgr = new ExecutionHierarchyManager("exec-1", "WORKFLOW");
      mgr.addChild({ childId: "c1", childType: "AGENT_LOOP", createdAt: Date.now() });

      const metadata = mgr.toMetadata();
      expect(metadata.children).toHaveLength(1);

      // Mutating metadata should not affect manager
      metadata.children.push({ childId: "c2", childType: "WORKFLOW", createdAt: Date.now() });
      expect(mgr.getChildren()).toHaveLength(1);
    });
  });
});
