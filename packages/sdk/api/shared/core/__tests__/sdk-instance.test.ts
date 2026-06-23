import { describe, it, expect, afterEach, vi } from "vitest";
import { SDKInstance } from "../sdk-instance.js";
import type { SDKOptions } from "../../types/core-types.js";
import type { CheckpointStorageAdapter } from "@wf-agent/storage";

// Increase max listeners to prevent MaxListenersExceededWarning in tests
// when multiple SDK instances register signal handlers
process.setMaxListeners(100);

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

describe("sdk-instance.ts", () => {
  const sdkInstances: SDKInstance[] = [];

  afterEach(async () => {
    for (const instance of sdkInstances) {
      try {
        await instance.destroy();
      } catch {
        // Ignore cleanup errors
      }
    }
    sdkInstances.length = 0;
  });

  describe("constructor", () => {
    it("should create SDKInstance with minimal config", () => {
      const options: SDKOptions = {
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });

    it("should create SDKInstance with debug option", () => {
      const options: SDKOptions = {
        debug: true,
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });

    it("should create SDKInstance with logging config", () => {
      const options: SDKOptions = {
        logging: {
          level: "debug",
          output: "console",
          format: "text",
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });

    it("should create SDKInstance with hooks", () => {
      const onBootstrapStart = vi.fn();
      const onBootstrapComplete = vi.fn();
      const onBootstrapError = vi.fn();

      const options: SDKOptions = {
        hooks: {
          onBootstrapStart,
          onBootstrapComplete,
          onBootstrapError,
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });

    it("should handle empty options", () => {
      const options: SDKOptions = {};

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });

    it("should create SDKInstance with presets disabled", () => {
      const options: SDKOptions = {
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });

    it("should create SDKInstance with mcp disabled", () => {
      const options: SDKOptions = {
        mcp: {
          enabled: false,
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });

    it("should create SDKInstance with validation config", () => {
      const options: SDKOptions = {
        validation: {
          enableWorkflowValidation: true,
          enableNodeValidation: true,
          enableGraphValidation: true,
          checkCycles: true,
          checkReachability: true,
          maxRecursionDepth: 100,
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });

    it("should create SDKInstance with workflow execution config", () => {
      const options: SDKOptions = {
        workflowExecution: {
          defaultTimeout: 60000,
          maxConcurrentExecutions: 10,
          enableRetry: true,
          maxRetryAttempts: 3,
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });

    it("should create SDKInstance with event config", () => {
      const options: SDKOptions = {
        events: {
          maxListenerQueueSize: 100,
          enableBackpressure: true,
          defaultListenerTimeout: 5000,
          slowListenerThreshold: 1000,
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });

    it("should create SDKInstance with graceful shutdown config", () => {
      const options: SDKOptions = {
        gracefulShutdown: {
          enabled: true,
          timeoutMs: 10000,
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });

    it("should create SDKInstance with file checkpoint config disabled", () => {
      const options: SDKOptions = {
        fileCheckpoint: {
          enabled: false,
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });

    it("should create SDKInstance with profiles config", () => {
      const options: SDKOptions = {
        profiles: {
          profiles: [
            {
              id: "test-profile",
              provider: "openai",
              model: "gpt-4",
            },
          ],
          defaultProfileId: "test-profile",
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(sdk).toBeDefined();
    });
  });

  describe("waitForReady", () => {
    it("should resolve when bootstrap completes", async () => {
      const options: SDKOptions = {
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      expect(sdk.isReady()).toBe(true);
    });

    it("should resolve immediately if already bootstrapped", async () => {
      const options: SDKOptions = {
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      await sdk.waitForReady();

      expect(sdk.isReady()).toBe(true);
    });
  });

  describe("isReady", () => {
    it("should return false before bootstrap", () => {
      const options: SDKOptions = {
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(sdk.isReady()).toBe(false);
    });

    it("should return true after bootstrap completes", async () => {
      const options: SDKOptions = {
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      expect(sdk.isReady()).toBe(true);
    });
  });

  describe("getGlobalContext", () => {
    it("should return global context when ready", async () => {
      const options: SDKOptions = {
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      const context = sdk.getGlobalContext();

      expect(context).toBeDefined();
    });

    it("should throw when not ready", () => {
      const options: SDKOptions = {
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      expect(() => sdk.getGlobalContext()).toThrow("SDK instance is not ready yet");
    });
  });

  describe("getFactory", () => {
    it("should return API factory", async () => {
      const options: SDKOptions = {
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      const factory = sdk.getFactory();

      expect(factory).toBeDefined();
    });
  });

  describe("reset", () => {
    it("should reset the factory", async () => {
      const options: SDKOptions = {
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      sdk.reset();

      expect(sdk).toBeDefined();
    });
  });

  describe("shutdown", () => {
    it("should shutdown successfully", async () => {
      const options: SDKOptions = {
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      await sdk.shutdown();

      expect(sdk).toBeDefined();
    });
  });

  describe("destroy", () => {
    it("should destroy successfully", async () => {
      const options: SDKOptions = {
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      await sdk.destroy();
      sdkInstances.pop();

      expect(sdk).toBeDefined();
    });

    it("should call onDestroy hook when destroying", async () => {
      const onDestroy = vi.fn();

      const options: SDKOptions = {
        hooks: {
          onDestroy,
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      await sdk.destroy();
      sdkInstances.pop();

      expect(onDestroy).toHaveBeenCalled();
    });
  });

  describe("lifecycle hooks", () => {
    it("should call onBootstrapStart hook", async () => {
      const onBootstrapStart = vi.fn();

      const options: SDKOptions = {
        hooks: {
          onBootstrapStart,
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      await sdk.waitForReady();

      expect(onBootstrapStart).toHaveBeenCalled();
    });

    it("should call onBootstrapComplete hook", async () => {
      const onBootstrapComplete = vi.fn();

      const options: SDKOptions = {
        hooks: {
          onBootstrapComplete,
        },
        checkpointStorageAdapter: createMockCheckpointAdapter(),
      };

      const sdk = new SDKInstance(options);
      sdkInstances.push(sdk);

      await sdk.waitForReady();

      expect(onBootstrapComplete).toHaveBeenCalled();
    });
  });

  describe("multiple instances", () => {
    it("should create independent SDK instances", async () => {
      const adapter1 = createMockCheckpointAdapter();
      const adapter2 = createMockCheckpointAdapter();

      const sdk1 = new SDKInstance({ checkpointStorageAdapter: adapter1 });
      const sdk2 = new SDKInstance({ checkpointStorageAdapter: adapter2 });

      sdkInstances.push(sdk1, sdk2);

      await sdk1.waitForReady();
      await sdk2.waitForReady();

      expect(sdk1).not.toBe(sdk2);
      expect(sdk1.getGlobalContext()).not.toBe(sdk2.getGlobalContext());
    });
  });
});
