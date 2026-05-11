/**
 * Tests for AgentLoopCheckpointStateManager
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { AgentLoopCheckpointStateManager } from "../checkpoint-state-manager.js";
import type { AgentLoopCheckpointStorageAdapter } from "@wf-agent/storage";
import type { AgentLoopCheckpoint } from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";

describe("AgentLoopCheckpointStateManager", () => {
  let stateManager: AgentLoopCheckpointStateManager;
  let mockStorageAdapter: AgentLoopCheckpointStorageAdapter;

  beforeEach(() => {
    mockStorageAdapter = {
      initialize: vi.fn(),
      save: vi.fn(),
      load: vi.fn(),
      delete: vi.fn(),
      list: vi.fn(),
      close: vi.fn(),
    } as unknown as AgentLoopCheckpointStorageAdapter;

    stateManager = new AgentLoopCheckpointStateManager(mockStorageAdapter);
  });

  const createCheckpoint = (id: string): AgentLoopCheckpoint => ({
    id,
    agentLoopId: "agent-1",
    timestamp: Date.now(),
    type: "FULL" as const,
    snapshot: {
      status: AgentLoopStatus.RUNNING,
      currentIteration: 1,
      toolCallCount: 0,
      startTime: Date.now(),
      endTime: null,
      error: undefined,
      messages: [],
      config: {},
    },
  });

  describe("constructor", () => {
    it("should initialize with storage adapter", () => {
      expect(stateManager).toBeDefined();
    });
  });

  describe("saveCheckpoint", () => {
    it("should save checkpoint successfully", async () => {
      const checkpoint = createCheckpoint("cp-1");
      mockStorageAdapter.save = vi.fn().mockResolvedValue(undefined);

      const checkpointId = await stateManager.saveCheckpoint(checkpoint);

      expect(checkpointId).toBe("cp-1");
      expect(mockStorageAdapter.save).toHaveBeenCalled();
    });

    it("should throw error when save fails", async () => {
      const checkpoint = createCheckpoint("cp-1");
      mockStorageAdapter.save = vi.fn().mockRejectedValue(new Error("Save failed"));

      await expect(stateManager.saveCheckpoint(checkpoint)).rejects.toThrow("Save failed");
    });
  });

  describe("getCheckpoint", () => {
    it("should return null when checkpoint not found", async () => {
      mockStorageAdapter.load = vi.fn().mockResolvedValue(null);

      const result = await stateManager.getCheckpoint("nonexistent");

      expect(result).toBeNull();
    });
  });

  describe("list", () => {
    it("should list all checkpoints", async () => {
      mockStorageAdapter.list = vi.fn().mockResolvedValue(["cp-1", "cp-2", "cp-3"]);

      const result = await stateManager.list();

      expect(result).toEqual(["cp-1", "cp-2", "cp-3"]);
    });

    it("should list checkpoints with options", async () => {
      mockStorageAdapter.list = vi.fn().mockResolvedValue(["cp-1"]);

      const result = await stateManager.list({ agentLoopId: "agent-1" });

      expect(result).toEqual(["cp-1"]);
    });

    it("should throw error when list fails", async () => {
      mockStorageAdapter.list = vi.fn().mockRejectedValue(new Error("List failed"));

      await expect(stateManager.list()).rejects.toThrow("List failed");
    });
  });

  describe("deleteCheckpoint", () => {
    it("should delete checkpoint successfully", async () => {
      mockStorageAdapter.delete = vi.fn().mockResolvedValue(undefined);

      await stateManager.deleteCheckpoint("cp-1");

      expect(mockStorageAdapter.delete).toHaveBeenCalledWith("cp-1");
    });

    it("should throw error when delete fails", async () => {
      mockStorageAdapter.delete = vi.fn().mockRejectedValue(new Error("Delete failed"));

      await expect(stateManager.deleteCheckpoint("cp-1")).rejects.toThrow("Delete failed");
    });
  });

  describe("executeCleanup", () => {
    it("should skip cleanup when no checkpoints exist", async () => {
      mockStorageAdapter.list = vi.fn().mockResolvedValue([]);

      const result = await stateManager.executeCleanup();

      expect(result.deletedCount).toBe(0);
      expect(result.deletedCheckpointIds).toEqual([]);
      expect(result.remainingCount).toBe(0);
    });
  });

  describe("initialize", () => {
    it("should initialize storage adapter", async () => {
      await stateManager.initialize();

      expect(mockStorageAdapter.initialize).toHaveBeenCalled();
    });
  });

  describe("cleanup", () => {
    it("should close storage adapter if close method exists", async () => {
      await stateManager.cleanup();

      expect(mockStorageAdapter.close).toHaveBeenCalled();
    });

    it("should handle storage adapter without close method", async () => {
      const adapterWithoutClose = {
        initialize: vi.fn(),
        save: vi.fn(),
        load: vi.fn(),
        delete: vi.fn(),
        list: vi.fn(),
      } as unknown as AgentLoopCheckpointStorageAdapter;

      const manager = new AgentLoopCheckpointStateManager(adapterWithoutClose);

      // Should not throw
      await expect(manager.cleanup()).resolves.not.toThrow();
    });
  });
});
