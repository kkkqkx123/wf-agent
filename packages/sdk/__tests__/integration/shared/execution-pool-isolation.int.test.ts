/**
 * Integration Test: Execution Pool and Global Context Isolation
 *
 * Tests the execution pool's behavior across multiple GlobalContext instances
 * and validates execution isolation between different SDK instances.
 *
 * ⚠️ DESIGN ISSUE DETECTION:
 * - ExecutionPool uses global static Map for singleton pattern
 * - Multiple GlobalContext instances will share same ExecutionPool
 * - This breaks execution isolation across SDK instances
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { ExecutionPool, type Executor, type ExecutorFactory } from "@sdk/shared/execution/execution-pool.js";

describe("Integration: Execution Pool and Context Isolation", () => {
  beforeEach(() => {
    ExecutionPool.resetAllInstances();
  });

  afterEach(() => {
    ExecutionPool.resetAllInstances();
  });

  // Mock executor for testing
  interface TestExecutionInstance {
    id: string;
    contextId: string;
    taskName: string;
  }

  function createMockExecutorFactory(contextId: string): ExecutorFactory<TestExecutionInstance> {
    return {
      create(): Executor<TestExecutionInstance> {
        return {
          id: `executor-${contextId}`,
          async execute(instance: TestExecutionInstance) {
            return {
              completed: true,
              contextId,
              instanceId: instance.id,
              taskName: instance.taskName,
            };
          },
        };
      },
    };
  }

  describe("Scenario 1: Single ExecutionPool Lifecycle", () => {
    /**
     * BUSINESS SCENARIO:
     * 1. Single SDK instance creates an ExecutionPool
     * 2. Multiple tasks are queued and executed
     * 3. Each task completes and executor is returned to pool
     * 4. Pool maintains consistency
     */
    it("should manage executor lifecycle correctly", async () => {
      const factory = createMockExecutorFactory("context-1");
      const pool = ExecutionPool.getInstance("agent-pool", factory, {
        minExecutors: 1,
        maxExecutors: 3,
      });

      expect(pool.getPoolId()).toBe("agent-pool");

      const stats1 = pool.getStats();
      expect(stats1.totalExecutors).toBeGreaterThanOrEqual(1);

      // Execute multiple tasks
      const instances: TestExecutionInstance[] = [
        { id: "task-1", contextId: "context-1", taskName: "Task 1" },
        { id: "task-2", contextId: "context-1", taskName: "Task 2" },
        { id: "task-3", contextId: "context-1", taskName: "Task 3" },
      ];

      const results = await Promise.all(
        instances.map(async (instance) => {
          const executor = await pool.allocateExecutor();
          try {
            const result = await executor.execute(instance);
            await pool.releaseExecutor(executor);
            return result;
          } catch (err) {
            await pool.releaseExecutor(executor);
            throw err;
          }
        })
      );

      expect(results).toHaveLength(3);
      results.forEach((result, idx) => {
        expect(result).toMatchObject({
          completed: true,
          contextId: "context-1",
          taskName: `Task ${idx + 1}`,
        });
      });
    });
  });

  describe("Scenario 2: Multiple SDK Instances with Same PoolId", () => {
    /**
     * BUSINESS SCENARIO (PROBLEMATIC):
     * 1. Two SDK instances are initialized independently
     * 2. Both create ExecutionPool with the same poolId
     * 3. ❌ They will SHARE the same pool (global singleton)
     * 4. ❌ Executions from SDK1 will use executors created by SDK1
     * 5. ❌ When SDK2 tries to execute, it gets executors from SDK1 (wrong context)
     *
     * This is a DESIGN FLAW that needs fixing.
     */
    it("should expose execution isolation problem with shared pool id", async () => {
      // SDK instance 1 creates an executor pool
      const factory1 = createMockExecutorFactory("sdk-instance-1");
      const pool1 = ExecutionPool.getInstance("shared-pool", factory1, {
        minExecutors: 1,
        maxExecutors: 2,
      });

      // SDK instance 2 tries to create ANOTHER pool with same ID
      const factory2 = createMockExecutorFactory("sdk-instance-2");
      const pool2 = ExecutionPool.getInstance("shared-pool", factory2, {
        minExecutors: 1,
        maxExecutors: 2,
      });

      // ❌ ISSUE: pool1 and pool2 are THE SAME OBJECT
      expect(pool1).toBe(pool2);

      // ❌ This means SDK-2's factory is IGNORED
      // All executors will be from SDK-1's factory
      const instance2 = { id: "task-2", contextId: "sdk-instance-2", taskName: "SDK2 Task" };
      const executor = await pool1.allocateExecutor();
      const result = await executor.execute(instance2);
      await pool1.releaseExecutor(executor);

      // ❌ The executor came from factory1, not factory2
      expect(result.contextId).toBe("sdk-instance-1"); // Wrong! Should be "sdk-instance-2"

      // This demonstrates the isolation issue
    });
  });

  describe("Scenario 3: Different PoolIds for Isolation", () => {
    /**
     * BUSINESS SCENARIO (WORKAROUND):
     * 1. Two SDK instances use DIFFERENT poolIds
     * 2. Each gets its own pool (workaround to avoid sharing)
     * 3. ✅ Execution isolation works
     * 4. But this requires unique poolIds (tedious and error-prone)
     */
    it("should isolate pools when different pool ids are used", async () => {
      const factory1 = createMockExecutorFactory("sdk-1");
      const pool1 = ExecutionPool.getInstance("pool-sdk-1", factory1, {
        minExecutors: 1,
        maxExecutors: 2,
      });

      const factory2 = createMockExecutorFactory("sdk-2");
      const pool2 = ExecutionPool.getInstance("pool-sdk-2", factory2, {
        minExecutors: 1,
        maxExecutors: 2,
      });

      // ✅ Different pools
      expect(pool1).not.toBe(pool2);
      expect(pool1.getPoolId()).toBe("pool-sdk-1");
      expect(pool2.getPoolId()).toBe("pool-sdk-2");

      // Execute on pool1
      const executor1 = await pool1.allocateExecutor();
      const result1 = await executor1.execute(
        { id: "t1", contextId: "sdk-1", taskName: "Task on SDK1" },
      );
      await pool1.releaseExecutor(executor1);

      // Execute on pool2
      const executor2 = await pool2.allocateExecutor();
      const result2 = await executor2.execute(
        { id: "t2", contextId: "sdk-2", taskName: "Task on SDK2" },
      );
      await pool2.releaseExecutor(executor2);

      // ✅ Results come from correct context
      expect(result1.contextId).toBe("sdk-1");
      expect(result2.contextId).toBe("sdk-2");

      // But this requires manual poolId management (tedious)
    });
  });

  describe("Scenario 4: Executor Reuse Across Contexts", () => {
    /**
     * BUSINESS SCENARIO:
     * 1. ExecutionPool reuses executors for efficiency
     * 2. When a task completes, executor returns to idle queue
     * 3. Next task can reuse same executor
     * 4. But executor's internal state must be CLEAN
     */
    it("should verify executor state isolation", async () => {
      const executorStates: Map<string, TestExecutionInstance[]> = new Map();

      const factory: ExecutorFactory<TestExecutionInstance> = {
        create(): Executor<TestExecutionInstance> {
          const id = `executor-${Date.now()}-${Math.random()}`;
          return {
            id,
            async execute(instance: TestExecutionInstance) {
              // Track which instances this executor has processed
              if (!executorStates.has(id)) {
                executorStates.set(id, []);
              }
              executorStates.get(id)!.push(instance);

              return { success: true, executorId: id, instance };
            },
          };
        },
      };

      const pool = ExecutionPool.getInstance("isolation-test", factory, {
        minExecutors: 2,
        maxExecutors: 2,
      });

      // Execute multiple tasks
      const tasks = [
        { id: "task-1", contextId: "ctx-1", taskName: "Task 1" },
        { id: "task-2", contextId: "ctx-1", taskName: "Task 2" },
        { id: "task-3", contextId: "ctx-1", taskName: "Task 3" },
        { id: "task-4", contextId: "ctx-1", taskName: "Task 4" },
      ];

      for (const task of tasks) {
        const executor = await pool.allocateExecutor();
        try {
          await executor.execute(task);
          await pool.releaseExecutor(executor);
        } catch (err) {
          await pool.releaseExecutor(executor);
          throw err;
        }
      }

      // Each executor should process multiple tasks without state bleed
      const totalExecuted = Array.from(executorStates.values()).reduce(
        (sum, tasks) => sum + tasks.length,
        0
      );
      expect(totalExecuted).toBe(4);

      // No executor should have cross-context contamination
      for (const [executorId, executedTasks] of executorStates) {
        // All tasks should be from same context (ctx-1 in this case)
        const contexts = new Set(executedTasks.map((t) => t.contextId));
        expect(contexts.size).toBeLessThanOrEqual(1);
      }
    });
  });

  describe("Scenario 5: Pool Configuration Per Context", () => {
    /**
     * BUSINESS SCENARIO:
     * Different SDK instances might have different pool configurations:
     * - SDK 1: minExecutors=2, maxExecutors=5
     * - SDK 2: minExecutors=1, maxExecutors=3
     *
     * ❌ With global singleton, only first config wins
     */
    it("should expose configuration conflict issue", () => {
      const factory1 = createMockExecutorFactory("sdk-1");
      const pool1 = ExecutionPool.getInstance("shared", factory1, {
        minExecutors: 2,
        maxExecutors: 5,
      });

      const config1 = pool1.getConfig();
      expect(config1.minExecutors).toBe(2);
      expect(config1.maxExecutors).toBe(5);

      // Second SDK tries to set different config
      const factory2 = createMockExecutorFactory("sdk-2");
      const pool2 = ExecutionPool.getInstance("shared", factory2, {
        minExecutors: 1,  // ❌ This will be IGNORED
        maxExecutors: 3,  // ❌ This will be IGNORED
      });

      const config2 = pool2.getConfig();
      // ❌ Still has config1's values
      expect(config2.minExecutors).toBe(2);
      expect(config2.maxExecutors).toBe(5);

      // Configuration conflict - SDK 2's config is silently ignored
    });
  });

  describe("Design Issue: Global ExecutionPool Singleton", () => {
    /**
     * This test documents the global singleton issue with ExecutionPool
     * and suggests the correct design.
     *
     * CURRENT PROBLEM:
     * - ExecutionPool.instances is a static Map
     * - Keyed only by poolId, not by SDK instance
     * - Multiple SDK instances with same poolId will collide
     *
     * REQUIRED FIX:
     * Option 1: Move ExecutionPool to DI Container
     *   - Each GlobalContext gets its own pool instances
     *   - No global state
     *
     * Option 2: Include SDK instance ID in poolId
     *   - Requires explicit context awareness
     *   - Tedious for users
     *
     * Option 3: Pass ExecutionPool factory from GlobalContext
     *   - Pool is created per context
     *   - Maintains isolation
     */
    it("documents the global singleton problem and required fix", () => {
      // Current problematic pattern:
      // ExecutionPool.instances = new Map<string, ExecutionPool>() // ← Global static

      // Correct pattern would be:
      // class GlobalContext {
      //   private pools = new Map<string, ExecutionPool>();
      //
      //   getExecutionPool(poolId, factory, config) {
      //     if (!this.pools.has(poolId)) {
      //       this.pools.set(poolId, new ExecutionPool(poolId, factory, config));
      //     }
      //     return this.pools.get(poolId);
      //   }
      // }

      // This way:
      // 1. Each GlobalContext has independent pools
      // 2. No global state pollution
      // 3. SDK instances remain isolated
      // 4. Pool IDs can be reused safely

      expect(true).toBe(true); // Documentation test
    });
  });
});
