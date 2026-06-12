/**
 * AsyncCompletionManager - Unit Tests
 * Tests for asynchronous task completion handler management
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { AsyncCompletionManager } from "../async-completion-manager.js";

// Mock EventRegistry
const mockEventRegistry = {
  emit: vi.fn().mockResolvedValue(undefined),
};

describe("AsyncCompletionManager", () => {
  let manager: AsyncCompletionManager<string>;

  beforeEach(() => {
    vi.clearAllMocks();
    manager = new AsyncCompletionManager<string>();
  });

  describe("constructor", () => {
    it("should initialize with empty handlers", () => {
      expect(manager.size()).toBe(0);
      expect(manager.isEmpty()).toBe(true);
      expect(manager.getExecutionIds()).toEqual([]);
    });

    it("should accept optional event registry", () => {
      const managerWithEvents = new AsyncCompletionManager<string>(mockEventRegistry as any);
      expect(managerWithEvents.size()).toBe(0);
    });
  });

  describe("registerHandler", () => {
    it("should register handler successfully", async () => {
      // Arrange
      const onComplete = vi.fn();
      const onError = vi.fn();

      // Act
      const result = await manager.registerHandler("exec-1", onComplete, onError);

      // Assert
      expect(result).toBe(true);
      expect(manager.size()).toBe(1);
      expect(manager.hasHandler("exec-1")).toBe(true);

      const handler = manager.getHandler("exec-1");
      expect(handler).toBeDefined();
      expect(handler?.executionId).toBe("exec-1");
      expect(handler?.onComplete).toBe(onComplete);
      expect(handler?.onError).toBe(onError);
      expect(handler?.registeredAt).toBeGreaterThan(0);
    });

    it("should reject duplicate registration", async () => {
      // Arrange
      const onComplete1 = vi.fn();
      const onError1 = vi.fn();
      const onComplete2 = vi.fn();
      const onError2 = vi.fn();

      // Act
      await manager.registerHandler("exec-1", onComplete1, onError1);
      const result = await manager.registerHandler("exec-1", onComplete2, onError2);

      // Assert
      expect(result).toBe(false);
      expect(manager.size()).toBe(1);

      const handler = manager.getHandler("exec-1");
      expect(handler?.onComplete).toBe(onComplete1); // Should keep original
    });

    it("should emit event when event registry is provided", async () => {
      // Arrange
      const managerWithEvents = new AsyncCompletionManager<string>(mockEventRegistry as any);
      const onComplete = vi.fn();
      const onError = vi.fn();

      // Act
      await managerWithEvents.registerHandler("exec-1", onComplete, onError);

      // Assert
      expect(mockEventRegistry.emit).toHaveBeenCalled();
    });
  });

  describe("triggerCompletion", () => {
    it("should trigger completion handler successfully", async () => {
      // Arrange
      const onComplete = vi.fn();
      const onError = vi.fn();
      await manager.registerHandler("exec-1", onComplete, onError);

      // Act
      const result = await manager.triggerCompletion("exec-1", "success result");

      // Assert
      expect(result).toBe(true);
      expect(onComplete).toHaveBeenCalledWith("success result");
      expect(onError).not.toHaveBeenCalled();
      expect(manager.hasHandler("exec-1")).toBe(false);
      expect(manager.size()).toBe(0);
    });

    it("should return false when handler not found", async () => {
      // Act
      const result = await manager.triggerCompletion("non-existent", "result");

      // Assert
      expect(result).toBe(false);
    });

    it("should handle completion callback error", async () => {
      // Arrange
      const onComplete = vi.fn().mockImplementation(() => {
        throw new Error("Completion callback error");
      });
      const onError = vi.fn();
      await manager.registerHandler("exec-1", onComplete, onError);

      // Act
      const result = await manager.triggerCompletion("exec-1", "result");

      // Assert
      expect(result).toBe(false);
      expect(manager.hasHandler("exec-1")).toBe(false); // Should be cleaned up
    });

    it("should emit resolved event when event registry is provided", async () => {
      // Arrange
      const managerWithEvents = new AsyncCompletionManager<string>(mockEventRegistry as any);
      const onComplete = vi.fn();
      const onError = vi.fn();
      await managerWithEvents.registerHandler("exec-1", onComplete, onError);

      // Act
      await managerWithEvents.triggerCompletion("exec-1", "result");

      // Assert
      expect(mockEventRegistry.emit).toHaveBeenCalled();
    });
  });

  describe("triggerError", () => {
    it("should trigger error handler successfully", async () => {
      // Arrange
      const onComplete = vi.fn();
      const onError = vi.fn();
      await manager.registerHandler("exec-1", onComplete, onError);
      const error = new Error("Test error");

      // Act
      const result = await manager.triggerError("exec-1", error);

      // Assert
      expect(result).toBe(true);
      expect(onError).toHaveBeenCalledWith(error);
      expect(onComplete).not.toHaveBeenCalled();
      expect(manager.hasHandler("exec-1")).toBe(false);
      expect(manager.size()).toBe(0);
    });

    it("should return false when handler not found", async () => {
      // Act
      const result = await manager.triggerError("non-existent", new Error("error"));

      // Assert
      expect(result).toBe(false);
    });

    it("should handle error callback exception", async () => {
      // Arrange
      const onComplete = vi.fn();
      const onError = vi.fn().mockImplementation(() => {
        throw new Error("Error callback exception");
      });
      await manager.registerHandler("exec-1", onComplete, onError);

      // Act
      const result = await manager.triggerError("exec-1", new Error("original error"));

      // Assert
      expect(result).toBe(false);
      expect(manager.hasHandler("exec-1")).toBe(false); // Should be cleaned up
    });

    it("should emit rejected event when event registry is provided", async () => {
      // Arrange
      const managerWithEvents = new AsyncCompletionManager<string>(mockEventRegistry as any);
      const onComplete = vi.fn();
      const onError = vi.fn();
      await managerWithEvents.registerHandler("exec-1", onComplete, onError);

      // Act
      await managerWithEvents.triggerError("exec-1", new Error("error"));

      // Assert
      expect(mockEventRegistry.emit).toHaveBeenCalled();
    });
  });

  describe("hasHandler and getHandler", () => {
    it("should return true when handler exists", async () => {
      // Arrange
      await manager.registerHandler("exec-1", vi.fn(), vi.fn());

      // Assert
      expect(manager.hasHandler("exec-1")).toBe(true);
      expect(manager.getHandler("exec-1")).toBeDefined();
    });

    it("should return false when handler does not exist", () => {
      // Assert
      expect(manager.hasHandler("non-existent")).toBe(false);
      expect(manager.getHandler("non-existent")).toBeUndefined();
    });
  });

  describe("cleanup", () => {
    it("should cleanup all handlers with error callbacks", async () => {
      // Arrange
      const onError1 = vi.fn();
      const onError2 = vi.fn();
      await manager.registerHandler("exec-1", vi.fn(), onError1);
      await manager.registerHandler("exec-2", vi.fn(), onError2);

      // Act
      await manager.cleanup();

      // Assert
      expect(manager.size()).toBe(0);
      expect(manager.isEmpty()).toBe(true);
      expect(onError1).toHaveBeenCalled();
      expect(onError2).toHaveBeenCalled();
    });

    it("should handle cleanup error gracefully", async () => {
      // Arrange
      const onError = vi.fn().mockImplementation(() => {
        throw new Error("Cleanup error");
      });
      await manager.registerHandler("exec-1", vi.fn(), onError);

      // Act
      await manager.cleanup();

      // Assert
      expect(manager.size()).toBe(0); // Should still clear
    });
  });

  describe("cleanupHandler", () => {
    it("should cleanup single handler successfully", async () => {
      // Arrange
      const onError = vi.fn();
      await manager.registerHandler("exec-1", vi.fn(), onError);

      // Act
      const result = await manager.cleanupHandler("exec-1");

      // Assert
      expect(result).toBe(true);
      expect(manager.hasHandler("exec-1")).toBe(false);
      expect(onError).toHaveBeenCalled();
    });

    it("should return false when handler not found", async () => {
      // Act
      const result = await manager.cleanupHandler("non-existent");

      // Assert
      expect(result).toBe(false);
    });
  });

  describe("createSnapshot and restoreFromSnapshot", () => {
    it("should create snapshot of handlers", async () => {
      // Arrange
      await manager.registerHandler("exec-1", vi.fn(), vi.fn());
      await manager.registerHandler("exec-2", vi.fn(), vi.fn());

      // Act
      const snapshot = manager.createSnapshot();

      // Assert
      expect(snapshot.size).toBe(2);
      expect(snapshot.has("exec-1")).toBe(true);
      expect(snapshot.has("exec-2")).toBe(true);
    });

    it("should restore from snapshot", async () => {
      // Arrange
      const onComplete = vi.fn();
      const onError = vi.fn();
      await manager.registerHandler("exec-1", onComplete, onError);
      const snapshot = manager.createSnapshot();

      await manager.cleanup();

      // Act
      manager.restoreFromSnapshot(snapshot);

      // Assert
      expect(manager.size()).toBe(1);
      expect(manager.hasHandler("exec-1")).toBe(true);
    });
  });

  describe("reset", () => {
    it("should reset to initial state", async () => {
      // Arrange
      await manager.registerHandler("exec-1", vi.fn(), vi.fn());

      // Act
      manager.reset();

      // Assert
      expect(manager.size()).toBe(0);
      expect(manager.isEmpty()).toBe(true);
    });
  });

  describe("getExecutionIds", () => {
    it("should return all execution IDs", async () => {
      // Arrange
      await manager.registerHandler("exec-1", vi.fn(), vi.fn());
      await manager.registerHandler("exec-2", vi.fn(), vi.fn());

      // Act
      const ids = manager.getExecutionIds();

      // Assert
      expect(ids).toEqual(["exec-1", "exec-2"]);
    });

    it("should return empty array when no handlers", () => {
      expect(manager.getExecutionIds()).toEqual([]);
    });
  });
});
