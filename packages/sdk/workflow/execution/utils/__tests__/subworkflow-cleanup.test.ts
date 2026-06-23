/**
 * Subworkflow Cleanup Unit Tests
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { cleanupFailedSubworkflow } from "../subworkflow-cleanup.js";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { ExecutionHierarchyRegistry } from "../../../../shared/registry/execution-hierarchy-registry.js";

// Mock the contextual logger
vi.mock("../../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

describe("cleanupFailedSubworkflow", () => {
  let mockChildEntity: WorkflowExecutionEntity;
  let mockParentEntity: WorkflowExecutionEntity;
  let mockRegistry: ExecutionHierarchyRegistry;

  beforeEach(() => {
    vi.clearAllMocks();

    mockChildEntity = {
      id: "child-exec-1",
      stop: vi.fn(),
    } as unknown as WorkflowExecutionEntity;

    mockParentEntity = {
      id: "parent-exec-1",
      unregisterChild: vi.fn(),
    } as unknown as WorkflowExecutionEntity;

    mockRegistry = {
      unregister: vi.fn(),
    } as unknown as ExecutionHierarchyRegistry;
  });

  describe("basic cleanup flow", () => {
    it("should stop the child execution", async () => {
      await cleanupFailedSubworkflow(mockChildEntity, mockParentEntity, mockRegistry);

      expect(mockChildEntity.stop).toHaveBeenCalled();
    });

    it("should unregister from hierarchy registry", async () => {
      await cleanupFailedSubworkflow(mockChildEntity, mockParentEntity, mockRegistry);

      expect(mockRegistry.unregister).toHaveBeenCalledWith("child-exec-1");
    });

    it("should remove child from parent's children list", async () => {
      await cleanupFailedSubworkflow(mockChildEntity, mockParentEntity, mockRegistry);

      expect(mockParentEntity.unregisterChild).toHaveBeenCalledWith("child-exec-1", "WORKFLOW");
    });
  });

  describe("registry handling", () => {
    it("should handle null registry gracefully", async () => {
      await cleanupFailedSubworkflow(
        mockChildEntity,
        mockParentEntity,
        null as unknown as ExecutionHierarchyRegistry,
      );

      expect(mockChildEntity.stop).toHaveBeenCalled();
      expect(mockParentEntity.unregisterChild).toHaveBeenCalled();
    });

    it("should handle undefined registry gracefully", async () => {
      await cleanupFailedSubworkflow(
        mockChildEntity,
        mockParentEntity,
        undefined as unknown as ExecutionHierarchyRegistry,
      );

      expect(mockChildEntity.stop).toHaveBeenCalled();
      expect(mockParentEntity.unregisterChild).toHaveBeenCalled();
    });

    it("should handle registry without unregister method", async () => {
      const registryWithoutUnregister = {} as ExecutionHierarchyRegistry;

      await cleanupFailedSubworkflow(mockChildEntity, mockParentEntity, registryWithoutUnregister);

      expect(mockChildEntity.stop).toHaveBeenCalled();
      expect(mockParentEntity.unregisterChild).toHaveBeenCalled();
    });
  });

  describe("error handling", () => {
    it("should not throw when stop throws an error", async () => {
      (mockChildEntity.stop as ReturnType<typeof vi.fn>).mockImplementation(() => {
        throw new Error("Stop failed");
      });

      await expect(
        cleanupFailedSubworkflow(mockChildEntity, mockParentEntity, mockRegistry),
      ).resolves.not.toThrow();
    });

    it("should not throw when registry.unregister throws an error", async () => {
      (mockRegistry.unregister as ReturnType<typeof vi.fn>).mockImplementation(() => {
        throw new Error("Unregister failed");
      });

      await expect(
        cleanupFailedSubworkflow(mockChildEntity, mockParentEntity, mockRegistry),
      ).resolves.not.toThrow();
    });

    it("should not throw when parent.unregisterChild throws an error", async () => {
      (mockParentEntity.unregisterChild as ReturnType<typeof vi.fn>).mockImplementation(() => {
        throw new Error("Unregister child failed");
      });

      await expect(
        cleanupFailedSubworkflow(mockChildEntity, mockParentEntity, mockRegistry),
      ).resolves.not.toThrow();
    });

    it("should continue cleanup even if stop fails", async () => {
      (mockChildEntity.stop as ReturnType<typeof vi.fn>).mockImplementation(() => {
        throw new Error("Stop failed");
      });

      await cleanupFailedSubworkflow(mockChildEntity, mockParentEntity, mockRegistry);

      expect(mockRegistry.unregister).toHaveBeenCalled();
      expect(mockParentEntity.unregisterChild).toHaveBeenCalled();
    });
  });

  describe("idempotency", () => {
    it("should be safe to call multiple times", async () => {
      await cleanupFailedSubworkflow(mockChildEntity, mockParentEntity, mockRegistry);
      await cleanupFailedSubworkflow(mockChildEntity, mockParentEntity, mockRegistry);
      await cleanupFailedSubworkflow(mockChildEntity, mockParentEntity, mockRegistry);

      expect(mockChildEntity.stop).toHaveBeenCalledTimes(3);
      expect(mockRegistry.unregister).toHaveBeenCalledTimes(3);
      expect(mockParentEntity.unregisterChild).toHaveBeenCalledTimes(3);
    });
  });

  describe("cleanup order", () => {
    it("should perform cleanup in correct order", async () => {
      const order: string[] = [];

      (mockChildEntity.stop as ReturnType<typeof vi.fn>).mockImplementation(() => {
        order.push("stop");
      });
      (mockRegistry.unregister as ReturnType<typeof vi.fn>).mockImplementation(() => {
        order.push("unregister");
      });
      (mockParentEntity.unregisterChild as ReturnType<typeof vi.fn>).mockImplementation(() => {
        order.push("removeFromParent");
      });

      await cleanupFailedSubworkflow(mockChildEntity, mockParentEntity, mockRegistry);

      expect(order).toEqual(["stop", "unregister", "removeFromParent"]);
    });
  });
});
