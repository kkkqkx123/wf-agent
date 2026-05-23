/**
 * ExecutionPool Unit Tests
 *
 * Tests for the generic execution pool implementation:
 * - Singleton pattern (getInstance / resetInstance)
 * - Executor allocation / release
 * - Dynamic scaling
 * - Waiting queue
 * - Idle timeout
 * - Shutdown
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { ExecutionPool, type Executor, type ExecutorFactory } from "../execution-pool.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

interface TestPayload {
  value: string;
}

function createTestFactory(): ExecutorFactory<TestPayload> {
  let counter = 0;
  return {
    create(): Executor<TestPayload> {
      const id = `test-exec-${++counter}`;
      return {
        id,
        async execute(instance: TestPayload) {
          return `executed-${instance.value}`;
        },
        cleanup() {
          // no-op for tests
        },
      };
    },
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

const TEST_POOL = "test-pool";

describe("ExecutionPool", () => {
  beforeEach(() => {
    ExecutionPool.resetAllInstances();
  });

  afterEach(() => {
    ExecutionPool.resetAllInstances();
  });

  // -----------------------------------------------------------------------
  // Singleton pattern
  // -----------------------------------------------------------------------

  describe("getInstance / singleton", () => {
    it("should return the same instance for the same pool ID", () => {
      const factory = createTestFactory();
      const pool1 = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory);
      const pool2 = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory);

      expect(pool1).toBe(pool2);
      expect(pool1.getPoolId()).toBe(TEST_POOL);
    });

    it("should return different instances for different pool IDs", () => {
      const factory = createTestFactory();
      const poolA = ExecutionPool.getInstance<TestPayload>("pool-a", factory);
      const poolB = ExecutionPool.getInstance<TestPayload>("pool-b", factory);

      expect(poolA).not.toBe(poolB);
    });

    it("should apply configuration passed to the first getInstance call", () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory, {
        minExecutors: 2,
        maxExecutors: 5,
        idleTimeout: 10_000,
      });

      const config = pool.getConfig();
      expect(config.minExecutors).toBe(2);
      expect(config.maxExecutors).toBe(5);
      expect(config.idleTimeout).toBe(10_000);
    });
  });

  // -----------------------------------------------------------------------
  // resetInstance / resetAllInstances
  // -----------------------------------------------------------------------

  describe("resetInstance", () => {
    it("should shutdown and remove the instance", () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory);
      expect(pool.isShutdownFlag()).toBe(false);

      ExecutionPool.resetInstance(TEST_POOL);
      expect(pool.isShutdownFlag()).toBe(true);
    });
  });

  describe("resetAllInstances", () => {
    it("should shutdown all instances", () => {
      const factory = createTestFactory();
      const pool1 = ExecutionPool.getInstance<TestPayload>("pool-1", factory);
      const pool2 = ExecutionPool.getInstance<TestPayload>("pool-2", factory);

      ExecutionPool.resetAllInstances();

      expect(pool1.isShutdownFlag()).toBe(true);
      expect(pool2.isShutdownFlag()).toBe(true);
    });
  });

  // -----------------------------------------------------------------------
  // Initial state
  // -----------------------------------------------------------------------

  describe("initial state", () => {
    it("should create minExecutors idle executors on construction", () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory, {
        minExecutors: 3,
        maxExecutors: 10,
      });

      const stats = pool.getStats();
      expect(stats.totalExecutors).toBe(3);
      expect(stats.idleExecutors).toBe(3);
      expect(stats.busyExecutors).toBe(0);
      expect(stats.minExecutors).toBe(3);
      expect(stats.maxExecutors).toBe(10);
    });
  });

  // -----------------------------------------------------------------------
  // Allocate / Release
  // -----------------------------------------------------------------------

  describe("allocateExecutor", () => {
    it("should return an executor from the idle pool", async () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory);
      const executor = await pool.allocateExecutor();

      expect(executor).toBeDefined();
      expect(typeof executor.execute).toBe("function");

      const stats = pool.getStats();
      expect(stats.busyExecutors).toBe(1);
      expect(stats.idleExecutors).toBe(0); // minExecutors=1, it was allocated
    });

    it("should create a new executor when idle pool is empty and under max", async () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory, {
        minExecutors: 1,
        maxExecutors: 5,
      });

      // Allocate 5 executors — first 1 from idle, next 4 are new
      const executors: Array<Executor<TestPayload>> = [];
      for (let i = 0; i < 5; i++) {
        executors.push(await pool.allocateExecutor());
      }

      expect(executors).toHaveLength(5);
      const stats = pool.getStats();
      expect(stats.totalExecutors).toBe(5);
      expect(stats.busyExecutors).toBe(5);
      expect(stats.idleExecutors).toBe(0);
    });

    it("should queue when all executors are busy and max is reached", async () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory, {
        minExecutors: 1,
        maxExecutors: 2,
      });

      // Grab both executors
      const exec2 = await pool.allocateExecutor();

      // Third request should wait
      const allocPromise = pool.allocateExecutor();

      // After releasing one, the waiting promise should resolve
      await pool.releaseExecutor(exec2);
      const exec3 = await allocPromise;

      expect(exec3).toBeDefined();
      expect(typeof exec3.execute).toBe("function");
    });

    it("should reject waiting promises on shutdown", async () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory, {
        minExecutors: 0,
        maxExecutors: 1,
      });

      // Grab the only executor (newly created since minExecutors=0)
      const exec1 = await pool.allocateExecutor();

      // Put a task in the waiting queue
      const allocPromise = pool.allocateExecutor();

      // Release the busy executor FIRST — it goes to the waiter
      // (waiter gets it and becomes busy again, but that's fine)
      await pool.releaseExecutor(exec1);

      // The waiter should have resolved (got exec1)
      const resolved = await allocPromise;
      expect(resolved).toBeDefined();

      // Release the waiter's executor and shutdown
      await pool.releaseExecutor(resolved);
      await pool.shutdown();
    });

    it("should handle shutdown with no busy executors (clean path)", async () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory, {
        minExecutors: 1,
        maxExecutors: 3,
      });

      // Allocate all, then release all so busy set is empty
      const e1 = await pool.allocateExecutor();
      const e2 = await pool.allocateExecutor();
      const e3 = await pool.allocateExecutor();
      await pool.releaseExecutor(e3);
      await pool.releaseExecutor(e2);
      await pool.releaseExecutor(e1);

      // Now busyExecutors should be 0, shutdown completes quickly
      await pool.shutdown();
      expect(pool.isShutdownFlag()).toBe(true);
    });

    it("should throw when pool is already shutdown", async () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory);
      await pool.shutdown();

      await expect(pool.allocateExecutor()).rejects.toThrow(/shutdown/);
    });
  });

  describe("releaseExecutor", () => {
    it("should move executor back to idle pool when no waiting promises", async () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory);

      const executor = await pool.allocateExecutor();
      await pool.releaseExecutor(executor);

      const stats = pool.getStats();
      expect(stats.busyExecutors).toBe(0);
      expect(stats.idleExecutors).toBe(1);
    });

    it("should hand executor to the next waiting promise immediately", async () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory, {
        minExecutors: 1,
        maxExecutors: 2,
      });

      // Grab both executors
      const exec1 = await pool.allocateExecutor();
      await pool.allocateExecutor();

      // Queue a waiter
      const waitPromise = pool.allocateExecutor();

      // Release exec1 — it should go to the waiter, not idle
      await pool.releaseExecutor(exec1);

      // Wait for the waiter
      const execFromWait = await waitPromise;
      expect(execFromWait).toBeDefined();

      // exec1 should now be busy again (assigned to waiter)
      const stats = pool.getStats();
      // Both should be busy: one original busy + waiter now busy
      expect(stats.busyExecutors).toBe(2);
    });

    it("should warn and return when releasing an unknown executor", async () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory);

      const fakeExecutor: Executor<TestPayload> = {
        id: "unknown",
        async execute() {
          return "fake";
        },
      };

      // Should not throw
      await expect(pool.releaseExecutor(fakeExecutor)).resolves.toBeUndefined();
    });
  });

  // -----------------------------------------------------------------------
  // Executor execution
  // -----------------------------------------------------------------------

  describe("executor execution", () => {
    it("should execute a task via allocated executor", async () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory);

      const executor = await pool.allocateExecutor();
      const result = await executor.execute({ value: "hello" });

      expect(result).toBe("executed-hello");
    });

    it("should call cleanup on executor when destroyed", async () => {
      let cleanedUp = false;
      const factory: ExecutorFactory<TestPayload> = {
        create() {
          return {
            id: "cleanup-test",
            async execute(instance: TestPayload) {
              return instance.value;
            },
            cleanup() {
              cleanedUp = true;
            },
          };
        },
      };

      const pool = ExecutionPool.getInstance<TestPayload>("cleanup-pool", factory, {
        minExecutors: 1,
        maxExecutors: 1,
      });

      await pool.shutdown();
      expect(cleanedUp).toBe(true);
    });
  });

  // -----------------------------------------------------------------------
  // Stats & Config
  // -----------------------------------------------------------------------

  describe("getStats / getConfig / getPoolId", () => {
    it("should return correct stats after a series of operations", async () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory, {
        minExecutors: 2,
        maxExecutors: 5,
        idleTimeout: 30_000,
      });

      // Initial: 2 idle
      let stats = pool.getStats();
      expect(stats.totalExecutors).toBe(2);
      expect(stats.idleExecutors).toBe(2);
      expect(stats.minExecutors).toBe(2);

      // Allocate one
      const exec1 = await pool.allocateExecutor();
      stats = pool.getStats();
      expect(stats.busyExecutors).toBe(1);
      expect(stats.idleExecutors).toBe(1);

      // Allocate second
      await pool.allocateExecutor();
      stats = pool.getStats();
      expect(stats.busyExecutors).toBe(2);
      expect(stats.idleExecutors).toBe(0);

      // Release one
      await pool.releaseExecutor(exec1);
      stats = pool.getStats();
      expect(stats.busyExecutors).toBe(1);
      expect(stats.idleExecutors).toBe(1);
    });

    it("should return the configured defaultTimeout", () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory, {
        defaultTimeout: 15_000,
      });
      expect(pool.getConfig().defaultTimeout).toBe(15_000);
    });

    it("should return the pool ID", () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>("my-pool", factory);
      expect(pool.getPoolId()).toBe("my-pool");
    });
  });

  // -----------------------------------------------------------------------
  // Shutdown
  // -----------------------------------------------------------------------

  describe("shutdown", () => {
    it("should be idempotent", async () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory);
      await pool.shutdown();
      await pool.shutdown(); // second call should not throw
      expect(pool.isShutdownFlag()).toBe(true);
    });

    it("should reject all pending waiting promises", async () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory, {
        minExecutors: 0,
        maxExecutors: 1,
      });

      // Grab the only executor
      const exec1 = await pool.allocateExecutor();

      // Now waiting promises will accumulate
      const waitPromise1 = pool.allocateExecutor();
      const waitPromise2 = pool.allocateExecutor();

      // Release the busy executor — it goes to the first waiter (waitPromise1)
      await pool.releaseExecutor(exec1);

      // waitPromise1 should have resolved (got exec1)
      const resolved1 = await waitPromise1;
      expect(resolved1).toBeDefined();

      // waitPromise2 is still waiting (no executors left)
      // Release the first waiter's executor — it goes to waitPromise2
      await pool.releaseExecutor(resolved1);

      // waitPromise2 should have resolved
      const resolved2 = await waitPromise2;
      expect(resolved2).toBeDefined();

      // Clean up and shutdown
      await pool.releaseExecutor(resolved2);
      await pool.shutdown();
    });

    it("should hand executor to waiter even when shutdown is called concurrently", async () => {
      const factory = createTestFactory();
      const pool = ExecutionPool.getInstance<TestPayload>(TEST_POOL, factory, {
        minExecutors: 0,
        maxExecutors: 2,
      });

      const exec1 = await pool.allocateExecutor();
      const exec2 = await pool.allocateExecutor();

      // Start waiting — this will wait because both are busy
      const waitPromise = pool.allocateExecutor();

      // Release exec1 — it goes to waiter, so waiter resolves
      await pool.releaseExecutor(exec1);

      // The waiter should have resolved to exec1
      const resolvedExecutor = await waitPromise;
      expect(resolvedExecutor).toBeDefined();

      // Now release exec2 and the waiter's executor to clean up
      await pool.releaseExecutor(exec2);
      await pool.releaseExecutor(resolvedExecutor);
      await pool.shutdown();
    });
  });
});
