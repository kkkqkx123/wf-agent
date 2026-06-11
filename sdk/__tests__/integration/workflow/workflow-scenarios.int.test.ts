/**
 * Integration Test: Advanced Workflow Scenarios
 *
 * Covers more complex workflow patterns including conditional routing,
 * validation, error handling, and input-driven execution.
 *
 * Test cases:
 *   WF-INT-06: Conditional routing with ROUTE node
 *   WF-INT-07: Workflow validation (invalid workflow detection)
 *   WF-INT-08: Error handling (missing script, empty workflow)
 *   WF-INT-09: Workflow execution with input data
 *
 * Architecture:
 * - Uses Memory storage adapters (no persistence)
 * - Creates fresh SDK instance per test for isolation
 * - Tests through public API only
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createSDK, ExecutionBuilder, WorkflowValidatorAPI } from "@/api/index.js";
import type { SDKInstance } from "@/api/index.js";
import type { WorkflowTemplate, StaticNode } from "@wf-agent/types";
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

// Helper: create a minimal valid WorkflowTemplate with given nodes and edges.
function buildSimpleWorkflow(
  id: string,
  nodes: StaticNode[],
  edges: Array<{ sourceNodeId: string; targetNodeId: string }>,
): WorkflowTemplate {
  return {
    id,
    name: `wf-${id}`,
    version: "1.0.0",
    type: "STANDALONE",
    description: `Test workflow: ${id}`,
    nodes,
    edges,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  } as unknown as WorkflowTemplate;
}

// =============================================================================
// Test Suite
// =============================================================================

describe("Workflow Advanced Scenarios Integration", () => {
  let sdk: SDKInstance;

  beforeEach(async () => {
    sdk = createIntegrationSDK();
    await sdk.waitForReady();
  });

  afterEach(async () => {
    await sdk.shutdown();
  });

  // ===========================================================================
  // WF-INT-06: Conditional routing with ROUTE node
  // ===========================================================================

  describe("Conditional Routing (WF-INT-06)", () => {
    it("should execute workflow with a ROUTE node", async () => {
      registerScript(sdk, "route-a", "echo route-a");
      registerScript(sdk, "route-b", "echo route-b");

      const wfId = "wf-route-int";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addRouteNode(
          "router",
          [
            { condition: { expression: "true" }, targetNodeId: "path-a", priority: 1 },
          ],
          "path-b",
        )
        .addNode("path-a", "SCRIPT", { scriptName: "route-a", risk: "none" }, "Route A")
        .addNode("path-b", "SCRIPT", { scriptName: "route-b", risk: "none" }, "Route B")
        .addEndNode("end")
        .addEdge("start", "router")
        .addEdge("router", "path-a")
        .addEdge("router", "path-b")
        .addEdge("path-a", "end")
        .addEdge("path-b", "end");

      const regResult = await sdk.workflows.create(builder.build());
      expect(regResult.result.isOk()).toBe(true);

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      expect(result.isOk()).toBe(true);
    });

    it("should fallback to default route when no condition matches", async () => {
      registerScript(sdk, "default-path", "echo default");
      registerScript(sdk, "alt-path", "echo alt");

      const wfId = "wf-route-default";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addRouteNode(
          "router",
          [
            // All conditions are false, should fallback to defaultTargetNodeId
            { condition: { expression: "false" }, targetNodeId: "alt", priority: 1 },
          ],
          "default",
        )
        .addNode("default", "SCRIPT", { scriptName: "default-path", risk: "none" }, "Default Path")
        .addNode("alt", "SCRIPT", { scriptName: "alt-path", risk: "none" }, "Alt Path")
        .addEndNode("end")
        .addEdge("start", "router")
        .addEdge("router", "default")
        .addEdge("router", "alt")
        .addEdge("default", "end")
        .addEdge("alt", "end");

      const regResult = await sdk.workflows.create(builder.build());
      expect(regResult.result.isOk()).toBe(true);

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      expect(result.isOk()).toBe(true);
    });
  });

  // ===========================================================================
  // WF-INT-07: Workflow validation
  //
  // Tests WorkflowValidatorAPI directly by constructing invalid workflows.
  // Note: builder.build() enforces some basic rules (START/END presence) by
  // throwing Errors during build. We validate the underlying validator by
  // manually constructing WorkflowTemplate objects.
  // ===========================================================================

  describe("Workflow Validation (WF-INT-07)", () => {
    it("should detect workflow with missing START node via builder", () => {
      // The builder itself enforces START node presence during build().
      const wfId = "wf-no-start";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addNode("only-node", "SCRIPT", { scriptName: "nonexistent", risk: "none" })
        .addEndNode("end")
        .addEdge("only-node", "end");

      expect(() => builder.build()).toThrow("START node");
    });

    it("should detect workflow with missing END node via builder", () => {
      const wfId = "wf-no-end";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addNode("only-node", "SCRIPT", { scriptName: "nonexistent", risk: "none" })
        .addEdge("start", "only-node");

      expect(() => builder.build()).toThrow("END node");
    });

    it("should detect workflow with duplicate node IDs via WorkflowValidatorAPI", () => {
      // The builder uses a Map keyed by node id, so addNode with duplicate id
      // silently overwrites. We construct the WorkflowTemplate manually to test
      // the validator's duplicate detection.
      const nodeA: StaticNode = {
        id: "dup",
        name: "Node A",
        type: "SCRIPT",
        config: { scriptName: "s1", risk: "none" },
      } as StaticNode;
      const nodeB: StaticNode = {
        id: "dup",
        name: "Node B",
        type: "SCRIPT",
        config: { scriptName: "s2", risk: "none" },
      } as StaticNode;

      const workflow = buildSimpleWorkflow(
        "wf-dup",
        [nodeA, nodeB],
        [
          { sourceNodeId: "start", targetNodeId: "dup" },
          { sourceNodeId: "dup", targetNodeId: "end" },
        ],
      );
      // Also add START and END to avoid other validation errors
      workflow.nodes.unshift({
        id: "start",
        name: "Start",
        type: "START",
        config: {},
      } as StaticNode);
      workflow.nodes.push({
        id: "end",
        name: "End",
        type: "END",
        config: {},
      } as StaticNode);
      workflow.edges.unshift({ sourceNodeId: "start", targetNodeId: "dup" });
      workflow.edges.push({ sourceNodeId: "dup", targetNodeId: "end" });

      const validator = new WorkflowValidatorAPI();
      const result = validator.validate(workflow);
      expect(result.isOk()).toBe(false);

      if (result.isErr()) {
        const errors = result.error;
        const dupErrors = errors.filter((e: any) =>
          e.message?.includes("Node ID must be unique") || e.message?.includes("duplicate"),
        );
        expect(dupErrors.length).toBeGreaterThan(0);
      }
    });

    it("should validate a correct workflow successfully", async () => {
      const wfId = "wf-valid";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addNode("work", "SCRIPT", { scriptName: "valid-script", risk: "none" })
        .addEndNode("end")
        .addEdge("start", "work")
        .addEdge("work", "end");

      const validator = new WorkflowValidatorAPI();
      const result = validator.validate(builder.build());
      expect(result.isOk()).toBe(true);
    });
  });

  // ===========================================================================
  // WF-INT-08: Error handling
  // ===========================================================================

  describe("Error Handling (WF-INT-08)", () => {
    it("should handle execution with missing script gracefully", async () => {
      // Register workflow that references a nonexistent script
      const wfId = "wf-missing-script";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addNode("bad-node", "SCRIPT", { scriptName: "does-not-exist", risk: "none" })
        .addEndNode("end")
        .addEdge("start", "bad-node")
        .addEdge("bad-node", "end");

      const regResult = await sdk.workflows.create(builder.build());
      expect(regResult.result.isOk()).toBe(true);

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      // Execution should complete (error is captured, not thrown)
      expect(result.isOk()).toBe(true);
    });

    it("should handle empty workflow gracefully", async () => {
      const wfId = "wf-empty";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addEndNode("end")
        .addEdge("start", "end");

      const regResult = await sdk.workflows.create(builder.build());
      expect(regResult.result.isOk()).toBe(true);

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder.withWorkflow(wfId).execute();
      // START -> END minimal workflows should execute
      expect(result.isOk()).toBe(true);
    });
  });

  // ===========================================================================
  // WF-INT-09: Workflow execution with input data
  // ===========================================================================

  describe("Input Data (WF-INT-09)", () => {
    it("should execute workflow with input data", async () => {
      registerScript(sdk, "input-script", "echo input-test");

      const wfId = "wf-input-int";
      const builder = sdk.createWorkflowBuilder(wfId);
      builder
        .addStartNode("start")
        .addNode("s", "SCRIPT", { scriptName: "input-script", risk: "none" })
        .addEndNode("end")
        .addEdge("start", "s")
        .addEdge("s", "end");

      const regResult = await sdk.workflows.create(builder.build());
      expect(regResult.result.isOk()).toBe(true);

      const execBuilder = new ExecutionBuilder(sdk.getGlobalContext());
      const result = await execBuilder
        .withWorkflow(wfId)
        .withInput({ userId: "test-123", action: "process" })
        .execute();
      expect(result.isOk()).toBe(true);
    });
  });
});