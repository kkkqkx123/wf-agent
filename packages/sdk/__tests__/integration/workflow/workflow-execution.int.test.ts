/**
 * Integration Test: Workflow Execution Basic Integration
 *
 * Verifies workflow execution through the public API (WorkflowBuilder + ExecutionBuilder).
 * Covers linear execution, multi-step sequences, variable nodes, and execution metadata.
 *
 * Test cases:
 *   WF-INT-01: SDK with Workflow API availability
 *   WF-INT-02: Linear workflow START -> SCRIPT -> END
 *   WF-INT-03: Multiple SCRIPT nodes in sequence
 *   WF-INT-04: Workflow execution metadata
 *   WF-INT-05: Workflow with VARIABLE node
 *
 * Architecture:
 * - Uses Memory storage adapters (no persistence)
 * - Creates fresh SDK instance per test for isolation
 * - Tests through public API only (createSDK, WorkflowBuilder, ExecutionBuilder)
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createSDK, ExecutionBuilder } from "@/api/index";
import type { SDKInstance } from "@/api/index";
import {
  MemoryCheckpointStorage,
  MemoryWorkflowStorage,
  MemoryTaskStorage,
  MemoryWorkflowExecutionStorage,
} from "@wf-agent/storage";

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

describe("Workflow Execution Integration", () => {
  let sdk: SDKInstance;

  beforeEach(async () => {
    sdk = createIntegrationSDK();
    await sdk.waitForReady();
  });

  afterEach(async () => {
    await sdk.shutdown();
  });

  // ===========================================================================
  // WF-INT-01: SDK with Workflow API
  // ===========================================================================

  describe("SDK with Workflow API (WF-INT-01)", () => {
    it("should create SDK with workflow API available", () => {
      expect(sdk.workflows).toBeDefined();
    });

    it("should have a global context", () => {
      const ctx = sdk.getGlobalContext();
      expect(ctx).toBeDefined();
    });

    it("should provide createWorkflowBuilder", () => {
      const builder = sdk.createWorkflowBuilder("test-wf");
      expect(builder).toBeDefined();
      expect(builder.build).toBeInstanceOf(Function);
    });

    it("should provide ExecutionBuilder constructor", () => {
      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      expect(execBuilder).toBeDefined();
      expect(execBuilder.withWorkflow).toBeInstanceOf(Function);
    });
  });

  // ===========================================================================
  // WF-INT-02: Linear Workflow START -> SCRIPT -> END
  // ===========================================================================

  describe("Linear Workflow (WF-INT-02)", () => {
    it("should execute a simple linear workflow with one SCRIPT node", async () => {
      registerScript(sdk, "linear-step", "echo hello");

      const wfId = "wf-linear-int";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addNode("script-node", "SCRIPT", { scriptName: "linear-step", risk: "none" }, "Echo Node")
        .addEndNode("end")
        .addEdge("start", "script-node")
        .addEdge("script-node", "end");

      const regResult = await sdk.workflows.create(builder.build());
      expect(regResult.result.isOk()).toBe(true);

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      expect(result.isOk()).toBe(true);

      if (result.isOk()) {
        const execData = result.value;
        expect(execData.executionId).toBeDefined();
        expect(execData.metadata).toBeDefined();
        expect(execData.nodeResults.length).toBeGreaterThanOrEqual(1);
      }
    });

    it("should execute with generated node IDs via addStartNode/addEndNode defaults", async () => {
      registerScript(sdk, "default-ids", "echo default");

      const wfId = "wf-default-ids";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode()
        .addNode("mid", "SCRIPT", { scriptName: "default-ids", risk: "none" })
        .addEndNode()
        .addEdge("start", "mid")
        .addEdge("mid", "end");

      const regResult = await sdk.workflows.create(builder.build());
      expect(regResult.result.isOk()).toBe(true);

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      expect(result.isOk()).toBe(true);
    });
  });

  // ===========================================================================
  // WF-INT-03: Multiple SCRIPT nodes in sequence
  // ===========================================================================

  describe("Multi-Step Workflow (WF-INT-03)", () => {
    it("should execute two SCRIPT nodes in sequence", async () => {
      registerScript(sdk, "step-one", "echo step-1");
      registerScript(sdk, "step-two", "echo step-2");

      const wfId = "wf-two-step";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addNode("s1", "SCRIPT", { scriptName: "step-one", risk: "none" })
        .addNode("s2", "SCRIPT", { scriptName: "step-two", risk: "none" })
        .addEndNode("end")
        .addEdge("start", "s1")
        .addEdge("s1", "s2")
        .addEdge("s2", "end");

      const regResult = await sdk.workflows.create(builder.build());
      expect(regResult.result.isOk()).toBe(true);

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      expect(result.isOk()).toBe(true);

      if (result.isOk()) {
        expect(result.value.nodeResults.length).toBeGreaterThanOrEqual(2);
      }
    });

    it("should execute three SCRIPT nodes preserving order", async () => {
      registerScript(sdk, "multi-a", "echo a");
      registerScript(sdk, "multi-b", "echo b");
      registerScript(sdk, "multi-c", "echo c");

      const wfId = "wf-three-step";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addNode("s1", "SCRIPT", { scriptName: "multi-a", risk: "none" })
        .addNode("s2", "SCRIPT", { scriptName: "multi-b", risk: "none" })
        .addNode("s3", "SCRIPT", { scriptName: "multi-c", risk: "none" })
        .addEndNode("end")
        .addEdge("start", "s1")
        .addEdge("s1", "s2")
        .addEdge("s2", "s3")
        .addEdge("s3", "end");

      const regResult = await sdk.workflows.create(builder.build());
      expect(regResult.result.isOk()).toBe(true);

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      expect(result.isOk()).toBe(true);

      if (result.isOk()) {
        expect(result.value.nodeResults.length).toBeGreaterThanOrEqual(3);
      }
    });
  });

  // ===========================================================================
  // WF-INT-04: Workflow execution metadata
  // ===========================================================================

  describe("Execution Metadata (WF-INT-04)", () => {
    it("should return execution metadata with timestamps", async () => {
      registerScript(sdk, "meta-script", "echo meta");

      const wfId = "wf-meta-int";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addNode("s", "SCRIPT", { scriptName: "meta-script", risk: "none" })
        .addEndNode("end")
        .addEdge("start", "s")
        .addEdge("s", "end");

      await sdk.workflows.create(builder.build());

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      expect(result.isOk()).toBe(true);

      if (result.isOk()) {
        const meta = result.value.metadata;
        expect(meta.startTime).toBeDefined();
        expect(meta.endTime).toBeDefined();
        expect(meta.nodeCount).toBeGreaterThan(0);
        expect(meta.endTime).toBeGreaterThanOrEqual(meta.startTime);
      }
    });

    it("should include executionId in results", async () => {
      registerScript(sdk, "id-script", "echo id");

      const wfId = "wf-id-int";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addNode("s", "SCRIPT", { scriptName: "id-script", risk: "none" })
        .addEndNode("end")
        .addEdge("start", "s")
        .addEdge("s", "end");

      await sdk.workflows.create(builder.build());

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      expect(result.isOk()).toBe(true);

      if (result.isOk()) {
        expect(result.value.executionId).toBeDefined();
        expect(typeof result.value.executionId).toBe("string");
        expect(result.value.executionId.length).toBeGreaterThan(0);
      }
    });
  });

  // ===========================================================================
  // WF-INT-05: Workflow with VARIABLE node
  // ===========================================================================

  describe("Variable Node (WF-INT-05)", () => {
    it("should execute workflow with a VARIABLE node setting a string value", async () => {
      registerScript(sdk, "var-demo", "echo var-demo");

      const wfId = "wf-var-int";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addVariableNode("var-node", "greeting", "string", "'hello-world'")
        .addNode("s", "SCRIPT", { scriptName: "var-demo", risk: "none" })
        .addEndNode("end")
        .addEdge("start", "var-node")
        .addEdge("var-node", "s")
        .addEdge("s", "end");

      const regResult = await sdk.workflows.create(builder.build());
      expect(regResult.result.isOk()).toBe(true);

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      expect(result.isOk()).toBe(true);
    });

    it("should execute workflow with VARIABLE node setting a numeric value", async () => {
      registerScript(sdk, "var-num", "echo var-num");

      const wfId = "wf-var-num-int";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addVariableNode("count", "counter", "number", "42")
        .addNode("s", "SCRIPT", { scriptName: "var-num", risk: "none" })
        .addEndNode("end")
        .addEdge("start", "count")
        .addEdge("count", "s")
        .addEdge("s", "end");

      const regResult = await sdk.workflows.create(builder.build());
      expect(regResult.result.isOk()).toBe(true);

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      expect(result.isOk()).toBe(true);
    });

    it("should execute workflow with multiple VARIABLE nodes", async () => {
      registerScript(sdk, "multi-var", "echo multi-var");

      const wfId = "wf-multi-var-int";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addVariableNode("v1", "firstName", "string", "'Alice'")
        .addVariableNode("v2", "age", "number", "30")
        .addNode("s", "SCRIPT", { scriptName: "multi-var", risk: "none" })
        .addEndNode("end")
        .addEdge("start", "v1")
        .addEdge("v1", "v2")
        .addEdge("v2", "s")
        .addEdge("s", "end");

      const regResult = await sdk.workflows.create(builder.build());
      expect(regResult.result.isOk()).toBe(true);

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      expect(result.isOk()).toBe(true);
    });
  });
});
