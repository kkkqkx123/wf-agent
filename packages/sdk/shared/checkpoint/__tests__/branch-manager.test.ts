/**
 * Branch Manager Tests
 * Tests for branch lifecycle management:
 * - Creating branches
 * - Switching branches
 * - Listing branches
 * - Caching behavior
 * - Error handling
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { BranchManager } from "../branch-manager.js";
import type { LayertwineExecutor } from "../../../services/executors/remote/implementations/layertwine/index.js";

// Mock Layertwine Executor
interface MockLayertwineExecutor {
  branchCreate: ReturnType<typeof vi.fn>;
  branchSwitch: ReturnType<typeof vi.fn>;
  branchList: ReturnType<typeof vi.fn>;
}

describe("BranchManager", () => {
  let branchManager: BranchManager;
  let mockExecutor: MockLayertwineExecutor;

  beforeEach(() => {
    mockExecutor = {
      branchCreate: vi.fn().mockResolvedValue({ name: "test-branch", head: "cp-123" }),
      branchSwitch: vi.fn().mockResolvedValue({ name: "test-branch", checkpointId: "cp-123" }),
      branchList: vi.fn().mockResolvedValue({
        branches: [
          { name: "main", head: "cp-1", updatedAt: new Date().toISOString(), isCurrent: true },
        ],
        current: "main",
      }),
    };

    branchManager = new BranchManager({
      executor: mockExecutor as any,
      enableCache: true,
      defaultBranch: "main",
    });
  });

  describe("getBranchName", () => {
    it("should generate correct branch name for agent-loop", () => {
      const branchName = branchManager.getBranchName("agent-123", "agent-loop");
      expect(branchName).toBe("agent-loop/agent-123");
    });

    it("should generate correct branch name for execution", () => {
      const branchName = branchManager.getBranchName("exec-456", "execution");
      expect(branchName).toBe("execution/exec-456");
    });

    it("should default to agent-loop type", () => {
      const branchName = branchManager.getBranchName("agent-123");
      expect(branchName).toBe("agent-loop/agent-123");
    });
  });

  describe("ensureBranch", () => {
    it("should create branch if it does not exist", async () => {
      const branchName = await branchManager.ensureBranch("agent-123");

      expect(branchName).toBe("agent-loop/agent-123");
      expect(mockExecutor.branchCreate).toHaveBeenCalledWith({
        name: "agent-loop/agent-123",
      });
    });

    it("should not create branch if it already exists", async () => {
      mockExecutor.branchList.mockResolvedValue({
        branches: [
          { name: "agent-loop/agent-123", head: "cp-1", updatedAt: new Date().toISOString(), isCurrent: false },
          { name: "main", head: "cp-2", updatedAt: new Date().toISOString(), isCurrent: true },
        ],
        current: "main",
      });

      const branchName = await branchManager.ensureBranch("agent-123");

      expect(branchName).toBe("agent-loop/agent-123");
      expect(mockExecutor.branchCreate).not.toHaveBeenCalled();
    });

    it("should use cache on subsequent calls", async () => {
      await branchManager.ensureBranch("agent-123");
      mockExecutor.branchList.mockClear();

      await branchManager.ensureBranch("agent-123");

      // Second call should use cache
      expect(mockExecutor.branchList).not.toHaveBeenCalled();
    });

    it("should support execution type branches", async () => {
      const branchName = await branchManager.ensureBranch("exec-456", "execution");

      expect(branchName).toBe("execution/exec-456");
      expect(mockExecutor.branchCreate).toHaveBeenCalledWith({
        name: "execution/exec-456",
      });
    });
  });

  describe("switchBranch", () => {
    it("should switch to specified branch", async () => {
      await branchManager.switchBranch("agent-loop/agent-123");

      expect(mockExecutor.branchSwitch).toHaveBeenCalledWith({
        name: "agent-loop/agent-123",
      });
      expect(branchManager.getCurrentBranch()).toBe("agent-loop/agent-123");
    });

    it("should not switch if already on same branch", async () => {
      branchManager.switchBranch("main");
      mockExecutor.branchSwitch.mockClear();

      await branchManager.switchBranch("main");

      expect(mockExecutor.branchSwitch).not.toHaveBeenCalled();
    });

    it("should propagate errors", async () => {
      mockExecutor.branchSwitch.mockRejectedValue(new Error("Switch failed"));

      await expect(branchManager.switchBranch("agent-loop/agent-123")).rejects.toThrow(
        "Switch failed"
      );
    });
  });

  describe("switchToDefaultBranch", () => {
    it("should switch to default branch", async () => {
      await branchManager.switchBranch("agent-loop/agent-123");
      await branchManager.switchToDefaultBranch();

      expect(branchManager.getCurrentBranch()).toBe("main");
    });
  });

  describe("getCurrentBranch", () => {
    it("should return current branch", () => {
      expect(branchManager.getCurrentBranch()).toBe("main");
    });

    it("should reflect changes after switch", async () => {
      await branchManager.switchBranch("agent-loop/agent-123");
      expect(branchManager.getCurrentBranch()).toBe("agent-loop/agent-123");
    });
  });

  describe("listActiveBranches", () => {
    it("should list all branches", async () => {
      mockExecutor.branchList.mockResolvedValue({
        branches: [
          { name: "main", head: "cp-1", updatedAt: new Date().toISOString(), isCurrent: true },
          { name: "agent-loop/agent-123", head: "cp-2", updatedAt: new Date().toISOString(), isCurrent: false },
          { name: "execution/exec-456", head: "cp-3", updatedAt: new Date().toISOString(), isCurrent: false },
        ],
        current: "main",
      });

      const branches = await branchManager.listActiveBranches();

      expect(branches).toHaveLength(3);
      expect(branches).toContain("main");
      expect(branches).toContain("agent-loop/agent-123");
      expect(branches).toContain("execution/exec-456");
    });

    it("should propagate errors", async () => {
      mockExecutor.branchList.mockRejectedValue(new Error("List failed"));

      await expect(branchManager.listActiveBranches()).rejects.toThrow("List failed");
    });
  });

  describe("listBranchesWithMetadata", () => {
    it("should parse branch metadata correctly", async () => {
      mockExecutor.branchList.mockResolvedValue({
        branches: [
          { name: "main", head: "cp-1", updatedAt: new Date().toISOString(), isCurrent: true },
          { name: "agent-loop/agent-123", head: "cp-2", updatedAt: new Date().toISOString(), isCurrent: false },
          { name: "execution/exec-456", head: "cp-3", updatedAt: new Date().toISOString(), isCurrent: false },
        ],
        current: "main",
      });

      const metadata = await branchManager.listBranchesWithMetadata();

      expect(metadata.size).toBe(2); // main is not parsed

      const agentBranch = metadata.get("agent-loop/agent-123");
      expect(agentBranch?.parentId).toBe("agent-123");
      expect(agentBranch?.type).toBe("agent-loop");

      const execBranch = metadata.get("execution/exec-456");
      expect(execBranch?.parentId).toBe("exec-456");
      expect(execBranch?.type).toBe("execution");
    });
  });

  describe("cleanupBranches", () => {
    it("should clear cache for completed parent IDs", async () => {
      await branchManager.ensureBranch("agent-123");
      expect(branchManager.getCacheSize()).toBe(1);

      await branchManager.cleanupBranches(["agent-123"]);

      expect(branchManager.getCacheSize()).toBe(0);
    });

    it("should handle multiple parent IDs", async () => {
      await branchManager.ensureBranch("agent-123");
      await branchManager.ensureBranch("agent-456", "agent-loop");

      await branchManager.cleanupBranches(["agent-123", "agent-456"]);

      expect(branchManager.getCacheSize()).toBe(0);
    });
  });

  describe("clearCache", () => {
    it("should clear all cached branches", async () => {
      await branchManager.ensureBranch("agent-123");
      await branchManager.ensureBranch("exec-456", "execution");

      expect(branchManager.getCacheSize()).toBe(2);

      branchManager.clearCache();

      expect(branchManager.getCacheSize()).toBe(0);
    });
  });

  describe("getCacheSize", () => {
    it("should return correct cache size", async () => {
      expect(branchManager.getCacheSize()).toBe(0);

      await branchManager.ensureBranch("agent-123");
      expect(branchManager.getCacheSize()).toBe(1);

      await branchManager.ensureBranch("agent-456");
      expect(branchManager.getCacheSize()).toBe(2);
    });
  });
});
