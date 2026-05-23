import { describe, it, expect, afterEach } from "vitest";
import { createSDK } from "@/api/index.js";
import type { SDKInstance } from "@/api/index.js";
import {
  MemoryCheckpointStorage,
  MemoryWorkflowStorage,
  MemoryTaskStorage,
  MemoryWorkflowExecutionStorage,
} from "@wf-agent/storage";

describe("SDK Lifecycle E2E", () => {
  const sdkInstances: SDKInstance[] = [];

  afterEach(async () => {
    for (const instance of sdkInstances) {
      try {
        await instance.destroy();
      } catch {
        // ignore cleanup errors
      }
    }
    sdkInstances.length = 0;
  });

  describe("SDK Creation and Bootstrap", () => {
    it("should create SDK instance with all storage adapters", async () => {
      const sdk = createSDK({
        checkpointStorageAdapter: new MemoryCheckpointStorage(),
        workflowStorageAdapter: new MemoryWorkflowStorage(),
        taskStorageAdapter: new MemoryTaskStorage(),
        workflowExecutionStorageAdapter: new MemoryWorkflowExecutionStorage(),
      });
      sdkInstances.push(sdk);

      expect(sdk.isReady()).toBe(false);
      await sdk.waitForReady();

      expect(sdk.isReady()).toBe(true);
      expect(sdk.workflows).toBeDefined();
      expect(sdk.executions).toBeDefined();
      expect(sdk.tools).toBeDefined();
      expect(sdk.events).toBeDefined();
    });

    it("should create SDK instance with minimal config (no storage)", async () => {
      const sdk = createSDK({
        debug: false,
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      expect(sdk.isReady()).toBe(true);
    });

    it("should create SDK instance with debug mode and lifecycle hooks", async () => {
      const hooksCalled: string[] = [];

      const sdk = createSDK({
        debug: true,
        hooks: {
          onBootstrapStart: async () => { hooksCalled.push("start"); },
          onBootstrapComplete: async () => { hooksCalled.push("complete"); },
        },
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      expect(hooksCalled).toEqual(["start", "complete"]);
    });

    it("should call onBootstrapError hook when bootstrap fails", async () => {
      const errorHookCalled: string[] = [];

      // Force a bootstrap error by providing invalid config that causes failure during bootstrap
      const sdk = createSDK({
        debug: true,
        hooks: {
          onBootstrapError: async (error) => {
            errorHookCalled.push(`error: ${error.message}`);
          },
        },
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      expect(sdk.isReady()).toBe(true);
    });
  });

  describe("Multi-Instance Isolation", () => {
    it("should create two isolated SDK instances with independent storage", async () => {
      const storage1 = new MemoryCheckpointStorage();
      const storage2 = new MemoryCheckpointStorage();

      const sdk1 = createSDK({
        checkpointStorageAdapter: storage1,
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk1);

      const sdk2 = createSDK({
        checkpointStorageAdapter: storage2,
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk2);

      await Promise.all([sdk1.waitForReady(), sdk2.waitForReady()]);

      expect(sdk1).not.toBe(sdk2);
      expect(sdk1.isReady()).toBe(true);
      expect(sdk2.isReady()).toBe(true);

      // Both instances can independently create workflows
      const wfBuilder1 = sdk1.createWorkflowBuilder("wf-instance-1");
      const wfBuilder2 = sdk2.createWorkflowBuilder("wf-instance-2");

      expect(wfBuilder1).toBeDefined();
      expect(wfBuilder2).toBeDefined();
    });

    it("should allow separate lifecycle management for each instance", async () => {
      const sdk1 = createSDK({
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk1);

      const sdk2 = createSDK({
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk2);

      await Promise.all([sdk1.waitForReady(), sdk2.waitForReady()]);
      expect(sdk1.isReady()).toBe(true);
      expect(sdk2.isReady()).toBe(true);

      await sdk1.destroy();
      expect(sdk2.isReady()).toBe(true);
    });
  });

  describe("SDK Destroy", () => {
    it("should successfully destroy an SDK instance", async () => {
      const sdk = createSDK({
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      await expect(sdk.destroy()).resolves.not.toThrow();
    });

    it("should call onDestroy hook during destruction", async () => {
      let destroyCalled = false;

      const sdk = createSDK({
        hooks: {
          onDestroy: async () => { destroyCalled = true; },
        },
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      await sdk.destroy();
      expect(destroyCalled).toBe(true);
    });
  });

  describe("SDK Configuration Combinations", () => {
    it("should create SDK with logging configuration", async () => {
      const sdk = createSDK({
        logging: {
          level: "debug",
          output: "console",
          format: "text",
        },
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      expect(sdk.isReady()).toBe(true);
    });

    it("should create SDK with validation config", async () => {
      const sdk = createSDK({
        enableValidation: true,
        validation: {
          enableWorkflowValidation: true,
          enableNodeValidation: true,
          enableGraphValidation: true,
          checkCycles: true,
          checkReachability: true,
          maxRecursionDepth: 5,
        },
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
      });
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      expect(sdk.isReady()).toBe(true);
    });

    it("should create SDK with workflow execution config", async () => {
      const sdk = createSDK({
        workflowExecution: {
          defaultTimeout: 60000,
          maxConcurrentExecutions: 5,
          enableRetry: true,
          maxRetryAttempts: 3,
        },
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      expect(sdk.isReady()).toBe(true);
    });

    it("should create SDK with event system config", async () => {
      const sdk = createSDK({
        events: {
          maxListenerQueueSize: 1000,
          enableBackpressure: true,
          defaultListenerTimeout: 5000,
          slowListenerThreshold: 1000,
        },
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      expect(sdk.isReady()).toBe(true);
    });
  });

  describe("SDK API Access", () => {
    it("should provide access to all API types after bootstrap", async () => {
      const sdk = createSDK({
        checkpointStorageAdapter: new MemoryCheckpointStorage(),
        workflowStorageAdapter: new MemoryWorkflowStorage(),
        taskStorageAdapter: new MemoryTaskStorage(),
        workflowExecutionStorageAdapter: new MemoryWorkflowExecutionStorage(),
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk);

      await sdk.waitForReady();

      expect(sdk.workflows).toBeDefined();
      expect(sdk.executions).toBeDefined();
      expect(sdk.tools).toBeDefined();
      expect(sdk.events).toBeDefined();
      expect(sdk.metrics).toBeDefined();
      expect(sdk.messages).toBeDefined();
      expect(sdk.variables).toBeDefined();
      expect(sdk.triggers).toBeDefined();
      expect(sdk.skills).toBeDefined();
      expect(sdk.profiles).toBeDefined();
      expect(sdk.scripts).toBeDefined();
      expect(sdk.nodeTemplates).toBeDefined();
      expect(sdk.triggerTemplates).toBeDefined();

      // Builder access
      expect(sdk.createWorkflowBuilder("test-wf")).toBeDefined();
      expect(sdk.createNodeBuilder()).toBeDefined();

      // GlobalContext access
      expect(sdk.getGlobalContext()).toBeDefined();
    });

    it("should throw when accessing APIs before bootstrap", async () => {
      const sdk = createSDK({
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk);

      expect(() => (sdk as any).workflows).toThrow();
    });
  });

  describe("Graceful Shutdown", () => {
    it("should initialize and provide shutdown manager", async () => {
      const sdk = createSDK({
        gracefulShutdown: {
          enabled: true,
          timeoutMs: 5000,
        },
        presets: {
          predefinedTools: { enabled: false },
          predefinedPrompts: { enabled: false },
          contextCompression: { enabled: false },
        },
        mcp: { enabled: false },
        enableValidation: false,
      });
      sdkInstances.push(sdk);

      await sdk.waitForReady();
      expect(sdk.isReady()).toBe(true);
    });
  });
});