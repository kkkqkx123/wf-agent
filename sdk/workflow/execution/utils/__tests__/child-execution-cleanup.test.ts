/**
 * Child Execution Cleanup Unit Tests
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { cleanupChildExecution, type CleanupReason } from "../child-execution-cleanup.js";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";

// Mock the contextual logger
vi.mock("../../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

describe("cleanupChildExecution", () => {
  let mockChildEntity: WorkflowExecutionEntity;
  let mockParentEntity: WorkflowExecutionEntity;

  beforeEach(() => {
    vi.clearAllMocks();

    mockChildEntity = {
      id: "child-exec-1",
      getStatus: vi.fn(),
      stop: vi.fn(),
      getParentContext: vi.fn(),
    } as unknown as WorkflowExecutionEntity;

    mockParentEntity = {
      id: "parent-exec-1",
      unregisterChild: vi.fn(),
    } as unknown as WorkflowExecutionEntity;
  });

  describe("when child is running", () => {
    it("should stop the child execution", async () => {
      (mockChildEntity.getStatus as ReturnType<typeof vi.fn>).mockReturnValue("RUNNING");
      (mockChildEntity.getParentContext as ReturnType<typeof vi.fn>).mockReturnValue({
        parentId: "parent-exec-1",
      });

      await cleanupChildExecution(mockChildEntity, mockParentEntity, "COMPLETED");

      expect(mockChildEntity.stop).toHaveBeenCalled();
    });
  });

  describe("when child is not running", () => {
    it("should not stop the child execution if already completed", async () => {
      (mockChildEntity.getStatus as ReturnType<typeof vi.fn>).mockReturnValue("COMPLETED");
      (mockChildEntity.getParentContext as ReturnType<typeof vi.fn>).mockReturnValue({
        parentId: "parent-exec-1",
      });

      await cleanupChildExecution(mockChildEntity, mockParentEntity, "COMPLETED");

      expect(mockChildEntity.stop).not.toHaveBeenCalled();
    });

    it("should not stop the child execution if paused", async () => {
      (mockChildEntity.getStatus as ReturnType<typeof vi.fn>).mockReturnValue("PAUSED");
      (mockChildEntity.getParentContext as ReturnType<typeof vi.fn>).mockReturnValue({
        parentId: "parent-exec-1",
      });

      await cleanupChildExecution(mockChildEntity, mockParentEntity, "FAILED");

      expect(mockChildEntity.stop).not.toHaveBeenCalled();
    });
  });

  describe("parent-child relationship cleanup", () => {
    it("should unregister child from parent when parent context exists", async () => {
      (mockChildEntity.getStatus as ReturnType<typeof vi.fn>).mockReturnValue("RUNNING");
      (mockChildEntity.getParentContext as ReturnType<typeof vi.fn>).mockReturnValue({
        parentId: "parent-exec-1",
      });

      await cleanupChildExecution(mockChildEntity, mockParentEntity, "COMPLETED");

      expect(mockParentEntity.unregisterChild).toHaveBeenCalledWith("child-exec-1", "WORKFLOW");
    });

    it("should not unregister child when no parent context", async () => {
      (mockChildEntity.getStatus as ReturnType<typeof vi.fn>).mockReturnValue("RUNNING");
      (mockChildEntity.getParentContext as ReturnType<typeof vi.fn>).mockReturnValue(null);

      await cleanupChildExecution(mockChildEntity, mockParentEntity, "CANCELLED");

      expect(mockParentEntity.unregisterChild).not.toHaveBeenCalled();
    });

    it("should not unregister child when parent context is undefined", async () => {
      (mockChildEntity.getStatus as ReturnType<typeof vi.fn>).mockReturnValue("RUNNING");
      (mockChildEntity.getParentContext as ReturnType<typeof vi.fn>).mockReturnValue(undefined);

      await cleanupChildExecution(mockChildEntity, mockParentEntity, "FAILED");

      expect(mockParentEntity.unregisterChild).not.toHaveBeenCalled();
    });
  });

  describe("cleanup reasons", () => {
    it("should handle COMPLETED reason", async () => {
      (mockChildEntity.getStatus as ReturnType<typeof vi.fn>).mockReturnValue("RUNNING");
      (mockChildEntity.getParentContext as ReturnType<typeof vi.fn>).mockReturnValue({
        parentId: "parent-exec-1",
      });

      await cleanupChildExecution(mockChildEntity, mockParentEntity, "COMPLETED");

      expect(mockChildEntity.stop).toHaveBeenCalled();
      expect(mockParentEntity.unregisterChild).toHaveBeenCalled();
    });

    it("should handle FAILED reason", async () => {
      (mockChildEntity.getStatus as ReturnType<typeof vi.fn>).mockReturnValue("RUNNING");
      (mockChildEntity.getParentContext as ReturnType<typeof vi.fn>).mockReturnValue({
        parentId: "parent-exec-1",
      });

      await cleanupChildExecution(mockChildEntity, mockParentEntity, "FAILED");

      expect(mockChildEntity.stop).toHaveBeenCalled();
      expect(mockParentEntity.unregisterChild).toHaveBeenCalled();
    });

    it("should handle CANCELLED reason", async () => {
      (mockChildEntity.getStatus as ReturnType<typeof vi.fn>).mockReturnValue("RUNNING");
      (mockChildEntity.getParentContext as ReturnType<typeof vi.fn>).mockReturnValue({
        parentId: "parent-exec-1",
      });

      await cleanupChildExecution(mockChildEntity, mockParentEntity, "CANCELLED");

      expect(mockChildEntity.stop).toHaveBeenCalled();
      expect(mockParentEntity.unregisterChild).toHaveBeenCalled();
    });
  });

  describe("error handling", () => {
    it("should not throw when stop throws an error", async () => {
      (mockChildEntity.getStatus as ReturnType<typeof vi.fn>).mockReturnValue("RUNNING");
      (mockChildEntity.stop as ReturnType<typeof vi.fn>).mockImplementation(() => {
        throw new Error("Stop failed");
      });
      (mockChildEntity.getParentContext as ReturnType<typeof vi.fn>).mockReturnValue({
        parentId: "parent-exec-1",
      });

      // Should not throw
      await expect(
        cleanupChildExecution(mockChildEntity, mockParentEntity, "COMPLETED"),
      ).resolves.not.toThrow();
    });

    it("should not throw when unregisterChild throws an error", async () => {
      (mockChildEntity.getStatus as ReturnType<typeof vi.fn>).mockReturnValue("RUNNING");
      (mockChildEntity.getParentContext as ReturnType<typeof vi.fn>).mockReturnValue({
        parentId: "parent-exec-1",
      });
      (mockParentEntity.unregisterChild as ReturnType<typeof vi.fn>).mockImplementation(() => {
        throw new Error("Unregister failed");
      });

      // Should not throw
      await expect(
        cleanupChildExecution(mockChildEntity, mockParentEntity, "COMPLETED"),
      ).resolves.not.toThrow();
    });

    it("should handle errors gracefully and continue", async () => {
      (mockChildEntity.getStatus as ReturnType<typeof vi.fn>).mockReturnValue("RUNNING");
      (mockChildEntity.getParentContext as ReturnType<typeof vi.fn>).mockReturnValue({
        parentId: "parent-exec-1",
      });

      // Should complete without throwing
      await cleanupChildExecution(mockChildEntity, mockParentEntity, "COMPLETED");

      expect(mockChildEntity.stop).toHaveBeenCalled();
      expect(mockParentEntity.unregisterChild).toHaveBeenCalled();
    });
  });

  describe("idempotency", () => {
    it("should be safe to call multiple times", async () => {
      (mockChildEntity.getStatus as ReturnType<typeof vi.fn>).mockReturnValue("RUNNING");
      (mockChildEntity.getParentContext as ReturnType<typeof vi.fn>).mockReturnValue({
        parentId: "parent-exec-1",
      });

      // Call multiple times
      await cleanupChildExecution(mockChildEntity, mockParentEntity, "COMPLETED");
      await cleanupChildExecution(mockChildEntity, mockParentEntity, "COMPLETED");
      await cleanupChildExecution(mockChildEntity, mockParentEntity, "COMPLETED");

      // All calls should complete without error
      expect(mockChildEntity.stop).toHaveBeenCalledTimes(3);
      expect(mockParentEntity.unregisterChild).toHaveBeenCalledTimes(3);
    });
  });
});

describe("CleanupReason type", () => {
  it("should accept valid cleanup reasons", () => {
    const reasons: CleanupReason[] = ["COMPLETED", "FAILED", "CANCELLED"];
    expect(reasons).toHaveLength(3);
  });
});
