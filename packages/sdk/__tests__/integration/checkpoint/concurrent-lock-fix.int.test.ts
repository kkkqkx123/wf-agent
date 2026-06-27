/**
 * Concurrent Cleanup Lock Test
 *
 * This test verifies the fix for the critical concurrent lock bug in BaseCheckpointStateManager.
 *
 * BUG: The original withEntityLock implementation had:
 *   const currentLock = previousLock.then(
 *     () => operation(),
 *     () => operation(),  // ← BUG: executes even if previous lock failed!
 *   );
 *
 * This violated lock semantics - if cleanup-A fails, cleanup-B should NOT start.
 * Instead, cleanup-B should wait and may inherit the failure.
 *
 * SCENARIO (Real Business Case):
 * - Concurrent cleanups on same entity should be serialized
 * - If first cleanup fails, second cleanup should not proceed
 * - This prevents data corruption from parallel mutations
 */

import { describe, it, expect, beforeEach } from "vitest";
import { MemoryCheckpointStorage } from "@wf-agent/storage";
import { BaseCheckpointStateManager } from "@sdk/shared/checkpoint/core/base-state-manager.js";
import type { CheckpointStorageMetadata } from "@wf-agent/types";
import type { BaseCheckpoint } from "@wf-agent/types";

interface TestCheckpoint extends BaseCheckpoint<unknown, unknown> {
  id: string;
  type: "FULL" | "DELTA";
  timestamp: number;
  snapshot?: unknown;
}

class TestStateManager extends BaseCheckpointStateManager<TestCheckpoint> {
  extractStorageMetadata(checkpoint: TestCheckpoint): CheckpointStorageMetadata {
    return {
      entityType: "workflow",
      entityId: "test-entity",
      timestamp: checkpoint.timestamp,
      checkpointType: checkpoint.type,
      blobSize: 100,
    };
  }

  buildCreatedEvent(_checkpoint: TestCheckpoint): unknown {
    return { type: "checkpoint.created" };
  }

  buildDeletedEvent(
    _checkpointId: string,
    _reason?: "manual" | "cleanup" | "policy",
  ): unknown {
    return { type: "checkpoint.deleted" };
  }

  buildFailedEvent(
    _checkpointId: string,
    _error: unknown,
    _operation?: "create" | "restore" | "delete",
  ): unknown {
    return { type: "checkpoint.failed" };
  }
}

describe("Concurrent Cleanup Lock - Bug Fix Verification", () => {
  let storage: MemoryCheckpointStorage;
  let manager: TestStateManager;

  beforeEach(async () => {
    storage = new MemoryCheckpointStorage();
    await storage.initialize();
    manager = new TestStateManager(storage);
  });

  describe("BUG-FIX: withEntityLock serialization", () => {
    it("should serialize cleanup operations for same entity (not run in parallel)", async () => {
      /**
       * SCENARIO:
       * Two cleanups submit to the same entity concurrently.
       * They should execute sequentially, not in parallel.
       */
      const executionLog: string[] = [];
      const entityId = "test-entity";

      // Simulate two cleanup operations
      const cleanup1 = async () => {
        executionLog.push("cleanup-1-start");
        await new Promise((resolve) => setTimeout(resolve, 50));
        executionLog.push("cleanup-1-end");
      };

      const cleanup2 = async () => {
        executionLog.push("cleanup-2-start");
        await new Promise((resolve) => setTimeout(resolve, 50));
        executionLog.push("cleanup-2-end");
      };

      // Submit both concurrently
      const [result1, result2] = await Promise.all([
        (manager as any).withEntityLock(entityId, cleanup1),
        (manager as any).withEntityLock(entityId, cleanup2),
      ]);

      // Verify sequential execution
      // Expected order: cleanup-1-start -> cleanup-1-end -> cleanup-2-start -> cleanup-2-end
      // NOT: cleanup-1-start -> cleanup-2-start -> ...
      expect(executionLog).toEqual([
        "cleanup-1-start",
        "cleanup-1-end",
        "cleanup-2-start",
        "cleanup-2-end",
      ]);
    });

    it("should NOT execute next operation if previous operation fails", async () => {
      /**
       * CRITICAL BUG FIX:
       * Original code: previousLock.then(() => op(), () => op())
       *   - Would execute op() in BOTH success and failure cases!
       *
       * Fixed code: previousLock.then(() => op())
       *   - Only executes op() after previous succeeds
       *   - If previous fails, the failure propagates
       */
      const executionLog: string[] = [];
      const entityId = "test-entity";

      const failingCleanup = async () => {
        executionLog.push("cleanup-1-executed");
        throw new Error("Storage failure");
      };

      const successCleanup = async () => {
        executionLog.push("cleanup-2-executed");
        return "success";
      };

      try {
        // Submit both to same entity
        await Promise.all([
          (manager as any).withEntityLock(entityId, failingCleanup),
          (manager as any).withEntityLock(entityId, successCleanup),
        ]);
      } catch {
        // Expected to fail due to first cleanup failure
      }

      // The bug would cause both to execute
      // Fixed code should only execute cleanup-1
      expect(executionLog).toEqual(["cleanup-1-executed"]);
      expect(executionLog).not.toContain("cleanup-2-executed");
    });

    it("should prevent data corruption from concurrent cleanups", async () => {
      /**
       * REAL WORLD SCENARIO:
       * Entity has 10 checkpoints, cleanup wants to delete 5.
       * During deletion: cp-1, cp-2, cp-3 deleted, then error occurs
       * If cleanup-2 starts in parallel (BUG), it might delete cp-4, cp-5
       * Result: 7 deleted instead of intended 5, data loss!
       */
      const entityId = "test-entity";
      const deletedCheckpoints: string[] = [];

      // Create 10 checkpoints
      for (let i = 0; i < 10; i++) {
        const cp: TestCheckpoint = {
          id: `cp-${i}`,
          type: "FULL",
          timestamp: Date.now() + i,
          snapshot: { value: i },
        };
        const data = new TextEncoder().encode(JSON.stringify(cp));
        await storage.save(cp.id, data, {
          entityType: "workflow",
          entityId,
          timestamp: cp.timestamp,
          checkpointType: "FULL",
          blobSize: data.length,
        });
      }

      // Simulate cleanup that fails halfway through
      const cleanup1 = async () => {
        // Delete cp-0, cp-1, cp-2
        for (let i = 0; i < 3; i++) {
          await storage.delete(`cp-${i}`);
          deletedCheckpoints.push(`cp-${i}`);
        }
        // Then fail
        throw new Error("Storage error");
      };

      // Simulate another cleanup trying to run in parallel
      const cleanup2 = async () => {
        // Should NOT execute due to lock
        await storage.delete("cp-4");
        deletedCheckpoints.push("cp-4");
      };

      try {
        await Promise.all([
          (manager as any).withEntityLock(entityId, cleanup1),
          (manager as any).withEntityLock(entityId, cleanup2),
        ]);
      } catch {
        // Expected to fail
      }

      // With the bug fix:
      // cleanup-1 should delete 3 items then fail
      // cleanup-2 should NOT start
      // Final result: only 3 deleted
      expect(deletedCheckpoints.length).toBe(3);
      expect(deletedCheckpoints).toEqual(["cp-0", "cp-1", "cp-2"]);

      // Verify remaining checkpoints still exist
      const remaining = await storage.list();
      expect(remaining.length).toBe(7);
    });

    it("should maintain lock state across successful operations", async () => {
      /**
       * Verify that locks are properly cleaned up after operation completes.
       * This ensures the system doesn't accumulate stale locks.
       */
      const entityId = "test-entity";

      const operation = async () => "done";

      // Execute first operation
      const result1 = await (manager as any).withEntityLock(entityId, operation);
      expect(result1).toBe("done");

      // Check that lock was cleaned up (internal state check)
      const locks = (manager as any).cleanupLocks as Map<string, Promise<void>>;
      expect(locks.has(entityId)).toBe(false);

      // Execute second operation - should work fine
      const result2 = await (manager as any).withEntityLock(entityId, operation);
      expect(result2).toBe("done");

      // Lock should be cleaned up again
      expect(locks.has(entityId)).toBe(false);
    });
  });

  describe("Real-world cleanup scenarios with lock protection", () => {
    it("should handle multiple entities with independent locks", async () => {
      /**
       * SCENARIO:
       * Two different entities being cleaned up concurrently.
       * Their locks should be independent.
       */
      const executionOrder: string[] = [];

      const cleanup = async (entityId: string) => {
        executionOrder.push(`${entityId}-start`);
        await new Promise((resolve) => setTimeout(resolve, 30));
        executionOrder.push(`${entityId}-end`);
      };

      // These should execute truly in parallel (different entity IDs)
      await Promise.all([
        (manager as any).withEntityLock("entity-A", () => cleanup("entity-A")),
        (manager as any).withEntityLock("entity-B", () => cleanup("entity-B")),
      ]);

      // Both should have started before either finished (parallel execution)
      const aStartIdx = executionOrder.indexOf("entity-A-start");
      const bStartIdx = executionOrder.indexOf("entity-B-start");
      const aEndIdx = executionOrder.indexOf("entity-A-end");

      // B should start before A ends (parallel)
      expect(bStartIdx).toBeLessThan(aEndIdx);
    });

    it("should handle rapid successive operations on same entity", async () => {
      /**
       * SCENARIO:
       * Multiple rapid cleanup attempts on same entity.
       * All should be queued and executed sequentially.
       */
      const entityId = "test-entity";
      const completedCount = { value: 0 };

      const operation = async (id: number) => {
        completedCount.value++;
        return id;
      };

      const results = await Promise.all(
        Array.from({ length: 10 }, (_, i) =>
          (manager as any).withEntityLock(entityId, () => operation(i)),
        ),
      );

      // All operations should complete
      expect(results.length).toBe(10);
      expect(completedCount.value).toBe(10);
    });
  });
});
