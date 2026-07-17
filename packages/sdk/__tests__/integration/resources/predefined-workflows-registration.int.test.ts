/**
 * Integration Test: Predefined Workflows Registration
 *
 * Tests the registerPredefinedWorkflows() and unregisterPredefinedWorkflows() functions.
 *
 * Real business scenarios:
 * 1. SDK bootstrap: Register the llm_summary_workflow at startup (skipIfExists=true)
 * 2. Custom config: Register the workflow with custom compressionPrompt/timeout/maxTriggers
 * 3. Security: Block specific workflows via blockList
 * 4. Cleanup: Unregister workflows when disabling the feature or during shutdown
 */

import { describe, it, expect, beforeEach } from "vitest";
import { WorkflowRegistry } from "@sdk/workflow/registry/workflow-registry.js";
import { WorkflowGraphRegistry } from "@sdk/workflow/registry/workflow-graph-registry.js";
import { WorkflowRelationshipRegistry } from "@sdk/workflow/registry/workflow-relationship-registry.js";
import type { WorkflowExecutionRegistry } from "@sdk/workflow/registry/workflow-execution-registry.js";
import {
  registerPredefinedWorkflows,
  unregisterPredefinedWorkflows,
  isPredefinedWorkflowRegistered,
} from "@/resources/predefined/workflow/registration.js";
import { LLM_SUMMARY_WORKFLOW_ID } from "@/resources/predefined/workflow/llm-summary.js";

// =============================================================================
// Constants
// =============================================================================

const WORKFLOW_ID = LLM_SUMMARY_WORKFLOW_ID;
const WORKFLOW_COUNT = 1; // Only one predefined workflow: llm_summary_workflow

// =============================================================================
// Mock WorkflowExecutionRegistry
// =============================================================================

function createMockExecutionRegistry(): WorkflowExecutionRegistry {
  return {
    isWorkflowActive: () => false,
    getAll: () => [],
  } as unknown as WorkflowExecutionRegistry;
}

// =============================================================================
// Tests
// =============================================================================

describe("Predefined Workflows Registration", () => {
  let registry: WorkflowRegistry;
  let graphRegistry: WorkflowGraphRegistry;
  let relationshipRegistry: WorkflowRelationshipRegistry;

  beforeEach(() => {
    graphRegistry = new WorkflowGraphRegistry();
    relationshipRegistry = new WorkflowRelationshipRegistry();
    const executionRegistry = createMockExecutionRegistry();
    registry = new WorkflowRegistry(null, executionRegistry, relationshipRegistry, graphRegistry);
  });

  // ---------------------------------------------------------------------------
  // Scenario: SDK bootstrap — register the default LLM summary workflow
  // ---------------------------------------------------------------------------
  describe("register default workflow (SDK bootstrap)", () => {
    it("should register the default LLM summary workflow successfully", async () => {
      const result = await registerPredefinedWorkflows(registry);

      expect(result.success).toHaveLength(WORKFLOW_COUNT);
      expect(result.success).toContain(WORKFLOW_ID);
      expect(result.failures).toHaveLength(0);
    });

    it("should make the workflow queryable in the registry after registration", async () => {
      await registerPredefinedWorkflows(registry);

      expect(registry.has(WORKFLOW_ID)).toBe(true);
      const workflow = registry.get(WORKFLOW_ID);
      expect(workflow).toBeDefined();
      expect(workflow!.id).toBe(WORKFLOW_ID);
    });

    it("should populate workflow template fields correctly", async () => {
      await registerPredefinedWorkflows(registry);

      const workflow = registry.get(WORKFLOW_ID)!;
      expect(workflow.name).toBe("LLM Summary Workflow");
      expect(workflow.type).toBe("TRIGGERED_SUBWORKFLOW");
      expect(workflow.description).toBeTruthy();
      expect(workflow.version).toBe("1.0.0");
      expect(workflow.nodes).toHaveLength(4);
      expect(workflow.edges).toHaveLength(3);
      expect(workflow.metadata).toBeDefined();
      expect(workflow.metadata!.category).toBe("system");
    });

    it("should have correct node structure (4 nodes: START, LLM, CONTEXT_PROCESSOR, CONTINUE)", async () => {
      await registerPredefinedWorkflows(registry);

      const workflow = registry.get(WORKFLOW_ID)!;
      const nodeTypes = workflow.nodes.map((n) => n.type);
      expect(nodeTypes).toContain("START_FROM_TRIGGER");
      expect(nodeTypes).toContain("LLM");
      expect(nodeTypes).toContain("CONTEXT_PROCESSOR");
      expect(nodeTypes).toContain("CONTINUE_FROM_TRIGGER");
    });

    it("should have correct edge connections (3 edges)", async () => {
      await registerPredefinedWorkflows(registry);

      const workflow = registry.get(WORKFLOW_ID)!;
      expect(workflow.edges).toHaveLength(3);
      expect(workflow.edges[0].sourceNodeId).toBe("llm-summary-start");
      expect(workflow.edges[0].targetNodeId).toBe("llm-summary-llm");
      expect(workflow.edges[1].sourceNodeId).toBe("llm-summary-llm");
      expect(workflow.edges[1].targetNodeId).toBe("llm-summary-truncate");
      expect(workflow.edges[2].sourceNodeId).toBe("llm-summary-truncate");
      expect(workflow.edges[2].targetNodeId).toBe("llm-summary-end");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Custom config — register with custom compressionPrompt/timeout/maxTriggers
  // ---------------------------------------------------------------------------
  describe("register with custom configuration", () => {
    it("should apply custom timeout and maxTriggers when config is provided", async () => {
      const result = await registerPredefinedWorkflows(registry, {
        config: {
          llmSummary: {
            timeout: 120000,
            maxTriggers: 5,
          },
        },
      });

      expect(result.success).toHaveLength(WORKFLOW_COUNT);
      const workflow = registry.get(WORKFLOW_ID)!;
      expect(workflow.triggeredSubworkflowConfig).toBeDefined();
      expect(workflow.triggeredSubworkflowConfig!.timeout).toBe(120000);
      expect(workflow.triggeredSubworkflowConfig!.maxRetries).toBe(5);
    });

    it("should apply custom compressionPrompt when config is provided", async () => {
      const customPrompt = "Custom summary prompt for testing";
      const result = await registerPredefinedWorkflows(registry, {
        config: {
          llmSummary: {
            compressionPrompt: customPrompt,
          },
        },
      });

      expect(result.success).toHaveLength(WORKFLOW_COUNT);
      const workflow = registry.get(WORKFLOW_ID)!;
      const llmNode = workflow.nodes.find((n) => n.id === "llm-summary-llm")!;
      expect(llmNode.config.parameters.systemPrompt).toBe(customPrompt);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: skipIfExists behavior
  // ---------------------------------------------------------------------------
  describe("skipIfExists behavior", () => {
    it("should skip already registered workflow when skipIfExists=true (default)", async () => {
      await registerPredefinedWorkflows(registry);
      const result = await registerPredefinedWorkflows(registry);

      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(0);
    });

    it("should report failures when skipIfExists=false and workflow already exists", async () => {
      await registerPredefinedWorkflows(registry);

      const result = await registerPredefinedWorkflows(registry, undefined, false);

      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(WORKFLOW_COUNT);
      for (const failure of result.failures) {
        expect(failure.error).toContain("already exists");
      }
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: allowList — only enable specific workflows
  // ---------------------------------------------------------------------------
  describe("allowList filtering", () => {
    it("should register the workflow when it is in the allowList", async () => {
      const result = await registerPredefinedWorkflows(registry, {
        allowList: [WORKFLOW_ID],
      });

      expect(result.success).toHaveLength(WORKFLOW_COUNT);
      expect(registry.has(WORKFLOW_ID)).toBe(true);
    });

    it("should skip the workflow when it is not in the allowList", async () => {
      const result = await registerPredefinedWorkflows(registry, {
        allowList: ["some_other_workflow"],
      });

      expect(result.success).toHaveLength(0);
      expect(registry.has(WORKFLOW_ID)).toBe(false);
    });

    it("should register all workflows when allowList is empty (no filtering)", async () => {
      const result = await registerPredefinedWorkflows(registry, {
        allowList: [],
      });

      expect(result.success).toHaveLength(WORKFLOW_COUNT);
      expect(registry.has(WORKFLOW_ID)).toBe(true);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: blockList — disable specific workflows
  // ---------------------------------------------------------------------------
  describe("blockList filtering", () => {
    it("should skip the workflow when it is in the blockList", async () => {
      const result = await registerPredefinedWorkflows(registry, {
        blockList: [WORKFLOW_ID],
      });

      expect(result.success).toHaveLength(0);
      expect(registry.has(WORKFLOW_ID)).toBe(false);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: allowList + blockList interaction
  // ---------------------------------------------------------------------------
  describe("allowList + blockList interaction", () => {
    it("should give priority to allowList when both are set", async () => {
      const result = await registerPredefinedWorkflows(registry, {
        allowList: [WORKFLOW_ID],
        blockList: [WORKFLOW_ID],
      });

      expect(result.success).toHaveLength(WORKFLOW_COUNT);
      expect(registry.has(WORKFLOW_ID)).toBe(true);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: unregister workflows
  // ---------------------------------------------------------------------------
  describe("unregister workflows", () => {
    it("should unregister the workflow when no workflowIds specified", async () => {
      await registerPredefinedWorkflows(registry);
      expect(registry.has(WORKFLOW_ID)).toBe(true);

      const result = await unregisterPredefinedWorkflows(registry);

      expect(result.success).toHaveLength(WORKFLOW_COUNT);
      expect(result.failures).toHaveLength(0);
      expect(registry.has(WORKFLOW_ID)).toBe(false);
    });

    it("should unregister only specified workflow IDs", async () => {
      await registerPredefinedWorkflows(registry);

      const result = await unregisterPredefinedWorkflows(registry, [WORKFLOW_ID]);

      expect(result.success).toHaveLength(WORKFLOW_COUNT);
      expect(registry.has(WORKFLOW_ID)).toBe(false);
    });

    it("should not fail when unregistering a non-existent workflow", async () => {
      const result = await unregisterPredefinedWorkflows(registry, ["non_existent_workflow"]);

      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: registration status check
  // ---------------------------------------------------------------------------
  describe("isPredefinedWorkflowRegistered", () => {
    it("should return false before registration", () => {
      expect(isPredefinedWorkflowRegistered(registry, WORKFLOW_ID)).toBe(false);
    });

    it("should return true after registration", async () => {
      await registerPredefinedWorkflows(registry);
      expect(isPredefinedWorkflowRegistered(registry, WORKFLOW_ID)).toBe(true);
    });

    it("should return false after unregistration", async () => {
      await registerPredefinedWorkflows(registry);
      await unregisterPredefinedWorkflows(registry, [WORKFLOW_ID]);
      expect(isPredefinedWorkflowRegistered(registry, WORKFLOW_ID)).toBe(false);
    });
  });
});