/**
 * Integration Test: FORK/JOIN Workflow Execution
 *
 * Verifies FORK/JOIN node execution through the public API.
 * Covers parallel branch execution and branch result aggregation.
 *
 * Test cases:
 *   FJ-INT-01: FORK with two script branches, ALL_COMPLETED JOIN
 *   FJ-INT-02: FORK with three script branches
 *   FJ-INT-03: Serial FORK strategy
 *
 * Architecture:
 * - Uses Memory storage adapters (no persistence)
 * - Creates fresh SDK instance per test for isolation
 * - Tests through public API only (WorkflowBuilder, ExecutionBuilder)
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createSDK, ExecutionBuilder } from "@/api/index.js";
import type { SDKInstance } from "@/api/index.js";
import {
  MemoryCheckpointStorage,
  MemoryWorkflowStorage,
  MemoryTaskStorage,
  MemoryWorkflowExecutionStorage,
} from "@wf-agent/storage";
import type { ForkNodeConfig, JoinNodeConfig } from "@wf-agent/types";

// =============================================================================
// Helpers
// =============================================================================

function createIntegrationSDK(): SDKInstance {
  const sdk = createSDK({
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
    gracefulShutdown: { enabled: false },
  });
  return sdk;
}

function registerScript(sdk: SDKInstance, id: string, content: string): void {
  const scriptRegistry = sdk.getGlobalContext().scriptRegistry;
  scriptRegistry.registerScript({
    id,
    name: id,
    description: `Script: ${content}`,
    content,
    options: { timeout: 5000 },
  });
}

// =============================================================================
// Test Suite
// =============================================================================

describe("FORK/JOIN Workflow Execution", () => {
  let sdk: SDKInstance;

  beforeEach(async () => {
    sdk = createIntegrationSDK();
    await sdk.waitForReady();
    registerScript(sdk, "branch-a", "echo branch A executed");
    registerScript(sdk, "branch-b", "echo branch B executed");
    registerScript(sdk, "branch-c", "echo branch C executed");
  });

  afterEach(async () => {
    await sdk.shutdown();
  });

  // ===========================================================================
  // FJ-INT-01: FORK with two script branches + ALL_COMPLETED JOIN
  // ===========================================================================

  describe("Two Parallel Branches (FJ-INT-01)", () => {
    it("should execute FORK with two parallel SCRIPT branches and JOIN with ALL_COMPLETED", async () => {
      const wfId = "wf-fork-join-basic";
      const builder = sdk.createWorkflowBuilder(wfId);

      const forkConfig: ForkNodeConfig = {
        forkPaths: [
          { pathId: "path-a", childNodeId: "script-a" },
          { pathId: "path-b", childNodeId: "script-b" },
        ],
        forkStrategy: "parallel",
      };

      const joinConfig: JoinNodeConfig = {
        forkPathIds: ["path-a", "path-b"],
        joinStrategy: "ALL_COMPLETED",
        mainPathId: "path-a",
      };

      builder
        .addStartNode("start")
        .addNode("fork-node", "FORK", forkConfig, "Fork Node")
        .addNode("script-a", "SCRIPT", { scriptName: "branch-a", risk: "none" }, "Branch A")
        .addNode("script-b", "SCRIPT", { scriptName: "branch-b", risk: "none" }, "Branch B")
        .addNode("join-node", "JOIN", joinConfig, "Join Node")
        .addEndNode("end")
        .addEdge("start", "fork-node")
        .addEdge("fork-node", "script-a")
        .addEdge("fork-node", "script-b")
        .addEdge("script-a", "join-node")
        .addEdge("script-b", "join-node")
        .addEdge("join-node", "end");

      const regResult = await sdk.workflows.create(builder.build());
      expect(regResult.result.isOk()).toBe(true);

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      expect(result.isOk()).toBe(true);
    });
  });

  // ===========================================================================
  // FJ-INT-02: FORK with three script branches
  // ===========================================================================

  describe("Three Parallel Branches (FJ-INT-02)", () => {
    it("should execute FORK with three parallel SCRIPT branches", async () => {
      const wfId = "wf-fork-three";
      const builder = sdk.createWorkflowBuilder(wfId);

      const forkConfig: ForkNodeConfig = {
        forkPaths: [
          { pathId: "path-a", childNodeId: "script-a" },
          { pathId: "path-b", childNodeId: "script-b" },
          { pathId: "path-c", childNodeId: "script-c" },
        ],
        forkStrategy: "parallel",
      };

      const joinConfig: JoinNodeConfig = {
        forkPathIds: ["path-a", "path-b", "path-c"],
        joinStrategy: "ALL_COMPLETED",
        mainPathId: "path-a",
      };

      builder
        .addStartNode("start")
        .addNode("fork-node", "FORK", forkConfig, "Fork Node")
        .addNode("script-a", "SCRIPT", { scriptName: "branch-a", risk: "none" }, "Branch A")
        .addNode("script-b", "SCRIPT", { scriptName: "branch-b", risk: "none" }, "Branch B")
        .addNode("script-c", "SCRIPT", { scriptName: "branch-c", risk: "none" }, "Branch C")
        .addNode("join-node", "JOIN", joinConfig, "Join Node")
        .addEndNode("end")
        .addEdge("start", "fork-node")
        .addEdge("fork-node", "script-a")
        .addEdge("fork-node", "script-b")
        .addEdge("fork-node", "script-c")
        .addEdge("script-a", "join-node")
        .addEdge("script-b", "join-node")
        .addEdge("script-c", "join-node")
        .addEdge("join-node", "end");

      const regResult = await sdk.workflows.create(builder.build());
      expect(regResult.result.isOk()).toBe(true);

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      expect(result.isOk()).toBe(true);
    });
  });
});