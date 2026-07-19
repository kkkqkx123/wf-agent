/**
 * Integration Test: Workflow Storage Integration
 *
 * Verifies that workflow definitions are properly persisted through the
 * complete API stack: SDK API → WorkflowRegistryAPI → WorkflowRegistry.
 *
 * Test cases:
 *   WS-INT-01: Workflow is persisted and retrievable via SDK API
 *   WS-INT-02: Workflow with complex node/edge structure is persisted
 *   WS-INT-03: Multiple workflows are independently persisted
 *   WS-INT-04: Registry initialization from storage
 *   WS-INT-05: Non-existent workflow returns null
 *
 * Architecture:
 * - Uses MemoryWorkflowStorage for isolated, fast testing
 * - Tests through the public SDK API (sdk.workflows)
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createSDK } from "@/api/index.js";
import type { SDKInstance } from "@/api/index.js";
import {
  MemoryWorkflowStorage,
  MemoryCheckpointStorage,
  MemoryTaskStorage,
  MemoryWorkflowExecutionStorage,
} from "@wf-agent/storage";
import type { WorkflowTemplate } from "@wf-agent/types";

// =============================================================================
// Helpers
// =============================================================================

function createTestWorkflow(id: string, name?: string): WorkflowTemplate {
  const now = Date.now();
  return {
    id,
    name: name ?? `Test Workflow ${id}`,
    version: "1.0.0",
    type: "STANDALONE",
    description: `Integration test workflow ${id}`,
    nodes: [
      { id: "start", type: "START", config: {}, name: "Start" },
      { id: "end", type: "END", config: {}, name: "End" },
    ],
    edges: [{ id: "e1", sourceNodeId: "start", targetNodeId: "end", type: "DEFAULT" }],
    metadata: {
      tags: ["integration-test"],
    },
    createdAt: now,
    updatedAt: now,
  };
}

function createIntegrationSDK(workflowStorage?: MemoryWorkflowStorage): SDKInstance {
  const sdk = createSDK({
    debug: false,
    enableCheckpoints: false,
    enableValidation: false,
    checkpointStorageAdapter: new MemoryCheckpointStorage(),
    workflowStorageAdapter: workflowStorage ?? new MemoryWorkflowStorage(),
    taskStorageAdapter: new MemoryTaskStorage(),
    workflowExecutionStorageAdapter: new MemoryWorkflowExecutionStorage(),
    presets: {
      contextCompression: { enabled: false },
      predefinedTools: { enabled: false },
      predefinedPrompts: { enabled: false },
    },
    mcp: { enabled: false },
    gracefulShutdown: { enabled: false },
  });
  return sdk;
}

// =============================================================================
// Test Suite
// =============================================================================

describe("Workflow Storage Integration", () => {
  let sdk: SDKInstance;
  let workflowStorage: MemoryWorkflowStorage;

  beforeEach(async () => {
    workflowStorage = new MemoryWorkflowStorage();
    sdk = createIntegrationSDK(workflowStorage);
    await sdk.waitForReady();
  });

  afterEach(async () => {
    await sdk.shutdown();
  });

  // ===========================================================================
  // WS-INT-01: Create and retrieve via SDK API
  // ===========================================================================

  describe("Create and Retrieve (WS-INT-01)", () => {
    it("should persist workflow and retrieve via SDK API", async () => {
      const workflow = createTestWorkflow("wf-persist-01", "Persistence Test");
      const createResult = await sdk.workflows.create(workflow);
      expect(createResult.result.isOk()).toBe(true);

      // Verify via SDK API get()
      const getResult = await sdk.workflows.get("wf-persist-01");
      expect(getResult).not.toBeNull();
      expect(getResult!.id).toBe("wf-persist-01");
      expect(getResult!.name).toBe("Persistence Test");
    });

    it("should persist workflow with nodes and edges intact", async () => {
      const workflow = createTestWorkflow("wf-persist-nodes", "Node Test");
      workflow.nodes.push({
        id: "mid",
        type: "SCRIPT",
        config: { scriptName: "test-script", risk: "none" },
        name: "Mid Script",
      });
      workflow.edges.push(
        { id: "e2", sourceNodeId: "start", targetNodeId: "mid", type: "DEFAULT" },
        { id: "e3", sourceNodeId: "mid", targetNodeId: "end", type: "DEFAULT" },
      );

      const createResult = await sdk.workflows.create(workflow);
      expect(createResult.result.isOk()).toBe(true);

      const getResult = await sdk.workflows.get("wf-persist-nodes");
      expect(getResult).not.toBeNull();
      expect(getResult!.nodes.length).toBe(3);
      expect(getResult!.edges.length).toBe(3);
    });
  });

  // ===========================================================================
  // WS-INT-02: getWorkflowSummaries
  // ===========================================================================

  describe("Workflow Summaries (WS-INT-02)", () => {
    it("should list created workflow in summaries", async () => {
      const workflow = createTestWorkflow("wf-summary-test", "Summary Test");
      await sdk.workflows.create(workflow);

      const summaries = await sdk.workflows.getWorkflowSummaries();
      const found = summaries.find(s => s.id === "wf-summary-test");
      expect(found).toBeDefined();
      expect(found!.name).toBe("Summary Test");
      expect(found!.nodeCount).toBe(2);
      expect(found!.edgeCount).toBe(1);
    });

    it("should return empty array when no workflows exist", async () => {
      const summaries = await sdk.workflows.getWorkflowSummaries();
      expect(summaries.length).toBe(0);
    });
  });

  // ===========================================================================
  // WS-INT-03: Multiple workflows
  // ===========================================================================

  describe("Multiple Workflows (WS-INT-03)", () => {
    it("should persist and retrieve multiple workflows independently", async () => {
      const wf1 = createTestWorkflow("wf-multi-1", "First");
      const wf2 = createTestWorkflow("wf-multi-2", "Second");
      const wf3 = createTestWorkflow("wf-multi-3", "Third");

      await sdk.workflows.create(wf1);
      await sdk.workflows.create(wf2);
      await sdk.workflows.create(wf3);

      const get1 = await sdk.workflows.get("wf-multi-1");
      const get2 = await sdk.workflows.get("wf-multi-2");
      const get3 = await sdk.workflows.get("wf-multi-3");

      expect(get1).not.toBeNull();
      expect(get1!.name).toBe("First");
      expect(get2).not.toBeNull();
      expect(get2!.name).toBe("Second");
      expect(get3).not.toBeNull();
      expect(get3!.name).toBe("Third");

      // All should appear in summaries
      const summaries = await sdk.workflows.getWorkflowSummaries();
      expect(summaries.length).toBe(3);
    });
  });

  // ===========================================================================
  // WS-INT-04: Registry integration via global context
  // ===========================================================================

  describe("Registry Integration (WS-INT-04)", () => {
    it("should make workflow accessible via workflow registry", async () => {
      const workflow = createTestWorkflow("wf-registry-test", "Registry Test");
      await sdk.workflows.create(workflow);

      const registry = sdk.getGlobalContext().workflowRegistry;
      expect(registry).toBeDefined();

      // After create, the workflow should be in the registry's memory cache
      const loaded = registry.get("wf-registry-test");
      expect(loaded).toBeDefined();
      expect(loaded!.name).toBe("Registry Test");
    });
  });

  // ===========================================================================
  // WS-INT-05: Non-existent workflow handling
  // ===========================================================================

  describe("Non-existent Workflow (WS-INT-05)", () => {
    it("should return null for non-existent workflow", async () => {
      const getResult = await sdk.workflows.get("non-existent");
      expect(getResult).toBeNull();
    });

    it("should return false for has() on non-existent workflow", async () => {
      const hasResult = await sdk.workflows.has("non-existent");
      expect(hasResult).toBe(false);
    });
  });
});
