import { describe, it, expect } from "vitest";
import { createSDK } from "../sdk.js";
import type { SDKOptions } from "../../types/core-types.js";
import type { CheckpointStorageAdapter } from "@wf-agent/storage";

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

describe("sdk.ts", () => {
  describe("createSDK", () => {
    it("should create an SDK instance with valid options", () => {
      const options: SDKOptions = {
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = createSDK(options);

      expect(sdk).toBeDefined();
      expect(typeof sdk.waitForReady).toBe("function");
      expect(typeof sdk.isReady).toBe("function");
      expect(typeof sdk.shutdown).toBe("function");
      expect(typeof sdk.destroy).toBe("function");
    });

    it("should create SDK instance with empty options", () => {
      const options: SDKOptions = {};

      const sdk = createSDK(options);

      expect(sdk).toBeDefined();
    });

    it("should create independent SDK instances", () => {
      const adapter1 = createMockCheckpointAdapter();
      const adapter2 = createMockCheckpointAdapter();

      const sdk1 = createSDK({ checkpointStorageAdapter: adapter1 });
      const sdk2 = createSDK({ checkpointStorageAdapter: adapter2 });

      expect(sdk1).not.toBe(sdk2);
    });

    it("should pass debug option to SDK instance", () => {
      const sdk = createSDK({
        debug: true,
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      });

      expect(sdk).toBeDefined();
    });

    it("should pass logging config to SDK instance", () => {
      const sdk = createSDK({
        logging: {
          level: "debug",
          output: "console",
          format: "text",
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      });

      expect(sdk).toBeDefined();
    });

    it("should pass presets config to SDK instance", () => {
      const sdk = createSDK({
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      });

      expect(sdk).toBeDefined();
    });

    it("should pass mcp config to SDK instance", () => {
      const sdk = createSDK({
        mcp: {
          enabled: false,
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      });

      expect(sdk).toBeDefined();
    });

    it("should pass hooks to SDK instance", () => {
      const onBootstrapStart = async () => {};
      const onBootstrapComplete = async () => {};
      const onDestroy = async () => {};

      const sdk = createSDK({
        hooks: {
          onBootstrapStart,
          onBootstrapComplete,
          onDestroy,
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      });

      expect(sdk).toBeDefined();
    });
  });
});
