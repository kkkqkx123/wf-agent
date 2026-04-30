/**
 * Trigger Template Registry Integration Testing
 *
 * Test Scenarios:
 * - Registration functionality
 * - Query functionality
 * - Update and deletion
 * - Search and import/export
 * - Template conversion
 */

import { describe, it, expect, beforeEach } from "vitest";
import { TriggerTemplateRegistry } from "../services/trigger-template-registry.js";
import type { TriggerTemplate, TriggerTemplateSummary } from "@wf-agent/types";
import { EventType } from "@wf-agent/types";
import { ConfigurationValidationError, TriggerTemplateNotFoundError } from "@wf-agent/types";

describe("Trigger Template Registry - Trigger Template Registry", () => {
  let registry: TriggerTemplateRegistry;

  beforeEach(() => {
    registry = new TriggerTemplateRegistry();
  });

  describe("registration function", () => {
    it("Test Registration of Trigger Templates: Successful registration of a valid trigger template", () => {
      const template: TriggerTemplate = {
        name: "test-template",
        description: "Test Trigger Templates",
        condition: {
          eventType: "WORKFLOW_EXECUTION_STARTED",
        },
        action: {
          type: "pause_workflow_execution",
          parameters: {
            executionId: "${executionId}",
          },
        },
        enabled: true,
        maxTriggers: 5,
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      registry.register(template);

      expect(registry.has("test-template")).toBe(true);
      expect(registry.get("test-template")).toEqual(template);
    });

    it("Test for duplicate registrations: duplicate registrations of templates with the same name should throw an error", () => {
      const template: TriggerTemplate = {
        name: "test-template",
        description: "Test Trigger Templates",
        condition: {
          eventType: "WORKFLOW_EXECUTION_STARTED",
        },
        action: {
          type: "pause_workflow_execution",
          parameters: {
            executionId: "${executionId}",
          },
        },
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      registry.register(template);

      expect(() => registry.register(template)).toThrow(ConfigurationValidationError);
    });

    it("Test Bulk Registration: Bulk Registration of Multiple Trigger Templates", () => {
      const templates: TriggerTemplate[] = [
        {
          name: "template-1",
          description: "Template 1",
          condition: { eventType: "WORKFLOW_EXECUTION_STARTED" },
          action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
          createdAt: Date.now(),
          updatedAt: Date.now(),
        },
        {
          name: "template-2",
          description: "Template 2",
          condition: { eventType: "WORKFLOW_EXECUTION_COMPLETED" },
          action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
          createdAt: Date.now(),
          updatedAt: Date.now(),
        },
        {
          name: "template-3",
          description: "Template 3",
          condition: { eventType: "NODE_FAILED" },
          action: { type: "resume_workflow_execution", parameters: { executionId: "${executionId}" } },
          createdAt: Date.now(),
          updatedAt: Date.now(),
        },
      ];

      registry.registerBatch(templates);

      expect(registry.size()).toBe(3);
      expect(registry.has("template-1")).toBe(true);
      expect(registry.has("template-2")).toBe(true);
      expect(registry.has("template-3")).toBe(true);
    });

    it("Test Validation Template Configuration: Missing Required Fields Should Throw an Error", () => {
      const template: any = {
        name: "test-template",
        // "Condition is missing"
        action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
      };

      expect(() => registry.register(template)).toThrow(ConfigurationValidationError);
    });

    it("Test to validate event types: invalid event types should throw an error", () => {
      const template: any = {
        name: "test-template",
        condition: {
          eventType: "INVALID_EVENT_TYPE",
        },
        action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
      };

      expect(() => registry.register(template)).toThrow(ConfigurationValidationError);
    });

    it("Tests validate action types: invalid action types should throw an error", () => {
      const template: any = {
        name: "test-template",
        condition: {
          eventType: "WORKFLOW_EXECUTION_STARTED",
        },
        action: {
          type: "invalid_action_type",
          parameters: { executionId: "${executionId}" },
        },
      };

      expect(() => registry.register(template)).toThrow(ConfigurationValidationError);
    });
  });

  describe("search function", () => {
    beforeEach(() => {
      const templates: TriggerTemplate[] = [
        {
          name: "template-1",
          description: "First template",
          condition: { eventType: "WORKFLOW_EXECUTION_STARTED" },
          action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
          metadata: { category: "lifecycle", tags: ["workflowExecution"] },
          createdAt: Date.now(),
          updatedAt: Date.now(),
        },
        {
          name: "template-2",
          description: "Second template",
          condition: { eventType: "NODE_FAILED" },
          action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
          metadata: { category: "error", tags: ["node", "error"] },
          createdAt: Date.now(),
          updatedAt: Date.now(),
        },
      ];

      registry.registerBatch(templates);
    });

    it("Test Get Trigger Template: Get Registered Template by Name", () => {
      const template = registry.get("template-1");

      expect(template).toBeDefined();
      expect(template!.name).toBe("template-1");
    });

    it("Test to get a template that doesn't exist: returns undefined", () => {
      const template = registry.get("non-existent");

      expect(template).toBeUndefined();
    });

    it("Tests check for existence: the has method correctly returns whether the template exists or not", () => {
      expect(registry.has("template-1")).toBe(true);
      expect(registry.has("non-existent")).toBe(false);
    });

    it("Test to list all templates: list method returns all registered templates", () => {
      const templates = registry.list();

      expect(templates).toHaveLength(2);
      expect(templates.map(t => t.name)).toEqual(
        expect.arrayContaining(["template-1", "template-2"]),
      );
    });

    it("Test List Summaries: listSummaries returns template summary information", () => {
      const summaries = registry.listSummaries();

      expect(summaries).toHaveLength(2);
      expect(summaries[0]).toBeDefined();
      expect(summaries[0]!).toMatchObject({
        name: "template-1",
        description: "First template",
        category: "lifecycle",
        tags: ["workflowExecution"],
      });
      expect(summaries[0]!.createdAt).toBeDefined();
      expect(summaries[0]!.updatedAt).toBeDefined();
    });

    it("Test to get the number of templates: the size method returns the number of templates", () => {
      expect(registry.size()).toBe(2);
    });
  });

  describe("Updates and deletions", () => {
    beforeEach(() => {
      const template: TriggerTemplate = {
        name: "test-template",
        description: "original description",
        condition: { eventType: "WORKFLOW_EXECUTION_STARTED" },
        action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
        enabled: true,
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      registry.register(template);
    });

    it("Testing update trigger templates: update method correctly updates template configuration", async () => {
      const originalTemplate = registry.get("test-template");
      const originalUpdatedAt = originalTemplate!.updatedAt;

      // Wait for 1 millisecond to ensure the timestamps are different.
      await new Promise(resolve => setTimeout(resolve, 1));

      registry.update("test-template", {
        description: "Updated description",
        enabled: false,
      });

      const updatedTemplate = registry.get("test-template");

      expect(updatedTemplate).toBeDefined();
      expect(updatedTemplate!.description).toBe("Updated description");
      expect(updatedTemplate!.enabled).toBe(false);
      expect(updatedTemplate!.name).toBe("test-template"); // The name should not be changed.
      expect(updatedTemplate!.updatedAt).toBeGreaterThan(originalUpdatedAt);
    });

    it("Test for updating a template that does not exist: an error should be thrown", () => {
      expect(() => {
        registry.update("non-existent", { description: "new description" });
      }).toThrow(TriggerTemplateNotFoundError);
    });

    it("Test update to invalid configuration: name should not be changed", () => {
      const originalName = "test-template";
      registry.update(originalName, {
        name: "invalid-name-change", // Name changes should not be allowed.
      } as any);

      // Verify that the name has not changed.
      expect(registry.get(originalName)?.name).toBe(originalName);
    });

    it("Test deleting trigger templates: unregister method deletes the template correctly", () => {
      expect(registry.has("test-template")).toBe(true);

      registry.unregister("test-template");

      expect(registry.has("test-template")).toBe(false);
      expect(registry.get("test-template")).toBeUndefined();
    });

    it("Test for deletion of non-existing templates: an error should be thrown", () => {
      expect(() => {
        registry.unregister("non-existent");
      }).toThrow(TriggerTemplateNotFoundError);
    });

    it("Test Batch Deletion: unregisterBatch Batch Delete Multiple Templates", () => {
      const templates: TriggerTemplate[] = [
        {
          name: "template-1",
          description: "Template 1",
          condition: { eventType: "WORKFLOW_EXECUTION_STARTED" },
          action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
          createdAt: Date.now(),
          updatedAt: Date.now(),
        },
        {
          name: "template-2",
          description: "Template 2",
          condition: { eventType: "WORKFLOW_EXECUTION_COMPLETED" },
          action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
          createdAt: Date.now(),
          updatedAt: Date.now(),
        },
        {
          name: "template-3",
          description: "Template 3",
          condition: { eventType: "NODE_FAILED" },
          action: { type: "resume_workflow_execution", parameters: { executionId: "${executionId}" } },
          createdAt: Date.now(),
          updatedAt: Date.now(),
        },
      ];

      registry.registerBatch(templates);

      expect(registry.size()).toBe(4); // Including the template from beforeEach

      registry.unregisterBatch(["template-1", "template-2"]);

      expect(registry.size()).toBe(2);
      expect(registry.has("template-1")).toBe(false);
      expect(registry.has("template-2")).toBe(false);
      expect(registry.has("template-3")).toBe(true);
      expect(registry.has("test-template")).toBe(true);
    });

    it("Test clear all: clear method clears all templates", () => {
      const templates: TriggerTemplate[] = [
        {
          name: "template-1",
          description: "Template 1",
          condition: { eventType: "WORKFLOW_EXECUTION_STARTED" },
          action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
          createdAt: Date.now(),
          updatedAt: Date.now(),
        },
        {
          name: "template-2",
          description: "Template 2",
          condition: { eventType: "WORKFLOW_EXECUTION_COMPLETED" },
          action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
          createdAt: Date.now(),
          updatedAt: Date.now(),
        },
      ];

      registry.registerBatch(templates);

      expect(registry.size()).toBe(3);

      registry.clear();

      expect(registry.size()).toBe(0);
      expect(registry.list()).toHaveLength(0);
    });
  });

  describe("search function", () => {
    beforeEach(() => {
      const templates: TriggerTemplate[] = [
        {
          name: "workflow-execution-started-template",
          description: "Triggered when workflow execution is started",
          condition: { eventType: "WORKFLOW_EXECUTION_STARTED" },
          action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
          metadata: { category: "lifecycle", tags: ["workflowExecution", "lifecycle"] },
          createdAt: Date.now(),
          updatedAt: Date.now(),
        },
        {
          name: "node-failed-template",
          description: "Triggered on node failure",
          condition: { eventType: "NODE_FAILED" },
          action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
          metadata: { category: "error", tags: ["node", "error"] },
          createdAt: Date.now(),
          updatedAt: Date.now(),
        },
        {
          name: "compression-template",
          description: "Context Compression Trigger",
          condition: { eventType: "MESSAGE_ADDED" },
          action: { type: "execute_script", parameters: { scriptName: "compress-context" } },
          metadata: { category: "optimization", tags: ["compression", "message"] },
          createdAt: Date.now(),
          updatedAt: Date.now(),
        },
      ];

      registry.registerBatch(templates);
    });

    it("Test search function: search method to search templates based on keywords", () => {
      const results = registry.search("workflowExecution");

      expect(results).toHaveLength(1);
      expect(results[0]!.name).toBe("workflow-execution-started-template");
    });

    it("Test search description: you can search based on the description keywords", () => {
      const results = registry.search("failure");

      expect(results).toHaveLength(1);
      expect(results[0]!.name).toBe("node-failed-template");
    });

    it("Test search tags: you can search by tags", () => {
      const results = registry.search("compression");

      expect(results).toHaveLength(1);
      expect(results[0]!.name).toBe("compression-template");
    });

    it("Test search categories: you can search by category", () => {
      const results = registry.search("error");

      expect(results).toHaveLength(1);
      expect(results[0]!.name).toBe("node-failed-template");
    });

    it("Test search with no results: return empty array when keywords don't match", () => {
      const results = registry.search("non-existent-keyword");

      expect(results).toHaveLength(0);
    });

    it("Testing for case-insensitive searches: searches should be case-insensitive", () => {
      const results1 = registry.search("THREAD");
      const results2 = registry.search("workflowExecution");

      expect(results1).toHaveLength(1);
      expect(results2).toHaveLength(1);
      expect(results1[0]!.name).toBe(results2[0]!.name);
    });
  });

  describe("Import and export function", () => {
    beforeEach(() => {
      const template: TriggerTemplate = {
        name: "export-test-template",
        description: "Templates for testing import and export",
        condition: { eventType: "WORKFLOW_EXECUTION_STARTED" },
        action: {
          type: "pause_workflow_execution",
          parameters: {
            executionId: "${executionId}",
          },
        },
        metadata: { key: "value" },
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      registry.register(template);
    });

    it("Test export function: export method to export the template as JSON", () => {
      const json = registry.export("export-test-template");

      expect(json).toBeDefined();
      expect(typeof json).toBe("string");

      const parsed = JSON.parse(json);
      expect(parsed.name).toBe("export-test-template");
      expect(parsed.description).toBe("Templates for testing import and export");
      expect(parsed.metadata).toEqual({ key: "value" });
    });

    it("Test exporting a non-existent template: an error should be thrown", () => {
      expect(() => {
        registry.export("non-existent");
      }).toThrow(TriggerTemplateNotFoundError);
    });

    it("Test the import function: import method to import templates from JSON", () => {
      const json = JSON.stringify({
        name: "import-test-template",
        description: "Templates imported from JSON",
        condition: { eventType: "WORKFLOW_EXECUTION_COMPLETED" },
        action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
        createdAt: Date.now(),
        updatedAt: Date.now(),
      });

      const templateName = registry.import(json);

      expect(templateName).toBe("import-test-template");
      expect(registry.has("import-test-template")).toBe(true);
    });

    it("Test for importing invalid JSON: should throw an error", () => {
      const invalidJson = "{ invalid json }";

      expect(() => {
        registry.import(invalidJson);
      }).toThrow(ConfigurationValidationError);
    });

    it("Test importing invalid template configurations: should throw validation errors", () => {
      const invalidConfig = JSON.stringify({
        name: "invalid-template",
        // "condition is missing"
        action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
      });

      expect(() => {
        registry.import(invalidConfig);
      }).toThrow(ConfigurationValidationError);
    });

    it("Test for importing duplicate templates: an error should be thrown", () => {
      const json = registry.export("export-test-template");

      expect(() => {
        registry.import(json);
      }).toThrow(ConfigurationValidationError);
    });
  });

  describe("template conversion", () => {
    beforeEach(() => {
      const template: TriggerTemplate = {
        name: "test-template",
        description: "Test Templates",
        condition: {
          eventType: "WORKFLOW_EXECUTION_STARTED",
        },
        action: {
          type: "pause_workflow_execution",
          parameters: {
            executionId: "${executionId}",
          },
        },
        enabled: true,
        maxTriggers: 10,
        metadata: { category: "test" },
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      registry.register(template);
    });

    it("Test convertToWorkflowTrigger: convertToWorkflowTrigger correctly converts the template", () => {
      const workflowTrigger = registry.convertToWorkflowTrigger(
        "test-template",
        "trigger-123",
        "Custom Trigger Names",
      );

      expect(workflowTrigger.id).toBe("trigger-123");
      expect(workflowTrigger.name).toBe("Custom Trigger Names");
      expect(workflowTrigger.description).toBe("Test Templates");
      expect(workflowTrigger.condition.eventType).toBe("WORKFLOW_EXECUTION_STARTED");
      expect(workflowTrigger.action.type).toBe("pause_workflow_execution");
      expect(workflowTrigger.enabled).toBe(true);
      expect(workflowTrigger.maxTriggers).toBe(10);
      expect(workflowTrigger.metadata).toEqual({ category: "test" });
    });

    it("Test conversion without specifying triggerName: use template name", () => {
      const workflowTrigger = registry.convertToWorkflowTrigger(
        "test-template",
        "trigger-123",
        // `triggerName` is not specified.
      );

      expect(workflowTrigger.name).toBe("test-template");
    });

    it("Test Configuration Override: Configuration override is supported during conversion", () => {
      const workflowTrigger = registry.convertToWorkflowTrigger(
        "test-template",
        "trigger-123",
        "Custom Trigger Names",
        {
          condition: {
            eventType: "WORKFLOW_EXECUTION_COMPLETED",
          },
          action: {
            type: "pause_workflow_execution",
          },
          enabled: false,
          maxTriggers: 20,
        },
      );

      expect(workflowTrigger.condition.eventType).toBe("WORKFLOW_EXECUTION_COMPLETED");
      expect(workflowTrigger.action.type).toBe("pause_workflow_execution");
      expect(workflowTrigger.enabled).toBe(false);
      expect(workflowTrigger.maxTriggers).toBe(20);
    });

    it("Test partial configuration override: override only specified fields", () => {
      const workflowTrigger = registry.convertToWorkflowTrigger(
        "test-template",
        "trigger-123",
        "Custom Trigger Names",
        {
          enabled: false,
        },
      );

      expect(workflowTrigger.enabled).toBe(false);
      expect(workflowTrigger.maxTriggers).toBe(10); // The value should be preserved as is.
      expect(workflowTrigger.condition.eventType).toBe("WORKFLOW_EXECUTION_STARTED"); // The value should be preserved as is.
      expect(workflowTrigger.action.type).toBe("pause_workflow_execution"); // The value should be preserved as is.
    });

    it("Test conversion of non-existent templates: an error should be thrown", () => {
      expect(() => {
        registry.convertToWorkflowTrigger("non-existent", "trigger-123");
      }).toThrow(TriggerTemplateNotFoundError);
    });
  });

  describe("Boundary situation", () => {
    it("Testing an empty registry operation: performing an operation on an empty registry", () => {
      const emptyRegistry = new TriggerTemplateRegistry();

      expect(emptyRegistry.size()).toBe(0);
      expect(emptyRegistry.list()).toHaveLength(0);
      expect(emptyRegistry.get("any-name")).toBeUndefined();
      expect(emptyRegistry.has("any-name")).toBe(false);
      expect(emptyRegistry.search("any-keyword")).toHaveLength(0);
    });

    it("Test template name contains special characters", () => {
      const template: TriggerTemplate = {
        name: "template-with-special-chars_123!@#",
        description: "Templates containing special characters",
        condition: { eventType: "WORKFLOW_EXECUTION_STARTED" },
        action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      registry.register(template);

      expect(registry.has("template-with-special-chars_123!@#")).toBe(true);
      expect(registry.get("template-with-special-chars_123!@#")?.name).toBe(
        "template-with-special-chars_123!@#",
      );
    });

    it("Test templates contain a lot of metadata", () => {
      const largeMetadata: Record<string, any> = {};
      for (let i = 0; i < 100; i++) {
        largeMetadata[`key${i}`] = `value${i}`;
      }

      const template: TriggerTemplate = {
        name: "large-metadata-template",
        description: "Templates with lots of metadata",
        condition: { eventType: "WORKFLOW_EXECUTION_STARTED" },
        action: { type: "pause_workflow_execution", parameters: { executionId: "${executionId}" } },
        metadata: largeMetadata,
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      registry.register(template);

      const retrieved = registry.get("large-metadata-template");
      expect(retrieved?.metadata).toEqual(largeMetadata);
    });
  });
});
