/**
 * User Interaction Adapter Unit Tests
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { UserInteractionAdapter } from "../user-interaction-adapter.js";
import { success, failure } from "@wf-agent/sdk";

// Mock SDK dependencies
vi.mock("@wf-agent/sdk", async () => {
  const actual = await vi.importActual("@wf-agent/sdk");
  return {
    ...actual,
  };
});

vi.mock("../../src/index.js", () => ({
  getSDKInstance: vi.fn(),
}));

vi.mock("../../src/utils/output.js", () => ({
  getOutput: vi.fn(() => ({
    infoLog: vi.fn(),
    errorLog: vi.fn(),
    success: vi.fn(),
    fail: vi.fn(),
    result: vi.fn(),
    errorResult: vi.fn(),
    debugLog: vi.fn(),
  })),
}));

describe("UserInteractionAdapter", () => {
  let adapter: UserInteractionAdapter;
  let mockSdk: any;
  let mockUserInteractionsApi: any;

  beforeEach(() => {
    vi.clearAllMocks();

    // Mock user interactions API
    mockUserInteractionsApi = {
      getAll: vi.fn(),
      get: vi.fn(),
      create: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
    };

    // Mock SDK instance
    mockSdk = {
      userInteractions: mockUserInteractionsApi,
    };

    // Setup getSDKInstance mock
    const { getSDKInstance } = require("../../src/index.js");
    getSDKInstance.mockReturnValue(mockSdk);

    // Create adapter instance
    adapter = new UserInteractionAdapter();
  });

  describe("listConfigs", () => {
    it("should list all user interaction configurations", async () => {
      const mockConfigs = [
        { id: "config-1", name: "Config 1", enabled: true },
        { id: "config-2", name: "Config 2", enabled: false },
      ];
      mockUserInteractionsApi.getAll.mockResolvedValue(success(mockConfigs, 30));

      const result = await adapter.listConfigs();

      expect(mockUserInteractionsApi.getAll).toHaveBeenCalled();
      expect(result).toHaveLength(2);
      expect(result[0]).toHaveProperty("id", "config-1");
    });

    it("should list configs with filter", async () => {
      const mockConfigs = [{ id: "config-1", name: "Config 1" }];
      const filter = { name: "Config 1" };
      mockUserInteractionsApi.getAll.mockResolvedValue(success(mockConfigs, 20));

      const result = await adapter.listConfigs(filter);

      expect(mockUserInteractionsApi.getAll).toHaveBeenCalledWith(filter);
      expect(result).toHaveLength(1);
    });

    it("should handle list failure", async () => {
      const mockError = new Error("List failed");
      mockUserInteractionsApi.getAll.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.listConfigs()).rejects.toThrow("List failed");
    });
  });

  describe("getConfig", () => {
    it("should get user interaction configuration by ID", async () => {
      const mockConfig = { id: "config-123", name: "Test Config", enabled: true };
      mockUserInteractionsApi.get.mockResolvedValue(success(mockConfig, 15));

      const result = await adapter.getConfig("config-123");

      expect(mockUserInteractionsApi.get).toHaveBeenCalledWith("config-123");
      expect(result).toEqual(mockConfig);
    });

    it("should throw error when config not found", async () => {
      mockUserInteractionsApi.get.mockResolvedValue(success(null, 15));

      await expect(adapter.getConfig("non-existent")).rejects.toThrow(
        "User interaction configuration not found: non-existent",
      );
    });

    it("should handle get failure", async () => {
      const mockError = new Error("Get failed");
      mockUserInteractionsApi.get.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.getConfig("config-123")).rejects.toThrow("Get failed");
    });
  });

  describe("createConfig", () => {
    it("should create user interaction configuration", async () => {
      const mockConfig = { id: "config-new", name: "New Config", enabled: true };
      mockUserInteractionsApi.create.mockResolvedValue(success(undefined, 25));

      const result = await adapter.createConfig(mockConfig);

      expect(mockUserInteractionsApi.create).toHaveBeenCalledWith(mockConfig);
      expect(result).toEqual(mockConfig);
    });

    it("should handle create failure", async () => {
      const mockError = new Error("Create failed");
      mockUserInteractionsApi.create.mockResolvedValue(failure(mockError as any, 0));

      const mockConfig = { id: "config-new", name: "New Config", enabled: true };
      await expect(adapter.createConfig(mockConfig)).rejects.toThrow("Create failed");
    });
  });

  describe("updateConfig", () => {
    it("should update user interaction configuration", async () => {
      const updates = { description: "Updated description" };
      const updatedConfig = { id: "config-123", name: "Test Config", description: "Updated description" };

      mockUserInteractionsApi.update.mockResolvedValue(success(undefined, 20));
      mockUserInteractionsApi.get.mockResolvedValue(success(updatedConfig, 15));

      const result = await adapter.updateConfig("config-123", updates);

      expect(mockUserInteractionsApi.update).toHaveBeenCalledWith("config-123", updates);
      expect(mockUserInteractionsApi.get).toHaveBeenCalledWith("config-123");
      expect(result).toEqual(updatedConfig);
    });

    it("should throw error when config not found after update", async () => {
      mockUserInteractionsApi.update.mockResolvedValue(success(undefined, 20));
      mockUserInteractionsApi.get.mockResolvedValue(success(null, 15));

      await expect(adapter.updateConfig("config-123", { description: "Updated" })).rejects.toThrow(
        "User interaction configuration not found: config-123",
      );
    });

    it("should handle update failure", async () => {
      const mockError = new Error("Update failed");
      mockUserInteractionsApi.update.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.updateConfig("config-123", { description: "Updated" })).rejects.toThrow(
        "Update failed",
      );
    });
  });

  describe("deleteConfig", () => {
    it("should delete user interaction configuration", async () => {
      mockUserInteractionsApi.delete.mockResolvedValue(success(undefined, 20));

      await adapter.deleteConfig("config-123");

      expect(mockUserInteractionsApi.delete).toHaveBeenCalledWith("config-123");
    });

    it("should handle delete failure", async () => {
      const mockError = new Error("Delete failed");
      mockUserInteractionsApi.delete.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.deleteConfig("config-123")).rejects.toThrow("Delete failed");
    });
  });

  describe("enableConfig", () => {
    it("should enable user interaction configuration", async () => {
      mockUserInteractionsApi.update.mockResolvedValue(success(undefined, 20));

      await adapter.enableConfig("config-123");

      expect(mockUserInteractionsApi.update).toHaveBeenCalledWith("config-123", { enabled: true });
    });

    it("should handle enable failure", async () => {
      const mockError = new Error("Enable failed");
      mockUserInteractionsApi.update.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.enableConfig("config-123")).rejects.toThrow("Enable failed");
    });
  });

  describe("disableConfig", () => {
    it("should disable user interaction configuration", async () => {
      mockUserInteractionsApi.update.mockResolvedValue(success(undefined, 20));

      await adapter.disableConfig("config-123");

      expect(mockUserInteractionsApi.update).toHaveBeenCalledWith("config-123", { enabled: false });
    });

    it("should handle disable failure", async () => {
      const mockError = new Error("Disable failed");
      mockUserInteractionsApi.update.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.disableConfig("config-123")).rejects.toThrow("Disable failed");
    });
  });
});
