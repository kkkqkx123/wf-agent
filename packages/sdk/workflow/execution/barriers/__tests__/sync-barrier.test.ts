import { describe, it, expect, vi, beforeEach } from "vitest";
import { SyncBarrier } from "../sync-barrier.js";
import type { EventRegistry } from "../../../../shared/registry/event-registry.js";
import type { ExecutionHierarchyRegistry } from "../../../../shared/registry/execution-hierarchy-registry.js";

// Mock the event-waiter module so waitForWorkflowExecutionCompleted is controllable
vi.mock("../../utils/event/event-waiter.js", () => ({
  waitForWorkflowExecutionCompleted: vi.fn(),
  WAIT_FOREVER: -1,
}));

// Mock isTimeoutError detection
vi.mock("../../../../shared/utils/timeout/timeout-utils.js", () => ({
  isTimeoutError: vi.fn(),
}));

import { waitForWorkflowExecutionCompleted } from "../../utils/event/event-waiter.js";
import { isTimeoutError } from "../../../../shared/utils/timeout/timeout-utils.js";

// ---------- helpers ----------

const mockEventRegistry = {
  waitFor: vi.fn(),
  on: vi.fn(),
  off: vi.fn(),
  emit: vi.fn(),
} as unknown as EventRegistry;

function createMockEntity(status: string = "COMPLETED") {
  return {
    id: "exec-child-1",
    getStatus: vi.fn().mockReturnValue(status),
  } as unknown as any;
}

function createMockRegistry(entity?: any): ExecutionHierarchyRegistry {
  return {
    get: vi.fn().mockReturnValue(entity ?? undefined),
  } as unknown as ExecutionHierarchyRegistry;
}

/** Resolve waitForWorkflowExecutionCompleted immediately */
function mockWaitResolve() {
  (waitForWorkflowExecutionCompleted as any).mockResolvedValue(undefined);
}

/** Reject waitForWorkflowExecutionCompleted with a timeout-like error */
function mockWaitReject(message = "Operation timed out") {
  (waitForWorkflowExecutionCompleted as any).mockRejectedValue(new Error(message));
}

// ---------- suite ----------

describe("SyncBarrier", () => {
  let barrier: SyncBarrier;

  beforeEach(() => {
    vi.clearAllMocks();
    barrier = new SyncBarrier("parent-1", mockEventRegistry);
  });

  // ---- constructor ----

  describe("constructor", () => {
    it("should initialize with given parentExecutionId and eventManager", () => {
      const stats = barrier.getStats();
      expect(stats.parentExecutionId).toBe("parent-1");
      expect(stats.totalPaths).toBe(0);
      expect(stats.paths).toEqual([]);
    });

    it("should accept an optional executionRegistry", () => {
      const registry = createMockRegistry();
      const b = new SyncBarrier("p-1", mockEventRegistry, registry);
      const stats = b.getStats();
      expect(stats.parentExecutionId).toBe("p-1");
    });
  });

  // ---- registerPath / query methods ----

  describe("registerPath", () => {
    it("should register a fork path to execution ID mapping", () => {
      barrier.registerPath("path-1", "exec-1");

      expect(barrier.getExecutionIdByPath("path-1")).toBe("exec-1");
      expect(barrier.hasPath("path-1")).toBe(true);
    });

    it("should allow reverse lookup via getPathByExecutionId", () => {
      barrier.registerPath("path-1", "exec-1");

      expect(barrier.getPathByExecutionId("exec-1")).toBe("path-1");
    });

    it("should overwrite an existing mapping and log a warning", () => {
      // First registration
      barrier.registerPath("path-1", "exec-1");
      expect(barrier.getExecutionIdByPath("path-1")).toBe("exec-1");

      // Overwrite
      barrier.registerPath("path-1", "exec-2");
      expect(barrier.getExecutionIdByPath("path-1")).toBe("exec-2");
      // Reverse mapping should also be updated
      expect(barrier.getPathByExecutionId("exec-2")).toBe("path-1");
      // Old reverse mapping should be gone
      expect(barrier.getPathByExecutionId("exec-1")).toBeUndefined();
    });

    it("should return undefined for unregistered paths", () => {
      expect(barrier.getExecutionIdByPath("unknown")).toBeUndefined();
      expect(barrier.hasPath("unknown")).toBe(false);
    });
  });

  // ---- collection methods ----

  describe("getAllPathIds / getAllExecutionIds", () => {
    it("should return all registered path IDs", () => {
      barrier.registerPath("p1", "e1");
      barrier.registerPath("p2", "e2");
      barrier.registerPath("p3", "e3");

      expect(barrier.getAllPathIds()).toEqual(["p1", "p2", "p3"]);
    });

    it("should return all registered execution IDs", () => {
      barrier.registerPath("p1", "e1");
      barrier.registerPath("p2", "e2");

      expect(barrier.getAllExecutionIds()).toEqual(["e1", "e2"]);
    });

    it("should return empty arrays when no paths registered", () => {
      expect(barrier.getAllPathIds()).toEqual([]);
      expect(barrier.getAllExecutionIds()).toEqual([]);
    });
  });

  // ---- waitForBranchCompletion ----

  describe("waitForBranchCompletion", () => {
    const branchPathId = "branch-1";
    const branchExecId = "exec-branch-1";

    beforeEach(() => {
      barrier.registerPath(branchPathId, branchExecId);
    });

    it("should throw if fork path is not registered", async () => {
      await expect(barrier.waitForBranchCompletion("non-existent")).rejects.toThrow(
        "Fork path not registered: non-existent",
      );
    });

    it("should resolve and return entity on successful completion", async () => {
      const entity = createMockEntity("COMPLETED");
      const registry = createMockRegistry(entity);
      const b = new SyncBarrier("parent-1", mockEventRegistry, registry);
      b.registerPath(branchPathId, branchExecId);

      mockWaitResolve();

      const result = await b.waitForBranchCompletion(branchPathId);
      expect(result.getStatus()).toBe("COMPLETED");
      expect(waitForWorkflowExecutionCompleted).toHaveBeenCalledWith(
        mockEventRegistry,
        branchExecId,
        -1, // WAIT_FOREVER
      );
    });

    it("should pass timeout in milliseconds to waitForWorkflowExecutionCompleted", async () => {
      const entity = createMockEntity("COMPLETED");
      const registry = createMockRegistry(entity);
      const b = new SyncBarrier("parent-1", mockEventRegistry, registry);
      b.registerPath(branchPathId, branchExecId);

      mockWaitResolve();

      await b.waitForBranchCompletion(branchPathId, 10);
      expect(waitForWorkflowExecutionCompleted).toHaveBeenCalledWith(
        mockEventRegistry,
        branchExecId,
        10000, // 10s → 10000ms
      );
    });

    it("should throw if execution entity is not found in registry after completion", async () => {
      const registry = createMockRegistry(undefined); // entity not found
      const b = new SyncBarrier("parent-1", mockEventRegistry, registry);
      b.registerPath(branchPathId, branchExecId);

      mockWaitResolve();

      await expect(b.waitForBranchCompletion(branchPathId)).rejects.toThrow(
        `Failed to get execution entity for executionId: ${branchExecId}`,
      );
    });

    it("should throw timeout error when wait times out", async () => {
      mockWaitReject("Timed out: 5000ms exceeded");
      (isTimeoutError as any).mockReturnValue(true);

      await expect(barrier.waitForBranchCompletion(branchPathId, 5)).rejects.toThrow(
        "Timed out: 5000ms exceeded",
      );
    });

    it("should re-throw non-timeout error from waitForWorkflowExecutionCompleted", async () => {
      mockWaitReject("Event registry error");
      (isTimeoutError as any).mockReturnValue(false);

      await expect(barrier.waitForBranchCompletion(branchPathId)).rejects.toThrow(
        "Event registry error",
      );
    });

    it("should work when no executionRegistry is provided", async () => {
      // barrier was created without executionRegistry
      mockWaitResolve();

      await expect(barrier.waitForBranchCompletion(branchPathId)).rejects.toThrow(
        `Failed to get execution entity`,
      );
    });
  });

  // ---- waitForMultipleBranches ----

  describe("waitForMultipleBranches", () => {
    it("should return all successful results", async () => {
      const entityA = createMockEntity("COMPLETED");
      const entityB = createMockEntity("COMPLETED");
      const registry = createMockRegistry(undefined);
      // Make get return different entities based on executionId
      (registry.get as any).mockImplementation((id: string) => {
        if (id === "exec-a") return entityA;
        if (id === "exec-b") return entityB;
        return undefined;
      });

      const b = new SyncBarrier("parent-1", mockEventRegistry, registry);
      b.registerPath("path-a", "exec-a");
      b.registerPath("path-b", "exec-b");

      mockWaitResolve();

      const result = await b.waitForMultipleBranches(["path-a", "path-b"]);
      expect(result.successful.size).toBe(2);
      expect(result.successful.get("path-a")?.getStatus()).toBe("COMPLETED");
      expect(result.successful.get("path-b")?.getStatus()).toBe("COMPLETED");
      expect(result.failed).toHaveLength(0);
      expect(result.totalRequested).toBe(2);
    });

    it("should collect failed branches separately", async () => {
      const entityA = createMockEntity("COMPLETED");
      const registry = createMockRegistry(undefined);
      (registry.get as any).mockImplementation((id: string) => {
        if (id === "exec-a") return entityA;
        return undefined;
      });

      const b = new SyncBarrier("parent-1", mockEventRegistry, registry);
      b.registerPath("path-a", "exec-a");
      b.registerPath("path-b", "exec-b");

      // First call succeeds, second call rejects
      (waitForWorkflowExecutionCompleted as any)
        .mockResolvedValueOnce(undefined)
        .mockRejectedValueOnce(new Error("Timed out: 5000ms exceeded"));
      (isTimeoutError as any).mockReturnValue(true);

      const result = await b.waitForMultipleBranches(["path-a", "path-b"]);
      expect(result.successful.size).toBe(1);
      expect(result.successful.get("path-a")?.getStatus()).toBe("COMPLETED");
      expect(result.failed).toHaveLength(1);
      expect(result.failed[0]!.pathId).toBe("path-b");
      expect(result.failed[0]!.error).toBeInstanceOf(Error);
      expect(result.totalRequested).toBe(2);
    });

    it("should handle empty input", async () => {
      const result = await barrier.waitForMultipleBranches([]);
      expect(result.successful.size).toBe(0);
      expect(result.failed).toHaveLength(0);
      expect(result.totalRequested).toBe(0);
    });
  });

  // ---- clear ----

  describe("clear", () => {
    it("should remove all mappings", () => {
      barrier.registerPath("p1", "e1");
      barrier.registerPath("p2", "e2");
      expect(barrier.getAllPathIds()).toHaveLength(2);

      barrier.clear();

      expect(barrier.getAllPathIds()).toHaveLength(0);
      expect(barrier.getExecutionIdByPath("p1")).toBeUndefined();
      expect(barrier.getPathByExecutionId("e1")).toBeUndefined();
    });

    it("should be idempotent", () => {
      barrier.clear();
      barrier.clear();
      expect(barrier.getAllPathIds()).toHaveLength(0);
    });
  });

  // ---- getStats ----

  describe("getStats", () => {
    it("should return correct statistics", () => {
      barrier.registerPath("p1", "e1");
      barrier.registerPath("p2", "e2");

      const stats = barrier.getStats();
      expect(stats.parentExecutionId).toBe("parent-1");
      expect(stats.totalPaths).toBe(2);
      expect(stats.paths).toEqual([
        { forkPathId: "p1", executionId: "e1" },
        { forkPathId: "p2", executionId: "e2" },
      ]);
    });
  });

  // ---- getPathByExecutionId ----

  describe("getPathByExecutionId", () => {
    it("should return fork path ID for a registered execution ID", () => {
      barrier.registerPath("path-x", "exec-x");
      expect(barrier.getPathByExecutionId("exec-x")).toBe("path-x");
    });

    it("should return undefined for an unregistered execution ID", () => {
      expect(barrier.getPathByExecutionId("unknown")).toBeUndefined();
    });
  });
});
