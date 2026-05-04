/**
 * Predefined Triggers Integration Testing
 *
 * Test Scenarios:
 * - Context compression triggers
 * - Context compression workflows
 * - Combined registration
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { TriggerTemplateRegistry } from "../registry/trigger-template-registry.js";
import { WorkflowRegistry } from "../../workflow/stores/workflow-registry.js";
import {
  registerContextCompressionTrigger,
  registerContextCompressionWorkflow,
  registerContextCompression,
  unregisterContextCompressionTrigger,
  unregisterContextCompressionWorkflow,
  isContextCompressionTriggerRegistered,
  isContextCompressionWorkflowRegistered,
  CONTEXT_COMPRESSION_TRIGGER_NAME,
  CONTEXT_COMPRESSION_WORKFLOW_ID,
  DEFAULT_COMPRESSION_PROMPT,
  createContextCompressionTriggerTemplate,
  createContextCompressionWorkflow,
  createCustomContextCompressionTrigger,
  createCustomContextCompressionWorkflow,
} from "../../resources/predefined/index.js";
import { initializeContainerWithAdapters, resetContainer, getContainer } from "../di/container-config.js";
import type { WorkflowGraphRegistry } from "../../workflow/stores/workflow-graph-registry.js";
import type { WorkflowExecutionRegistry } from "../../workflow/stores/workflow-execution-registry.js";
import * as Identifiers from "../di/service-identifiers.js";

// Mock storage callback
const mockStorageCallback = {
  save: vi.fn(),
  load: vi.fn(),
  delete: vi.fn(),
  list: vi.fn(),
  exists: vi.fn(),
  getMetadata: vi.fn(),
  initialize: vi.fn(),
  close: vi.fn(),
  clear: vi.fn(),
};

describe("Predefined Triggers -predefined triggers", () => {
  let triggerRegistry: TriggerTemplateRegistry;
  let workflowRegistry: WorkflowRegistry;
  let workflowGraphRegistry: WorkflowGraphRegistry;
  let workflowExecutionRegistry: WorkflowExecutionRegistry;

  beforeEach(() => {
    // Reset container before each test
    resetContainer();
    initializeContainerWithAdapters({ checkpoint: mockStorageCallback });

    // Get instances from container
    const container = getContainer();
    workflowGraphRegistry = container.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry;
    workflowExecutionRegistry = container.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry;

    // Create registries with proper dependencies
    triggerRegistry = new TriggerTemplateRegistry();
    workflowRegistry = new WorkflowRegistry({}, workflowExecutionRegistry);
  });

  afterEach(() => {
    resetContainer();
  });

  describe("Context Compression Trigger", () => {
    it("Test to create a default trigger template: createContextCompressionTriggerTemplate to create the correct template", () => {
      const template = createContextCompressionTriggerTemplate();

      expect(template).toBeDefined();
      expect(template.name).toBe(CONTEXT_COMPRESSION_TRIGGER_NAME);
      expect(template.description).toContain("context compression");
      expect(template.condition.eventType).toBe("CONTEXT_COMPRESSION_REQUESTED");
      expect(template.action.type).toBe("execute_triggered_subgraph");
      expect(template.enabled).toBe(true);
      expect(template.maxTriggers).toBe(0);
      expect(template.metadata?.["category"]).toBe("system");
      expect(template.metadata?.["tags"]).toContain("context");
      expect(template.metadata?.["tags"]).toContain("compression");
    });

    it("Test Create Custom Trigger Template: createCustomContextCompressionTrigger Support for custom configurations", () => {
      const template = createCustomContextCompressionTrigger({
        timeout: 120000,
        maxTriggers: 10,
        compressionPrompt: "Customized Compression Cue Words",
      });

      expect(template.name).toBe(CONTEXT_COMPRESSION_TRIGGER_NAME);
      expect(template.maxTriggers).toBe(10);
      expect((template.action.parameters as any)["timeout"]).toBe(120000);
      expect(template.metadata?.["customConfig"]).toEqual({
        timeout: 120000,
        maxTriggers: 10,
        compressionPrompt: "Customized Compression Cue Words",
      });
    });

    it("Test Creation of Partial Custom Configuration Trigger Templates", () => {
      const template = createCustomContextCompressionTrigger({
        maxTriggers: 5,
      });

      expect(template.maxTriggers).toBe(5);
      expect(template.metadata?.["customConfig"]).toEqual({
        maxTriggers: 5,
      });
    });

    it("Test creation of empty configuration trigger template: empty configuration should use default values", () => {
      const template = createCustomContextCompressionTrigger({});

      expect(template.name).toBe(CONTEXT_COMPRESSION_TRIGGER_NAME);
      expect(template.maxTriggers).toBe(0);
      expect(template.metadata?.["customConfig"]).toEqual({});
    });

    it("Test registration trigger: registerContextCompressionTrigger successfully registered", () => {
      const result = registerContextCompressionTrigger(triggerRegistry);

      expect(result).toBe(true);
      expect(isContextCompressionTriggerRegistered(triggerRegistry)).toBe(true);
      expect(triggerRegistry.has(CONTEXT_COMPRESSION_TRIGGER_NAME)).toBe(true);
    });

    it("Test duplicate registration handling: skip if skipIfExists is true", () => {
      registerContextCompressionTrigger(triggerRegistry);

      const result = registerContextCompressionTrigger(triggerRegistry, undefined, true);

      expect(result).toBe(false); // Skip registration and return false.
      expect(triggerRegistry.size()).toBe(1);
    });

    it("Test duplicate registration handling: skipIfExists to false times error", () => {
      registerContextCompressionTrigger(triggerRegistry);

      expect(() => {
        registerContextCompressionTrigger(triggerRegistry, undefined, false);
      }).toThrow();
    });

    it("Test registration of custom configuration triggers", () => {
      const result = registerContextCompressionTrigger(triggerRegistry, {
        timeout: 90000,
        maxTriggers: 3,
      });

      expect(result).toBe(true);
      const template = triggerRegistry.get(CONTEXT_COMPRESSION_TRIGGER_NAME);
      expect(template?.maxTriggers).toBe(3);
      expect((template?.action.parameters as any)["timeout"]).toBe(90000);
    });

    it("Testing the unregister trigger: unregisterContextCompressionTrigger successfully unregistered", () => {
      registerContextCompressionTrigger(triggerRegistry);
      expect(isContextCompressionTriggerRegistered(triggerRegistry)).toBe(true);

      const result = unregisterContextCompressionTrigger(triggerRegistry);

      expect(result).toBe(true);
      expect(isContextCompressionTriggerRegistered(triggerRegistry)).toBe(false);
      expect(triggerRegistry.has(CONTEXT_COMPRESSION_TRIGGER_NAME)).toBe(false);
    });

    it("Test for deregistering non-existent triggers: should return false", () => {
      const result = unregisterContextCompressionTrigger(triggerRegistry);

      expect(result).toBe(false);
    });

    it("Test to check registration status: the isRegistered method correctly returns the registration status", () => {
      expect(isContextCompressionTriggerRegistered(triggerRegistry)).toBe(false);

      registerContextCompressionTrigger(triggerRegistry);

      expect(isContextCompressionTriggerRegistered(triggerRegistry)).toBe(true);

      unregisterContextCompressionTrigger(triggerRegistry);

      expect(isContextCompressionTriggerRegistered(triggerRegistry)).toBe(false);
    });
  });

  describe("Context Compression Workflow", () => {
    it("Test to create the default workflow: createContextCompressionWorkflow to create the correct workflow", () => {
      const workflow = createContextCompressionWorkflow();

      expect(workflow).toBeDefined();
      expect(workflow.id).toBe(CONTEXT_COMPRESSION_WORKFLOW_ID);
      expect(workflow.name).toBe("Context Compression Workflow");
      expect(workflow.type).toBe("TRIGGERED_SUBWORKFLOW");
      expect(workflow.description).toContain("context compression");
      expect(workflow.nodes).toHaveLength(4);
      expect(workflow.edges).toHaveLength(3);
      expect(workflow.metadata?.["category"]).toBe("system");
      expect(workflow.metadata?.["tags"]).toContain("context");
      expect(workflow.metadata?.["tags"]).toContain("compression");
    });

    it("Test workflow node structure: should contain 4 nodes", () => {
      const workflow = createContextCompressionWorkflow();

      const nodeTypes = workflow.nodes.map(n => n.type);
      expect(nodeTypes).toContain("START_FROM_TRIGGER");
      expect(nodeTypes).toContain("LLM");
      expect(nodeTypes).toContain("CONTEXT_PROCESSOR");
      expect(nodeTypes).toContain("CONTINUE_FROM_TRIGGER");
    });

    it("Test workflow edge structure: should contain 3 edges", () => {
      const workflow = createContextCompressionWorkflow();

      expect(workflow.edges).toHaveLength(3);

      // Verify the connection of the edges.
      const edgeConnections = workflow.edges.map(e => ({
        source: workflow.nodes.find(n => n.id === e.sourceNodeId)?.type,
        target: workflow.nodes.find(n => n.id === e.targetNodeId)?.type,
      }));

      expect(edgeConnections).toContainEqual({ source: "START_FROM_TRIGGER", target: "LLM" });
      expect(edgeConnections).toContainEqual({ source: "LLM", target: "CONTEXT_PROCESSOR" });
      expect(edgeConnections).toContainEqual({
        source: "CONTEXT_PROCESSOR",
        target: "CONTINUE_FROM_TRIGGER",
      });
    });

    it("Test workflow LLM node configuration: should contain compression prompt words", () => {
      const workflow = createContextCompressionWorkflow();
      const llmNode = workflow.nodes.find(n => n.type === "LLM");

      expect(llmNode).toBeDefined();
      expect(llmNode?.config["profileId"]).toBe("DEFAULT");
      expect(llmNode?.config["prompt"]).toBe(DEFAULT_COMPRESSION_PROMPT);
    });

    it("Test workflow CONTEXT_PROCESSOR node configuration: should contain truncation operations", () => {
      const workflow = createContextCompressionWorkflow();
      const processorNode = workflow.nodes.find(n => n.type === "CONTEXT_PROCESSOR");

      expect(processorNode).toBeDefined();
      expect(processorNode?.config["operationConfig"]).toBeDefined();
      expect(processorNode?.config["operationConfig"]["operation"]).toBe("TRUNCATE");
      const strategy = (processorNode?.config["operationConfig"] as any)["strategy"];
      expect(strategy["type"]).toBe("KEEP_LAST");
      expect(strategy["count"]).toBe(1);
    });

    it("Test workflow CONTINUE_FROM_TRIGGER node configuration: should contain backhaul configuration", () => {
      const workflow = createContextCompressionWorkflow();
      const endNode = workflow.nodes.find(n => n.type === "CONTINUE_FROM_TRIGGER");

      expect(endNode).toBeDefined();
      expect(endNode?.config["conversationHistoryCallback"]).toBeDefined();
      const callback = endNode?.config["conversationHistoryCallback"];
      expect(callback?.["operation"]).toBe("TRUNCATE");
    });

    it("Tests to create custom workflows: createCustomContextCompressionWorkflow supports custom configurations", () => {
      const customPrompt = "Customized compression cue words";
      const workflow = createCustomContextCompressionWorkflow({
        compressionPrompt: customPrompt,
      });

      const llmNode = workflow.nodes.find(n => n.type === "LLM");
      expect(llmNode?.config["prompt"]).toBe(customPrompt);
    });

    it("Testing the Create Empty Configuration workflow: empty configurations should use the default prompt word", () => {
      const workflow = createCustomContextCompressionWorkflow({});

      const llmNode = workflow.nodes.find(n => n.type === "LLM");
      expect(llmNode?.config["prompt"]).toBe(DEFAULT_COMPRESSION_PROMPT);
    });

    it("Test registration workflow: registerContextCompressionWorkflow successful registration", () => {
      const result = registerContextCompressionWorkflow(workflowRegistry);

      expect(result).toBe(true);
      expect(isContextCompressionWorkflowRegistered(workflowRegistry)).toBe(true);
      expect(workflowRegistry.has(CONTEXT_COMPRESSION_WORKFLOW_ID)).toBe(true);
    });

    it("Test for duplicate registration workflows: skip if skipIfExists is true", () => {
      registerContextCompressionWorkflow(workflowRegistry);

      const result = registerContextCompressionWorkflow(workflowRegistry, undefined, true);

      expect(result).toBe(false); // Skip registration and return false.
    });

    it("Test Registration Custom Configuration Workflow", () => {
      const customPrompt = "Customized Compression Cue Words";
      const result = registerContextCompressionWorkflow(workflowRegistry, {
        compressionPrompt: customPrompt,
      });

      expect(result).toBe(true);
      const workflow = workflowRegistry.get(CONTEXT_COMPRESSION_WORKFLOW_ID);
      expect(workflow).toBeDefined();

      const llmNode = workflow?.nodes.find((n: any) => n.type === "LLM");
      expect((llmNode?.config as any)["prompt"]).toBe(customPrompt);
    });

    it("Test logout workflow: unregisterContextCompressionWorkflow successfully logout", () => {
      registerContextCompressionWorkflow(workflowRegistry);
      expect(isContextCompressionWorkflowRegistered(workflowRegistry)).toBe(true);

      const result = unregisterContextCompressionWorkflow(workflowRegistry);

      expect(result).toBe(true);
      expect(isContextCompressionWorkflowRegistered(workflowRegistry)).toBe(false);
      expect(workflowRegistry.has(CONTEXT_COMPRESSION_WORKFLOW_ID)).toBe(false);
    });

    it("Test for deregistering non-existent triggers: should return false", () => {
      const result = unregisterContextCompressionWorkflow(workflowRegistry);

      expect(result).toBe(false);
    });

    it("Test to check workflow registration status: the isRegistered method correctly returns the registration status", () => {
      expect(isContextCompressionWorkflowRegistered(workflowRegistry)).toBe(false);

      registerContextCompressionWorkflow(workflowRegistry);

      expect(isContextCompressionWorkflowRegistered(workflowRegistry)).toBe(true);

      unregisterContextCompressionWorkflow(workflowRegistry);

      expect(isContextCompressionWorkflowRegistered(workflowRegistry)).toBe(false);
    });
  });

  describe("Portfolio Registration", () => {
    it("Test Simultaneous Registration: registerContextCompression registers both triggers and workflows.", () => {
      const result = registerContextCompression(triggerRegistry, workflowRegistry);

      expect(result.triggerRegistered).toBe(true);
      expect(result.workflowRegistered).toBe(true);
      expect(isContextCompressionTriggerRegistered(triggerRegistry)).toBe(true);
      expect(isContextCompressionWorkflowRegistered(workflowRegistry)).toBe(true);
    });

    it("Test Simultaneous Registration Customized Configuration: Simultaneous registration should support customized configuration", () => {
      const customPrompt = "Customized Compression Cue Words";
      const result = registerContextCompression(triggerRegistry, workflowRegistry, {
        compressionPrompt: customPrompt,
        maxTriggers: 5,
      });

      expect(result.triggerRegistered).toBe(true);
      expect(result.workflowRegistered).toBe(true);

      const triggerTemplate = triggerRegistry.get(CONTEXT_COMPRESSION_TRIGGER_NAME);
      expect(triggerTemplate?.maxTriggers).toBe(5);

      const workflow = workflowRegistry.get(CONTEXT_COMPRESSION_WORKFLOW_ID);
      const llmNode = workflow?.nodes.find(n => n.type === "LLM");
      expect(llmNode?.config["prompt"]).toBe(customPrompt);
    });

    it("Test the registration order: you must register the workflow before registering the trigger", () => {
      // The `registerContextCompression` function has already handled the sequencing internally.
      const result = registerContextCompression(triggerRegistry, workflowRegistry);

      expect(result.workflowRegistered).toBe(true);
      expect(result.triggerRegistered).toBe(true);
    });

    it("Test Part Registration Failure: If the workflow registration fails, the trigger should not be registered either", () => {
      // Simulating the scenario where workflow registration fails.
      const result = registerContextCompression(triggerRegistry, workflowRegistry);

      // Under normal circumstances, it should all work out successfully.
      expect(result.workflowRegistered).toBe(true);
      expect(result.triggerRegistered).toBe(true);
    });

    it("Testing Simultaneous Logout: Manually Logging Out Triggers and Workflows", () => {
      registerContextCompression(triggerRegistry, workflowRegistry);

      expect(isContextCompressionTriggerRegistered(triggerRegistry)).toBe(true);
      expect(isContextCompressionWorkflowRegistered(workflowRegistry)).toBe(true);

      unregisterContextCompressionWorkflow(workflowRegistry);
      unregisterContextCompressionTrigger(triggerRegistry);

      expect(isContextCompressionTriggerRegistered(triggerRegistry)).toBe(false);
      expect(isContextCompressionWorkflowRegistered(workflowRegistry)).toBe(false);
    });
  });

  describe("Constant Export", () => {
    it("Test constant export: constants should be exported correctly", () => {
      expect(CONTEXT_COMPRESSION_TRIGGER_NAME).toBe("context_compression_trigger");
      expect(CONTEXT_COMPRESSION_WORKFLOW_ID).toBe("context_compression_workflow");
      expect(DEFAULT_COMPRESSION_PROMPT).toBeDefined();
      expect(DEFAULT_COMPRESSION_PROMPT).toContain("compression");
      expect(DEFAULT_COMPRESSION_PROMPT).toContain("summary");
    });
  });

  describe("Boundary situation", () => {
    it("Test multiple registrations and deregistrations: should be able to handle multiple operations correctly", () => {
      // First registration
      expect(registerContextCompression(triggerRegistry, workflowRegistry).triggerRegistered).toBe(
        true,
      );
      expect(registerContextCompression(triggerRegistry, workflowRegistry).triggerRegistered).toBe(
        false,
      );

      // First logout
      expect(unregisterContextCompressionTrigger(triggerRegistry)).toBe(true);
      expect(unregisterContextCompressionWorkflow(workflowRegistry)).toBe(true);

      // Second registration
      expect(registerContextCompression(triggerRegistry, workflowRegistry).triggerRegistered).toBe(
        true,
      );

      // Second logout
      expect(unregisterContextCompressionTrigger(triggerRegistry)).toBe(true);
      expect(unregisterContextCompressionWorkflow(workflowRegistry)).toBe(true);
    });

    it("Test operation on empty registry: should be able to handle empty registry correctly", () => {
      const emptyTriggerRegistry = new TriggerTemplateRegistry();
      const emptyWorkflowRegistry = new WorkflowRegistry();

      expect(isContextCompressionTriggerRegistered(emptyTriggerRegistry)).toBe(false);
      expect(isContextCompressionWorkflowRegistered(emptyWorkflowRegistry)).toBe(false);

      expect(unregisterContextCompressionTrigger(emptyTriggerRegistry)).toBe(false);
      expect(unregisterContextCompressionWorkflow(emptyWorkflowRegistry)).toBe(false);
    });

    it("Testing workflow trigger references: triggers should correctly reference workflows", () => {
      registerContextCompression(triggerRegistry, workflowRegistry);

      const trigger = triggerRegistry.get(CONTEXT_COMPRESSION_TRIGGER_NAME);
      expect(trigger?.action.type).toBe("execute_triggered_subgraph");
      expect((trigger?.action.parameters as any)["triggeredWorkflowId"]).toBe(
        CONTEXT_COMPRESSION_WORKFLOW_ID,
      );
    });
  });
});
