import { describe, it, expect, beforeEach } from "vitest";
import { StorageBackedChildResolver, CachedChildResolver } from "../hierarchy/child-resolver.js";
import type { ChildCheckpointDescriptor } from "../hierarchy/child-resolver.js";
import { ChildCheckpointRestorer } from "../hierarchy/child-restorer.js";
import type { ChildRestoreDependencies } from "../hierarchy/child-restorer.js";
import { RestoreStrategyRegistry } from "../hierarchy/restore-strategy.js";
import { MemoryCheckpointStorage } from "@wf-agent/storage";
import type { CheckpointStorageMetadata, ChildExecutionReference, ID } from "@wf-agent/types";

interface IExecutionEntity {
  id: ID;
  type: string;
  instanceType?: string;
  setParentContext?: (ctx: Record<string, unknown>) => void;
  getChildReferences?: () => ChildExecutionReference[];
}

describe("ChildCheckpointResolver integration", () => {
  describe("CP-INT-06: resolve latest checkpoint", () => {
    let storage: MemoryCheckpointStorage;
    let resolver: StorageBackedChildResolver;

    beforeEach(async () => {
      storage = new MemoryCheckpointStorage();
      await storage.initialize();
      resolver = new StorageBackedChildResolver(storage);
    });

    it("should return null when no checkpoints exist for entity", async () => {
      const ref: ChildExecutionReference = { childId: "nonexistent" as ID, childType: "workflow" as any };
      const result = await resolver.resolveLatestCheckpoint(ref);
      expect(result).toBeNull();
    });

    it("should return latest checkpoint metadata", async () => {
      const metadata1: CheckpointStorageMetadata = {
        entityType: "workflow",
        entityId: "child-1",
        timestamp: 100,
        checkpointType: "FULL",
      };
      const metadata2: CheckpointStorageMetadata = {
        entityType: "workflow",
        entityId: "child-1",
        timestamp: 200,
        checkpointType: "DELTA",
      };

      await storage.save(
        "cp-1",
        new TextEncoder().encode(JSON.stringify({ id: "cp-1" })),
        metadata1,
      );
      await storage.save(
        "cp-2",
        new TextEncoder().encode(JSON.stringify({ id: "cp-2" })),
        metadata2,
      );

      const ref: ChildExecutionReference = { childId: "child-1" as ID, childType: "workflow" as any };
      const result = await resolver.resolveLatestCheckpoint(ref);
      expect(result).not.toBeNull();
      expect(result!.checkpointId).toBe("cp-2");
    });

    it("should resolve multiple checkpoints in batch", async () => {
      for (let i = 1; i <= 3; i++) {
        await storage.save(
          `cp-child-${i}`,
          new TextEncoder().encode(JSON.stringify({ id: `cp-child-${i}` })),
          { entityType: "agent", entityId: `child-${i}`, timestamp: i * 100, checkpointType: "FULL" },
        );
      }

      const refs: ChildExecutionReference[] = [
        { childId: "child-1" as ID, childType: "agent" as any },
        { childId: "child-2" as ID, childType: "agent" as any },
        { childId: "child-3" as ID, childType: "agent" as any },
      ];

      const results = await resolver.resolveLatestCheckpoints(refs);
      expect(results.size).toBe(3);
      expect(results.get("child-1")!.checkpointId).toBe("cp-child-1");
      expect(results.get("child-2")!.checkpointId).toBe("cp-child-2");
    });
  });

  describe("CachedChildResolver preload and cache", () => {
    it("should return cached results without storage access", async () => {
      const resolver = new CachedChildResolver();
      const descriptor: ChildCheckpointDescriptor = {
        checkpointId: "cp-cached",
        metadata: { entityType: "workflow", entityId: "e1", timestamp: 100, checkpointType: "FULL" },
      };

      resolver.preload("e1", [descriptor]);
      const ref: ChildExecutionReference = { childId: "e1" as ID, childType: "workflow" as any };
      const result = await resolver.resolveLatestCheckpoint(ref);
      expect(result!.checkpointId).toBe("cp-cached");
    });

    it("should clear cache for specific entity", async () => {
      const resolver = new CachedChildResolver();
      resolver.preload("e1", [{ checkpointId: "cp-1", metadata: { entityType: "workflow", entityId: "e1", timestamp: 100, checkpointType: "FULL" } }]);
      resolver.clearCache("e1");

      const ref: ChildExecutionReference = { childId: "e1" as ID, childType: "workflow" as any };
      const result = await resolver.resolveLatestCheckpoint(ref);
      expect(result).toBeNull();
    });
  });
});

describe("ChildCheckpointRestorer integration", () => {
  describe("CP-INT-07: restore children in parallel", () => {
    it("should restore multiple children with concurrency control", async () => {
      const restorer = new ChildCheckpointRestorer();
      const restoredChildren: string[] = [];
      const parentEntity: IExecutionEntity = {
        id: "parent-1" as ID,
        type: "workflow",
        instanceType: "workflowExecution",
        setParentContext: () => {},
        getChildReferences: () => [],
      };

      const childRefs: ChildExecutionReference[] = [
        { childId: "child-a" as ID, childType: "WORKFLOW" as any },
        { childId: "child-b" as ID, childType: "WORKFLOW" as any },
      ];

      const deps: ChildRestoreDependencies = {
        findCheckpoint: async (childId: ID) => `cp-for-${childId}`,
        restoreEntity: async (checkpointId: ID, _childType: any, _parentId: ID) => {
          restoredChildren.push(checkpointId);
          return {
            id: checkpointId.replace("cp-for-", "") as ID,
            type: "workflow",
            instanceType: "workflowExecution",
            setParentContext: () => {},
            getChildReferences: () => [],
          } as IExecutionEntity;
        },
        registerChild: () => {},
        maxConcurrency: 2,
      };

      const results = await restorer.restoreChildren(parentEntity, childRefs, deps);
      expect(results).toHaveLength(2);
      expect(results[0].success).toBe(true);
      expect(results[1].success).toBe(true);
      expect(restoredChildren).toHaveLength(2);
    });

    it("should summarize results correctly", () => {
      const results = [
        { childId: "child-a" as ID, childType: "WORKFLOW" as any, success: true } as ChildRestoreResult,
        { childId: "child-b" as ID, childType: "WORKFLOW" as any, success: true } as ChildRestoreResult,
      ];

      const summary = ChildCheckpointRestorer.summarizeResults(results);
      expect(summary.total).toBe(2);
      expect(summary.succeeded).toBe(2);
      expect(summary.failed).toBe(0);
    });

    it("should report failures in summary", () => {
      const results = [
        { childId: "child-a" as ID, childType: "WORKFLOW" as any, success: true } as ChildRestoreResult,
        { childId: "child-b" as ID, childType: "WORKFLOW" as any, success: false, error: "Not found" } as ChildRestoreResult,
      ];

      const summary = ChildCheckpointRestorer.summarizeResults(results);
      expect(summary.total).toBe(2);
      expect(summary.succeeded).toBe(1);
      expect(summary.failed).toBe(1);
      expect(summary.failures).toHaveLength(1);
      expect(summary.failures[0].error).toBe("Not found");
    });
  });
});

describe("RestoreStrategyRegistry integration", () => {
  describe("CP-INT-09: register and query strategies", () => {
    it("should register and retrieve strategies by type", () => {
      const registry = new RestoreStrategyRegistry();
      const workflowStrategy = {
        executionType: "workflow" as any,
        findCheckpoint: async () => "cp-wf",
        restoreEntity: async () => ({ id: "wf-1" as ID, type: "workflow" as any }) as IExecutionEntity,
        registerChild: () => {},
      };

      registry.register(workflowStrategy);
      expect(registry.has("workflow" as any)).toBe(true);
      expect(registry.get("workflow" as any)).toBe(workflowStrategy);
      expect(registry.get("agent" as any)).toBeUndefined();
    });

    it("should return all registered strategies", () => {
      const registry = new RestoreStrategyRegistry();
      const s1 = { executionType: "workflow" as any, findCheckpoint: async () => "cp", restoreEntity: async () => ({ id: "id" as ID, type: "workflow" as any }) as IExecutionEntity, registerChild: () => {} };
      const s2 = { executionType: "agent" as any, findCheckpoint: async () => "cp", restoreEntity: async () => ({ id: "id" as ID, type: "agent" as any }) as IExecutionEntity, registerChild: () => {} };

      registry.register(s1);
      registry.register(s2);

      const all = registry.getAll();
      expect(all).toHaveLength(2);
    });

    it("should return false for unregistered type", () => {
      const registry = new RestoreStrategyRegistry();
      expect(registry.has("workflow" as any)).toBe(false);
    });
  });
});
