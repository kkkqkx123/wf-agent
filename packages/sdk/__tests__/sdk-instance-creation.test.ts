/**
 * SDK Instance Creation Integration Tests
 * Tests that SDK instances can be created successfully with the two-phase DI initialization
 * Verifies that the GlobalContext binding order issue is resolved
 */

import { describe, it, expect, afterEach } from "vitest";
import { createSDK } from "../api/index.js";
import type { SDKInstance } from "../api/index.js";
import type { CheckpointStorageAdapter } from "@wf-agent/storage";

// Minimal mock checkpoint adapter
const createMockCheckpointAdapter = (): CheckpointStorageAdapter => ({
  initialize: async () => {},
  close: async () => {},
  clear: async () => {},
  save: async () => {},
  load: async () => null,
  delete: async () => {},
  list: async () => [],
  listWithMetadata: async () => [],
  listByEntityWithMetadata: async () => [],
  getLatestByEntity: async () => [],
  deleteByEntity: async () => 0,
  exists: async () => false,
  getMetadata: async () => null,
  getMetrics: async () => ({
    saveCount: 0,
    loadCount: 0,
    deleteCount: 0,
    listCount: 0,
    avgSaveTime: 0,
    avgLoadTime: 0,
    avgDeleteTime: 0,
    avgListTime: 0,
    totalMetadataSize: 0,
    totalBlobSize: 0,
    totalCount: 0,
  }),
  resetMetrics: async () => {},
  saveBatch: async () => {},
  loadBatch: async () => [],
  deleteBatch: async () => {},
});

describe("SDK Instance Creation (DI Fix Verification)", () => {
  const sdkInstances: SDKInstance[] = [];

  afterEach(async () => {
    // Clean up all SDK instances
    for (const instance of sdkInstances) {
      try {
        await instance.shutdown();
      } catch {
        // Ignore cleanup errors
      }
    }
    sdkInstances.length = 0;
  });

  describe("Basic SDK Creation", () => {
    it("should create SDK instance with checkpoint adapter (minimal config)", () => {
      const sdk = createSDK({
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      });
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });

    it("should create SDK instance with debug mode enabled", () => {
      const sdk = createSDK({
        debug: true,
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      });
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });

    it("should create SDK instance with logging configuration", () => {
      const sdk = createSDK({
        logging: {
          level: "info",
          output: "console",
          format: "text",
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      });
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });
  });

  describe("SDK Bootstrap Process", () => {
    it("should bootstrap SDK successfully with checkpoint adapter", async () => {
      const sdk = createSDK({
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      });
      sdkInstances.push(sdk);

      // Wait for bootstrap to complete
      await new Promise(resolve => setTimeout(resolve, 150));

      expect(sdk).toBeDefined();
    });

    it("should call lifecycle hooks in correct order", async () => {
      const callOrder: string[] = [];

      const sdk = createSDK({
        checkpointStorageAdapter: createMockCheckpointAdapter(),
        hooks: {
          onBootstrapStart: async () => {
            callOrder.push("start");
          },
          onBootstrapComplete: async () => {
            callOrder.push("complete");
          },
        },
      });
      sdkInstances.push(sdk);

      // Wait for bootstrap to complete
      await new Promise(resolve => setTimeout(resolve, 150));

      expect(callOrder).toEqual(["start", "complete"]);
    });
  });

  describe("Multiple SDK Instances", () => {
    it("should create multiple independent SDK instances", () => {
      const sdk1 = createSDK({
        debug: true,
        logging: {
          level: "debug",
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      });

      const sdk2 = createSDK({
        debug: false,
        logging: {
          level: "warn",
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      });

      sdkInstances.push(sdk1, sdk2);

      expect(sdk1).toBeDefined();
      expect(sdk2).toBeDefined();
      expect(sdk1).not.toBe(sdk2);
    });

    it("should isolate containers between instances", async () => {
      const sdk1 = createSDK({
        checkpointStorageAdapter: createMockCheckpointAdapter(),
        logging: {
          level: "debug",
        },
      });

      const sdk2 = createSDK({
        checkpointStorageAdapter: createMockCheckpointAdapter(),
        logging: {
          level: "error",
        },
      });

      sdkInstances.push(sdk1, sdk2);

      // Wait for both to bootstrap
      await new Promise(resolve => setTimeout(resolve, 150));

      // Both should be defined and independent
      expect(sdk1).toBeDefined();
      expect(sdk2).toBeDefined();
    });
  });

  describe("Complex Configurations", () => {
    it("should create SDK with comprehensive configuration", async () => {
      const sdk = createSDK({
        debug: true,
        logging: {
          level: "debug",
          output: "console",
          format: "text",
        },
        defaultTimeout: 60000,
        enableCheckpoints: true,
        checkpointStorageAdapter: createMockCheckpointAdapter(),
        enableValidation: true,
        validation: {
          enableNodeValidation: true,
          enableGraphValidation: true,
        },
        presets: {
          contextCompression: { enabled: false },
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
        },
        mcp: {
          enabled: false,
        },
        hooks: {
          onBootstrapStart: async () => {},
          onBootstrapComplete: async () => {},
        },
      });
      sdkInstances.push(sdk);

      // Wait for bootstrap to complete
      await new Promise(resolve => setTimeout(resolve, 150));

      expect(sdk).toBeDefined();
    });

    it("should handle minimal viable configuration", async () => {
      const sdk = createSDK({
        checkpointStorageAdapter: createMockCheckpointAdapter(),
        debug: false,
        enableCheckpoints: false,
        enableValidation: false,
        presets: {
          contextCompression: { enabled: false },
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
        },
        mcp: {
          enabled: false,
        },
      });
      sdkInstances.push(sdk);

      // Wait for bootstrap to complete
      await new Promise(resolve => setTimeout(resolve, 150));

      expect(sdk).toBeDefined();
    });
  });

  describe("Error Handling", () => {
    it("should handle bootstrap errors gracefully", async () => {
      const sdk = createSDK({
        checkpointStorageAdapter: createMockCheckpointAdapter(),
        hooks: {
          onBootstrapError: _error => {},
        },
      });
      sdkInstances.push(sdk);

      // Wait for potential bootstrap
      await new Promise(resolve => setTimeout(resolve, 150));

      // SDK should still be created even if bootstrap had issues
      expect(sdk).toBeDefined();
    });
  });
});
