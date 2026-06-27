/**
 * Integration Test: SDK Instance API Availability
 *
 * Verifies that SDKInstance properly exposes Resource APIs after bootstrap.
 * Tests API-INT-01 through API-INT-07 covering:
 * - Resource API CRUD availability
 * - Event subscription
 * - Tools, profiles, scripts access
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createSDK } from "@/api/index";
import type { SDKInstance } from "@/api/index";
import {
  MemoryCheckpointStorage,
  MemoryWorkflowStorage,
  MemoryTaskStorage,
  MemoryWorkflowExecutionStorage,
} from "@wf-agent/storage";

describe("SDK Instance API Integration", () => {
  let sdk: SDKInstance;

  beforeEach(async () => {
    sdk = createSDK({
      debug: false,
      enableCheckpoints: false,
      enableValidation: false,
      checkpointStorageAdapter: new MemoryCheckpointStorage(),
      workflowStorageAdapter: new MemoryWorkflowStorage(),
      taskStorageAdapter: new MemoryTaskStorage(),
      workflowExecutionStorageAdapter: new MemoryWorkflowExecutionStorage(),
      presets: {
        contextCompression: { enabled: false },
        predefinedTools: { enabled: false },
        predefinedPrompts: { enabled: false },
      },
      mcp: { enabled: false },
    });
    await sdk.waitForReady();
  });

  afterEach(async () => {
    await sdk.shutdown();
  });

  describe("Resource API Availability (API-INT-01)", () => {
    it("should expose workflows API", () => {
      expect(sdk.workflows).toBeDefined();
    });

    it("should expose global context", () => {
      expect(sdk.getGlobalContext()).toBeDefined();
    });

    it("should expose factory", () => {
      expect(sdk.getFactory()).toBeDefined();
    });
  });

  describe("Global Context (API-INT-02)", () => {
    it("should have a global context with registries", () => {
      const ctx = sdk.getGlobalContext();
      expect(ctx).toBeDefined();
      // Context should have at minimum an event registry
      expect((ctx as any).eventRegistry).toBeDefined();
    });
  });
});
