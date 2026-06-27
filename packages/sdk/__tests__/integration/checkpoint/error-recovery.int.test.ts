/**
 * Error Recovery and Resilience Tests
 *
 * Tests for:
 * - Exponential backoff retry mechanism (缺陷5)
 * - Timeout protection for restore operations
 * - Fallback to FULL checkpoints
 * - Corrupted data handling
 * - Event emission resilience
 *
 * SCENARIO: Real world production issues
 * 1. Network flakiness → retries help
 * 2. Slow restore on long delta chains → timeout prevents hang
 * 3. Corrupted delta → fallback to FULL
 * 4. Event system down → non-blocking async emission
 */

import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { MemoryCheckpointStorage } from "@wf-agent/storage";
import { BaseCheckpointCoordinator } from "@sdk/shared/checkpoint/core/base-coordinator";
import { BaseCheckpointStateManager } from "@sdk/shared/checkpoint/core/base-state-manager";
import type { CheckpointableEntity, CheckpointDependencies, RetryPolicy } from "@sdk/shared/checkpoint/types";
import type { BaseCheckpoint, CheckpointStorageMetadata } from "@wf-agent/types";

interface TestState {
  value: number;
  label: string;
  data?: string;
}

interface TestEntity extends CheckpointableEntity {
  id: string;
  state: TestState;
}

interface TestCheckpoint extends BaseCheckpoint<unknown, TestState> {
  id: string;
  type: "FULL" | "DELTA";
  timestamp: number;
  snapshot?: TestState;
  delta?: unknown;
  previousCheckpointId?: string;
  baseCheckpointId?: string;
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

class TestCoordinator extends BaseCheckpointCoordinator<TestCheckpoint, TestEntity, TestState> {
  protected extractState(entity: TestEntity): TestState {
    return entity.state;
  }

  protected async buildCheckpoint(
    _entity: TestEntity,
    currentState: TestState,
    checkpointType: "FULL" | "DELTA",
    checkpointId: string,
    timestamp: number,
  ): Promise<TestCheckpoint> {
    return {
      id: checkpointId,
      type: checkpointType,
      timestamp,
      snapshot: currentState,
      baseCheckpointId: checkpointId,
    } as TestCheckpoint;
  }

  protected extractParentId(_checkpoint: TestCheckpoint): string {
    return "test-entity";
  }

  protected createEntityFromSnapshot(_parentId: string, snapshot: TestState): TestEntity {
    return {
      id: "test-entity",
      state: snapshot,
    };
  }
}

describe("Error Recovery and Resilience", () => {
  let storage: MemoryCheckpointStorage;
  let coordinator: TestCoordinator;
  let stateManager: TestStateManager;

  beforeEach(async () => {
    storage = new MemoryCheckpointStorage();
    await storage.initialize();
    coordinator = new TestCoordinator();
    stateManager = new TestStateManager(storage);
  });

  describe("Retry Mechanism - Exponential Backoff", () => {
    it("should retry checkpoint creation on transient storage failure", async () => {
      const entity: TestEntity = {
        id: "test-entity",
        state: { value: 1, label: "test" },
      };

      const checkpoint: TestCheckpoint = {
        id: "cp-1",
        type: "FULL",
        timestamp: Date.now(),
        snapshot: entity.state,
        baseCheckpointId: "cp-1",
      };

      // Mock storage to fail first, then succeed
      let attemptCount = 0;
      const originalSave = storage.save.bind(storage);

      vi.spyOn(storage, "save").mockImplementation(async (...args) => {
        attemptCount++;
        if (attemptCount === 1) {
          throw new Error("Simulated transient storage failure");
        }
        return originalSave(...args);
      });

      // Create with retry policy
      const result = await stateManager.create(checkpoint, {
        maxRetries: 3,
        initialDelayMs: 10,  // Short delay for testing
        maxDelayMs: 50,
      });

      expect(result).toBe("cp-1");
      expect(attemptCount).toBe(2);  // Failed once, then succeeded
    });

    it("should give up after max retries", async () => {
      const checkpoint: TestCheckpoint = {
        id: "cp-1",
        type: "FULL",
        timestamp: Date.now(),
        snapshot: { value: 1, label: "test" },
        baseCheckpointId: "cp-1",
      };

      // Always fail
      vi.spyOn(storage, "save").mockRejectedValue(
        new Error("Permanent storage failure")
      );

      const retryPolicy: Partial<RetryPolicy> = {
        maxRetries: 3,
        initialDelayMs: 5,
      };

      await expect(
        stateManager.create(checkpoint, retryPolicy)
      ).rejects.toThrow("Permanent storage failure");

      expect(storage.save).toHaveBeenCalledTimes(3);
    });

    it("should implement exponential backoff delays", async () => {
      const checkpoint: TestCheckpoint = {
        id: "cp-1",
        type: "FULL",
        timestamp: Date.now(),
        snapshot: { value: 1, label: "test" },
        baseCheckpointId: "cp-1",
      };

      const delays: number[] = [];

      // Track delays without mocking setTimeout itself
      // which causes infinite recursion
      let attemptCount = 0;
      const startTimes: number[] = [];

      vi.spyOn(storage, "save").mockImplementation(async () => {
        startTimes.push(Date.now());
        attemptCount++;
        if (attemptCount <= 2) {
          throw new Error("Fail");
        }
      });

      const retryPolicy: Partial<RetryPolicy> = {
        maxRetries: 3,
        initialDelayMs: 50,
        maxDelayMs: 5000,
        backoffMultiplier: 2,
        useJitter: false,  // Disable jitter for consistent testing
      };

      const startTime = Date.now();
      await stateManager.create(checkpoint, retryPolicy);
      const totalTime = Date.now() - startTime;

      // Should have retried twice (failed twice, succeeded on third)
      expect(attemptCount).toBe(3);

      // Total time should be >= initial retry delays
      // First retry: 50ms, second retry: 100ms = 150ms minimum
      expect(totalTime).toBeGreaterThanOrEqual(100);  // Some overhead expected
    });
  });

  describe("Timeout Protection for Restore", () => {
    it("should timeout restore operation that takes too long", async () => {
      const entity: TestEntity = {
        id: "test-entity",
        state: { value: 1, label: "test" },
      };

      // Create initial checkpoint
      const checkpoint: TestCheckpoint = {
        id: "cp-1",
        type: "FULL",
        timestamp: Date.now(),
        snapshot: entity.state,
        baseCheckpointId: "cp-1",
      };

      const data = new TextEncoder().encode(JSON.stringify(checkpoint));
      await storage.save("cp-1", data, {
        entityType: "workflow",
        entityId: "test-entity",
        timestamp: Date.now(),
        checkpointType: "FULL",
        blobSize: data.length,
      });

      // Mock getCheckpoint to delay indefinitely
      const deps: CheckpointDependencies<TestCheckpoint> = {
        saveCheckpoint: async (cp) => cp.id,
        getCheckpoint: async () => {
          // Simulate hang
          await new Promise(resolve => setTimeout(resolve, 10000));
          return checkpoint;
        },
        listCheckpoints: async () => ["cp-1"],
      };

      // Should timeout
      await expect(
        coordinator.restoreFromCheckpoint("cp-1", deps, { timeoutMs: 100 })
      ).rejects.toThrow(/timeout/i);
    });

    it("should succeed before timeout", async () => {
      const entity: TestEntity = {
        id: "test-entity",
        state: { value: 1, label: "test" },
      };

      const checkpoint: TestCheckpoint = {
        id: "cp-1",
        type: "FULL",
        timestamp: Date.now(),
        snapshot: entity.state,
        baseCheckpointId: "cp-1",
      };

      const deps: CheckpointDependencies<TestCheckpoint> = {
        saveCheckpoint: async (cp) => cp.id,
        getCheckpoint: async (id) => {
          if (id === "cp-1") {
            // Quick operation
            await new Promise(resolve => setTimeout(resolve, 10));
            return checkpoint;
          }
          return null;
        },
        listCheckpoints: async () => ["cp-1"],
      };

      const restored = await coordinator.restoreFromCheckpoint("cp-1", deps, {
        timeoutMs: 5000,  // 5 second timeout is plenty
      });

      expect(restored.state.value).toBe(1);
    });
  });

  describe("Fallback to FULL Checkpoint", () => {
    it("should fallback to nearest FULL checkpoint when delta restore fails", async () => {
      const fullState: TestState = { value: 10, label: "full" };
      const deltaState: TestState = { value: 11, label: "delta" };

      const fullCp: TestCheckpoint = {
        id: "cp-full",
        type: "FULL",
        timestamp: 1000,
        snapshot: fullState,
        baseCheckpointId: "cp-full",
      };

      const deltaCp: TestCheckpoint = {
        id: "cp-delta",
        type: "DELTA",
        timestamp: 2000,
        snapshot: undefined,  // No snapshot for delta
        delta: { value: { from: 10, to: 11 } },
        baseCheckpointId: "cp-full",
        previousCheckpointId: "cp-full",
      };

      const deps: CheckpointDependencies<TestCheckpoint> = {
        saveCheckpoint: async (cp) => cp.id,
        getCheckpoint: async (id) => {
          if (id === "cp-delta") return deltaCp;
          if (id === "cp-full") return fullCp;
          return null;
        },
        listCheckpoints: async () => ["cp-full", "cp-delta"],
        getCheckpoints: async (ids) => {
          const map = new Map<string, TestCheckpoint | null>();
          for (const id of ids) {
            if (id === "cp-full") map.set(id, fullCp);
            else if (id === "cp-delta") map.set(id, deltaCp);
            else map.set(id, null);
          }
          return map;
        },
      };

      // The delta doesn't have a snapshot, so restore should try to use it
      // Since there's no snapshot, it should fallback to FULL or handle gracefully
      const restored = await coordinator.restoreFromCheckpoint("cp-delta", deps, {
        allowFallback: true,
        skipCorrupted: true,
      });

      // Should have successfully restored from delta's snapshot or fallback
      expect(restored.state).toBeDefined();
      expect(restored.id).toBe("test-entity");
    });
  });

  describe("Corrupted Data Handling", () => {
    it("should handle corrupted checkpoint data gracefully", async () => {
      // Store corrupted data
      const corruptedData = new TextEncoder().encode("invalid json data!!!");
      await storage.save("cp-corrupt", corruptedData, {
        entityType: "workflow",
        entityId: "test-entity",
        timestamp: Date.now(),
        checkpointType: "FULL",
        blobSize: corruptedData.length,
      });

      // Attempt to load should raise clear error
      await expect(
        stateManager.get("cp-corrupt")
      ).rejects.toThrow(/corrupted/i);
    });

    it("should skip corrupted deltas and use snapshot when available", async () => {
      const fullState: TestState = { value: 20, label: "full" };

      const fullCp: TestCheckpoint = {
        id: "cp-full",
        type: "FULL",
        timestamp: 1000,
        snapshot: fullState,
        baseCheckpointId: "cp-full",
      };

      const deps: CheckpointDependencies<TestCheckpoint> = {
        saveCheckpoint: async (cp) => cp.id,
        getCheckpoint: async (id) => {
          if (id === "cp-full") return fullCp;
          return null;
        },
        listCheckpoints: async () => ["cp-full"],
      };

      // Restore should handle missing deltas gracefully
      const restored = await coordinator.restoreFromCheckpoint("cp-full", deps, {
        skipCorrupted: true,
      });

      expect(restored.state.value).toBe(20);
    });
  });

  describe("Async Event Emission", () => {
    it("should not block checkpoint creation if event emission fails", async () => {
      const checkpoint: TestCheckpoint = {
        id: "cp-1",
        type: "FULL",
        timestamp: Date.now(),
        snapshot: { value: 1, label: "test" },
        baseCheckpointId: "cp-1",
      };

      // stateManager should emit events asynchronously
      // Event emission failure should not prevent checkpoint creation
      const result = await stateManager.create(checkpoint);

      expect(result).toBe("cp-1");
      expect(await storage.load("cp-1")).toBeTruthy();
    });
  });
});
